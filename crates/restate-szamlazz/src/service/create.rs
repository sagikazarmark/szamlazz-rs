//! The create protocol for the four document kinds and for correctives, in
//! the order the steps run.
//!
//! The handlers keep no state. After validation and the reference checks
//! (read-only steps under the read policy), issuing is two durable steps: a
//! read-only **lookup** (`lookup-{kind}`) under the read policy that settles
//! every case needing no create, and a **create** (`create-{kind}`) under the
//! issue policy's run retry policy, query-first on every execution; the
//! external-id query inside the create closure is what finds a document an
//! earlier execution issued. Domain outcomes are data and faults are
//! `TerminalError`s; a read that szamlazz.hu never answered is `unavailable`,
//! a create step that ends without a settled outcome is `outcome_unknown`.

use std::ops::ControlFlow;
use std::sync::Arc;

use restate_sdk::errors::{HandlerError, TerminalError};
use restate_sdk::prelude::ObjectContext;
use szamlazz_agent::ops::invoice::CreateInvoice;

use super::prologue::Execution;
use super::support::{Fault, verified_document};
use super::support::{lookup, run_reading, run_retrying, verify};
use crate::contract::{
    ConflictReason, CorrectRequest, CreateRequest, CreateResponse, DocumentInput, DocumentKind,
    IssuedKind, Outcome, ProformaLink, Warning, outstanding,
};
use crate::gateway::{
    CreateOutcome, CreateStepRequest, DocumentRefs, FoundDocument, LookupOutcome, LookupRequest,
    OwnershipOutcome, QueryOutcome,
};
use crate::identity::Namespace;
use crate::identity::{ExternalId, OrderKey, normalize_buyer_name};

/// The identity fields every [`CreateResponse`] carries.
#[derive(Debug, Clone)]
struct Identity {
    kind: IssuedKind,
    external_id: ExternalId,
}

impl Identity {
    fn of_kind(namespace: &Namespace, order: &OrderKey, kind: DocumentKind) -> Self {
        Self {
            kind: kind.into(),
            external_id: ExternalId::for_kind(namespace, order, kind),
        }
    }

    fn respond(&self, outcome: Outcome) -> CreateResponse {
        CreateResponse::new(outcome, self.kind, self.external_id.as_str())
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
        let gross = Some(found.gross_total);
        let mut response = self.respond(outcome).with_invoice_number(&found.number);
        response.net_total = Some(found.net_total);
        response.gross_total = gross;
        response.outstanding = outstanding(gross, &found.payment_amounts());

        response
    }

    /// `outcome: reversed` for `number`, reversed by `storno_number`.
    fn reversed(&self, number: &str, storno_number: Option<String>) -> CreateResponse {
        let mut response = self.respond(Outcome::Reversed).with_invoice_number(number);
        response.storno_number = storno_number;
        response
    }

    /// `fault`, about this document of `order`.
    fn about(&self, order: &OrderKey, fault: Fault) -> Fault {
        fault.about(order, Some(self.kind), &self.external_id)
    }

    /// Step 5 of the create protocol: the settled create step as the caller's
    /// response. Pure: every szamlazz.hu answer is data.
    ///
    /// # Errors
    ///
    /// The faults a settled step can still be: rejected credentials, and the
    /// leading query answered with another code or `szlahu_down`
    /// (`unavailable`, as the lookup step answers the same; nothing was
    /// sent). The caller attaches the document's identity.
    fn respond_to(
        &self,
        outcome: CreateOutcome,
        namespace: &Namespace,
    ) -> Result<CreateResponse, Fault> {
        Ok(match outcome {
            CreateOutcome::Issued(issued) => {
                let mut response = self
                    .respond(Outcome::Issued)
                    .with_invoice_number(issued.number);
                response.net_total = issued.net_total;
                response.gross_total = issued.gross_total;
                response.outstanding = issued.outstanding;
                response.customer_account_url = issued.customer_account_url;
                if issued.notification_delivery_failed {
                    response = response.with_warning(Warning::NotificationDeliveryFailed);
                }
                response
            }
            // An earlier execution of the step created it: the caller asked
            // for this document and has it.
            CreateOutcome::Found(found) => self.found(Outcome::Issued, &found),
            // Issued and reversed since the lookup, by an earlier execution
            // of the step and anyone's storno. As if the lookup had seen it:
            // `reversed`, and a new document needs an explicit `reissue`.
            // The storno number is not looked up here; the next call's lookup
            // reports it.
            CreateOutcome::Reversed(found) => self.reversed(&found.number, None),
            // The document the lookup saw reversed is reported live: what
            // the lookup would have answered under `reissue`.
            CreateOutcome::LiveAgain(found) => {
                self.conflict_about(ConflictReason::Live, found.number)
            }
            CreateOutcome::Reconciled(found) => self.found(Outcome::Reconciled, &found),
            CreateOutcome::Collision(found) => {
                self.conflict_about(ConflictReason::ExternalIdCollision, found.number)
            }
            CreateOutcome::DuplicateOrderNumber {
                answer,
                existing_number,
            } => {
                let mut response = self
                    .conflict(ConflictReason::DuplicateOrderNumber)
                    .with_code(answer.code)
                    .with_message(answer.message);
                response.existing_number = existing_number;
                response
            }
            CreateOutcome::Rejected(rejection) => self.rejected(rejection.code, rejection.message),
            CreateOutcome::CredentialsRejected(answer) => {
                return Err(Fault::credentials_rejected(namespace, answer));
            }
            // The leading query answered with a code or `szlahu_down`: the
            // fault the lookup step raises for the same answer, at once:
            // nothing was sent (#63).
            CreateOutcome::Api(answer) => {
                return Err(Fault::inconclusive_answer(answer));
            }
            CreateOutcome::Unavailable { message } => {
                return Err(Fault::szlahu_down_answer(message));
            }
        })
    }
}

/// The validated input of a create request (step 0).
struct Prepared {
    order: OrderKey,
    document: DocumentInput,
    reissue: bool,
    proforma: ProformaLink,
}

/// The references resolved in steps 1–2.
#[derive(Debug, Default)]
struct Refs {
    /// The proforma reference sent in the create (`dijbekeroSzamlaszam`).
    proforma: Option<String>,
    /// The prepayment a final invoice settles (`elolegSzamlaszam`).
    prepayment: Option<String>,
    /// Numbers known to be ours; the hint ignores them.
    our_numbers: Vec<String>,
}

/// One document to issue: what the lookup and create steps (steps 3–4) need.
struct Intent {
    identity: Identity,
    create: CreateInvoice,
    reissue: bool,
    /// Numbers known to be ours; the hint ignores them.
    our_numbers: Vec<String>,
}

/// Step 1's table: the other kinds whose live document of ours refuses a
/// create of `kind`, with the reason. The invoice and prepayment chains are
/// exclusive (`prepaid_chain`), and the final invoice is the prepayment
/// chain's settled end: a live `VS` refuses a plain invoice and a new
/// prepayment invoice the same way, even after its `ES` is reversed: without
/// that row `ES` → `VS` → storno of the `ES` let a plain `SZ` land beside a
/// live `VS`, the double billing the worker exists to prevent. A proforma
/// after any of the three makes no sense (`order_invoiced`), and
/// without these lookups the order-number hint of step 3 would report the
/// order's own invoice as `foreign`, which claims another channel issued it.
/// The final invoice's own check is [`Execution::prepayment_for_final`], not
/// exclusivity: a live `SZ` cannot coexist with the live `ES` it requires.
const fn exclusive_with(kind: DocumentKind) -> &'static [(DocumentKind, ConflictReason)] {
    match kind {
        DocumentKind::Invoice => &[
            (DocumentKind::Prepayment, ConflictReason::PrepaidChain),
            (DocumentKind::Final, ConflictReason::PrepaidChain),
        ],
        DocumentKind::Prepayment => &[
            (DocumentKind::Invoice, ConflictReason::PrepaidChain),
            (DocumentKind::Final, ConflictReason::PrepaidChain),
        ],
        DocumentKind::Proforma => &[
            (DocumentKind::Invoice, ConflictReason::OrderInvoiced),
            (DocumentKind::Prepayment, ConflictReason::OrderInvoiced),
            (DocumentKind::Final, ConflictReason::OrderInvoiced),
        ],
        DocumentKind::Final => &[],
    }
}

