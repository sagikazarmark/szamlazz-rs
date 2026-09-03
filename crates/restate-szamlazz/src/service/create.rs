//! The create protocol (design §6) for the four slot kinds and for
//! correctives, in the order the steps appear in the design.
//!
//! Every szamlazz.hu call is a journaled run; every ledger mutation that must
//! survive a crash is followed by a `ctx.set`; timestamps come from a
//! journaled run; domain outcomes are data and faults are `TerminalError`s.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;
use szamlazz_agent::ops::invoice::CreateInvoice;

use super::Order;
use super::support::object::{
    hint, load, now, proforma_fate, query_external_id, run_once, save, sleep, verify,
};
use super::support::{
    Fault, ProformaFate, expected_supplier, learn_supplier, next_backoff, order_key,
    remaining_backoff, validate_found,
};
use crate::contract::response::outstanding;
use crate::contract::{
    ConflictReason, CorrectRequest, CreateRequest, CreateResponse, DocumentInput, DocumentKind,
    IssuedKind, Outcome, ProformaLink, RequestId, Warning,
};
use crate::identity::{ExternalId, Fingerprint, OrderKey, normalize_buyer_name};
use crate::ledger::{
    CommittedDocument, HistoryKind, Ledger, Origin, RequestRef, Reversal, ReversalOrigin, Slot,
    SlotStatus, Target,
};
use crate::steps::{
    DocumentRefs, FoundDocument, IssueOutcome, IssueRequest, QueryOutcome, gross_total,
};

/// The identity fields every [`CreateResponse`] carries.
#[derive(Debug, Clone)]
struct Identity {
    request_id: RequestId,
    kind: IssuedKind,
    generation: u32,
    external_id: ExternalId,
}

impl Identity {
    fn corrective(
        order: &OrderKey,
        slug: &crate::config::AccountSlug,
        id: &RequestId,
        cseq: u32,
    ) -> Self {
        Self {
            request_id: id.clone(),
            kind: IssuedKind::Corrective,
            generation: cseq,
            external_id: ExternalId::for_corrective(slug, order, cseq),
        }
    }

    fn respond(&self, outcome: Outcome) -> CreateResponse {
        CreateResponse::new(
            outcome,
            self.request_id.clone(),
            self.kind,
            self.generation,
            self.external_id.as_str(),
        )
    }

    fn conflict(&self, reason: ConflictReason) -> CreateResponse {
        self.respond(Outcome::Conflict).with_conflict_reason(reason)
    }

    fn conflict_about(&self, reason: ConflictReason, number: impl Into<String>) -> CreateResponse {
        self.conflict(reason).with_existing_number(number)
    }

    fn rejected(&self, code: impl Into<String>, message: impl Into<String>) -> CreateResponse {
        self.respond(Outcome::Rejected)
            .with_code(code)
            .with_message(message)
    }

    /// A response carrying the found document's number and totals.
    fn found(&self, outcome: Outcome, found: &FoundDocument) -> CreateResponse {
        let mut response = self
            .respond(outcome)
            .with_invoice_number(found.number.clone());
        response.net_total = found.net;
        response.gross_total = found.gross;
        response.outstanding = outstanding(found.gross, &found.payments);
        response
    }

    /// `outcome: reversed` for `number`, reversed by `by`.
    fn reversed(&self, number: Option<&str>, by: Option<&str>) -> CreateResponse {
        let mut response = self.respond(Outcome::Reversed);
        response.invoice_number = number.map(str::to_owned);
        response.storno_number = by.map(str::to_owned);
        response
    }
}

/// The validated input of a create request (step 0).
struct Prepared {
    order: OrderKey,
    kind: DocumentKind,
    request_id: RequestId,
    document: DocumentInput,
    reissue: bool,
    proforma: ProformaLink,
    fp: Fingerprint,
}

impl Prepared {
    fn issued_kind(&self) -> IssuedKind {
        self.kind.into()
    }
}

/// The validated input of a correct request.
struct Corrective {
    order: OrderKey,
    invoice_number: String,
    request_id: RequestId,
    document: DocumentInput,
    fp: Fingerprint,
}

/// Where a dispatch step leads.
enum Flow {
    /// Answer now.
    Respond(Box<CreateResponse>),
    /// Allocate a new intent (step 3) and issue.
    Allocate,
    /// The slot is already `pending` under the request id: issue without
    /// allocating.
    Resume,
}

impl Flow {
    fn respond(response: CreateResponse) -> Self {
        Self::Respond(Box::new(response))
    }
}

/// The document references resolved in step 3.
#[derive(Debug, Default)]
struct Refs {
    proforma: Option<String>,
    prepayment: Option<String>,
    /// The proforma the ledger owns and the hint may see converted.
    our_proforma: Option<String>,
    /// Run the order-number hint on the first attempt.
    check_hint: bool,
}

/// One document to issue: what the attempt loop (step 4) needs.
struct Intent {
    target: Target,
    identity: Identity,
    create: CreateInvoice,
    check_hint: bool,
    /// The proforma reference sent in the create (`dijbekeroSzamlaszam`).
    proforma_ref: Option<String>,
    our_proforma: Option<String>,
}

/// The generation a new allocation on `kind` would use.
fn candidate_generation(ledger: &Ledger, kind: DocumentKind) -> u32 {
    ledger.slot(kind).map_or(0, |slot| slot.generation)
}

impl Order {
    // ----- entry points ----------------------------------------------------

    /// `create_proforma` / `create_invoice` / `create_prepayment` /
    /// `create_final`.
    pub(super) async fn issue_slot(
        &self,
        ctx: &ObjectContext<'_>,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<CreateResponse, HandlerError> {
        // Step 0: validate (pure).
        let prepared = self.prepare(ctx.key(), kind, request)?;
        let mut ledger = load(ctx).await?;

        // Step 0: identity.
        let flow = match ledger.lookup_request(&prepared.request_id).cloned() {
            Some(RequestRef::Slot {
                kind: known,
                generation,
            }) => {
                self.known_slot_request(ctx, &mut ledger, &prepared, known, generation)
                    .await?
            }
            Some(RequestRef::Corrective { cseq }) => Flow::respond(
                self.identity(&prepared, cseq)
                    .conflict(ConflictReason::RequestIdReused),
            ),
            // The request's pending slot was taken over by another request id;
            // the id can no longer be honoured.
            Some(RequestRef::Abandoned) => Flow::respond(
                self.identity(&prepared, candidate_generation(&ledger, kind))
                    .conflict(ConflictReason::RequestIdReused),
            ),
            None => self.new_slot_request(ctx, &mut ledger, &prepared).await?,
        };

        match flow {
            Flow::Respond(response) => Ok(*response),
            Flow::Allocate => self.allocate_and_issue(ctx, ledger, &prepared).await,
            Flow::Resume => self.resume_and_issue(ctx, ledger, &prepared).await,
        }
    }

