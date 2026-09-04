//! The storno protocol (design §6), proforma deletion and the `get` live view.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{ObjectContext, SharedObjectContext};

use super::Order;
use super::support::{Fault, Lookup, account_matches, next_backoff, order_key};
use super::support::{object, shared};
use crate::contract::{
    ConflictReason, DeleteProformaRequest, DeleteProformaResponse, DocumentKind, DocumentState,
    DocumentStatus, IssuedKind, OrderStatus, StornoOutcome, StornoRequest, StornoResponse,
};
use crate::identity::ExternalId;
use crate::steps::{
    DeleteOutcome, FoundDocument, QueryOutcome, StornoAttempt, StornoDocument,
    StornoOutcome as StepsStorno, issued_kind_of,
};

/// Storno re-send attempts per invocation (design §6 step 2).
const STORNO_ATTEMPTS: u32 = 3;

impl Order {
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

        // Step 1: verify the document.
        let found =
            match object::verify(ctx, &self.steps, format!("verify-storno-{number}"), &number)
                .await?
            {
                QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
                QueryOutcome::NotFound => {
                    return Err(Fault::invalid_input(format!(
                        "invoice {number} is not known to szamlazz.hu (not_found)"
                    ))
                    .into());
                }
                QueryOutcome::Found(found) => found,
            };
        if found.order_number.as_deref().map(str::trim) != Some(order.as_str()) {
            return Ok(StornoResponse::new(StornoOutcome::Conflict, number)
                .with_conflict_reason(ConflictReason::NotManaged));
        }
        let account = &self.config.account;
        if !account_matches(&found, account.mode.is_test(), account.supplier_id) {
            return Err(Fault::account_mismatch(format!(
                "document {number} carries this order's number but belongs to another szamlazz.hu account (teszt = {}, supplier {:?})",
                found.test, found.supplier_id
            ))
            .into());
        }
        if found.reversed == Some(true) {
            // Idempotent: already reversed by anyone.
            let storno_number = object::storno_number_of(ctx, &self.steps, &order, &number).await?;
            let mut response = StornoResponse::new(StornoOutcome::Reversed, number);
            response.storno_number = storno_number;
            return Ok(response);
        }
        if !matches!(found.document_type.as_str(), "SZ" | "ES" | "VS" | "HS") {
            return Ok(not_stornoable(number));
        }
        let e_invoice = found.e_invoice.unwrap_or(self.config.defaults.e_invoice);

        // Step 2: the query-first re-send loop.
        let storno_id = ExternalId::for_storno(&self.config.account.slug, &order, &number);
        let mut backoff = self.config.issue.first_backoff;
        for attempt in 1..=STORNO_ATTEMPTS {
            let outcome = {
                let steps = Arc::clone(&self.steps);
                let number = number.clone();
                let storno_id = storno_id.clone();
                let comment = comment.clone();
                object::run_once(
                    ctx,
                    format!("storno-{number}-{attempt}"),
                    move || async move {
                        steps
                            .storno(StornoAttempt {
                                invoice_number: &number,
                                external_id: &storno_id,
                                comment: comment.as_deref(),
                                e_invoice,
                            })
                            .await
                    },
                )
                .await?
            };
            // Step 3.
            return Ok(match outcome {
                StepsStorno::Reversed(StornoDocument { storno_number, .. })
                | StepsStorno::AlreadyReversed { storno_number } => {
                    StornoResponse::new(StornoOutcome::Reversed, number)
                        .with_storno_number(storno_number)
                }
                StepsStorno::NotStornoable => not_stornoable(number),
                StepsStorno::Rejected { code, message } => {
                    StornoResponse::new(StornoOutcome::Rejected, number)
                        .with_code(code)
                        .with_message(message)
                }
                StepsStorno::Unknown { .. } | StepsStorno::Transport(_) => {
                    if attempt < STORNO_ATTEMPTS {
                        object::sleep(ctx, backoff).await?;
                        backoff = next_backoff(backoff, self.config.issue.max_backoff);
                    }
                    continue;
                }
            });
        }

        // Exhausted: the next call is safe — step 1 and the storno pre-query
        // find the storno if it landed.
        Err(Fault::outcome_unknown(format!(
            "{STORNO_ATTEMPTS} storno attempts exhausted without confirmation; retry with a new Idempotency-Key"
        ))
        .about(
            &order,
            issued_kind_of(&found.document_type),
            storno_id.as_str(),
        )
        .into())
    }

    // ----- delete_proforma (§6 tail) ---------------------------------------

    pub(super) async fn delete(
        &self,
        ctx: &ObjectContext<'_>,
        request: DeleteProformaRequest,
    ) -> Result<DeleteProformaResponse, HandlerError> {
        let order = order_key(ctx.key())?;
        let kind = DocumentKind::Proforma;
        let proforma_id = ExternalId::for_kind(&self.config.account.slug, &order, kind);
        let found = match object::lookup(
            ctx,
            &self.steps,
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

        let number = found.number;
        let outcome = {
            let steps = Arc::clone(&self.steps);
            let number = number.clone();
            object::run_once(
                ctx,
                format!("delete-proforma-{number}"),
                move || async move { steps.delete_proforma(&number).await },
            )
            .await?
        };
        match outcome {
            DeleteOutcome::Deleted | DeleteOutcome::AlreadyGone => {
                Ok(DeleteProformaResponse::deleted())
            }
            DeleteOutcome::Rejected { code, .. } => Ok(DeleteProformaResponse::not_deleted(code)),
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
            let external_id = ExternalId::for_kind(&self.config.account.slug, &order, kind);
            let found = shared::lookup(
                ctx,
                &self.steps,
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
fn document_status(found: &FoundDocument) -> DocumentStatus {
    let state = if found.is_live() {
        DocumentState::Live
    } else {
        DocumentState::Reversed {
            storno_number: None,
        }
    };
    let mut status = DocumentStatus::new(found.number.clone(), state);
    status.gross = found.gross;
    status.net = found.net;
    status.payments.clone_from(&found.payments);
    status
        .referenced_proforma
        .clone_from(&found.referenced_proforma);
    status.e_invoice = found.e_invoice;
    status
}

/// The answer for a document szamlazz.hu cannot reverse (proformas, delivery
/// notes, stornos).
fn not_stornoable(number: String) -> StornoResponse {
    StornoResponse::new(StornoOutcome::Rejected, number)
        .with_code("not_stornoable")
        .with_message("the document cannot be reversed: only invoices can be stornoed")
}