/// Step 2's gate: the kinds that convert a proforma and so take
/// `options.proforma` and run the proforma link. The invoice and the
/// prepayment invoice do: the Agent carries `dijbekeroSzamlaszam` on both,
/// and szamlazz.hu links the order's live proforma to either by shared
/// order number regardless, which is what makes `none` a
/// `conflict{proforma_live}` on both. A proforma has nothing to convert; the
/// final invoice settles a prepayment invoice, and the proforma the order had
/// was consumed by that prepayment invoice: a live proforma of ours cannot
/// exist beside it, because `create_proforma` is refused once the prepayment
/// invoice is live (`order_invoiced`).
const fn links_proforma(kind: DocumentKind) -> bool {
    matches!(kind, DocumentKind::Invoice | DocumentKind::Prepayment)
}

/// Step 1's decision on what the other kind's external id holds
/// ([`exclusive_with`]), pure: a live document of ours refuses the create as
/// `conflict{reason, existing_number}`; a document that fails validation is
/// `conflict{external_id_collision}`, never "absent", because the query
/// returns the newest holder, so another order's or kind's document may hide
/// a live document of ours behind it, and refusing to create is the only safe
/// answer; a reversed document of ours, or nothing, lets the create proceed
/// (`None`).
///
/// # Errors
///
/// The faults an answered read can be: another szamlazz.hu code
/// (`unavailable`, nothing may be concluded) or a credential code
/// (`credentials_rejected`). The caller attaches the document's identity.
fn decide_exclusivity(
    found: OwnershipOutcome,
    reason: ConflictReason,
    identity: &Identity,
    namespace: &Namespace,
) -> Result<Option<CreateResponse>, Fault> {
    Ok(match found {
        OwnershipOutcome::Collision(found) => {
            Some(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number))
        }
        OwnershipOutcome::Live(found) => Some(identity.conflict_about(reason, found.number)),
        OwnershipOutcome::Absent | OwnershipOutcome::Reversed(_) => None,
        OwnershipOutcome::Api(answer) => return Err(Fault::inconclusive_answer(answer)),
        OwnershipOutcome::CredentialsRejected(answer) => {
            return Err(Fault::credentials_rejected(namespace, answer));
        }
    })
}

/// Step 1's decision for a final invoice on what `…:prepayment` holds, pure:
/// the live prepayment invoice it settles is recorded in `refs` (the
/// `elolegSzamlaszam` reference, and a number of ours for the hint to
/// ignore) and the create proceeds (`None`); nothing under the id is
/// `conflict{prepayment_missing}`, a reversed one
/// `conflict{prepayment_reversed}`, a document that fails validation
/// `conflict{external_id_collision}` (see [`decide_exclusivity`] for why a
/// collision is never treated as absent). Nothing is recorded on a refusal.
///
/// # Errors
///
/// As [`decide_exclusivity`]'s.
fn decide_prepayment_for_final(
    found: OwnershipOutcome,
    identity: &Identity,
    namespace: &Namespace,
    refs: &mut Refs,
) -> Result<Option<CreateResponse>, Fault> {
    Ok(match found {
        OwnershipOutcome::Absent => Some(identity.conflict(ConflictReason::PrepaymentMissing)),
        OwnershipOutcome::Collision(found) => {
            Some(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number))
        }
        OwnershipOutcome::Reversed(found) => {
            Some(identity.conflict_about(ConflictReason::PrepaymentReversed, found.number))
        }
        OwnershipOutcome::Live(found) => {
            refs.our_numbers.push(found.number.clone());
            refs.prepayment = Some(found.number);
            None
        }
        OwnershipOutcome::Api(answer) => return Err(Fault::inconclusive_answer(answer)),
        OwnershipOutcome::CredentialsRejected(answer) => {
            return Err(Fault::credentials_rejected(namespace, answer));
        }
    })
}

/// Step 2's decision under `auto` and `none` on what `…:proforma` holds,
/// pure. A live proforma of ours is linked under `auto` (recorded in `refs`:
/// the `dijbekeroSzamlaszam` reference, and a number of ours for the hint to
/// ignore) and is `conflict{proforma_live}` under `none`, since the server
/// links by shared order number regardless, so refusing is the only honest
/// answer; a reversed one, or nothing, links nothing and the create proceeds;
/// a document that fails validation is `conflict{external_id_collision}` (see
/// [`decide_exclusivity`] for why a collision is never treated as absent).
/// Nothing is recorded on a refusal. A `{number}` link is
/// [`decide_proforma_by_number`]'s: the shell never passes it here, and a
/// live proforma of ours under it would be linked as under `auto`.
///
/// # Errors
///
/// As [`decide_exclusivity`]'s.
fn decide_proforma_link(
    found: OwnershipOutcome,
    link: &ProformaLink,
    identity: &Identity,
    namespace: &Namespace,
    refs: &mut Refs,
) -> Result<Option<CreateResponse>, Fault> {
    let live = match found {
        OwnershipOutcome::Collision(found) => {
            return Ok(Some(identity.conflict_about(
                ConflictReason::ExternalIdCollision,
                found.number,
            )));
        }
        OwnershipOutcome::Live(found) => found,
        OwnershipOutcome::Absent | OwnershipOutcome::Reversed(_) => return Ok(None),
        OwnershipOutcome::Api(answer) => return Err(Fault::inconclusive_answer(answer)),
        OwnershipOutcome::CredentialsRejected(answer) => {
            return Err(Fault::credentials_rejected(namespace, answer));
        }
    };
    Ok(match link {
        ProformaLink::None => {
            Some(identity.conflict_about(ConflictReason::ProformaLive, live.number))
        }
        ProformaLink::Auto | ProformaLink::Number(_) => {
            refs.our_numbers.push(live.number.clone());
            refs.proforma = Some(live.number);
            None
        }
    })
}

/// Step 2's decision under `{number}` on what the verify of `number` found,
/// pure. The named document is checked like every other document found by
/// number, in the order the other verifies use: this order's number first
/// (another order's, or none, is `conflict{not_managed, existing_number}`;
/// without it a caller could link another order's live proforma into this
/// order's invoice), then the kind. A proforma of ours is linked as named
/// (recorded in `refs`) and the create proceeds (`None`); code 7 is
/// `conflict{proforma_missing, existing_number}`, an outcome, never
/// `not_found`. Nothing is recorded on a refusal.
///
/// # Errors
///
/// A document of this order that is not a proforma is `invalid_input`
/// naming the number and its `tipus`: the caller's request, so it carries
/// no document identity. Another szamlazz.hu code (`unavailable`) and a
/// credential code (`credentials_rejected`) are about the document being
/// created and carry its identity.
fn decide_proforma_by_number(
    outcome: QueryOutcome,
    number: &str,
    order: &OrderKey,
    identity: &Identity,
    namespace: &Namespace,
    refs: &mut Refs,
) -> Result<Option<CreateResponse>, Fault> {
    match outcome {
        QueryOutcome::Api(answer) => Err(identity.about(order, Fault::inconclusive_answer(answer))),
        QueryOutcome::CredentialsRejected(answer) => {
            Err(identity.about(order, Fault::credentials_rejected(namespace, answer)))
        }
        QueryOutcome::NotFound => Ok(Some(
            identity.conflict_about(ConflictReason::ProformaMissing, number),
        )),
        QueryOutcome::Found(found) => {
            if !found.carries_order(order) {
                return Ok(Some(
                    identity.conflict_about(ConflictReason::NotManaged, number),
                ));
            }
            if found.document_type != "D" {
                return Err(Fault::invalid_input(format!(
                    "{number} is not a proforma (tipus {})",
                    found.document_type
                )));
            }
            refs.our_numbers.push(found.number);
            refs.proforma = Some(number.to_owned());
            Ok(None)
        }
    }
}

/// `correct_invoice`'s decision on the verified base, pure: the base must be
/// a live invoice carrying this order's number. Another order's, or none, is
/// `conflict{not_managed, existing_number}` (checked first: a reversed
/// document of another order is still not this order's); a reversed one of
/// ours is `conflict{base_reversed, existing_number}`; a live one of ours
/// proceeds (`None`).
fn decide_base(
    found: &FoundDocument,
    order: &OrderKey,
    number: &str,
    identity: &Identity,
) -> Option<CreateResponse> {
    if !found.carries_order(order) {
        return Some(identity.conflict_about(ConflictReason::NotManaged, number));
    }
    if found.reversed == Some(true) {
        return Some(identity.conflict_about(ConflictReason::BaseReversed, number));
    }
    None
}

/// The fault of a create step whose run ended without a settled outcome:
/// the issue policy exhausted (500, carrying the last `Unconfirmed`'s
/// display) or the invocation cancelled (409). `outcome_unknown` about the
/// document being created: nothing is recorded, and the next invocation's
/// lookup finds whatever landed.
fn create_outcome_unknown(error: &TerminalError, order: &OrderKey, identity: &Identity) -> Fault {
    identity.about(
        order,
        Fault::outcome_unknown(format!(
            "the create step ended without a confirmed outcome ({}): {}; retry with a new Idempotency-Key",
            error.code(),
            error.message()
        )),
    )
}