    /// `correct_invoice`.
    pub(super) async fn correct(
        &self,
        ctx: &ObjectContext<'_>,
        request: CorrectRequest,
    ) -> Result<CreateResponse, HandlerError> {
        let order = order_key(ctx.key())?;
        let CorrectRequest {
            invoice_number,
            request_id,
            document,
        } = request;
        let fp = self.fingerprint(IssuedKind::Corrective, &document, &order)?;
        let input = Corrective {
            order,
            invoice_number,
            request_id,
            document,
            fp,
        };
        let ledger = load(ctx).await?;
        match ledger.lookup_request(&input.request_id).cloned() {
            Some(RequestRef::Corrective { cseq }) => {
                self.correct_known(ctx, ledger, &input, cseq).await
            }
            Some(RequestRef::Slot { .. } | RequestRef::Abandoned) => Ok(Identity::corrective(
                &input.order,
                &self.config.account.slug,
                &input.request_id,
                ledger.next_cseq(),
            )
            .conflict(ConflictReason::RequestIdReused)),
            None => self.correct_new(ctx, ledger, input).await,
        }
    }

    /// `correct_invoice` for a known request id: the corrective's current
    /// state.
    async fn correct_known(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        input: &Corrective,
        cseq: u32,
    ) -> Result<CreateResponse, HandlerError> {
        let identity = Identity::corrective(
            &input.order,
            &self.config.account.slug,
            &input.request_id,
            cseq,
        );
        let Some(entry) = ledger.corrective(&input.request_id).cloned() else {
            return Err(Fault::invalid_input(format!(
                "ledger inconsistent: request {} has no corrective entry",
                input.request_id
            ))
            .into());
        };
        if entry.corrected_number != input.invoice_number || entry.fp != input.fp {
            return Ok(identity.conflict_about(
                ConflictReason::PayloadMismatch,
                entry.corrected_number.clone(),
            ));
        }
        let target = Target::Corrective(input.request_id.clone());
        match entry.status {
            SlotStatus::Pending => {
                self.presleep(ctx, entry.last_attempt_at).await?;
                let outcome = query_external_id(
                    ctx,
                    &self.steps,
                    format!("reconcile-corrective-{cseq}"),
                    &identity.external_id,
                )
                .await?;
                match outcome {
                    QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
                    QueryOutcome::NotFound => {
                        let create = self.build_corrective(input, &identity.external_id)?;
                        self.attempt_loop(
                            ctx,
                            ledger,
                            &input.order,
                            corrective_intent(target, identity, create),
                        )
                        .await
                    }
                    QueryOutcome::Found(found) => {
                        if validate_found(
                            &found,
                            &input.order,
                            IssuedKind::Corrective,
                            &self.config,
                            expected_supplier(&self.config, &ledger),
                        )
                        .is_err()
                        {
                            return Ok(identity.conflict_about(
                                ConflictReason::ExternalIdCollision,
                                found.number,
                            ));
                        }
                        adopt_found(ctx, &mut ledger, &target, &identity, &found, true, false)
                            .map(|flow| flow_response(flow, "corrective adoption"))?
                    }
                }
            }
            SlotStatus::Committed | SlotStatus::ReversalUnverified => {
                let Some(number) = entry.number.clone() else {
                    return Err(Fault::invalid_input(
                        "ledger inconsistent: committed corrective without a number",
                    )
                    .into());
                };
                let retrying = entry.status == SlotStatus::ReversalUnverified;
                self.verify_corrective(ctx, ledger, &target, &identity, &number, retrying)
                    .await
            }
            SlotStatus::Rejected { code, message } => Ok(identity.rejected(code, message)),
            SlotStatus::Reversed { by, .. } => {
                Ok(identity.reversed(entry.number.as_deref(), by.as_deref()))
            }
            SlotStatus::Blocked { .. }
            | SlotStatus::Consumed { .. }
            | SlotStatus::Deleted
            | SlotStatus::Vacant => Err(Fault::invalid_input(format!(
                "ledger inconsistent: corrective {cseq} is {}",
                entry.status
            ))
            .into()),
        }
    }

