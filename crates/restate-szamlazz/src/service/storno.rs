//! The storno protocol (design §6), proforma deletion and the `get` live view.

use std::ops::ControlFlow;
use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{ObjectContext, SharedObjectContext};
use szamlazz_agent::ops::query_xml::InvoiceDocument;

use super::prologue::Execution;
use super::support::{Fault, Lookup, order_key};
use super::support::{object, shared};
use crate::contract::{
    ConflictReason, DeleteProformaRequest, DeleteProformaResponse, DocumentKind, DocumentState,
    DocumentStatus, IssuedKind, OrderStatus, StornoOutcome, StornoRequest, StornoResponse,
};
use crate::gateway::{
    DeleteOutcome, InvoiceDocumentExt as _, QueryOutcome, StornoAttempt, StornoLookupOutcome,
    StornoOutcome as GatewayStorno, issued_kind_of,
};
use crate::identity::{ExternalId, OrderKey};

/// What the storno step sends, settled by the verify step.
struct StornoIntent {
    /// The kind of the invoice being reversed, for the fault's identity.
    kind: Option<IssuedKind>,
    /// The invoice to reverse.
    number: String,
    /// `{namespace}:{order}:storno:{number}`.
    storno_id: ExternalId,
    comment: Option<String>,
    e_invoice: bool,
}

impl Execution {
    // ----- storno_invoice (§6) ---------------------------------------------

    pub(super) async fn storno(
        &self,
        ctx: &ObjectContext<'_>,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let order = order_key(ctx.key())?;
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;
        let gateway = &self.gateway;
        let namespace = &self.config.namespace;
        let storno_id = ExternalId::for_storno(namespace, &order, &number);

        // Step 1: verify the document.
        let found = match self
            .verify_for_storno(ctx, &order, &number, &storno_id)
            .await?
        {
            ControlFlow::Continue(found) => found,
            ControlFlow::Break(response) => return Ok(response),
        };
        let kind = issued_kind_of(&found.info.document_type);
        let e_invoice = found
            .e_invoice()
            .unwrap_or(gateway.account().defaults.e_invoice);
        let intent = StornoIntent {
            kind,
            number: number.clone(),
            storno_id,
            comment,
            e_invoice,
        };

        // Step 2: lookup — a storno of ours already under the id.
        match object::lookup_storno(ctx, gateway, &intent.storno_id, &number).await? {
            StornoLookupOutcome::Absent => {}
            StornoLookupOutcome::AlreadyReversed { storno_number } => {
                return Ok(StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno_number));
            }
            StornoLookupOutcome::CredentialsRejected { code, message } => {
                return Err(Fault::credentials_rejected(namespace, code, message)
                    .about(&order, kind, intent.storno_id.as_str())
                    .into());
            }
            StornoLookupOutcome::Transport(message) => {
                return Err(Fault::unavailable(message)
                    .about(&order, kind, intent.storno_id.as_str())
                    .into());
            }
        }

        // Step 3: the storno step, under the issue policy.
        let outcome = self.storno_step(ctx, &order, &intent).await?;

