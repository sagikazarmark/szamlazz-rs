//! The create protocol (design §5) for the four document kinds and for
//! correctives, in the order the steps appear in the design.
//!
//! The handlers keep no state. After validation and the reference checks
//! (read-only steps under the read policy), issuing is two durable steps: a
//! read-only **lookup** (`lookup-{kind}`) under the read policy that settles
//! every case needing no create, and a **create** (`create-{kind}`) under the
//! issue policy's run retry policy, query-first on every execution — the
//! external-id query inside the create closure is what finds a document an
//! earlier execution issued. Domain outcomes are data and faults are
//! `TerminalError`s; a read that szamlazz.hu never answered is `unavailable`,
//! a create step that ends without a settled outcome is `outcome_unknown`.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;
use szamlazz_agent::ops::invoice::CreateInvoice;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::prologue::Execution;
use super::support::object::{lookup, run_reading, run_retrying, verify};
use super::support::{Fault, Lookup, check_pins};
use crate::config::Namespace;
use crate::contract::{
    ConflictReason, CorrectRequest, CreateRequest, CreateResponse, DocumentInput, DocumentKind,
    IssuedKind, Outcome, ProformaLink, Warning, outstanding,
};
use crate::gateway::{
    CreateOutcome, CreateStepRequest, DocumentRefs, InvoiceDocumentExt as _, LookupOutcome,
    LookupRequest, QueryOutcome,
};
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
    fn found(&self, outcome: Outcome, found: &InvoiceDocument) -> CreateResponse {
        let gross = Some(found.totals.total.gross);
        let mut response = self.respond(outcome).with_invoice_number(found.number());
        response.net_total = Some(found.totals.total.net);
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

    /// Step 5 of the create protocol (design §5): the settled create step as
    /// the caller's response. Pure — every szamlazz.hu answer is data.
    ///
    /// # Errors
    ///
    /// The two faults a settled step can still be: rejected credentials, and
    /// an `Issued` without a number (a gateway bug, answered as
    /// `outcome_unknown`). The caller attaches the document's identity.
    fn respond_to(
        &self,
        outcome: CreateOutcome,
        namespace: &Namespace,
    ) -> Result<CreateResponse, Fault> {
        Ok(match outcome {
            CreateOutcome::Issued(issued) => {
                // The gateway reports `Issued` only with a number; a bare
                // result here would be a bug, answered as a fault.
                let Some(number) = issued.invoice_number else {
                    return Err(Fault::outcome_unknown(
                        "issued without a document number; retry with a new Idempotency-Key",
                    ));
                };
                let mut response = self
                    .respond(Outcome::Issued)
                    .with_invoice_number(number.as_str());
                response.net_total = issued.net_total;
                response.gross_total = issued.gross_total;
                response.outstanding = issued.outstanding;
                response.customer_account_url = issued.customer_account_url;
                if issued.notification_delivery_failed {
                    response = response.with_warning(Warning::NotificationDeliveryFailed);
                }
                response
            }
            // An earlier execution of the step created it (ADR 0003): the
            // caller asked for this document and has it.
            CreateOutcome::Found(found) => self.found(Outcome::Issued, &found),
            // Issued and reversed since the lookup — by an earlier execution
            // of the step and anyone's storno. As if the lookup had seen it:
            // `reversed`, and a new document needs an explicit `reissue`
            // (ADR 0003). The storno number is not looked up here; the next
            // call's lookup reports it.
            CreateOutcome::Reversed(found) => self.reversed(found.number(), None),
            // The document the lookup saw reversed is reported live: what
            // the lookup would have answered under `reissue`.
            CreateOutcome::LiveAgain(found) => {
                self.conflict_about(ConflictReason::Live, found.number())
            }
            CreateOutcome::Reconciled(found) => self.found(Outcome::Reconciled, &found),
            CreateOutcome::Collision(found) => {
                self.conflict_about(ConflictReason::ExternalIdCollision, found.number())
            }
            CreateOutcome::DuplicateOrderNumber {
                code,
                message,
                existing_number,
            } => {
                let mut response = self
                    .conflict(ConflictReason::DuplicateOrderNumber)
                    .with_code(code)
                    .with_message(message);
                response.existing_number = existing_number;
                response
            }
            CreateOutcome::Rejected { code, message } => self.rejected(code, message),
            CreateOutcome::CredentialsRejected { code, message } => {
                return Err(Fault::credentials_rejected(namespace, code, message));
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
/// prepayment invoice the same way, even after its `ES` is reversed — without
/// that row `ES` → `VS` → storno of the `ES` let a plain `SZ` land beside a
/// live `VS`, the double billing the worker exists to prevent (#62). A
/// proforma after any of the three makes no sense (`order_invoiced`) — and
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

        // Step 1: exclusivity — the other kinds whose live document refuses
        // this create — then, for a final invoice, its prepayment.
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

        // Step 2: the proforma link. Invoices only: the Agent cannot carry
        // `dijbekeroSzamlaszam` on a prepayment invoice, and the server
        // converts the order's live proforma by shared order number anyway
        // (`docs/szamlazz-hu-behaviour.md`, "Proformas: conversion,
        // auto-linking, deletion"), so a prepayment skips the lookup.
        if kind == DocumentKind::Invoice
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
        self.validate_document(IssuedKind::Corrective, &document, &order)?;
        let identity = Identity {
            kind: IssuedKind::Corrective,
            external_id: ExternalId::for_corrective(&self.config.namespace, &order, &correction_id),
        };

        // The base must be a live invoice carrying this order's number.
        let about =
            |fault: Fault| fault.about(&order, Some(identity.kind), identity.external_id.as_str());
        match verify(ctx, self, format!("verify-base-{number}"), &number)
            .await
            .map_err(about)?
        {
            QueryOutcome::Api { code, message } => {
                return Err(about(Fault::inconclusive_answer(code, message)).into());
            }
            QueryOutcome::CredentialsRejected { code, message } => {
                return Err(about(Fault::credentials_rejected(
                    &self.config.namespace,
                    code,
                    message,
                ))
                .into());
            }
            QueryOutcome::NotFound => {
                return Err(Fault::invalid_input(format!(
                    "invoice {number} is not known to szamlazz.hu (not_found)"
                ))
                .into());
            }
            QueryOutcome::Found(found) => {
                if !found.carries_order(&order) {
                    return Ok(identity.conflict_about(ConflictReason::NotManaged, number));
                }
                check_pins(self.gateway.account(), &found)?;
                if found.info.reversed == Some(true) {
                    return Ok(identity.conflict_about(ConflictReason::BaseReversed, number));
                }
            }
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
        if options.proforma != ProformaLink::Auto && kind != DocumentKind::Invoice {
            return Err(Fault::invalid_input(format!(
                "options.proforma applies to create_invoice only, not create_{kind}"
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
    /// create as `conflict{reason, existing_number}` ([`exclusive_with`]).
    ///
    /// A document under the other id that fails validation is
    /// `conflict{external_id_collision}`, never "absent": the query returns
    /// the newest holder, so a foreign document may hide a live document of
    /// ours behind it, and refusing to create is the only safe answer.
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
        Ok(match found {
            Lookup::Collision(found) => {
                Some(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number()))
            }
            Lookup::Ours(found) if found.is_live() => {
                Some(identity.conflict_about(reason, found.number()))
            }
            Lookup::Absent | Lookup::Ours(_) => None,
        })
    }

    /// The prepayment a final invoice settles must be live.
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
        Ok(match found {
            Lookup::Absent => Some(identity.conflict(ConflictReason::PrepaymentMissing)),
            Lookup::Collision(found) => {
                Some(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number()))
            }
            Lookup::Ours(found) if !found.is_live() => {
                Some(identity.conflict_about(ConflictReason::PrepaymentReversed, found.number()))
            }
            Lookup::Ours(found) => {
                refs.our_numbers.push(found.number().to_owned());
                refs.prepayment = Some(found.number().to_owned());
                None
            }
        })
    }

    // ----- step 2: the proforma link ---------------------------------------

    /// `options.proforma` for an invoice.
    ///
    /// Under `auto` and `none` a document under `…:proforma` that fails
    /// validation is `conflict{external_id_collision}` — see
    /// [`Self::exclusivity`] for why a collision is never treated as absent.
    ///
    /// Under `{number}` the named document is verified and checked like every
    /// other document found by number (design §3): another order's number, or
    /// none, is `conflict{not_managed, existing_number}`; a pin that is not
    /// the resolved account's is the `account_mismatch` fault; a document
    /// that is not a proforma is `invalid_input`.
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
                let live = match found {
                    Lookup::Collision(found) => {
                        return Ok(Some(identity.conflict_about(
                            ConflictReason::ExternalIdCollision,
                            found.number(),
                        )));
                    }
                    Lookup::Ours(found) if found.is_live() => found,
                    Lookup::Absent | Lookup::Ours(_) => return Ok(None),
                };
                if prepared.proforma == ProformaLink::None {
                    // The server links by shared order number regardless, so
                    // refusing is the only honest answer.
                    return Ok(Some(
                        identity.conflict_about(ConflictReason::ProformaLive, live.number()),
                    ));
                }
                refs.our_numbers.push(live.number().to_owned());
                refs.proforma = Some(live.number().to_owned());
                Ok(None)
            }
            ProformaLink::Number(number) => {
                let about = |fault: Fault| {
                    fault.about(
                        &prepared.order,
                        Some(identity.kind),
                        identity.external_id.as_str(),
                    )
                };
                match verify(ctx, self, format!("verify-proforma-{number}"), number)
                    .await
                    .map_err(about)?
                {
                    QueryOutcome::Api { code, message } => {
                        Err(about(Fault::inconclusive_answer(code, message)).into())
                    }
                    QueryOutcome::CredentialsRejected { code, message } => Err(about(
                        Fault::credentials_rejected(&self.config.namespace, code, message),
                    )
                    .into()),
                    QueryOutcome::NotFound => Ok(Some(
                        identity.conflict_about(ConflictReason::ProformaMissing, number.clone()),
                    )),
                    // Checked like every other document found by number
                    // (design §3), in the order the other verifies use: this
                    // order's number, then the account pins, then the kind.
                    // Without the first two a caller could link another
                    // order's — or another account's — live proforma into
                    // this order's invoice.
                    QueryOutcome::Found(found) => {
                        if !found.carries_order(&prepared.order) {
                            return Ok(Some(
                                identity.conflict_about(ConflictReason::NotManaged, number.clone()),
                            ));
                        }
                        check_pins(self.gateway.account(), &found)?;
                        if found.info.document_type != "D" {
                            return Err(Fault::invalid_input(format!(
                                "{number} is not a proforma (tipus {})",
                                found.info.document_type
                            ))
                            .into());
                        }
                        refs.our_numbers.push(found.number().to_owned());
                        refs.proforma = Some(number.clone());
                        Ok(None)
                    }
                }
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
        let about =
            |fault: Fault| fault.about(order, Some(identity.kind), identity.external_id.as_str());

        // Step 3: lookup.
        let reversed = match self.lookup_step(ctx, order, &intent).await.map_err(about)? {
            LookupOutcome::Api { code, message } => {
                return Err(about(Fault::inconclusive_answer(code, message)).into());
            }
            LookupOutcome::CredentialsRejected { code, message } => {
                return Err(about(Fault::credentials_rejected(
                    &self.config.namespace,
                    code,
                    message,
                ))
                .into());
            }
            LookupOutcome::Live(found) if intent.reissue => {
                return Ok(identity.conflict_about(ConflictReason::Live, found.number()));
            }
            LookupOutcome::Live(found) => {
                return Ok(identity.found(Outcome::AlreadyIssued, &found));
            }
            LookupOutcome::Reversed {
                document,
                storno_number,
            } if !intent.reissue => {
                return Ok(identity.reversed(document.number(), storno_number));
            }
            LookupOutcome::Reversed { document, .. } => Some(document.number().to_owned()),
            LookupOutcome::Collision(found) => {
                return Ok(
                    identity.conflict_about(ConflictReason::ExternalIdCollision, found.number())
                );
            }
            LookupOutcome::Foreign(found) => {
                return Ok(identity.conflict_about(ConflictReason::Foreign, found.number()));
            }
            LookupOutcome::Absent => None,
        };

        // Step 4: create.
        let outcome = self.create_step(ctx, order, &intent, reversed).await?;

        // Step 5: branch on data.
        Ok(identity
            .respond_to(outcome, &self.config.namespace)
            .map_err(about)?)
    }

    /// Step 3: one read-only durable step under the read policy — the
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
    /// retry and re-send). Any `Err` from the run — exhaustion (500) or
    /// cancellation (409) — is `outcome_unknown` about this document: nothing
    /// is recorded, the next invocation's lookup finds whatever landed.
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
        .map_err(|error| {
            Fault::outcome_unknown(format!(
                "the create step ended without a confirmed outcome ({}): {}; retry with a new Idempotency-Key",
                error.code(),
                error.message()
            ))
            .about(order, Some(kind), intent.identity.external_id.as_str())
            .into()
        })
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;
    use szamlazz_agent::Credentials;

    use super::*;
    use crate::account::{Account, Endpoint};
    use crate::config::{AccountMode, WorkerConfig};
    use crate::contract::TerminalCode;
    use crate::contract::document::tests::sample_document;
    use crate::gateway::Gateway;

    /// An execution as the prologue would build it for the test account.
    fn order() -> Execution {
        let mut account = Account::new("acct", "acct");
        account.mode = AccountMode::Test;
        account.endpoint = Endpoint::parse("http://127.0.0.1:1/").expect("endpoint");
        Execution {
            gateway: Arc::new(
                Gateway::open(account, Credentials::agent_key("key")).expect("gateway"),
            ),
            config: WorkerConfig::new("acct".parse().expect("namespace")),
        }
    }

    fn request(proforma: ProformaLink) -> CreateRequest {
        let mut request = CreateRequest::new(sample_document());
        request.options.proforma = proforma;
        request
    }

    fn invalid_input(fault: Fault) -> String {
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), 400);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], TerminalCode::InvalidInput.as_str());
        body["message"].as_str().expect("message").to_owned()
    }

    fn ord_1() -> OrderKey {
        OrderKey::parse("ORD-1").expect("order")
    }

    /// Step 1's table: the invoice and prepayment chains refuse each other
    /// (`prepaid_chain`), and the final invoice — the prepayment chain's
    /// settled end, live after its `ES` is reversed — refuses both the same
    /// way (#62: without that row, `ES` → `VS` → storno of the `ES` let a
    /// plain `SZ` land beside a live `VS`); a proforma is refused by all
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

    /// `options.proforma` is an invoice option: the Agent cannot carry
    /// `dijbekeroSzamlaszam` on a prepayment invoice, and the other kinds
    /// have nothing to convert.
    #[test]
    fn options_proforma_applies_to_create_invoice_only() {
        let order = order();
        for link in [ProformaLink::None, ProformaLink::Number("D-1".to_owned())] {
            let prepared = order
                .prepare(ord_1(), DocumentKind::Invoice, request(link.clone()))
                .expect("create_invoice accepts options.proforma");
            assert_eq!(prepared.proforma, link);

            for kind in [
                DocumentKind::Prepayment,
                DocumentKind::Proforma,
                DocumentKind::Final,
            ] {
                let fault = order
                    .prepare(ord_1(), kind, request(link.clone()))
                    .err()
                    .unwrap_or_else(|| panic!("create_{kind} must refuse {link:?}"));
                let message = invalid_input(fault);
                assert!(
                    message.contains(&format!("not create_{kind}")),
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

    /// Step 5 (design §5): every settled create outcome as the caller's
    /// response — in particular the two the create step settles when its
    /// leading query or re-query finds something the lookup did not see: a
    /// document reversed since the lookup is `reversed` (never issued past
    /// without `reissue`, ADR 0003), and the lookup's reversed document
    /// reported live is `conflict{live}`.
    #[test]
    fn a_settled_create_step_maps_onto_the_response() {
        use crate::service::tests::{SUPPLIER, found};

        let namespace: Namespace = "acct".parse().expect("namespace");
        let order = OrderKey::parse("ORD-1").expect("order");
        let identity = Identity::of_kind(&namespace, &order, DocumentKind::Invoice);
        let respond = |outcome: CreateOutcome| identity.respond_to(outcome, &namespace);

        let live = found(SUPPLIER, &[]);
        let reversed = found(SUPPLIER, &[("sztornozott", "true")]);

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
            code: "152".to_owned(),
            message: "dup".to_owned(),
            existing_number: Some("SZ-77".to_owned()),
        })
        .expect("data");
        assert_eq!(
            response.conflict_reason,
            Some(ConflictReason::DuplicateOrderNumber)
        );
        assert_eq!(response.existing_number.as_deref(), Some("SZ-77"));
        assert_eq!(response.code.as_deref(), Some("152"));

        let response = respond(CreateOutcome::Rejected {
            code: "259".to_owned(),
            message: "net".to_owned(),
        })
        .expect("data");
        assert_eq!(response.outcome, Outcome::Rejected);
        assert_eq!(response.code.as_deref(), Some("259"));

        let fault = respond(CreateOutcome::CredentialsRejected {
            code: "3".to_owned(),
            message: "login".to_owned(),
        })
        .expect_err("a fault");
        let error = TerminalError::from(fault);
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], TerminalCode::CredentialsRejected.as_str());
    }
}