    /// A committed corrective, verified live.
    async fn verify_corrective(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        target: &Target,
        identity: &Identity,
        number: &str,
        retrying: bool,
    ) -> Result<CreateResponse, HandlerError> {
        let outcome = verify(
            ctx,
            &self.steps,
            format!("verify-corrective-{}", identity.generation),
            number,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                Ok(identity.conflict_about(ConflictReason::RecordedDocumentMissing, number))
            }
            QueryOutcome::Found(found) => {
                learn_supplier(&mut ledger, &found)?;
                if found.reversed == Some(true) {
                    let origin = if retrying {
                        ReversalOrigin::Service
                    } else {
                        ReversalOrigin::External
                    };
                    ledger
                        .mark_reversed(
                            target,
                            Reversal::new(origin).with_payments_before(found.payments.clone()),
                        )
                        .map_err(Fault::from)?;
                    save(ctx, &ledger);
                    return Ok(identity.reversed(Some(number), None));
                }
                save(ctx, &ledger);
                Ok(identity.found(Outcome::AlreadyIssued, &found))
            }
        }
    }

    /// `correct_invoice` for a new request id: check the base, allocate,
    /// issue.
    async fn correct_new(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        input: Corrective,
    ) -> Result<CreateResponse, HandlerError> {
        let slug = &self.config.account.slug;
        let identity =
            Identity::corrective(&input.order, slug, &input.request_id, ledger.next_cseq());
        let number = input.invoice_number.clone();

        // The base must be a committed, live invoice of this order.
        let Some(base) = ledger.find_by_number(&number) else {
            return Err(Fault::invalid_input(format!(
                "invoice {number} is not managed by this order (not_managed)"
            ))
            .into());
        };
        if base == Target::Slot(DocumentKind::Proforma) {
            return Err(Fault::invalid_input("a proforma cannot be corrected").into());
        }
        let status = match &base {
            Target::Slot(kind) => ledger.slot(*kind).map(|slot| slot.status.clone()),
            Target::Corrective(id) => ledger.corrective(id).map(|entry| entry.status.clone()),
        };
        match status {
            Some(SlotStatus::Reversed { .. }) => {
                return Ok(identity.conflict_about(ConflictReason::BaseReversed, number));
            }
            Some(SlotStatus::Pending) => {
                return Ok(identity.conflict_about(ConflictReason::Pending, number));
            }
            Some(SlotStatus::Committed | SlotStatus::ReversalUnverified) => {}
            _ => {
                return Err(Fault::invalid_input(format!(
                    "invoice {number} is not a committed document of this order"
                ))
                .into());
            }
        }
        match verify(ctx, &self.steps, format!("verify-base-{number}"), &number).await? {
            QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                return Ok(identity.conflict_about(ConflictReason::RecordedDocumentMissing, number));
            }
            QueryOutcome::Found(found) => {
                learn_supplier(&mut ledger, &found)?;
                if found.reversed == Some(true) {
                    ledger
                        .mark_reversed(
                            &base,
                            Reversal::new(ReversalOrigin::External)
                                .with_payments_before(found.payments.clone()),
                        )
                        .map_err(Fault::from)?;
                    save(ctx, &ledger);
                    return Ok(identity.conflict_about(ConflictReason::BaseReversed, number));
                }
            }
        }

        // Allocate the corrective intent before any issuing call.
        let cseq = ledger
            .allocate_corrective(input.request_id.clone(), number.clone(), input.fp.clone())
            .map_err(Fault::from)?;
        let identity = Identity::corrective(&input.order, slug, &input.request_id, cseq);
        let create = self.build_corrective(&input, &identity.external_id)?;
        save(ctx, &ledger);
        let target = Target::Corrective(input.request_id.clone());
        self.attempt_loop(
            ctx,
            ledger,
            &input.order,
            corrective_intent(target, identity, create),
        )
        .await
    }

    // ----- step 0: validation ----------------------------------------------

    fn prepare(
        &self,
        key: &str,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<Prepared, Fault> {
        let order = order_key(key)?;
        let CreateRequest {
            request_id,
            document,
            options,
        } = request;
        if options.proforma != ProformaLink::Ledger
            && !matches!(kind, DocumentKind::Invoice | DocumentKind::Prepayment)
        {
            return Err(Fault::invalid_input(format!(
                "options.proforma applies to create_invoice and create_prepayment only, not create_{kind}"
            )));
        }
        let fp = self.fingerprint(kind.into(), &document, &order)?;
        Ok(Prepared {
            order,
            kind,
            request_id,
            document,
            reissue: options.reissue,
            proforma: options.proforma,
            fp,
        })
    }

    /// Validates the document by building it once (with placeholder
    /// references) and computes the payload fingerprint.
    fn fingerprint(
        &self,
        kind: IssuedKind,
        document: &DocumentInput,
        order: &OrderKey,
    ) -> Result<Fingerprint, Fault> {
        let name = normalize_buyer_name(&document.buyer.name);
        if name.is_empty() {
            return Err(Fault::invalid_input("buyer.name must not be empty"));
        }
        let placeholder = ExternalId::new("-");
        let create = self.build(
            kind,
            document,
            order,
            &placeholder,
            DocumentRefs {
                proforma: None,
                prepayment: Some("-"),
                corrected: Some("-"),
            },
        )?;
        Ok(Fingerprint::compute(
            self.config.account.fp_secret.expose().as_bytes(),
            &name,
            gross_total(&create),
            document.issue_date,
            Some(document.due_date),
            Some(document.fulfillment_date),
        ))
    }

    fn build(
        &self,
        kind: IssuedKind,
        document: &DocumentInput,
        order: &OrderKey,
        external_id: &ExternalId,
        refs: DocumentRefs<'_>,
    ) -> Result<CreateInvoice, Fault> {
        self.steps
            .build_create(kind, document, order, external_id, refs)
            .map_err(|error| Fault::invalid_input(error.to_string()))
    }

    fn build_corrective(
        &self,
        input: &Corrective,
        external_id: &ExternalId,
    ) -> Result<CreateInvoice, Fault> {
        self.build(
            IssuedKind::Corrective,
            &input.document,
            &input.order,
            external_id,
            DocumentRefs {
                corrected: Some(&input.invoice_number),
                ..DocumentRefs::default()
            },
        )
    }

    fn identity(&self, prepared: &Prepared, generation: u32) -> Identity {
        Identity {
            request_id: prepared.request_id.clone(),
            kind: prepared.issued_kind(),
            generation,
            external_id: ExternalId::for_slot(
                &self.config.account.slug,
                &prepared.order,
                prepared.kind,
                generation,
            ),
        }
    }

    // ----- step 0: identity of a known request id --------------------------

    async fn known_slot_request(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        known_kind: DocumentKind,
        generation: u32,
    ) -> Result<Flow, HandlerError> {
        let identity = self.identity(prepared, generation);
        if known_kind != prepared.kind {
            return Ok(Flow::respond(
                identity.conflict(ConflictReason::RequestIdReused),
            ));
        }
        if prepared.reissue {
            return Err(Fault::invalid_input("reissue requires a new request_id").into());
        }
        let Some(slot) = ledger.slot(prepared.kind).cloned() else {
            return Err(Fault::invalid_input(format!(
                "ledger inconsistent: request {} refers to a missing {} slot",
                prepared.request_id, prepared.kind
            ))
            .into());
        };
        if slot.request_id != prepared.request_id {
            // The id owned an earlier generation that has since been closed;
            // its state is in the history.
            return Ok(Flow::respond(closed_generation(
                ledger, &identity, generation,
            )));
        }
        if slot.fp != prepared.fp {
            let mut response = identity.conflict(ConflictReason::PayloadMismatch);
            response.existing_number.clone_from(&slot.number);
            return Ok(Flow::respond(response));
        }
        match &slot.status {
            SlotStatus::Pending => {
                self.resume_pending(ctx, ledger, prepared, &slot, true)
                    .await
            }
            SlotStatus::Committed | SlotStatus::ReversalUnverified => {
                self.verify_committed(ctx, ledger, prepared, &slot, true)
                    .await
            }
            SlotStatus::Reversed { by, .. } => Ok(Flow::respond(
                identity.reversed(slot.number.as_deref(), by.as_deref()),
            )),
            SlotStatus::Rejected { code, message } => {
                Ok(Flow::respond(identity.rejected(code, message)))
            }
            SlotStatus::Blocked { .. } => {
                self.reconcile_blocked(ctx, ledger, prepared, &slot, true)
                    .await
            }
            SlotStatus::Consumed { by } => Ok(Flow::respond(
                identity.conflict_about(ConflictReason::ProformaConsumed, by),
            )),
            // A deleted proforma: gone, nothing new issued; a new request id
            // issues the next generation flag-free.
            SlotStatus::Deleted => Ok(Flow::respond(
                identity.reversed(slot.number.as_deref(), None),
            )),
            // An operator `forget` moved the slot past the generation this id
            // allocated: the id's document is gone and it never re-allocates.
            SlotStatus::Vacant if slot.generation != generation => Ok(Flow::respond(
                closed_generation(ledger, &identity, generation),
            )),
            // Cleared after a foreign detection at the same generation: the id
            // may re-allocate it.
            SlotStatus::Vacant => Ok(Flow::Allocate),
        }
    }

    // ----- steps 1–2: a new request id -------------------------------------

    async fn new_slot_request(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
    ) -> Result<Flow, HandlerError> {
        let identity = self.identity(prepared, candidate_generation(ledger, prepared.kind));

        // Step 1: exclusivity (a check, not state).
        if let Some(response) = exclusivity(ledger, prepared, &identity)? {
            return Ok(Flow::respond(response));
        }

        // Step 2: slot dispatch.
        let Some(slot) = ledger.slot(prepared.kind).cloned() else {
            return Ok(Flow::Allocate);
        };
        match &slot.status {
            SlotStatus::Rejected { .. } | SlotStatus::Vacant | SlotStatus::Deleted => {
                Ok(Flow::Allocate)
            }
            SlotStatus::Consumed { by } => Ok(Flow::respond(
                identity.conflict_about(ConflictReason::ProformaConsumed, by),
            )),
            SlotStatus::Pending => {
                self.resume_pending(ctx, ledger, prepared, &slot, false)
                    .await
            }
            SlotStatus::Committed | SlotStatus::ReversalUnverified => {
                self.verify_committed(ctx, ledger, prepared, &slot, false)
                    .await
            }
            SlotStatus::Reversed { by, origin } => match origin {
                ReversalOrigin::Service => Ok(Flow::Allocate),
                ReversalOrigin::External | ReversalOrigin::Operator if prepared.reissue => {
                    Ok(Flow::Allocate)
                }
                ReversalOrigin::External | ReversalOrigin::Operator => Ok(Flow::respond(
                    self.identity(prepared, slot.generation.saturating_sub(1))
                        .reversed(slot.number.as_deref(), by.as_deref()),
                )),
            },
            SlotStatus::Blocked { .. } => {
                self.reconcile_blocked(ctx, ledger, prepared, &slot, false)
                    .await
            }
        }
    }

    /// Step 2, `pending`: pre-sleep, then reconcile by external id.
    async fn resume_pending(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        slot: &Slot,
        same_id: bool,
    ) -> Result<Flow, HandlerError> {
        let identity = self.identity(prepared, slot.generation);
        let target = Target::Slot(prepared.kind);
        self.presleep(ctx, slot.last_attempt_at).await?;
        let outcome = query_external_id(
            ctx,
            &self.steps,
            format!("reconcile-{}-{}", prepared.kind, slot.generation),
            &identity.external_id,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                if !same_id {
                    ledger
                        .take_over(
                            prepared.kind,
                            prepared.request_id.clone(),
                            prepared.fp.clone(),
                            prepared.document.issue_date,
                        )
                        .map_err(Fault::from)?;
                    save(ctx, ledger);
                }
                Ok(Flow::Resume)
            }
            QueryOutcome::Found(found) => {
                if validate_found(
                    &found,
                    &prepared.order,
                    prepared.issued_kind(),
                    &self.config,
                    expected_supplier(&self.config, ledger),
                )
                .is_err()
                {
                    return Ok(Flow::respond(identity.conflict_about(
                        ConflictReason::ExternalIdCollision,
                        found.number,
                    )));
                }
                let fp_equal = same_id || slot.fp == prepared.fp;
                let mut flow = adopt_found(
                    ctx,
                    ledger,
                    &target,
                    &identity,
                    &found,
                    fp_equal,
                    prepared.reissue && !same_id,
                )?;
                if let Flow::Respond(response) = &mut flow
                    && response.conflict_reason == Some(ConflictReason::PayloadMismatch)
                    && prepared.kind == DocumentKind::Final
                {
                    response.conflict_reason = Some(ConflictReason::FinalExists);
                }
                Ok(flow)
            }
        }
    }

    /// Step 2, `committed` (and `reversal_unverified`): verify live.
    async fn verify_committed(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        slot: &Slot,
        same_id: bool,
    ) -> Result<Flow, HandlerError> {
        let identity = self.identity(prepared, slot.generation);
        let target = Target::Slot(prepared.kind);
        let Some(number) = slot.number.clone() else {
            return Err(Fault::invalid_input(format!(
                "ledger inconsistent: committed {} slot without a number",
                prepared.kind
            ))
            .into());
        };
        let outcome = verify(
            ctx,
            &self.steps,
            format!("verify-{}-{}", prepared.kind, slot.generation),
            &number,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::Found(found) => {
                learn_supplier(ledger, &found)?;
                if found.reversed == Some(true) {
                    let origin = if slot.status == SlotStatus::ReversalUnverified {
                        ReversalOrigin::Service
                    } else {
                        ReversalOrigin::External
                    };
                    ledger
                        .mark_reversed(
                            &target,
                            Reversal::new(origin).with_payments_before(found.payments.clone()),
                        )
                        .map_err(Fault::from)?;
                    save(ctx, ledger);
                    let open = !same_id && (prepared.reissue || origin == ReversalOrigin::Service);
                    if open {
                        return Ok(Flow::Allocate);
                    }
                    return Ok(Flow::respond(identity.reversed(Some(&number), None)));
                }
                save(ctx, ledger);
                if prepared.reissue {
                    return Ok(Flow::respond(
                        identity.conflict_about(ConflictReason::Live, number),
                    ));
                }
                if same_id || slot.fp == prepared.fp {
                    return Ok(Flow::respond(
                        identity.found(Outcome::AlreadyIssued, &found),
                    ));
                }
                let reason = if prepared.kind == DocumentKind::Final {
                    ConflictReason::FinalExists
                } else {
                    ConflictReason::PayloadMismatch
                };
                Ok(Flow::respond(identity.conflict_about(reason, number)))
            }
            QueryOutcome::NotFound => {
                if prepared.kind != DocumentKind::Proforma {
                    return Ok(Flow::respond(
                        identity.conflict_about(ConflictReason::RecordedDocumentMissing, number),
                    ));
                }
                match proforma_fate(ctx, &self.steps, &prepared.order, &number).await? {
                    ProformaFate::Consumed(consumer) => {
                        learn_supplier(ledger, &consumer)?;
                        ledger
                            .mark_consumed(consumer.number.clone())
                            .map_err(Fault::from)?;
                        save(ctx, ledger);
                        Ok(Flow::respond(identity.conflict_about(
                            ConflictReason::ProformaConsumed,
                            consumer.number.clone(),
                        )))
                    }
                    ProformaFate::Deleted => {
                        ledger.mark_deleted().map_err(Fault::from)?;
                        save(ctx, ledger);
                        if same_id {
                            Ok(Flow::respond(identity.reversed(Some(&number), None)))
                        } else {
                            Ok(Flow::Allocate)
                        }
                    }
                }
            }
        }
    }

    /// Step 2, `blocked`: reconcile by external id, never allocate anew.
    async fn reconcile_blocked(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        slot: &Slot,
        same_id: bool,
    ) -> Result<Flow, HandlerError> {
        let identity = self.identity(prepared, slot.generation);
        let target = Target::Slot(prepared.kind);
        let outcome = query_external_id(
            ctx,
            &self.steps,
            format!("reconcile-blocked-{}-{}", prepared.kind, slot.generation),
            &identity.external_id,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                let mut response = identity.conflict(ConflictReason::DuplicateOrderNumber);
                if let SlotStatus::Blocked { existing_number } = &slot.status {
                    response.existing_number.clone_from(existing_number);
                }
                Ok(Flow::respond(response))
            }
            QueryOutcome::Found(found) => {
                if validate_found(
                    &found,
                    &prepared.order,
                    prepared.issued_kind(),
                    &self.config,
                    expected_supplier(&self.config, ledger),
                )
                .is_err()
                {
                    return Ok(Flow::respond(identity.conflict_about(
                        ConflictReason::ExternalIdCollision,
                        found.number,
                    )));
                }
                adopt_found(
                    ctx,
                    ledger,
                    &target,
                    &identity,
                    &found,
                    same_id || slot.fp == prepared.fp,
                    prepared.reissue && !same_id,
                )
            }
        }
    }

    /// The pre-sleep before touching a `pending` slot: the remainder of the
    /// first backoff since the last attempt.
    async fn presleep(
        &self,
        ctx: &ObjectContext<'_>,
        last_attempt_at: Option<jiff::Timestamp>,
    ) -> Result<(), HandlerError> {
        if let Some(last) = last_attempt_at {
            let now = now(ctx).await?;
            sleep(
                ctx,
                remaining_backoff(self.config.issue.first_backoff, last, now),
            )
            .await?;
        }
        Ok(())
    }

    // ----- step 3: allocate ------------------------------------------------

    async fn allocate_and_issue(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        prepared: &Prepared,
    ) -> Result<CreateResponse, HandlerError> {
        let refs = match self.resolve_refs(ctx, &mut ledger, prepared).await? {
            Ok(refs) => refs,
            Err(response) => return Ok(response),
        };
        let generation = ledger
            .allocate_intent(
                prepared.kind,
                prepared.request_id.clone(),
                prepared.fp.clone(),
                prepared.document.issue_date,
            )
            .map_err(Fault::from)?;
        let intent = self.slot_intent(prepared, generation, &refs)?;
        // The intent is durable before any issuing call.
        save(ctx, &ledger);
        self.attempt_loop(ctx, ledger, &prepared.order, intent)
            .await
    }

    /// The slot is `pending` under the request id already (a resumed or
    /// taken-over intent): resolve the references and issue.
    async fn resume_and_issue(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        prepared: &Prepared,
    ) -> Result<CreateResponse, HandlerError> {
        let refs = match self.resolve_refs(ctx, &mut ledger, prepared).await? {
            Ok(refs) => refs,
            Err(response) => return Ok(response),
        };
        let generation = candidate_generation(&ledger, prepared.kind);
        let intent = self.slot_intent(prepared, generation, &refs)?;
        self.attempt_loop(ctx, ledger, &prepared.order, intent)
            .await
    }

    fn slot_intent(
        &self,
        prepared: &Prepared,
        generation: u32,
        refs: &Refs,
    ) -> Result<Intent, Fault> {
        let identity = self.identity(prepared, generation);
        let create = self.build(
            prepared.issued_kind(),
            &prepared.document,
            &prepared.order,
            &identity.external_id,
            DocumentRefs {
                proforma: refs.proforma.as_deref(),
                prepayment: refs.prepayment.as_deref(),
                corrected: None,
            },
        )?;
        Ok(Intent {
            target: Target::Slot(prepared.kind),
            identity,
            create,
            check_hint: refs.check_hint,
            proforma_ref: refs.proforma.clone(),
            our_proforma: refs.our_proforma.clone(),
        })
    }

    /// Resolves `options.proforma` and the final invoice's prepayment
    /// reference (step 3), pre-querying szamlazz.hu where the design says so.
    async fn resolve_refs(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
    ) -> Result<Result<Refs, CreateResponse>, HandlerError> {
        let identity = self.identity(prepared, candidate_generation(ledger, prepared.kind));
        let mut refs = Refs {
            check_hint: self.config.issue.detect_foreign,
            ..Refs::default()
        };
        let conflict = match prepared.kind {
            DocumentKind::Proforma => None,
            DocumentKind::Invoice | DocumentKind::Prepayment => {
                self.resolve_proforma_link(ctx, ledger, prepared, &identity, &mut refs)
                    .await?
            }
            DocumentKind::Final => {
                self.resolve_prepayment(ctx, ledger, prepared, &identity, &mut refs)
                    .await?
            }
        };
        Ok(match conflict {
            Some(response) => Err(response),
            None => Ok(refs),
        })
    }

    /// `options.proforma` for an invoice or prepayment.
    async fn resolve_proforma_link(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        match &prepared.proforma {
            ProformaLink::Ledger => {
                let proforma = ledger.slot(DocumentKind::Proforma).cloned();
                match proforma.map(|slot| (slot.status, slot.number)) {
                    Some((SlotStatus::Committed, Some(number))) => {
                        self.resolve_ledger_proforma(ctx, ledger, prepared, identity, refs, number)
                            .await
                    }
                    Some((SlotStatus::Consumed { by }, _)) => Ok(Some(
                        identity.conflict_about(ConflictReason::ProformaConsumed, by),
                    )),
                    Some((SlotStatus::Pending, _)) => {
                        Ok(Some(identity.conflict(ConflictReason::Pending)))
                    }
                    // Nothing recorded to reference: the `none` rule applies, so
                    // a live proforma under the order number is still refused.
                    _ => {
                        self.resolve_no_proforma(ctx, ledger, prepared, identity)
                            .await
                    }
                }
            }
            ProformaLink::None => {
                self.resolve_no_proforma(ctx, ledger, prepared, identity)
                    .await
            }
            ProformaLink::Number(number) => {
                let outcome = verify(
                    ctx,
                    &self.steps,
                    format!("verify-proforma-{number}"),
                    number,
                )
                .await?;
                match outcome {
                    QueryOutcome::Found(found) if found.document_type == "D" && found.is_live() => {
                        refs.proforma = Some(number.clone());
                        Ok(None)
                    }
                    QueryOutcome::Found(_) | QueryOutcome::NotFound => Ok(Some(
                        identity.conflict_about(ConflictReason::ProformaMissing, number.clone()),
                    )),
                    QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
                }
            }
        }
    }

    /// No proforma reference: refused while a live proforma exists under the
    /// order number, because szamlazz.hu links by shared order number
    /// regardless.
    async fn resolve_no_proforma(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &Ledger,
        prepared: &Prepared,
        identity: &Identity,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        if let Some(slot) = ledger.slot(DocumentKind::Proforma)
            && matches!(slot.status, SlotStatus::Pending | SlotStatus::Committed)
        {
            let mut response = identity.conflict(ConflictReason::ProformaLive);
            response.existing_number.clone_from(&slot.number);
            return Ok(Some(response));
        }
        match hint(ctx, &self.steps, "hint-proforma-none", &prepared.order).await? {
            QueryOutcome::Found(found) if found.document_type == "D" && found.is_live() => Ok(
                Some(identity.conflict_about(ConflictReason::ProformaLive, found.number)),
            ),
            QueryOutcome::Found(_) | QueryOutcome::NotFound => Ok(None),
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
        }
    }

    /// `options.proforma = ledger` with a committed proforma: pre-query it by
    /// number; a code 7 is disambiguated into `consumed` or `deleted`.
    async fn resolve_ledger_proforma(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
        number: String,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let outcome = verify(
            ctx,
            &self.steps,
            format!("verify-proforma-{number}"),
            &number,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::Found(found) if found.document_type == "D" && found.is_live() => {
                learn_supplier(ledger, &found)?;
                refs.proforma = Some(number.clone());
                refs.our_proforma = Some(number);
                refs.check_hint = true;
                Ok(None)
            }
            QueryOutcome::Found(_) => Ok(Some(
                identity.conflict_about(ConflictReason::ProformaMissing, number),
            )),
            QueryOutcome::NotFound => {
                match proforma_fate(ctx, &self.steps, &prepared.order, &number).await? {
                    ProformaFate::Consumed(consumer) => {
                        learn_supplier(ledger, &consumer)?;
                        ledger
                            .mark_consumed(consumer.number.clone())
                            .map_err(Fault::from)?;
                        save(ctx, ledger);
                        Ok(Some(identity.conflict_about(
                            ConflictReason::ProformaConsumed,
                            consumer.number.clone(),
                        )))
                    }
                    ProformaFate::Deleted => {
                        ledger.mark_deleted().map_err(Fault::from)?;
                        save(ctx, ledger);
                        Ok(Some(
                            identity.conflict_about(ConflictReason::ProformaMissing, number),
                        ))
                    }
                }
            }
        }
    }

    /// The prepayment a final invoice settles: the committed prepayment,
    /// verified live first.
    async fn resolve_prepayment(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let Some(number) = ledger
            .slot(DocumentKind::Prepayment)
            .and_then(|slot| slot.number.clone())
        else {
            return Err(Fault::invalid_input(
                "create_final requires a committed prepayment invoice",
            )
            .into());
        };
        let outcome = verify(
            ctx,
            &self.steps,
            format!("verify-prepayment-{number}"),
            &number,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => Ok(Some(
                identity.conflict_about(ConflictReason::RecordedDocumentMissing, number),
            )),
            QueryOutcome::Found(found) => {
                learn_supplier(ledger, &found)?;
                if found.reversed == Some(true) {
                    ledger
                        .mark_reversed(
                            &Target::Slot(DocumentKind::Prepayment),
                            Reversal::new(ReversalOrigin::External)
                                .with_payments_before(found.payments.clone()),
                        )
                        .map_err(Fault::from)?;
                    save(ctx, ledger);
                    return Ok(Some(
                        identity.conflict_about(ConflictReason::PrepaymentReversed, number),
                    ));
                }
                let _ = prepared;
                refs.prepayment = Some(number);
                Ok(None)
            }
        }
    }

    // ----- step 4: the attempt loop ----------------------------------------

    async fn attempt_loop(
        &self,
        ctx: &ObjectContext<'_>,
        mut ledger: Ledger,
        order: &OrderKey,
        intent: Intent,
    ) -> Result<CreateResponse, HandlerError> {
        let max_attempts = self.config.issue.max_attempts.max(1);
        let mut backoff = self.config.issue.first_backoff;
        let mut budget = 0u32;

        while budget < max_attempts {
            budget += 1;
            let now = now(ctx).await?;
            ledger
                .record_attempt(&intent.target, now)
                .map_err(Fault::from)?;
            save(ctx, &ledger);

            let outcome = self
                .issue_once(ctx, &ledger, order, &intent, budget == 1)
                .await?;
            match outcome {
                IssueOutcome::Transport(_) | IssueOutcome::Unknown { .. } => {
                    if budget < max_attempts {
                        sleep(ctx, backoff).await?;
                        backoff = next_backoff(backoff, self.config.issue.max_backoff);
                    }
                }
                IssueOutcome::DuplicateOrderNumber { .. }
                    if budget <= 2 && budget < max_attempts =>
                {
                    // The re-executed closure re-queries the external id.
                    sleep(ctx, backoff).await?;
                    backoff = next_backoff(backoff, self.config.issue.max_backoff);
                }
                settled => {
                    return self.settle(ctx, &mut ledger, &intent, settled);
                }
            }
        }

        // Step 5: exhausted. The entry stays `pending`.
        Err(Fault::outcome_unknown(format!(
            "{max_attempts} issuing attempts exhausted without a confirmed outcome; call again with the same request id"
        ))
        .about(
            order,
            intent.identity.kind,
            intent.identity.generation,
            intent.identity.external_id.as_str(),
            Some(&intent.identity.request_id),
        )
        .into())
    }

    /// One journaled issuing attempt.
    async fn issue_once(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &Ledger,
        order: &OrderKey,
        intent: &Intent,
        first: bool,
    ) -> Result<IssueOutcome, HandlerError> {
        let steps = Arc::clone(&self.steps);
        let external_id = intent.identity.external_id.clone();
        let kind = intent.identity.kind;
        let order = order.clone();
        let create = intent.create.clone();
        let check_hint = first && intent.check_hint;
        let our_numbers = ledger.our_numbers();
        let our_proforma = intent.our_proforma.clone();
        let expect_supplier_id = expected_supplier(&self.config, ledger);
        let expect_test = self.config.account.mode.is_test();
        let attempts = entry_attempts(ledger, &intent.target);
        run_once(
            ctx,
            format!(
                "issue-{}-{}-{attempts}",
                intent.identity.kind, intent.identity.generation
            ),
            move || async move {
                steps
                    .issue(IssueRequest {
                        external_id: &external_id,
                        kind,
                        order: &order,
                        create: &create,
                        check_hint,
                        our_numbers: &our_numbers,
                        our_proforma: our_proforma.as_deref(),
                        expect_supplier_id,
                        expect_test,
                    })
                    .await
            },
        )
        .await
    }

    /// Branches on a settled attempt outcome (step 4, "branch on data").
    fn settle(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        intent: &Intent,
        outcome: IssueOutcome,
    ) -> Result<CreateResponse, HandlerError> {
        let identity = &intent.identity;
        let target = &intent.target;
        match outcome {
            IssueOutcome::Issued(issued) => {
                ledger
                    .commit(
                        target,
                        CommittedDocument::issued(issued.number.clone())
                            .with_totals(issued.gross, issued.net)
                            .with_test(self.config.account.mode.is_test()),
                    )
                    .map_err(Fault::from)?;
                // The create response carries no `hivdijbekszam`: the
                // reference we sent is taken as honoured.
                if let Some(proforma) = &intent.proforma_ref {
                    consume_proforma(ledger, proforma, &issued.number)?;
                }
                save(ctx, ledger);
                let mut response = identity
                    .respond(Outcome::Issued)
                    .with_invoice_number(issued.number);
                response.net_total = issued.net;
                response.gross_total = issued.gross;
                response.outstanding = issued.outstanding;
                response.customer_account_url = issued.customer_account_url;
                if issued.notification_delivery_failed {
                    response = response.with_warning(Warning::NotificationDeliveryFailed);
                }
                Ok(response)
            }
            IssueOutcome::Found(found) => {
                let flow = adopt_found(ctx, ledger, target, identity, &found, true, false)?;
                let mut response = flow_response(flow, "adoption inside the attempt loop")?;
                // The found document says whether the reference we sent took:
                // its `hivdijbekszam` names our proforma, another one, or —
                // silently dropped by the server — none.
                if response.outcome == Outcome::Reconciled
                    && let Some(proforma) = &intent.proforma_ref
                {
                    match &found.referenced_proforma {
                        Some(referenced) if referenced == proforma => {
                            consume_proforma(ledger, proforma, &found.number)?;
                            save(ctx, ledger);
                        }
                        Some(_) => {}
                        None => response = response.with_warning(Warning::ProformaLinkDropped),
                    }
                }
                Ok(response)
            }
            IssueOutcome::Collision(found) => {
                Ok(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number))
            }
            IssueOutcome::Foreign(found) => {
                if let Target::Slot(kind) = target {
                    ledger.clear_pending(*kind).map_err(Fault::from)?;
                }
                ledger.set_foreign_hint(found.number.clone(), found.document_type.clone());
                save(ctx, ledger);
                Ok(identity.conflict_about(ConflictReason::Foreign, found.number))
            }
            IssueOutcome::Rejected { code, message } => {
                ledger
                    .mark_rejected(target, code.clone(), message.clone())
                    .map_err(Fault::from)?;
                save(ctx, ledger);
                Ok(identity.rejected(code, message))
            }
            IssueOutcome::DuplicateOrderNumber { code, message } => match target {
                Target::Slot(kind) => {
                    let existing = ledger.foreign_hint().map(|hint| hint.number.clone());
                    ledger
                        .mark_blocked(*kind, existing.clone())
                        .map_err(Fault::from)?;
                    save(ctx, ledger);
                    let mut response = identity.conflict(ConflictReason::DuplicateOrderNumber);
                    response.existing_number = existing;
                    response.code = Some(code);
                    response.message = Some(message);
                    Ok(response)
                }
                // Correctives are exempt from the order-number check; a 71 on
                // one is an ordinary rejection.
                Target::Corrective(_) => {
                    ledger
                        .mark_rejected(target, code.clone(), message.clone())
                        .map_err(Fault::from)?;
                    save(ctx, ledger);
                    Ok(identity.rejected(code, message))
                }
            },
            IssueOutcome::Transport(message) | IssueOutcome::Unknown { message, .. } => {
                Err(Fault::invalid_input(format!(
                    "ledger inconsistent: unsettled outcome reached settlement: {message}"
                ))
                .into())
            }
        }
    }
}

