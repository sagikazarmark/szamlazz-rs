//! The create protocol (design §5) for the four document kinds and for
//! correctives, in the order the steps appear in the design.
//!
//! The handlers keep no state. After validation and the reference checks,
//! issuing is two durable steps: a read-only **lookup** (`lookup-{kind}`) that
//! settles every case needing no create, and a **create** (`create-{kind}`)
//! under the issue policy's run retry policy, query-first on every execution
//! — the external-id query inside the create closure is what finds a document
//! an earlier execution issued. Domain outcomes are data and faults are
//! `TerminalError`s; a create step that ends without a settled outcome is
//! `outcome_unknown`.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::ObjectContext;
use szamlazz_agent::ops::invoice::CreateInvoice;
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::Order;
use super::support::object::{lookup, run_once, run_retrying, verify};
use super::support::{Fault, Lookup, order_key};
use crate::config::Namespace;
use crate::contract::response::outstanding;
use crate::contract::{
    ConflictReason, CorrectRequest, CreateRequest, CreateResponse, DocumentInput, DocumentKind,
    IssuedKind, Outcome, ProformaLink, Warning,
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
            QueryOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message)
                        .about(&order, Some(identity.kind), identity.external_id.as_str())
                        .into(),
                );
            }
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
            our_numbers: Vec::new(),
        };
        self.issue(ctx, &order, intent).await
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
            &self.config.namespace,
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
            &self.config.namespace,
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
    ) -> Result<Option<CreateResponse>, HandlerError> {
        let kind = DocumentKind::Proforma;
        match &prepared.proforma {
            ProformaLink::Auto | ProformaLink::None => {
                let proforma_id =
                    ExternalId::for_kind(&self.config.namespace, &prepared.order, kind);
                let found = lookup(
                    ctx,
                    &self.gateway,
                    &self.config.namespace,
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
                match verify(
                    ctx,
                    &self.gateway,
                    format!("verify-proforma-{number}"),
                    number,
                )
                .await?
                {
                    QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
                    QueryOutcome::CredentialsRejected { code, message } => Err(
                        Fault::credentials_rejected(&self.config.namespace, code, message)
                            .about(
                                &prepared.order,
                                Some(identity.kind),
                                identity.external_id.as_str(),
                            )
                            .into(),
                    ),
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

        // Step 3: lookup.
        let reversed = match self.lookup_step(ctx, order, &intent).await? {
            LookupOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
            LookupOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message)
                        .about(order, Some(identity.kind), identity.external_id.as_str())
                        .into(),
                );
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
        Ok(match outcome {
            CreateOutcome::Issued(issued) => {
                // The gateway reports `Issued` only with a number; a bare
                // result here would be a bug, answered as a fault.
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
                response
            }
            // An earlier execution of the step created it (ADR 0003): the
            // caller asked for this document and has it.
            CreateOutcome::Found(found) => identity.found(Outcome::Issued, &found),
            CreateOutcome::Reconciled(found) => identity.found(Outcome::Reconciled, &found),
            CreateOutcome::Collision(found) => {
                identity.conflict_about(ConflictReason::ExternalIdCollision, found.number())
            }
            CreateOutcome::DuplicateOrderNumber {
                code,
                message,
                existing_number,
            } => {
                let mut response = identity
                    .conflict(ConflictReason::DuplicateOrderNumber)
                    .with_code(code)
                    .with_message(message);
                response.existing_number = existing_number;
                response
            }
            CreateOutcome::Rejected { code, message } => identity.rejected(code, message),
            CreateOutcome::CredentialsRejected { code, message } => {
                return Err(
                    Fault::credentials_rejected(&self.config.namespace, code, message)
                        .about(order, Some(identity.kind), identity.external_id.as_str())
                        .into(),
                );
            }
        })
    }

    /// Step 3: one read-only durable step — the external id and, for every
    /// kind but correctives, the order-number hint.
    async fn lookup_step(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &Intent,
    ) -> Result<LookupOutcome, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let external_id = intent.identity.external_id.clone();
        let kind = intent.identity.kind;
        let order = order.clone();
        let our_numbers = intent.our_numbers.clone();
        run_once(ctx, format!("lookup-{kind}"), move || async move {
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