/// Step 3's decision on what the lookup step found, pure: every case that
/// needs no create is settled as the caller's response (`Break`), and what
/// proceeds to the create step carries the number of the reversed document
/// the lookup saw (`Continue(Some)`, a reissue: the one holder the create
/// step may send past) or nothing (`Continue(None)`).
///
/// A live document of ours is `already_issued`, or `conflict{live}` under
/// `reissue`, so the flag can never cause a duplicate; a reversed one is
/// `reversed{storno_number}` without `reissue`; a collision or a foreign
/// document refuses with or without it.
///
/// # Errors
///
/// The faults an answered lookup can be: another szamlazz.hu code
/// (`unavailable`, nothing may be concluded) or a credential code
/// (`credentials_rejected`). The caller attaches the document's identity.
fn decide_lookup(
    outcome: LookupOutcome,
    reissue: bool,
    identity: &Identity,
    namespace: &Namespace,
) -> Result<ControlFlow<CreateResponse, Option<String>>, Fault> {
    Ok(match outcome {
        LookupOutcome::Api(answer) => {
            return Err(Fault::inconclusive_answer(answer));
        }
        LookupOutcome::CredentialsRejected(answer) => {
            return Err(Fault::credentials_rejected(namespace, answer));
        }
        LookupOutcome::Live(found) if reissue => {
            ControlFlow::Break(identity.conflict_about(ConflictReason::Live, found.number))
        }
        LookupOutcome::Live(found) => {
            ControlFlow::Break(identity.found(Outcome::AlreadyIssued, &found))
        }
        LookupOutcome::Reversed {
            document,
            storno_number,
        } if !reissue => ControlFlow::Break(identity.reversed(&document.number, storno_number)),
        LookupOutcome::Reversed { document, .. } => ControlFlow::Continue(Some(document.number)),
        LookupOutcome::Collision(found) => ControlFlow::Break(
            identity.conflict_about(ConflictReason::ExternalIdCollision, found.number),
        ),
        LookupOutcome::Foreign(found) => {
            ControlFlow::Break(identity.conflict_about(ConflictReason::Foreign, found.number))
        }
        LookupOutcome::Absent => ControlFlow::Continue(None),
    })
}

impl Execution {
    // ----- entry points ----------------------------------------------------