/// A validated document of ours was found for a `pending` or `blocked`
/// entry: commit it (`reconciled`), or record its reversal (`reversed`) —
/// never re-allocate here unless the caller asked to reissue. A live document
/// with `reissue` is `conflict{live}`, as on the committed path.
fn adopt_found(
    ctx: &ObjectContext<'_>,
    ledger: &mut Ledger,
    target: &Target,
    identity: &Identity,
    found: &FoundDocument,
    fp_equal: bool,
    reissue: bool,
) -> Result<Flow, HandlerError> {
    learn_supplier(ledger, found)?;
    let known_reversed = ledger
        .history_reversed_numbers()
        .iter()
        .any(|number| number == &found.number);
    if found.reversed == Some(true) || known_reversed {
        ledger
            .mark_reversed(
                target,
                Reversal::new(ReversalOrigin::External)
                    .with_number(found.number.clone())
                    .with_payments_before(found.payments.clone()),
            )
            .map_err(Fault::from)?;
        save(ctx, ledger);
        if reissue {
            return Ok(Flow::Allocate);
        }
        return Ok(Flow::respond(identity.reversed(Some(&found.number), None)));
    }
    let origin = if found.adopted {
        Origin::Adopted
    } else {
        Origin::Service
    };
    ledger
        .commit(
            target,
            CommittedDocument::reconciled(found.number.clone(), origin)
                .with_totals(found.gross, found.net)
                .with_test(found.test),
        )
        .map_err(Fault::from)?;
    save(ctx, ledger);
    if reissue {
        return Ok(Flow::respond(
            identity.conflict_about(ConflictReason::Live, found.number.clone()),
        ));
    }
    if fp_equal {
        Ok(Flow::respond(identity.found(Outcome::Reconciled, found)))
    } else {
        Ok(Flow::respond(identity.conflict_about(
            ConflictReason::PayloadMismatch,
            found.number.clone(),
        )))
    }
}