        // Step 4: branch on data.
        Ok(match outcome {
            GatewayStorno::Reversed(storno) => StornoResponse::new(StornoOutcome::Reversed, number)
                .with_storno_number(storno.invoice_number.as_str()),
            GatewayStorno::AlreadyReversed { storno_number } => {
                StornoResponse::new(StornoOutcome::Reversed, number)
                    .with_storno_number(storno_number)
            }
            GatewayStorno::NotStornoable => not_stornoable(number),
            GatewayStorno::Rejected { code, message } => {
                StornoResponse::new(StornoOutcome::Rejected, number)
                    .with_code(code)
                    .with_message(message)
            }
            GatewayStorno::CredentialsRejected { code, message } => {
                return Err(Fault::credentials_rejected(namespace, code, message)
                    .about(&order, kind, intent.storno_id.as_str())
                    .into());
            }
        })
    }

    /// Step 3: one durable step under the issue policy's run retry policy,
    /// query-first on every execution (the query is inside the closure: a
    /// separate journaled pre-query would replay its stale "nothing" on the
    /// retry and re-send). Any `Err` from the run — exhaustion (500) or
    /// cancellation (409) — is `outcome_unknown` about this storno: nothing
    /// is recorded, the next invocation's verify and lookup find whatever
    /// landed.
    async fn storno_step(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        intent: &StornoIntent,
    ) -> Result<GatewayStorno, HandlerError> {
        let gateway = Arc::clone(&self.gateway);
        let number = intent.number.clone();
        let external_id = intent.storno_id.clone();
        let comment = intent.comment.clone();
        let e_invoice = intent.e_invoice;
        object::run_retrying(
            ctx,
            format!("storno-{}", intent.number),
            self.config.issue.run_retry_policy(),
            move || async move {
                gateway
                    .storno(StornoAttempt {
                        invoice_number: &number,
                        external_id: &external_id,
                        comment: comment.as_deref(),
                        e_invoice,
                    })
                    .await
            },
        )
        .await
        .map_err(|error| {
            Fault::outcome_unknown(format!(
                "the storno step ended without a confirmed outcome ({}): {}; retry with a new Idempotency-Key",
                error.code(),
                error.message()
            ))
            .about(order, intent.kind, intent.storno_id.as_str())
            .into()
        })
    }

    /// Step 1 of the storno protocol: the document must be known, carry this
    /// order's number, belong to the gateway's account and be a live invoice
    /// kind. `Break(response)` is the answer for anything that stops the
    /// storno before it is sent (not managed, already reversed, not
    /// stornoable) — a domain outcome, not a fault.
    async fn verify_for_storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        number: &str,
        storno_id: &ExternalId,
    ) -> Result<ControlFlow<StornoResponse, Box<InvoiceDocument>>, HandlerError> {
        let gateway = &self.gateway;
        let namespace = &self.config.namespace;
        let found =
            match object::verify(ctx, gateway, format!("verify-storno-{number}"), number).await? {
                QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
                QueryOutcome::CredentialsRejected { code, message } => {
                    return Err(Fault::credentials_rejected(namespace, code, message)
                        .about(order, None, storno_id.as_str())
                        .into());
                }
                QueryOutcome::NotFound => {
                    return Err(Fault::invalid_input(format!(
                        "invoice {number} is not known to szamlazz.hu (not_found)"
                    ))
                    .into());
                }
                QueryOutcome::Found(found) => found,
            };
        if found.info.order_number.as_deref().map(str::trim) != Some(order.as_str()) {
            return Ok(ControlFlow::Break(
                StornoResponse::new(StornoOutcome::Conflict, number)
                    .with_conflict_reason(ConflictReason::NotManaged),
            ));
        }
        let account = gateway.account();
        if !found.account_matches(account.mode.is_test(), account.supplier_id) {
            return Err(Fault::account_mismatch(format!(
                "document {number} carries this order's number but belongs to another szamlazz.hu account (teszt = {}, supplier {:?})",
                found.info.test, found.supplier.id
            ))
            .into());
        }
        if found.info.reversed == Some(true) {
            // Idempotent: already reversed by anyone.
            let storno_number =
                object::storno_number_of(ctx, gateway, namespace, order, number).await?;
            let mut response = StornoResponse::new(StornoOutcome::Reversed, number);
            response.storno_number = storno_number;
            return Ok(ControlFlow::Break(response));
        }
        if !matches!(found.info.document_type.as_str(), "SZ" | "ES" | "VS" | "HS") {
            return Ok(ControlFlow::Break(not_stornoable(number.to_owned())));
        }
        Ok(ControlFlow::Continue(found))
    }

    // ----- delete_proforma (§6 tail) ---------------------------------------

    pub(super) async fn delete(
        &self,
        ctx: &ObjectContext<'_>,
        request: DeleteProformaRequest,
    ) -> Result<DeleteProformaResponse, HandlerError> {
        let order = order_key(ctx.key())?;
        let kind = DocumentKind::Proforma;
        let proforma_id = ExternalId::for_kind(&self.config.namespace, &order, kind);
        let found = match object::lookup(
            ctx,
            &self.gateway,
            &self.config.namespace,
            "proforma-for-delete",
            &proforma_id,
            &order,
            kind.into(),
        )
        .await?
        {
            // Deleted or consumed — `get` tells which.
            Lookup::Absent => return Ok(DeleteProformaResponse::absent()),
            Lookup::Collision(_) => {
                return Ok(DeleteProformaResponse::not_deleted("external_id_collision"));
            }
            Lookup::Ours(found) => found,
        };
        // The server has no guard against deleting a paid proforma.
        if !found.payments.is_empty() && !request.force {
            return Ok(DeleteProformaResponse::not_deleted("proforma_paid"));
        }

        let number = found.number().to_owned();
        let outcome = {
            let gateway = Arc::clone(&self.gateway);
            let number = number.clone();
            object::run_once(
                ctx,
                format!("delete-proforma-{number}"),
                move || async move { gateway.delete_proforma(&number).await },
            )
            .await?
        };
        match outcome {
            DeleteOutcome::Deleted | DeleteOutcome::AlreadyGone => {
                Ok(DeleteProformaResponse::deleted())
            }
            DeleteOutcome::Rejected { code, .. } => Ok(DeleteProformaResponse::not_deleted(code)),
            DeleteOutcome::CredentialsRejected { code, message } => Err(
                Fault::credentials_rejected(&self.config.namespace, code, message)
                    .about(&order, Some(IssuedKind::Proforma), proforma_id.as_str())
                    .into(),
            ),
            DeleteOutcome::Transport(message) => Err(Fault::outcome_unknown(format!(
                "proforma deletion outcome unknown: {message}; retry with a new Idempotency-Key"
            ))
            .about(&order, Some(IssuedKind::Proforma), proforma_id.as_str())
            .into()),
        }
    }

    // ----- get -------------------------------------------------------------

    /// The live view: what szamlazz.hu holds under the order's four external
    /// ids right now (design §6).
    ///
    /// A collision under an id leaves its slot `None`: a read must not fail,
    /// and the issuing handlers are the ones that refuse it.
    pub(super) async fn status(
        &self,
        ctx: &SharedObjectContext<'_>,
    ) -> Result<OrderStatus, HandlerError> {
        let order = order_key(ctx.key())?;
        let mut status = OrderStatus::default();
        for kind in DocumentKind::ALL {
            let external_id = ExternalId::for_kind(&self.config.namespace, &order, kind);
            let found = shared::lookup(
                ctx,
                &self.gateway,
                &self.config.namespace,
                format!("get-{kind}"),
                &external_id,
                &order,
                kind.into(),
            )
            .await?;
            match found {
                Lookup::Ours(found) => status.set(kind, Some(document_status(&found))),
                Lookup::Absent | Lookup::Collision(_) => {}
            }
        }
        // A proforma szamlazz.hu no longer returns while an invoice or
        // prepayment references it was consumed by that document.
        if status.proforma.is_none()
            && let Some(consumer) = [&status.invoice, &status.prepayment]
                .into_iter()
                .flatten()
                .find(|document| document.referenced_proforma.is_some())
            && let Some(proforma) = &consumer.referenced_proforma
        {
            status.proforma = Some(DocumentStatus::new(
                proforma,
                DocumentState::Consumed {
                    by: consumer.number.clone(),
                },
            ));
        }
        Ok(status)
    }
}

/// The `get` projection of a document of ours.
fn document_status(found: &InvoiceDocument) -> DocumentStatus {
    let state = if found.is_live() {
        DocumentState::Live
    } else {
        DocumentState::Reversed {
            storno_number: None,
        }
    };
    let mut status = DocumentStatus::new(found.number(), state);
    status.gross = Some(found.totals.total.gross);
    status.net = Some(found.totals.total.net);
    status.payments = found.payment_amounts();
    status.referenced_proforma = found
        .info
        .referenced_proforma_number
        .as_ref()
        .map(|number| number.as_str().to_owned());
    status.e_invoice = found.e_invoice();

    status
}

/// The answer for a document szamlazz.hu cannot reverse (proformas, delivery
/// notes, stornos).
fn not_stornoable(number: String) -> StornoResponse {
    StornoResponse::new(StornoOutcome::Rejected, number)
        .with_code("not_stornoable")
        .with_message("the document cannot be reversed: only invoices can be stornoed")
}
