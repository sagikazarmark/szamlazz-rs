//! The create protocol (design §5) for the four document kinds and for
//! correctives, in the order the steps appear in the design.
//!
//! Every szamlazz.hu call is a journaled run with `max_attempts(1)`; the
//! handlers keep no state — the external-id pre-query inside every attempt is
//! what finds a document an earlier attempt or invocation issued. Domain
//! outcomes are data and faults are `TerminalError`s.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;
use szamlazz_agent::ops::invoice::CreateInvoice;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::Order;
use super::support::object::{lookup, run_once, sleep, storno_number_of, verify};
use super::support::{Fault, Lookup, next_backoff, order_key};
use crate::config::Namespace;
use crate::contract::response::outstanding;
use crate::contract::{
    ConflictReason, CorrectRequest, CreateRequest, CreateResponse, DocumentInput, DocumentKind,
    IssuedKind, Outcome, ProformaLink, Warning,
};
use crate::gateway::{
    DocumentRefs, InvoiceDocumentExt as _, IssueOutcome, IssueRequest, QueryOutcome,
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

/// One document to issue: what the attempt loop (step 3) needs.
struct Intent {
    identity: Identity,
    create: CreateInvoice,
    reissue: bool,
    /// Run the order-number hint on the first attempt.
    check_hint: bool,
    our_numbers: Vec<String>,
    /// Correctives are exempt from the order-number check: a 71/152 that the
    /// re-query cannot resolve is an ordinary rejection, not a conflict.
    corrective: bool,
}

impl Order {
    // ----- entry points ----------------------------------------------------

    /// `create_proforma` / `create_invoice` / `create_prepayment` /
    /// `create_final`.
    pub(super) async fn issue_kind(
        &self,
        ctx: &ObjectContext<'_>,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<CreateResponse, HandlerError> {
        // Step 0: validate (pure).
        let prepared = self.prepare(ctx.key(), kind, request)?;
        let identity = Identity::of_kind(&self.config.namespace, &prepared.order, kind);
        let mut refs = Refs::default();

        // Step 1: exclusivity.
        match kind {
            DocumentKind::Invoice => {
                if let Some(response) = self
                    .exclusivity(ctx, &prepared, &identity, DocumentKind::Prepayment)
                    .await?
                {
                    return Ok(response);
                }
            }
            DocumentKind::Prepayment => {
                if let Some(response) = self
                    .exclusivity(ctx, &prepared, &identity, DocumentKind::Invoice)
                    .await?
                {
                    return Ok(response);
                }
            }
            DocumentKind::Final => {
                if let Some(response) = self
                    .prepayment_for_final(ctx, &prepared, &identity, &mut refs)
                    .await?
                {
                    return Ok(response);
                }
            }
            DocumentKind::Proforma => {}
        }

        // Step 2: the proforma link. Invoices only: the Agent cannot carry
        // `dijbekeroSzamlaszam` on a prepayment invoice, and the server
        // converts the order's live proforma by shared order number anyway
        // (`docs/szamlazz-hu-behaviour.md`, "Proformas: conversion,
        // auto-linking, deletion"), so a prepayment skips the lookup.
        let mut proforma_linked = false;
        if kind == DocumentKind::Invoice
            && let Some(response) = self
                .proforma_link(ctx, &prepared, &identity, &mut refs, &mut proforma_linked)
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
            check_hint: self.config.issue.detect_foreign || proforma_linked,
            our_numbers: refs.our_numbers,
            corrective: false,
        };
        self.attempt_loop(ctx, &prepared.order, intent).await
    }

    /// `correct_invoice`.
    pub(super) async fn correct(
        &self,
        ctx: &ObjectContext<'_>,
        request: CorrectRequest,
    ) -> Result<CreateResponse, HandlerError> {
        let order = order_key(ctx.key())?;
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
        match verify(ctx, &self.gateway, format!("verify-base-{number}"), &number).await? {
            QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                return Err(Fault::invalid_input(format!(
                    "invoice {number} is not known to szamlazz.hu (not_found)"
                ))
                .into());
            }
            QueryOutcome::Found(found) => {
                if found.info.order_number.as_deref().map(str::trim) != Some(order.as_str()) {
                    return Ok(identity.conflict_about(ConflictReason::NotManaged, number));
                }
                self.check_account(&found)?;
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
            check_hint: false,
            our_numbers: Vec::new(),
            corrective: true,
        };
        self.attempt_loop(ctx, &order, intent).await
    }

    // ----- step 0: validation ----------------------------------------------

    fn prepare(
        &self,
        key: &str,
        kind: DocumentKind,
        request: CreateRequest,
    ) -> Result<Prepared, Fault> {
        let order = order_key(key)?;
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

    /// A document verified by number that carries this order's number must
    /// also belong to the gateway's account.
    fn check_account(&self, found: &InvoiceDocument) -> Result<(), Fault> {
        let account = self.gateway.account();
        if found.account_matches(account.mode.is_test(), account.supplier_id) {
            Ok(())
        } else {
            Err(Fault::account_mismatch(format!(
                "document {} carries this order's number but belongs to another szamlazz.hu account (teszt = {}, supplier {:?})",
                found.number(),
                found.info.test,
                found.supplier.id
            )))
        }
    }

    // ----- step 1: exclusivity ---------------------------------------------

    /// The invoice and prepayment chains are exclusive: a live document of
    /// `other` is `conflict{prepaid_chain}`.
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
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let other_id = ExternalId::for_kind(&self.config.namespace, &prepared.order, other);
        let found = lookup(
            ctx,
            &self.gateway,
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
                Some(identity.conflict_about(ConflictReason::PrepaidChain, found.number()))
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
            &self.gateway,
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
    async fn proforma_link(
        &self,
        ctx: &ObjectContext<'_>,
        prepared: &Prepared,
        identity: &Identity,
        refs: &mut Refs,
        linked: &mut bool,
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let kind = DocumentKind::Proforma;
        match &prepared.proforma {
            ProformaLink::Auto | ProformaLink::None => {
                let proforma_id =
                    ExternalId::for_kind(&self.config.namespace, &prepared.order, kind);
                let found = lookup(
                    ctx,
                    &self.gateway,
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
                *linked = true;
                Ok(None)
            }
            ProformaLink::Number(number) => {
                match verify(
                    ctx,
                    &self.gateway,
                    format!("verify-proforma-{number}"),
                    number,
                )
                .await?
                {
                    QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
                    QueryOutcome::NotFound => Ok(Some(
                        identity.conflict_about(ConflictReason::ProformaMissing, number.clone()),
                    )),
                    QueryOutcome::Found(found) if found.info.document_type != "D" => {
                        Err(Fault::invalid_input(format!(
                            "{number} is not a proforma (tipus {})",
                            found.info.document_type
                        ))
                        .into())
                    }
                    QueryOutcome::Found(found) => {
                        refs.our_numbers.push(found.number().to_owned());
                        refs.proforma = Some(number.clone());
                        *linked = true;
                        Ok(None)
                    }
                }
            }
        }
    }

    // ----- step 3: the attempt loop ----------------------------------------

    async fn attempt_loop(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: Intent,
    ) -> Result<CreateResponse, HandlerError> {
        let max_attempts = self.config.issue.max_attempts.max(1);
        let mut backoff = self.config.issue.first_backoff;

        for attempt in 1..=max_attempts {
            let outcome = self.issue_once(ctx, order, &intent, attempt).await?;
            match outcome {
                IssueOutcome::Transport(_) | IssueOutcome::Unknown { .. } => {
                    if attempt < max_attempts {
                        sleep(ctx, backoff).await?;
                        backoff = next_backoff(backoff, self.config.issue.max_backoff);
                    }
                }
                IssueOutcome::DuplicateOrderNumber { .. }
                    if !intent.corrective && attempt <= 2 && attempt < max_attempts =>
                {
                    // Treated as unknown: the re-executed closure re-queries
                    // the external id after the backoff.
                    sleep(ctx, backoff).await?;
                    backoff = next_backoff(backoff, self.config.issue.max_backoff);
                }
                settled => return self.settle(ctx, order, &intent, settled).await,
            }
        }

        // Step 4: exhausted. Nothing to record: the next invocation's
        // pre-query finds whatever landed.
        Err(Fault::outcome_unknown(format!(
            "{max_attempts} issuing attempts exhausted without a confirmed outcome; retry with a new Idempotency-Key"
        ))
        .about(
            order,
            Some(intent.identity.kind),
            intent.identity.external_id.as_str(),
        )
        .into())
    }

    /// One journaled issuing attempt.
    async fn issue_once(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &Intent,
        attempt: u32,
    ) -> Result<IssueOutcome, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let external_id = intent.identity.external_id.clone();
        let kind = intent.identity.kind;
        let order = order.clone();
        let create = intent.create.clone();
        let reissue = intent.reissue;
        let check_hint = attempt == 1 && intent.check_hint;
        let our_numbers = intent.our_numbers.clone();
        run_once(ctx, format!("issue-{kind}-{attempt}"), move || async move {
            gateway
                .issue(IssueRequest {
                    external_id: &external_id,
                    kind,
                    order: &order,
                    create: &create,
                    reissue,
                    check_hint,
                    our_numbers: &our_numbers,
                })
                .await
        })
        .await
    }

    /// Branches on a settled attempt outcome (step 3, "branch on data").
    async fn settle(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &Intent,
        outcome: IssueOutcome,
    ) -> Result<CreateResponse, HandlerError> {
        let identity = &intent.identity;
        match outcome {
            IssueOutcome::Issued(issued) => {
                // The gateway reports a success without a number as `Unknown`;
                // a bare result here would be a bug, answered as a fault.
                let Some(number) = issued.invoice_number else {
                    return Err(Fault::outcome_unknown(
                        "issued without a document number; retry with a new Idempotency-Key",
                    )
                    .about(order, Some(identity.kind), identity.external_id.as_str())
                    .into());
                };
                let mut response = identity
                    .respond(Outcome::Issued)
                    .with_invoice_number(number.as_str());
                response.net_total = issued.net_total;
                response.gross_total = issued.gross_total;
                response.outstanding = issued.outstanding;
                response.customer_account_url = issued.customer_account_url;
                if issued.notification_delivery_failed {
                    response = response.with_warning(Warning::NotificationDeliveryFailed);
                }
                Ok(response)
            }
            IssueOutcome::Found(found) if intent.reissue => {
                Ok(identity.conflict_about(ConflictReason::Live, found.number()))
            }
            IssueOutcome::Found(found) => Ok(identity.found(Outcome::AlreadyIssued, &found)),
            IssueOutcome::Reconciled(found) => Ok(identity.found(Outcome::Reconciled, &found)),
            IssueOutcome::FoundReversed(found) => {
                let storno_number =
                    storno_number_of(ctx, &self.gateway, order, found.number()).await?;
                Ok(identity.reversed(found.number(), storno_number))
            }
            IssueOutcome::Collision(found) => {
                Ok(identity.conflict_about(ConflictReason::ExternalIdCollision, found.number()))
            }
            IssueOutcome::Foreign(found) => {
                Ok(identity.conflict_about(ConflictReason::Foreign, found.number()))
            }
            IssueOutcome::Rejected { code, message } => Ok(identity.rejected(code, message)),
            IssueOutcome::DuplicateOrderNumber { code, message } if intent.corrective => {
                Ok(identity.rejected(code, message))
            }
            IssueOutcome::DuplicateOrderNumber { code, message } => Ok(identity
                .conflict(ConflictReason::DuplicateOrderNumber)
                .with_code(code)
                .with_message(message)),
            IssueOutcome::Transport(message) | IssueOutcome::Unknown { message, .. } => {
                Err(Fault::outcome_unknown(format!(
                    "issuing attempt ended without a confirmed outcome: {message}; retry with a new Idempotency-Key"
                ))
                .about(order, Some(identity.kind), identity.external_id.as_str())
                .into())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;
    use serde_json::json;

    use super::*;
    use crate::config::Config;
    use crate::contract::TerminalCode;
    use crate::contract::document::tests::sample_document;

    fn order() -> Order {
        let config: Config = serde_json::from_value(json!({
            "account": {
                "slug": "acct",
                "agent_key": "key",
                "endpoint": "http://127.0.0.1:1/",
                "mode": "test",
            },
        }))
        .expect("config");
        Order::new(&config).expect("order")
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

    /// `options.proforma` is an invoice option: the Agent cannot carry
    /// `dijbekeroSzamlaszam` on a prepayment invoice, and the other kinds
    /// have nothing to convert.
    #[test]
    fn options_proforma_applies_to_create_invoice_only() {
        let order = order();
        for link in [ProformaLink::None, ProformaLink::Number("D-1".to_owned())] {
            let prepared = order
                .prepare("ORD-1", DocumentKind::Invoice, request(link.clone()))
                .expect("create_invoice accepts options.proforma");
            assert_eq!(prepared.proforma, link);

            for kind in [
                DocumentKind::Prepayment,
                DocumentKind::Proforma,
                DocumentKind::Final,
            ] {
                let fault = order
                    .prepare("ORD-1", kind, request(link.clone()))
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
                .prepare("ORD-1", kind, request(ProformaLink::Auto))
                .unwrap_or_else(|error| panic!("create_{kind} accepts auto: {error:?}"));
            assert_eq!(prepared.proforma, ProformaLink::Auto);
        }
    }
}