/// The proforma reference `sent` in a create was honoured by the document
/// `by`: when it names the ledger's committed proforma, that slot becomes
/// `consumed{by}`. A reference to any other proforma leaves the ledger alone.
fn consume_proforma(ledger: &mut Ledger, sent: &str, by: &str) -> Result<(), Fault> {
    let ours = ledger.slot(DocumentKind::Proforma).is_some_and(|slot| {
        slot.status == SlotStatus::Committed && slot.number.as_deref() == Some(sent)
    });
    if ours {
        ledger.mark_consumed(by).map_err(Fault::from)?;
    }
    Ok(())
}

/// Step 1: the invoice / prepayment chains are exclusive, and a final invoice
/// needs a committed prepayment.
fn exclusivity(
    ledger: &Ledger,
    prepared: &Prepared,
    identity: &Identity,
) -> Result<Option<CreateResponse>, Fault> {
    match prepared.kind {
        DocumentKind::Invoice | DocumentKind::Prepayment => {
            let other = if prepared.kind == DocumentKind::Invoice {
                DocumentKind::Prepayment
            } else {
                DocumentKind::Invoice
            };
            if let Some(slot) = ledger.slot(other)
                && matches!(
                    slot.status,
                    SlotStatus::Pending
                        | SlotStatus::Committed
                        | SlotStatus::Blocked { .. }
                        | SlotStatus::ReversalUnverified
                )
            {
                let mut response = identity.conflict(ConflictReason::PrepaidChain);
                response.existing_number.clone_from(&slot.number);
                return Ok(Some(response));
            }
            Ok(None)
        }
        DocumentKind::Final => {
            let missing =
                || Fault::invalid_input("create_final requires a committed prepayment invoice");
            let slot = ledger.slot(DocumentKind::Prepayment).ok_or_else(missing)?;
            let reason = match &slot.status {
                SlotStatus::Committed | SlotStatus::ReversalUnverified => return Ok(None),
                SlotStatus::Pending | SlotStatus::Blocked { .. } => ConflictReason::Pending,
                SlotStatus::Reversed { .. } => ConflictReason::PrepaymentReversed,
                SlotStatus::Rejected { .. }
                | SlotStatus::Consumed { .. }
                | SlotStatus::Deleted
                | SlotStatus::Vacant => return Err(missing()),
            };
            let mut response = identity.conflict(reason);
            response.existing_number.clone_from(&slot.number);
            Ok(Some(response))
        }
        DocumentKind::Proforma => Ok(None),
    }
}