    /// `create_proforma` / `create_invoice` / `create_prepayment` /
    /// `create_final`, on the `order` the handler parsed from its key.
    pub(super) async fn issue_kind(
        &self,
        ctx: &ObjectContext<'_>,
        order: OrderKey,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<CreateResponse, HandlerError> {
        // Step 0: validate (pure).
        let prepared = self.prepare(order, kind, request)?;
        let identity = Identity::of_kind(&self.config.namespace, &prepared.order, kind);
        let mut refs = Refs::default();

        // Step 1: exclusivity (the other kinds whose live document refuses
        // this create), then, for a final invoice, its prepayment.
        for &(other, reason) in exclusive_with(kind) {
            if let Some(response) = self
                .exclusivity(ctx, &prepared, &identity, other, reason)
                .await?
            {
                return Ok(response);
            }
        }
        if kind == DocumentKind::Final
            && let Some(response) = self
                .prepayment_for_final(ctx, &prepared, &identity, &mut refs)
                .await?
        {
            return Ok(response);
        }

        // Step 2: the proforma link, on the kinds that convert a proforma
        // (`links_proforma`): the invoice and the prepayment invoice.
        if links_proforma(kind)
            && let Some(response) = self
                .proforma_link(ctx, &prepared, &identity, &mut refs)
                .await?
        {
            return Ok(response);
        }

        let create = self.build(
            identity.kind,
            &prepared.document,
            &prepared.order,
            &identity.external_id,
            DocumentRefs {
                proforma: refs.proforma.as_deref(),
                prepayment: refs.prepayment.as_deref(),
                corrected: None,
            },
        )?;
        let intent = Intent {
            identity,
            create,
            reissue: prepared.reissue,
            our_numbers: refs.our_numbers,
        };
        self.issue(ctx, &prepared.order, intent).await
    }

    /// `correct_invoice`, on the `order` the handler parsed from its key.
    pub(super) async fn correct(
        &self,
        ctx: &ObjectContext<'_>,
        order: OrderKey,
        request: CorrectRequest,
    ) -> Result<CreateResponse, HandlerError> {
        let CorrectRequest {
            invoice_number: number,
            correction_id,
            document,
        } = request;
        let number = String::from(number);
        self.validate_document(IssuedKind::Corrective, &document, &order)?;
        let identity = Identity {
            kind: IssuedKind::Corrective,
            external_id: ExternalId::for_corrective(&self.config.namespace, &order, &correction_id),
        };

        // The base must be a live invoice carrying this order's number
        // (`decide_base`).
        let about = |fault: Fault| identity.about(&order, fault);
        let found = verify(ctx, self, format!("verify-base-{number}"), &number)
            .await
            .map_err(about)?;
        let found = verified_document(found, &number, &self.config.namespace).map_err(about)?;
        if let Some(response) = decide_base(&found, &order, &number, &identity) {
            return Ok(response);
        }

        let create = self.build(
            IssuedKind::Corrective,
            &document,
            &order,
            &identity.external_id,
            DocumentRefs {
                corrected: Some(&number),
                ..DocumentRefs::default()
            },
        )?;
        let intent = Intent {
            identity,
            create,
            reissue: false,
            our_numbers: Vec::new(),
        };
        self.issue(ctx, &order, intent).await
    }

    // ----- step 0: validation ----------------------------------------------

    fn prepare(
        &self,
        order: OrderKey,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<Prepared, Fault> {
        let CreateRequest { document, options } = request;
        if options.proforma != ProformaLink::Auto && !links_proforma(kind) {
            return Err(Fault::invalid_input(format!(
                "options.proforma applies to create_invoice and create_prepayment only, not create_{kind}"
            )));
        }
        self.validate_document(kind.into(), &document, &order)?;
        Ok(Prepared {
            order,
            document,
            reissue: options.reissue,
            proforma: options.proforma,
        })
    }

    /// Validates the document by building it once with placeholder
    /// references.
    fn validate_document(
        &self,
        kind: IssuedKind,
        document: &DocumentInput,
        order: &OrderKey,
    ) -> Result<(), Fault> {
        if normalize_buyer_name(&document.buyer.name).is_empty() {
            return Err(Fault::invalid_input("buyer.name must not be empty"));
        }
        self.build(
            kind,
            document,
            order,
            &ExternalId::new("-"),
            DocumentRefs {
                proforma: None,
                prepayment: Some("-"),
                corrected: Some("-"),
            },
        )?;
        Ok(())
    }

    fn build(
        &self,
        kind: IssuedKind,
        document: &DocumentInput,
        order: &OrderKey,
        external_id: &ExternalId,
        refs: DocumentRefs<'_>,
    ) -> Result<CreateInvoice, Fault> {
        self.gateway
            .build_create(kind, document, order, external_id, refs)
            .map_err(|error| Fault::invalid_input(error.to_string()))
    }

    // ----- step 1: exclusivity ---------------------------------------------

    /// A live document of ours under `other`'s external id refuses the
    /// create as `conflict{reason, existing_number}` ([`exclusive_with`]);
    /// the decision is [`decide_exclusivity`].
    async fn exclusivity(
        &self,
        ctx: &ObjectContext<'_>,
        prepared: &Prepared,
        identity: &Identity,
        other: DocumentKind,
        reason: ConflictReason,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let other_id = ExternalId::for_kind(&self.config.namespace, &prepared.order, other);
        let found = lookup(
            ctx,
            self,
            format!("exclusivity-{other}"),
            &other_id,
            &prepared.order,
            other.into(),
        )
        .await?;
        decide_exclusivity(found, reason, identity, &self.config.namespace)
            .map_err(|fault| identity.about(&prepared.order, fault).into())
    }

    /// The prepayment a final invoice settles must be live; the decision is
    /// [`decide_prepayment_for_final`].
    async fn prepayment_for_final(
        &self,
        ctx: &ObjectContext<'_>,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let kind = DocumentKind::Prepayment;
        let prepayment_id = ExternalId::for_kind(&self.config.namespace, &prepared.order, kind);
        let found = lookup(
            ctx,
            self,
            "prepayment-for-final",
            &prepayment_id,
            &prepared.order,
            kind.into(),
        )
        .await?;
        decide_prepayment_for_final(found, identity, &self.config.namespace, refs)
            .map_err(|fault| identity.about(&prepared.order, fault).into())
    }

    // ----- step 2: the proforma link ---------------------------------------

    /// `options.proforma` for an invoice or a prepayment invoice
    /// ([`links_proforma`]): under `auto` and `none` the `proforma-link`
    /// lookup of `…:proforma`, decided by [`decide_proforma_link`]; under
    /// `{number}` the `verify-proforma-{number}` read, decided by
    /// [`decide_proforma_by_number`].
    async fn proforma_link(
        &self,
        ctx: &ObjectContext<'_>,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let kind = DocumentKind::Proforma;
        match &prepared.proforma {
            ProformaLink::Auto | ProformaLink::None => {
                let proforma_id =
                    ExternalId::for_kind(&self.config.namespace, &prepared.order, kind);
                let found = lookup(
                    ctx,
                    self,
                    "proforma-link",
                    &proforma_id,
                    &prepared.order,
                    kind.into(),
                )
                .await?;
                decide_proforma_link(
                    found,
                    &prepared.proforma,
                    identity,
                    &self.config.namespace,
                    refs,
                )
                .map_err(|fault| identity.about(&prepared.order, fault).into())
            }
            ProformaLink::Number(number) => {
                let number = number.as_str();
                let found = verify(ctx, self, format!("verify-proforma-{number}"), number)
                    .await
                    .map_err(|fault| identity.about(&prepared.order, fault))?;
                decide_proforma_by_number(
                    found,
                    number,
                    &prepared.order,
                    identity,
                    &self.config.namespace,
                    refs,
                )
                .map_err(HandlerError::from)
            }
        }
    }

    // ----- steps 3–5: lookup, create, branch on data -----------------------

    /// Issues `intent`: the lookup step settles every case that needs no
    /// create; the create step, under the issue policy, sends it; the result
    /// is branched on as data.
    async fn issue(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: Intent,
    ) -> Result<CreateResponse, HandlerError> {
        let identity = &intent.identity;
        let about = |fault: Fault| identity.about(order, fault);

        // Step 3: lookup, then decide on what it found.
        let found = self.lookup_step(ctx, order, &intent).await.map_err(about)?;
        let reversed = match decide_lookup(found, intent.reissue, identity, &self.config.namespace)
            .map_err(about)?
        {
            ControlFlow::Break(response) => return Ok(response),
            ControlFlow::Continue(reversed) => reversed,
        };

        // Step 4: create.
        let outcome = self.create_step(ctx, order, &intent, reversed).await?;

        // Step 5: branch on data.
        Ok(identity
            .respond_to(outcome, &self.config.namespace)
            .map_err(about)?)
    }

    /// Step 3: one read-only durable step under the read policy, querying the
    /// external id and, for every kind but correctives, the order-number
    /// hint. A lookup szamlazz.hu never answered is `unavailable`; the caller
    /// attaches the document.
    async fn lookup_step(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &Intent,
    ) -> Result<LookupOutcome, Fault> {
        let gateway = Arc::clone(&self.gateway);
        let external_id = intent.identity.external_id.clone();
        let kind = intent.identity.kind;
        let order = order.clone();
        let our_numbers = intent.our_numbers.clone();
        run_reading(ctx, format!("lookup-{kind}"), self, move || async move {
            gateway
                .lookup(LookupRequest {
                    external_id: &external_id,
                    kind,
                    order: &order,
                    our_numbers: &our_numbers,
                })
                .await
        })
        .await
    }

    /// Step 4: one durable step under the issue policy's run retry policy,
    /// query-first on every execution (the query is inside the closure: a
    /// separate journaled pre-query would replay its stale "nothing" on the
    /// retry and re-send). Any `Err` from the run (exhaustion (500) or
    /// cancellation (409)) is `outcome_unknown` about this document
    /// ([`create_outcome_unknown`]): nothing is recorded, the next
    /// invocation's lookup finds whatever landed.
    async fn create_step(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &Intent,
        reversed: Option<String>,
    ) -> Result<CreateOutcome, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let external_id = intent.identity.external_id.clone();
        let kind = intent.identity.kind;
        let order_key = order.clone();
        let create = intent.create.clone();
        run_retrying(
            ctx,
            format!("create-{kind}"),
            self.config.issue.run_retry_policy(),
            move || async move {
                gateway
                    .create(CreateStepRequest {
                        external_id: &external_id,
                        kind,
                        order: &order_key,
                        create: &create,
                        reversed: reversed.as_deref(),
                    })
                    .await
            },
        )
        .await
        .map_err(|error| create_outcome_unknown(&error, order, &intent.identity).into())
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;
    use rust_decimal::dec;
    use szamlazz_agent::Credentials;

    use super::*;
    use crate::account::{Account, Endpoint};
    use crate::config::WorkerConfig;
    use crate::contract::TerminalCode;
    use crate::contract::document::tests::sample_document;
    use crate::gateway::{IssuedDocument, Rejection, SzamlazzAnswer};
    use crate::test_support::{Doc, open_gateway};

    /// An execution as the prologue would build it for the test account.
    fn order() -> Execution {
        let mut account = Account::new("acct", "acct");
        account.endpoint = Endpoint::parse("http://127.0.0.1:1/").expect("endpoint");
        Execution {
            gateway: Arc::new(open_gateway(account, Credentials::agent_key("key"))),
            config: WorkerConfig::new("acct".parse().expect("namespace")),
        }
    }

    fn request(proforma: ProformaLink) -> CreateRequest {
        let mut request = CreateRequest::new(sample_document());
        request.options.proforma = proforma;
        request
    }

    /// The message of an `invalid_input` fault (400).
    fn invalid_input(fault: Fault) -> String {
        let (status, body) = fault_body(fault);
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["code"], TerminalCode::InvalidInput.as_str());
        body["message"].as_str().expect("message").to_owned()
    }

    fn ord_1() -> OrderKey {
        OrderKey::parse("ORD-1").expect("order")
    }

    /// Step 1's table: the invoice and prepayment chains refuse each other
    /// (`prepaid_chain`), and the final invoice (the prepayment chain's
    /// settled end, live after its `ES` is reversed) refuses both the same
    /// way (without that row, `ES` → `VS` → storno of the `ES` let a plain
    /// `SZ` land beside a live `VS`); a proforma is refused by all
    /// three (`order_invoiced`), so that the order's own invoice is never met
    /// by the hint as `foreign`; the final invoice's own check is
    /// `prepayment_for_final`, not exclusivity.
    #[test]
    fn exclusivity_table_names_the_other_kinds_and_their_reasons() {
        assert_eq!(
            exclusive_with(DocumentKind::Invoice),
            [
                (DocumentKind::Prepayment, ConflictReason::PrepaidChain),
                (DocumentKind::Final, ConflictReason::PrepaidChain),
            ]
        );
        assert_eq!(
            exclusive_with(DocumentKind::Prepayment),
            [
                (DocumentKind::Invoice, ConflictReason::PrepaidChain),
                (DocumentKind::Final, ConflictReason::PrepaidChain),
            ]
        );
        assert_eq!(
            exclusive_with(DocumentKind::Proforma),
            [
                (DocumentKind::Invoice, ConflictReason::OrderInvoiced),
                (DocumentKind::Prepayment, ConflictReason::OrderInvoiced),
                (DocumentKind::Final, ConflictReason::OrderInvoiced),
            ]
        );
        assert_eq!(exclusive_with(DocumentKind::Final), []);
    }

    /// `options.proforma` is an option of the kinds that convert a proforma:
    /// the invoice and the prepayment invoice; the Agent carries
    /// `dijbekeroSzamlaszam` on both. A proforma has nothing to convert and
    /// the final invoice settles a prepayment invoice, so both refuse
    /// anything but `auto` before any read.
    #[test]
    fn options_proforma_applies_to_the_kinds_that_convert_a_proforma() {
        let order = order();
        for link in [
            ProformaLink::None,
            ProformaLink::Number("D-1".parse().expect("valid number")),
        ] {
            for kind in [DocumentKind::Invoice, DocumentKind::Prepayment] {
                assert!(links_proforma(kind));
                let prepared = order
                    .prepare(ord_1(), kind, request(link.clone()))
                    .unwrap_or_else(|error| {
                        panic!("create_{kind} accepts options.proforma: {error:?}")
                    });
                assert_eq!(prepared.proforma, link);
            }

            for kind in [DocumentKind::Proforma, DocumentKind::Final] {
                assert!(!links_proforma(kind));
                let fault = order
                    .prepare(ord_1(), kind, request(link.clone()))
                    .err()
                    .unwrap_or_else(|| panic!("create_{kind} must refuse {link:?}"));
                let message = invalid_input(fault);
                assert!(
                    message.contains(&format!("not create_{kind}"))
                        && message.contains("create_invoice and create_prepayment"),
                    "{kind}: {message}"
                );
            }
        }

        for kind in DocumentKind::ALL {
            let prepared = order
                .prepare(ord_1(), kind, request(ProformaLink::Auto))
                .unwrap_or_else(|error| panic!("create_{kind} accepts auto: {error:?}"));
            assert_eq!(prepared.proforma, ProformaLink::Auto);
        }
    }

    /// Step 0: a body whose line-item arithmetic overflows a decimal is the
    /// `invalid_input` fault naming the item, raised by the handler's own
    /// validation before any read; never a panic, which on the SDK's
    /// connection task would take every in-flight invocation down with it.
    /// Runs after the Prologue (the check needs the account's currency
    /// defaults), so `namespace` and `account` are journaled; nothing is sent.
    #[test]
    fn an_overflowing_line_item_is_invalid_input_not_a_panic() {
        use crate::contract::LineItemInput;
        use rust_decimal::{Decimal, dec};

        let order = order();
        for kind in DocumentKind::ALL {
            let mut request = request(ProformaLink::Auto);
            request.document.items.push(LineItemInput::new(
                "adversarial",
                dec!(10),
                "db",
                Decimal::MAX,
                "27",
            ));
            let fault = order
                .prepare(ord_1(), kind, request)
                .err()
                .unwrap_or_else(|| panic!("create_{kind}: overflow is refused"));
            let message = invalid_input(fault);
            assert!(
                message.contains("items[1]") && message.contains("overflows a decimal"),
                "create_{kind}: names the item and the rule: {message}"
            );
        }

        let mut correct = CorrectRequest {
            invoice_number: "SZ-1".parse().expect("valid number"),
            correction_id: "c-1".parse().expect("valid id"),
            document: sample_document(),
        };
        correct.document.items[0].quantity = Decimal::MAX;
        correct.document.items[0].unit_price = dec!(10);
        let fault = order
            .validate_document(IssuedKind::Corrective, &correct.document, &ord_1())
            .expect_err("overflow is refused on a corrective too");
        assert!(invalid_input(fault).contains("items[0]"));
    }

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn invoice_identity() -> Identity {
        Identity::of_kind(&namespace(), &ord_1(), DocumentKind::Invoice)
    }

    /// The `{code, message}` body of a fault, with its HTTP status.
    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// The response a lookup decision settled without a create.
    fn settled(
        decision: Result<ControlFlow<CreateResponse, Option<String>>, Fault>,
    ) -> CreateResponse {
        match decision {
            Ok(ControlFlow::Break(response)) => response,
            other => panic!("settled without a create: {other:?}"),
        }
    }

    // ----- step 3: the lookup decision -------------------------------------

    /// Step 3, a live document of ours under the external id: the caller
    /// asked for this document and has it (`already_issued`, with its number
    /// and totals); with `reissue` the same document is `conflict{live}`
    /// naming it, so the flag can never cause a duplicate. Nothing proceeds
    /// to the create step either way.
    #[test]
    fn a_live_document_under_the_id_is_already_issued_or_a_live_conflict_under_reissue() {
        let identity = invoice_identity();
        let live = Doc::default().boxed();

        let response = settled(decide_lookup(
            LookupOutcome::Live(live.clone()),
            false,
            &identity,
            &namespace(),
        ));
        assert_eq!(response.outcome, Outcome::AlreadyIssued);
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-1"));
        assert_eq!(response.net_total, Some(dec!(1000)));
        assert_eq!(response.gross_total, Some(dec!(1270)));
        assert_eq!(response.outstanding, Some(dec!(1270)));
        assert_eq!(response.external_id, "acct:ORD-1:invoice");
        assert_eq!(response.kind, IssuedKind::Invoice);

        let response = settled(decide_lookup(
            LookupOutcome::Live(live),
            true,
            &identity,
            &namespace(),
        ));
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(response.conflict_reason, Some(ConflictReason::Live));
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));
        assert_eq!(response.invoice_number, None);
    }

    /// Step 3, a reversed document of ours under the external id: without
    /// `reissue` the answer is `reversed` naming the document and, when the
    /// hint named it, its storno; with `reissue` the create proceeds carrying
    /// the reversed number, the one holder the create step may send past.
    #[test]
    fn a_reversed_document_under_the_id_is_reversed_or_proceeds_under_reissue() {
        let identity = invoice_identity();
        let reversed = Doc {
            reversed: true,
            ..Doc::default()
        }
        .boxed();

        let response = settled(decide_lookup(
            LookupOutcome::Reversed {
                document: reversed.clone(),
                storno_number: Some("SS-1".to_owned()),
            },
            false,
            &identity,
            &namespace(),
        ));
        assert_eq!(response.outcome, Outcome::Reversed);
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-1"));
        assert_eq!(response.storno_number.as_deref(), Some("SS-1"));

        // The hint did not name the storno: `reversed` all the same, without
        // the number.
        let response = settled(decide_lookup(
            LookupOutcome::Reversed {
                document: reversed.clone(),
                storno_number: None,
            },
            false,
            &identity,
            &namespace(),
        ));
        assert_eq!(response.outcome, Outcome::Reversed);
        assert_eq!(response.storno_number, None);

        assert_eq!(
            decide_lookup(
                LookupOutcome::Reversed {
                    document: reversed,
                    storno_number: Some("SS-1".to_owned()),
                },
                true,
                &identity,
                &namespace(),
            )
            .expect("data"),
            ControlFlow::Continue(Some("SZ-1".to_owned())),
            "reissue proceeds past the reversed document, carrying its number"
        );
    }

    /// Step 3, nothing to settle or something never to create past: `Absent`
    /// proceeds carrying nothing; a collision (the newest holder of the id is
    /// another order's or kind's) and a foreign document (another channel's
    /// live invoice under the order) are conflicts naming the document, with
    /// or without `reissue`.
    #[test]
    fn absent_proceeds_while_a_collision_or_a_foreign_document_refuses() {
        let identity = invoice_identity();
        for reissue in [false, true] {
            assert_eq!(
                decide_lookup(LookupOutcome::Absent, reissue, &identity, &namespace())
                    .expect("data"),
                ControlFlow::Continue(None),
                "reissue {reissue}"
            );

            let other_order = Doc {
                order: Some("ORD-2"),
                ..Doc::new("SZ-9", "SZ")
            }
            .boxed();
            let response = settled(decide_lookup(
                LookupOutcome::Collision(other_order),
                reissue,
                &identity,
                &namespace(),
            ));
            assert_eq!(response.outcome, Outcome::Conflict, "reissue {reissue}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::ExternalIdCollision),
                "reissue {reissue}"
            );
            assert_eq!(response.existing_number.as_deref(), Some("SZ-9"));

            let foreign = Doc::new("SZ-77", "SZ").boxed();
            let response = settled(decide_lookup(
                LookupOutcome::Foreign(foreign),
                reissue,
                &identity,
                &namespace(),
            ));
            assert_eq!(response.outcome, Outcome::Conflict, "reissue {reissue}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::Foreign),
                "reissue {reissue}"
            );
            assert_eq!(response.existing_number.as_deref(), Some("SZ-77"));
        }
    }

    /// Step 3, szamlazz.hu answered the external-id query with a code: another
    /// code is `unavailable` (503) carrying it in `szamlazz_code`, nothing may
    /// be concluded; a credential code is `credentials_rejected` (503). Both
    /// are faults, never outcomes, and neither proceeds.
    #[test]
    fn an_answered_code_on_the_lookup_is_a_fault() {
        let identity = invoice_identity();

        let fault = decide_lookup(
            LookupOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")),
            false,
            &identity,
            &namespace(),
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], TerminalCode::Unavailable.as_str(), "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("nothing may be concluded"),
            "{body}"
        );

        let fault = decide_lookup(
            LookupOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "135",
                "Aktív böngésző session.",
            )),
            true,
            &identity,
            &namespace(),
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(
            body["code"],
            TerminalCode::CredentialsRejected.as_str(),
            "{body}"
        );
        assert_eq!(body["szamlazz_code"], "135", "{body}");
    }

    // ----- step 1: exclusivity ---------------------------------------------

    /// Step 1, what the other kind's external id holds: a live document of
    /// ours refuses the create with the table's reason (`prepaid_chain` for
    /// the other chain, `order_invoiced` for a proforma after the invoice),
    /// naming it; a document that fails validation is
    /// `conflict{external_id_collision}`, never "absent", since the newest
    /// holder may hide a live document of ours; a **reversed** document of
    /// ours and nothing at all let the create proceed (the negative case the
    /// table exists for: a reversed `ES` is what lets a plain `SZ` land). An
    /// answered code is a fault
    /// ([`an_answered_code_on_an_ownership_read_is_a_fault`]).
    #[test]
    fn exclusivity_refuses_a_live_other_kind_and_a_collision_and_passes_a_reversed_one() {
        let identity = invoice_identity();
        let namespace = namespace();

        let live_prepayment = Doc::new("ES-1", "ES").boxed();
        let response = decide_exclusivity(
            OwnershipOutcome::Live(live_prepayment),
            ConflictReason::PrepaidChain,
            &identity,
            &namespace,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(response.conflict_reason, Some(ConflictReason::PrepaidChain));
        assert_eq!(response.existing_number.as_deref(), Some("ES-1"));
        assert_eq!(
            response.kind,
            IssuedKind::Invoice,
            "about the create, not the other kind"
        );
        assert_eq!(response.external_id, "acct:ORD-1:invoice");

        let proforma_identity = Identity::of_kind(&namespace, &ord_1(), DocumentKind::Proforma);
        let response = decide_exclusivity(
            OwnershipOutcome::Live(Doc::default().boxed()),
            ConflictReason::OrderInvoiced,
            &proforma_identity,
            &namespace,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::OrderInvoiced)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));

        let other_order = Doc {
            order: Some("ORD-2"),
            ..Doc::new("ES-9", "ES")
        }
        .boxed();
        let response = decide_exclusivity(
            OwnershipOutcome::Collision(other_order),
            ConflictReason::PrepaidChain,
            &identity,
            &namespace,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ExternalIdCollision),
            "never the table's reason: the holder is not ours"
        );
        assert_eq!(response.existing_number.as_deref(), Some("ES-9"));

        let reversed_prepayment = Doc {
            reversed: true,
            ..Doc::new("ES-1", "ES")
        }
        .boxed();
        assert_eq!(
            decide_exclusivity(
                OwnershipOutcome::Reversed(reversed_prepayment),
                ConflictReason::PrepaidChain,
                &identity,
                &namespace,
            )
            .expect("data"),
            None,
            "a reversed other-kind document does not refuse"
        );
        assert_eq!(
            decide_exclusivity(
                OwnershipOutcome::Absent,
                ConflictReason::PrepaidChain,
                &identity,
                &namespace,
            )
            .expect("data"),
            None
        );
    }

    /// Step 1 for a final invoice, what `…:prepayment` holds: the live
    /// prepayment invoice it settles is carried as the `elolegSzamlaszam`
    /// reference and as a number of ours for the hint to ignore; nothing
    /// under the id is `conflict{prepayment_missing}` (no number to name); a
    /// reversed one is `conflict{prepayment_reversed}` naming it; a document
    /// that fails validation is `conflict{external_id_collision}`. Nothing is
    /// recorded on a refusal.
    #[test]
    fn a_final_invoice_settles_a_live_prepayment_and_refuses_everything_else() {
        let identity = Identity::of_kind(&namespace(), &ord_1(), DocumentKind::Final);
        let namespace = namespace();

        let mut refs = Refs::default();
        let live = Doc::new("ES-1", "ES").boxed();
        assert_eq!(
            decide_prepayment_for_final(
                OwnershipOutcome::Live(live),
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(refs.prepayment.as_deref(), Some("ES-1"));
        assert_eq!(refs.our_numbers, ["ES-1"]);
        assert_eq!(refs.proforma, None);

        let mut refs = Refs::default();
        let response =
            decide_prepayment_for_final(OwnershipOutcome::Absent, &identity, &namespace, &mut refs)
                .expect("data")
                .expect("refused");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::PrepaymentMissing)
        );
        assert_eq!(response.existing_number, None);
        assert_eq!(response.kind, IssuedKind::Final);
        assert_eq!(response.external_id, "acct:ORD-1:final");

        let reversed = Doc {
            reversed: true,
            ..Doc::new("ES-1", "ES")
        }
        .boxed();
        let response = decide_prepayment_for_final(
            OwnershipOutcome::Reversed(reversed),
            &identity,
            &namespace,
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::PrepaymentReversed)
        );
        assert_eq!(response.existing_number.as_deref(), Some("ES-1"));

        let other_kind = Doc::new("SZ-9", "SZ").boxed();
        let response = decide_prepayment_for_final(
            OwnershipOutcome::Collision(other_kind),
            &identity,
            &namespace,
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ExternalIdCollision)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-9"));

        assert_eq!(refs.prepayment, None, "nothing recorded on a refusal");
        assert!(refs.our_numbers.is_empty());
    }

    /// Steps 1 and 2 on an answered code, at every decision that reads an
    /// external id of the order: another code is `unavailable` (503) carrying
    /// it in `szamlazz_code`, nothing may be concluded; a credential code is
    /// `credentials_rejected` (503). Faults, never outcomes, and nothing is
    /// recorded.
    #[test]
    fn an_answered_code_on_an_ownership_read_is_a_fault() {
        type Decide<'a> = &'a dyn Fn(OwnershipOutcome) -> Result<Option<CreateResponse>, Fault>;

        let identity = invoice_identity();
        let namespace = namespace();
        let api = || OwnershipOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML."));
        let rejected = || {
            OwnershipOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "135",
                "Aktív böngésző session.",
            ))
        };
        let decisions: [(&str, Decide<'_>); 3] = [
            ("exclusivity", &|found| {
                decide_exclusivity(found, ConflictReason::PrepaidChain, &identity, &namespace)
            }),
            ("prepayment-for-final", &|found| {
                let mut refs = Refs::default();
                let decided = decide_prepayment_for_final(found, &identity, &namespace, &mut refs);
                assert_eq!(refs.prepayment, None, "nothing recorded on a fault");
                decided
            }),
            ("proforma-link", &|found| {
                let mut refs = Refs::default();
                let decided = decide_proforma_link(
                    found,
                    &ProformaLink::Auto,
                    &identity,
                    &namespace,
                    &mut refs,
                );
                assert_eq!(refs.proforma, None, "nothing recorded on a fault");
                decided
            }),
        ];
        for (label, decide) in decisions {
            let (status, body) = fault_body(decide(api()).expect_err("a fault"));
            assert_eq!(status, 503, "{label}: {body}");
            assert_eq!(
                body["code"],
                TerminalCode::Unavailable.as_str(),
                "{label}: {body}"
            );
            assert_eq!(body["szamlazz_code"], "57", "{label}: {body}");
            assert!(
                body["message"]
                    .as_str()
                    .expect("message")
                    .contains("nothing may be concluded"),
                "{label}: {body}"
            );

            let (status, body) = fault_body(decide(rejected()).expect_err("a fault"));
            assert_eq!(status, 503, "{label}: {body}");
            assert_eq!(
                body["code"],
                TerminalCode::CredentialsRejected.as_str(),
                "{label}: {body}"
            );
            assert_eq!(body["szamlazz_code"], "135", "{label}: {body}");
        }
    }

    // ----- step 2: the proforma link ---------------------------------------

    /// Step 2 under `auto`, what `…:proforma` holds: a live proforma of ours
    /// is linked (the `dijbekeroSzamlaszam` reference, and a number of ours
    /// for the hint to ignore); a reversed one, or nothing, links nothing and
    /// the create proceeds; a document that fails validation is
    /// `conflict{external_id_collision}`.
    #[test]
    fn under_auto_a_live_proforma_is_linked_and_anything_else_links_nothing() {
        let identity = invoice_identity();
        let namespace = namespace();

        let mut refs = Refs::default();
        let live = Doc::new("D-1", "D").boxed();
        assert_eq!(
            decide_proforma_link(
                OwnershipOutcome::Live(live),
                &ProformaLink::Auto,
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(refs.proforma.as_deref(), Some("D-1"));
        assert_eq!(refs.our_numbers, ["D-1"]);
        assert_eq!(refs.prepayment, None);

        let mut refs = Refs::default();
        let reversed = Doc {
            reversed: true,
            ..Doc::new("D-1", "D")
        }
        .boxed();
        assert_eq!(
            decide_proforma_link(
                OwnershipOutcome::Reversed(reversed),
                &ProformaLink::Auto,
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(
            decide_proforma_link(
                OwnershipOutcome::Absent,
                &ProformaLink::Auto,
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(refs.proforma, None, "nothing linked");
        assert!(refs.our_numbers.is_empty());

        let other_order = Doc {
            order: Some("ORD-2"),
            ..Doc::new("D-9", "D")
        }
        .boxed();
        let response = decide_proforma_link(
            OwnershipOutcome::Collision(other_order),
            &ProformaLink::Auto,
            &identity,
            &namespace,
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ExternalIdCollision)
        );
        assert_eq!(response.existing_number.as_deref(), Some("D-9"));
        assert_eq!(refs.proforma, None, "nothing recorded on a refusal");
    }

    /// Step 2 under `none`: a live proforma of ours is
    /// `conflict{proforma_live}` naming it (szamlazz.hu would link it by
    /// shared order number regardless, so refusing is the only honest
    /// answer); a reversed one, or nothing, is what `none` asked for and the
    /// create proceeds linking nothing; a collision refuses as under `auto`.
    #[test]
    fn under_none_a_live_proforma_is_a_conflict_and_anything_else_proceeds() {
        let identity = invoice_identity();
        let namespace = namespace();
        let mut refs = Refs::default();

        let live = Doc::new("D-1", "D").boxed();
        let response = decide_proforma_link(
            OwnershipOutcome::Live(live),
            &ProformaLink::None,
            &identity,
            &namespace,
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(response.conflict_reason, Some(ConflictReason::ProformaLive));
        assert_eq!(response.existing_number.as_deref(), Some("D-1"));
        assert_eq!(refs.proforma, None, "nothing recorded on a refusal");

        let reversed = Doc {
            reversed: true,
            ..Doc::new("D-1", "D")
        }
        .boxed();
        assert_eq!(
            decide_proforma_link(
                OwnershipOutcome::Reversed(reversed),
                &ProformaLink::None,
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(
            decide_proforma_link(
                OwnershipOutcome::Absent,
                &ProformaLink::None,
                &identity,
                &namespace,
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(refs.proforma, None);
        assert!(refs.our_numbers.is_empty());

        let other_order = Doc {
            order: Some("ORD-2"),
            ..Doc::new("D-9", "D")
        }
        .boxed();
        let response = decide_proforma_link(
            OwnershipOutcome::Collision(other_order),
            &ProformaLink::None,
            &identity,
            &namespace,
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ExternalIdCollision)
        );
        assert_eq!(response.existing_number.as_deref(), Some("D-9"));
    }

    /// Step 2 under `{number}`, the verify's answer checked like every other
    /// document found by number, in the order the other verifies use: this
    /// order's number first (another order's proforma, or an order-less one,
    /// is `conflict{not_managed}` naming it, so a caller cannot link another
    /// order's live proforma into this order's invoice), then the kind (a
    /// document of ours that is not a proforma is `invalid_input` naming the
    /// number and its `tipus`: the caller's request). A proforma of ours is
    /// linked as named. Code 7 is `conflict{proforma_missing}` naming the
    /// number: an outcome, never `not_found`.
    #[test]
    fn a_proforma_by_number_is_checked_like_every_document_found_by_number() {
        let identity = invoice_identity();
        let order = ord_1();

        let mut refs = Refs::default();
        let ours = Doc::new("D-1", "D").boxed();
        assert_eq!(
            decide_proforma_by_number(
                QueryOutcome::Found(ours),
                "D-1",
                &order,
                &identity,
                &namespace(),
                &mut refs
            )
            .expect("data"),
            None
        );
        assert_eq!(refs.proforma.as_deref(), Some("D-1"));
        assert_eq!(refs.our_numbers, ["D-1"]);

        let mut refs = Refs::default();
        let response = decide_proforma_by_number(
            QueryOutcome::NotFound,
            "D-MISSING",
            &order,
            &identity,
            &namespace(),
            &mut refs,
        )
        .expect("data")
        .expect("refused");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ProformaMissing)
        );
        assert_eq!(response.existing_number.as_deref(), Some("D-MISSING"));

        for (label, order_number) in [("another order's", Some("ORD-2")), ("order-less", None)] {
            let proforma = Doc {
                order: order_number,
                ..Doc::new("D-2", "D")
            }
            .boxed();
            let response = decide_proforma_by_number(
                QueryOutcome::Found(proforma),
                "D-2",
                &order,
                &identity,
                &namespace(),
                &mut refs,
            )
            .expect("data")
            .unwrap_or_else(|| panic!("{label}: refused"));
            assert_eq!(response.outcome, Outcome::Conflict, "{label}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "{label}"
            );
            assert_eq!(response.existing_number.as_deref(), Some("D-2"), "{label}");
        }

        // This order's invoice named as the proforma: the order check passes,
        // the kind check refuses.
        let invoice = Doc::new("SZ-42", "SZ").boxed();
        let fault = decide_proforma_by_number(
            QueryOutcome::Found(invoice),
            "SZ-42",
            &order,
            &identity,
            &namespace(),
            &mut refs,
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 400, "{body}");
        assert_eq!(body["code"], TerminalCode::InvalidInput.as_str(), "{body}");
        assert_eq!(
            body["message"].as_str().expect("message"),
            "SZ-42 is not a proforma (tipus SZ)"
        );
        assert_eq!(
            body.get("order"),
            None,
            "about the caller's request, not a document: {body}"
        );

        assert_eq!(refs.proforma, None, "nothing recorded on a refusal");
        assert!(refs.our_numbers.is_empty());
    }

    /// Step 2 under `{number}`, szamlazz.hu answering the verify with a code:
    /// another code is `unavailable` carrying it, a credential code is
    /// `credentials_rejected`; both about the document being created.
    #[test]
    fn an_answered_code_on_the_proforma_verify_is_a_fault_about_the_create() {
        let identity = invoice_identity();
        let mut refs = Refs::default();

        let fault = decide_proforma_by_number(
            QueryOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")),
            "D-1",
            &ord_1(),
            &identity,
            &namespace(),
            &mut refs,
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], TerminalCode::Unavailable.as_str(), "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");
        assert_eq!(body["order"], "ORD-1", "{body}");
        assert_eq!(body["kind"], "invoice", "{body}");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice", "{body}");

        let fault = decide_proforma_by_number(
            QueryOutcome::CredentialsRejected(SzamlazzAnswer::new(
                "3",
                "Sikertelen bejelentkezés.",
            )),
            "D-1",
            &ord_1(),
            &identity,
            &namespace(),
            &mut refs,
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(
            body["code"],
            TerminalCode::CredentialsRejected.as_str(),
            "{body}"
        );
        assert_eq!(body["szamlazz_code"], "3", "{body}");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice", "{body}");
    }

    // ----- correct_invoice: the base ---------------------------------------

    /// The corrective's base, verified by number: it must be a live invoice
    /// carrying this order's number. Another order's, or an order-less one,
    /// is `conflict{not_managed}` naming it (checked first: a reversed
    /// document of another order is still not this order's); a reversed one
    /// of ours is `conflict{base_reversed}`; a live one of ours proceeds.
    #[test]
    fn a_correctives_base_must_be_a_live_invoice_of_this_order() {
        let identity = Identity {
            kind: IssuedKind::Corrective,
            external_id: ExternalId::for_corrective(
                &namespace(),
                &ord_1(),
                &"c-1".parse().expect("valid id"),
            ),
        };
        let order = ord_1();

        assert_eq!(
            decide_base(&Doc::default().parse(), &order, "SZ-1", &identity),
            None
        );

        for (label, base) in [
            (
                "another order's",
                Doc {
                    order: Some("ORD-2"),
                    ..Doc::default()
                },
            ),
            (
                "order-less",
                Doc {
                    order: None,
                    ..Doc::default()
                },
            ),
            (
                "another order's, reversed",
                Doc {
                    order: Some("ORD-2"),
                    reversed: true,
                    ..Doc::default()
                },
            ),
        ] {
            let response = decide_base(&base.parse(), &order, "SZ-1", &identity)
                .unwrap_or_else(|| panic!("{label}: refused"));
            assert_eq!(response.outcome, Outcome::Conflict, "{label}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "{label}"
            );
            assert_eq!(response.existing_number.as_deref(), Some("SZ-1"), "{label}");
            assert_eq!(response.kind, IssuedKind::Corrective, "{label}");
            assert_eq!(response.external_id, "acct:ORD-1:corrective:c-1", "{label}");
        }

        let reversed = Doc {
            reversed: true,
            ..Doc::default()
        }
        .parse();
        let response = decide_base(&reversed, &order, "SZ-1", &identity).expect("refused");
        assert_eq!(response.conflict_reason, Some(ConflictReason::BaseReversed));
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));
    }

    // ----- step 4: the create step's own fault -----------------------------

    /// Step 4, a create step whose run ended without a settled outcome
    /// (the issue policy exhausted, 500, or the invocation cancelled, 409):
    /// the `outcome_unknown` fault (500) naming how the run ended and the
    /// last `Unconfirmed` display, telling the caller to retry with a new
    /// `Idempotency-Key`, and about the document being created.
    #[test]
    fn an_unsettled_create_step_is_outcome_unknown_repeating_the_last_failure() {
        let identity = invoice_identity();

        let exhausted = TerminalError::new_with_code(
            500,
            "open code 55: signing; the re-query that would have settled it failed: HTTP 500",
        );
        let (status, body) = fault_body(create_outcome_unknown(&exhausted, &ord_1(), &identity));
        assert_eq!(status, 500, "{body}");
        assert_eq!(
            body["code"],
            TerminalCode::OutcomeUnknown.as_str(),
            "{body}"
        );
        let message = body["message"].as_str().expect("message");
        assert!(
            message.contains("(500)"),
            "names how the run ended: {message}"
        );
        assert!(
            message.contains("open code 55"),
            "the last failure: {message}"
        );
        assert!(message.contains("HTTP 500"), "{message}");
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
        assert_eq!(body["order"], "ORD-1", "{body}");
        assert_eq!(body["kind"], "invoice", "{body}");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice", "{body}");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");

        let cancelled = TerminalError::new_with_code(409, "cancelled");
        let (status, body) = fault_body(create_outcome_unknown(&cancelled, &ord_1(), &identity));
        assert_eq!(status, 500, "still outcome_unknown: {body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("(409)"), "{message}");
        assert!(message.contains("cancelled"), "{message}");
    }

    /// A create reply parsed the way the gateway parses it and projected the
    /// way the create step projects it: the Számla Agent crate's result type
    /// is `#[non_exhaustive]`, so the wire is the seam, and the test states
    /// the answer szamlazz.hu gives.
    fn issued_reply(headers: &[(&str, &str)], body: &str) -> IssuedDocument {
        use szamlazz_agent::wire::{AgentRequest as _, RawResponse};

        let create = order()
            .build(
                IssuedKind::Invoice,
                &sample_document(),
                &ord_1(),
                &ExternalId::new("acct:ORD-1:invoice"),
                DocumentRefs::default(),
            )
            .expect("build");
        let result = create
            .parse(&RawResponse::new(
                headers.iter().copied(),
                body.as_bytes().to_vec(),
            ))
            .expect("xmlszamlavalasz parses");
        IssuedDocument::try_from(result).expect("a numbered reply")
    }

    /// Step 5, the create step's `Issued`: szamlazz.hu issued the document,
    /// and the response carries its number, totals and the buyer's account
    /// URL, with no warning. A success szamlazz.hu answered with code 56 (the
    /// invoice was issued, its notification not delivered) is `issued` all
    /// the same with the `notification_delivery_failed` warning; the gateway
    /// reports it with a number, and its totals as szamlazz.hu managed to
    /// send them.
    #[test]
    fn an_issued_document_is_issued_with_the_notification_warning_when_56_said_so() {
        let identity = invoice_identity();
        let respond = |outcome: CreateOutcome| identity.respond_to(outcome, &namespace());

        let issued = issued_reply(
            &[("szlahu_id", "924307747")],
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>SZ-2</szamlaszam><szamlanetto>1000</szamlanetto><szamlabrutto>1270</szamlabrutto><kintlevoseg>1270</kintlevoseg><vevoifiokurl>https://www.szamlazz.hu/szamla/fiok/example</vevoifiokurl></xmlszamlavalasz>"#,
        );
        assert!(!issued.notification_delivery_failed);
        let response = respond(CreateOutcome::Issued(issued)).expect("data");
        assert_eq!(response.outcome, Outcome::Issued);
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-2"));
        assert_eq!(response.net_total, Some(dec!(1000)));
        assert_eq!(response.gross_total, Some(dec!(1270)));
        assert_eq!(response.outstanding, Some(dec!(1270)));
        assert_eq!(
            response.customer_account_url.as_deref(),
            Some("https://www.szamlazz.hu/szamla/fiok/example")
        );
        assert!(response.warnings.is_empty(), "{:?}", response.warnings);
        assert_eq!(response.external_id, "acct:ORD-1:invoice");

        // Code 56 with a number: issued, notification not delivered.
        let issued = issued_reply(
            &[
                ("szlahu_error_code", "56"),
                ("szlahu_error", "notification failed"),
                ("szlahu_szamlaszam", "SZ-3"),
                ("szlahu_nettovegosszeg", "1000"),
                ("szlahu_bruttovegosszeg", "1270"),
                ("szlahu_kintlevoseg", "1270"),
            ],
            r#"<?xml version="1.0" encoding="UTF-8"?><xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet><szamlaszam>SZ-3</szamlaszam></xmlszamlavalasz>"#,
        );
        assert!(issued.notification_delivery_failed);
        let response = respond(CreateOutcome::Issued(issued)).expect("data");
        assert_eq!(response.outcome, Outcome::Issued, "issued, not rejected");
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-3"));
        assert_eq!(response.gross_total, Some(dec!(1270)));
        assert_eq!(response.warnings, [Warning::NotificationDeliveryFailed]);
        assert_eq!(response.code, None, "56 is not a rejection code here");
    }

    /// Step 5: every settled create outcome as the caller's response, in
    /// particular the two the create step settles when its leading query or
    /// re-query finds something the lookup did not see: a document reversed
    /// since the lookup is `reversed` (never issued past without `reissue`),
    /// and the lookup's reversed document reported live is `conflict{live}`.
    #[test]
    fn a_settled_create_step_maps_onto_the_response() {
        let namespace: Namespace = "acct".parse().expect("namespace");
        let order = OrderKey::parse("ORD-1").expect("order");
        let identity = Identity::of_kind(&namespace, &order, DocumentKind::Invoice);
        let respond = |outcome: CreateOutcome| identity.respond_to(outcome, &namespace);

        let live = Doc::default().boxed();
        let reversed = Doc {
            reversed: true,
            ..Doc::default()
        }
        .boxed();

        let response = respond(CreateOutcome::Found(live.clone())).expect("data");
        assert_eq!(response.outcome, Outcome::Issued);
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-1"));
        assert_eq!(response.external_id, "acct:ORD-1:invoice");

        let response = respond(CreateOutcome::Reconciled(live.clone())).expect("data");
        assert_eq!(response.outcome, Outcome::Reconciled);

        let response = respond(CreateOutcome::Reversed(reversed)).expect("data");
        assert_eq!(response.outcome, Outcome::Reversed);
        assert_eq!(response.invoice_number.as_deref(), Some("SZ-1"));
        assert_eq!(
            response.storno_number, None,
            "the create step does not look the storno number up"
        );

        let response = respond(CreateOutcome::LiveAgain(live.clone())).expect("data");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(response.conflict_reason, Some(ConflictReason::Live));
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));

        let response = respond(CreateOutcome::Collision(live)).expect("data");
        assert_eq!(response.outcome, Outcome::Conflict);
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::ExternalIdCollision)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-1"));

        let response = respond(CreateOutcome::DuplicateOrderNumber {
            answer: SzamlazzAnswer::new("152", "dup"),
            existing_number: Some("SZ-77".to_owned()),
        })
        .expect("data");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::DuplicateOrderNumber)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-77"));
        assert_eq!(response.code.as_deref(), Some("152"));

        let response = respond(CreateOutcome::Rejected(Rejection::from(
            SzamlazzAnswer::new("259", "net"),
        )))
        .expect("data");
        assert_eq!(response.outcome, Outcome::Rejected);
        assert_eq!(response.code.as_deref(), Some("259"));

        let fault = respond(CreateOutcome::CredentialsRejected(SzamlazzAnswer::new(
            "3", "login",
        )))
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], TerminalCode::CredentialsRejected.as_str());

        // The leading query's answers (#63): `unavailable` at once, the shape
        // the lookup step gives the same code, with the szamlazz.hu code beside
        // it, never in `code`; `szlahu_down` has no code to carry.
        let fault = respond(CreateOutcome::Api(SzamlazzAnswer::new("57", "Hibás XML.")))
            .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], TerminalCode::Unavailable.as_str());
        assert_eq!(body["szamlazz_code"], "57");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("code 57"), "{message}");
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );

        let fault = respond(CreateOutcome::Unavailable {
            message: "maintenance".to_owned(),
        })
        .expect_err("a fault");
        let (status, body) = fault_body(fault);
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], TerminalCode::Unavailable.as_str());
        assert!(body.get("szamlazz_code").is_none(), "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("szlahu_down"), "{message}");
        assert!(message.contains("maintenance"), "{message}");
        assert!(message.contains("nothing was sent"), "{message}");
    }
}