fn corrective_intent(target: Target, identity: Identity, create: CreateInvoice) -> Intent {
    Intent {
        target,
        identity,
        create,
        check_hint: false,
        proforma_ref: None,
        our_proforma: None,
    }
}

/// A `Flow` where only a response is possible.
fn flow_response(flow: Flow, context: &str) -> Result<CreateResponse, HandlerError> {
    match flow {
        Flow::Respond(response) => Ok(*response),
        Flow::Allocate | Flow::Resume => Err(Fault::invalid_input(format!(
            "ledger inconsistent: {context} cannot continue"
        ))
        .into()),
    }
}

/// The current attempt counter of an entry.
fn entry_attempts(ledger: &Ledger, target: &Target) -> u32 {
    match target {
        Target::Slot(kind) => ledger.slot(*kind).map_or(0, |slot| slot.attempts),
        Target::Corrective(id) => ledger.corrective(id).map_or(0, |entry| entry.attempts),
    }
}

/// The state of a generation the request id owned that the slot has since
/// moved past: read from the history.
fn closed_generation(ledger: &Ledger, identity: &Identity, generation: u32) -> CreateResponse {
    let event = ledger
        .history()
        .iter()
        .rev()
        .find(|event| event.kind == identity.kind && event.generation == generation);
    match event.map(|event| event.event) {
        Some(HistoryKind::Consumed) => {
            let mut response = identity.conflict(ConflictReason::ProformaConsumed);
            response.existing_number = event.and_then(|event| event.by.clone());
            response
        }
        Some(HistoryKind::Forgotten) => {
            let mut response = identity.conflict(ConflictReason::RecordedDocumentMissing);
            response.existing_number = event.and_then(|event| event.number.clone());
            response
        }
        Some(HistoryKind::Abandoned) => identity.conflict(ConflictReason::RequestIdReused),
        Some(
            HistoryKind::Reversed
            | HistoryKind::Deleted
            | HistoryKind::Issued
            | HistoryKind::Reconciled
            | HistoryKind::Adopted
            | HistoryKind::RecordedByOperator,
        )
        | None => identity.reversed(
            event.and_then(|event| event.number.as_deref()),
            event.and_then(|event| event.by.as_deref()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use rust_decimal::dec;

    use super::*;
    use crate::ledger::CommittedDocument;

    fn rid(id: &str) -> RequestId {
        id.parse().expect("valid request id")
    }

    fn fp() -> Fingerprint {
        Fingerprint::compute(b"secret", "Buyer", dec!(1270), None, None, None)
    }

    fn identity(kind: IssuedKind, generation: u32) -> Identity {
        Identity {
            request_id: rid("r-1"),
            kind,
            generation,
            external_id: ExternalId::new(format!("acct:ORD-1:{kind}:{generation}")),
        }
    }

    /// A ledger with the proforma `D-1` committed and the invoice slot
    /// pending.
    fn proforma_then_pending_invoice() -> Ledger {
        let mut ledger = Ledger::new();
        ledger
            .allocate_intent(DocumentKind::Proforma, rid("p-1"), fp(), None)
            .expect("allocate proforma");
        ledger
            .commit(
                &Target::Slot(DocumentKind::Proforma),
                CommittedDocument::issued("D-1"),
            )
            .expect("commit proforma");
        ledger
            .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp(), None)
            .expect("allocate invoice");
        ledger
    }

    #[test]
    fn issuing_with_our_proforma_reference_consumes_it() {
        let mut ledger = proforma_then_pending_invoice();
        consume_proforma(&mut ledger, "D-1", "SZ-1").expect("consume");
        let proforma = ledger.slot(DocumentKind::Proforma).expect("proforma");
        assert_eq!(
            proforma.status,
            SlotStatus::Consumed {
                by: "SZ-1".to_owned()
            }
        );
        assert_eq!(proforma.generation, 1);
        assert_eq!(
            ledger.history().last().expect("event").event,
            HistoryKind::Consumed
        );
    }

    #[test]
    fn a_reference_to_another_proforma_leaves_the_ledger_alone() {
        let mut ledger = proforma_then_pending_invoice();
        let before = ledger.clone();
        consume_proforma(&mut ledger, "D-999", "SZ-1").expect("no-op");
        assert_eq!(ledger, before);

        let mut no_proforma = Ledger::new();
        no_proforma
            .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp(), None)
            .expect("allocate invoice");
        let before = no_proforma.clone();
        consume_proforma(&mut no_proforma, "D-1", "SZ-1").expect("no-op");
        assert_eq!(no_proforma, before);
    }

    #[test]
    fn a_forgotten_generation_answers_recorded_document_missing() {
        let mut ledger = Ledger::new();
        ledger
            .allocate_intent(DocumentKind::Invoice, rid("r-1"), fp(), None)
            .expect("allocate");
        ledger
            .commit(
                &Target::Slot(DocumentKind::Invoice),
                CommittedDocument::issued("SZ-1"),
            )
            .expect("commit");
        assert_eq!(ledger.forget(DocumentKind::Invoice), Ok(1));

        // The retry's request ref still names generation 0; the slot is at 1.
        let Some(RequestRef::Slot { generation, .. }) = ledger.lookup_request(&rid("r-1")) else {
            panic!("the forgotten id keeps its slot ref");
        };
        let slot = ledger.slot(DocumentKind::Invoice).expect("slot");
        assert_eq!(slot.status, SlotStatus::Vacant);
        assert_ne!(slot.generation, *generation);

        let response = closed_generation(
            &ledger,
            &identity(IssuedKind::Invoice, *generation),
            *generation,
        );
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::RecordedDocumentMissing)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));
        assert_eq!(response.generation, 0);
        assert_eq!(response.request_id, rid("r-1"));
    }
}
