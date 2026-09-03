//! The storno protocol (design §7), proforma deletion, the `get` snapshot
//! and the operator handlers.

use std::sync::Arc;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{ObjectContext, SharedObjectContext};

use super::Order;
use super::support::{
    Fault, ProformaFate, expected_supplier, learn_supplier, next_backoff, order_key,
    remaining_backoff, validate_found,
};
use super::support::{object, shared};
use crate::contract::{
    ConflictReason, DeleteProformaRequest, DeleteProformaResponse, DocumentKind,
    DocumentVerification, ForgetRequest, Freshness, GetRequest, IssuedKind, OrderSnapshot,
    RecordReversalRequest, RecordedReversal, StornoOutcome, StornoRequest, StornoResponse,
    VerificationResult,
};
use crate::identity::ExternalId;
use crate::ledger::{
    CommittedDocument, Ledger, Origin, Reversal, ReversalOrigin, SlotStatus, Target,
};
use crate::steps::{
    DeleteOutcome, FoundDocument, QueryOutcome, StornoAttempt, StornoDocument,
    StornoOutcome as StepsStorno,
};

/// Storno re-send attempts per invocation (design §7 step 3).
const STORNO_ATTEMPTS: u32 = 3;

impl Order {
    // ----- storno_invoice (§7) ---------------------------------------------

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
        let mut ledger = object::load(ctx).await?;

        // Step 1: locate and pre-check.
        let Located {
            target,
            status,
            kind,
            external_id,
        } = match locate_for_storno(&self.config, &order, &ledger, &number)? {
            Ok(located) => located,
            Err(response) => return Ok(response),
        };
        let retrying = status == SlotStatus::ReversalUnverified;

        // Step 2: verify live; capture the payments for the history event.
        let (payments, e_invoice) =
            match object::verify(ctx, &self.steps, format!("verify-storno-{number}"), &number)
                .await?
            {
                QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
                QueryOutcome::NotFound => {
                    return Ok(StornoResponse::new(StornoOutcome::Conflict, number)
                        .with_conflict_reason(ConflictReason::RecordedDocumentMissing));
                }
                QueryOutcome::Found(found) => {
                    learn_supplier(&mut ledger, &found)?;
                    if found.reversed == Some(true) {
                        let by = self.storno_number_of(ctx, &order, &number).await?;
                        let origin = if retrying {
                            ReversalOrigin::Service
                        } else {
                            ReversalOrigin::External
                        };
                        let mut reversal =
                            Reversal::new(origin).with_payments_before(found.payments.clone());
                        reversal.by.clone_from(&by);
                        ledger
                            .mark_reversed(&target, reversal)
                            .map_err(Fault::from)?;
                        object::save(ctx, &ledger);
                        let mut response = StornoResponse::new(StornoOutcome::Reversed, number);
                        response.storno_number = by;
                        return Ok(response);
                    }
                    object::save(ctx, &ledger);
                    let e_invoice = found.e_invoice.unwrap_or(self.config.defaults.e_invoice);
                    (found.payments, e_invoice)
                }
            };

        // Steps 3–4: idempotent re-send loop with the query-first guard.
        let send = StornoSend {
            target: &target,
            number: &number,
            storno_id: external_id.storno_of(),
            comment: comment.as_deref(),
            e_invoice,
            payments,
        };
        if let Some(response) = self.storno_loop(ctx, &mut ledger, send).await? {
            return Ok(response);
        }

        // Exhausted: transient — the next storno_invoice retries.
        if !retrying {
            ledger
                .mark_reversal_unverified(&target)
                .map_err(Fault::from)?;
            object::save(ctx, &ledger);
        }
        Err(Fault::outcome_unknown(format!(
            "{STORNO_ATTEMPTS} storno attempts exhausted without confirmation (reversal_unverified); call storno_invoice again"
        ))
        .about(&order, kind, external_generation(&external_id), external_id.as_str(), None)
        .into())
    }

    /// Steps 3–4: up to [`STORNO_ATTEMPTS`] journaled storno attempts; `None`
    /// when every attempt ended without a confirmed outcome.
    async fn storno_loop(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        send: StornoSend<'_>,
    ) -> Result<Option<StornoResponse>, HandlerError> {
        let StornoSend {
            target,
            number,
            storno_id,
            comment,
            e_invoice,
            payments,
        } = send;
        let mut backoff = self.config.issue.first_backoff;
        for attempt in 1..=STORNO_ATTEMPTS {
            let outcome = {
                let steps = Arc::clone(&self.steps);
                let number = number.to_owned();
                let storno_id = storno_id.clone();
                let comment = comment.map(str::to_owned);
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
            // Step 4.
            let response = match outcome {
                StepsStorno::Reversed(StornoDocument { storno_number, .. })
                | StepsStorno::AlreadyReversed { storno_number } => {
                    ledger
                        .mark_reversed(
                            target,
                            Reversal::new(ReversalOrigin::Service)
                                .with_by(storno_number.clone())
                                .with_payments_before(payments),
                        )
                        .map_err(Fault::from)?;
                    object::save(ctx, ledger);
                    StornoResponse::new(StornoOutcome::Reversed, number)
                        .with_storno_number(storno_number)
                }
                StepsStorno::NotStornoable => StornoResponse::new(StornoOutcome::Rejected, number)
                    .with_code("not_stornoable")
                    .with_message(
                        "szamlazz.hu echoed the document unchanged: it cannot be reversed",
                    ),
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
            };
            return Ok(Some(response));
        }
        Ok(None)
    }

    /// The storno number of an externally reversed document: the order-number
    /// hint when it is the `SS` referencing it.
    async fn storno_number_of(
        &self,
        ctx: &ObjectContext<'_>,
        order: &crate::identity::OrderKey,
        number: &str,
    ) -> Result<Option<String>, HandlerError> {
        Ok(
            match object::hint(ctx, &self.steps, format!("hint-storno-{number}"), order).await? {
                QueryOutcome::Found(found)
                    if found.document_type == "SS"
                        && found.referenced_invoice.as_deref() == Some(number) =>
                {
                    Some(found.number)
                }
                _ => None,
            },
        )
    }

    // ----- delete_proforma (§7 tail) ---------------------------------------

    pub(super) async fn delete(
        &self,
        ctx: &ObjectContext<'_>,
        request: DeleteProformaRequest,
    ) -> Result<DeleteProformaResponse, HandlerError> {
        let order = order_key(ctx.key())?;
        let mut ledger = object::load(ctx).await?;
        let kind = DocumentKind::Proforma;
        let Some(slot) = ledger.slot(kind).cloned() else {
            return Ok(DeleteProformaResponse::not_deleted("no_proforma"));
        };
        let external_id =
            ExternalId::for_slot(&self.config.account.slug, &order, kind, slot.generation);

        let number = match &slot.status {
            SlotStatus::Pending => {
                match self
                    .reconcile_pending_proforma(ctx, &mut ledger, &order, &slot, &external_id)
                    .await?
                {
                    Ok(number) => number,
                    Err(response) => return Ok(response),
                }
            }
            SlotStatus::Committed => match &slot.number {
                Some(number) => number.clone(),
                None => {
                    return Err(Fault::invalid_input(
                        "ledger inconsistent: committed proforma without a number",
                    )
                    .into());
                }
            },
            other => return Ok(undeletable(other)?),
        };

        // Committed: pre-query the payments.
        match object::verify(
            ctx,
            &self.steps,
            format!("verify-proforma-delete-{number}"),
            &number,
        )
        .await?
        {
            QueryOutcome::Transport(message) => return Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => {
                match object::proforma_fate(ctx, &self.steps, &order, &number).await? {
                    ProformaFate::Consumed(consumer) => {
                        learn_supplier(&mut ledger, &consumer)?;
                        ledger.mark_consumed(consumer.number).map_err(Fault::from)?;
                        object::save(ctx, &ledger);
                        return Ok(DeleteProformaResponse::not_deleted("proforma_consumed"));
                    }
                    ProformaFate::Deleted => {
                        ledger.mark_deleted().map_err(Fault::from)?;
                        object::save(ctx, &ledger);
                        return Ok(DeleteProformaResponse::deleted());
                    }
                }
            }
            QueryOutcome::Found(found) => {
                learn_supplier(&mut ledger, &found)?;
                object::save(ctx, &ledger);
                if !found.payments.is_empty() && !request.force {
                    return Ok(DeleteProformaResponse::not_deleted("proforma_paid"));
                }
            }
        }

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
                ledger.mark_deleted().map_err(Fault::from)?;
                object::save(ctx, &ledger);
                Ok(DeleteProformaResponse::deleted())
            }
            DeleteOutcome::Rejected { code, .. } => Ok(DeleteProformaResponse::not_deleted(code)),
            DeleteOutcome::Transport(message) => Err(Fault::outcome_unknown(format!(
                "proforma deletion outcome unknown: {message}; call delete_proforma again"
            ))
            .about(
                &order,
                IssuedKind::Proforma,
                slot.generation,
                external_id.as_str(),
                Some(&slot.request_id),
            )
            .into()),
        }
    }

    /// A `pending` proforma before deletion: pre-sleep, reconcile by external
    /// id and commit what is found; `Err` carries the answer when nothing can
    /// be deleted.
    async fn reconcile_pending_proforma(
        &self,
        ctx: &ObjectContext<'_>,
        ledger: &mut Ledger,
        order: &crate::identity::OrderKey,
        slot: &crate::ledger::Slot,
        external_id: &ExternalId,
    ) -> Result<Result<String, DeleteProformaResponse>, HandlerError> {
        if let Some(last) = slot.last_attempt_at {
            let now = object::now(ctx).await?;
            object::sleep(
                ctx,
                remaining_backoff(self.config.issue.first_backoff, last, now),
            )
            .await?;
        }
        let outcome = object::query_external_id(
            ctx,
            &self.steps,
            format!("reconcile-proforma-{}", slot.generation),
            external_id,
        )
        .await?;
        match outcome {
            QueryOutcome::Transport(message) => Err(Fault::unavailable(message).into()),
            QueryOutcome::NotFound => Ok(Err(DeleteProformaResponse::not_deleted("pending"))),
            QueryOutcome::Found(found) => {
                if validate_found(
                    &found,
                    order,
                    IssuedKind::Proforma,
                    &self.config,
                    expected_supplier(&self.config, ledger),
                )
                .is_err()
                {
                    return Ok(Err(DeleteProformaResponse::not_deleted(
                        "external_id_collision",
                    )));
                }
                learn_supplier(ledger, &found)?;
                ledger
                    .commit(
                        &Target::Slot(DocumentKind::Proforma),
                        CommittedDocument::reconciled(found.number.clone(), Origin::Service)
                            .with_totals(found.gross, found.net)
                            .with_test(found.test),
                    )
                    .map_err(Fault::from)?;
                object::save(ctx, ledger);
                Ok(Ok(found.number))
            }
        }
    }

    // ----- get -------------------------------------------------------------

    /// The snapshot; with `verify`, every committed document is checked live
    /// (read-only: the ledger is not updated, the findings are reported).
    pub(super) async fn snapshot(
        &self,
        ctx: &SharedObjectContext<'_>,
        request: GetRequest,
    ) -> Result<OrderSnapshot, HandlerError> {
        let ledger = shared::load(ctx).await?;
        if !request.verify {
            return Ok(ledger.snapshot(Freshness::Snapshot));
        }
        let mut verification = Vec::new();
        for kind in DocumentKind::ALL {
            if let Some(slot) = ledger.slot(kind)
                && matches!(
                    slot.status,
                    SlotStatus::Committed | SlotStatus::ReversalUnverified
                )
                && let Some(number) = &slot.number
            {
                let result = self.verify_shared(ctx, number).await?;
                verification.push(DocumentVerification::new(
                    kind.into(),
                    slot.generation,
                    number.clone(),
                    result,
                ));
            }
        }
        for entry in ledger.correctives() {
            if matches!(
                entry.status,
                SlotStatus::Committed | SlotStatus::ReversalUnverified
            ) && let Some(number) = &entry.number
            {
                let result = self.verify_shared(ctx, number).await?;
                verification.push(DocumentVerification::new(
                    IssuedKind::Corrective,
                    entry.cseq,
                    number.clone(),
                    result,
                ));
            }
        }
        let mut snapshot = ledger.snapshot(Freshness::Live);
        snapshot.verification = verification;
        Ok(snapshot)
    }

    async fn verify_shared(
        &self,
        ctx: &SharedObjectContext<'_>,
        number: &str,
    ) -> Result<VerificationResult, HandlerError> {
        Ok(
            match shared::verify(ctx, &self.steps, format!("verify-{number}"), number).await? {
                QueryOutcome::Found(FoundDocument {
                    reversed: Some(true),
                    ..
                }) => VerificationResult::Reversed,
                QueryOutcome::Found(_) => VerificationResult::Live,
                QueryOutcome::NotFound => VerificationResult::Missing,
                QueryOutcome::Transport(_) => VerificationResult::Unavailable,
            },
        )
    }

    // ----- operator handlers -----------------------------------------------

    pub(super) async fn record(
        &self,
        ctx: &ObjectContext<'_>,
        request: RecordReversalRequest,
    ) -> Result<OrderSnapshot, HandlerError> {
        order_key(ctx.key())?;
        let mut ledger = object::load(ctx).await?;
        match request.result {
            RecordedReversal::Reversed { storno_number } => {
                ledger
                    .record_operator_reversal(&request.invoice_number, storno_number)
                    .map_err(|error| Fault::invalid_input(error.to_string()))?;
            }
            RecordedReversal::Live => {
                ledger
                    .record_operator_live(&request.invoice_number)
                    .map_err(|error| Fault::invalid_input(error.to_string()))?;
            }
        }
        object::save(ctx, &ledger);
        Ok(ledger.snapshot(Freshness::Snapshot))
    }

    pub(super) async fn forget_slot(
        &self,
        ctx: &ObjectContext<'_>,
        request: ForgetRequest,
    ) -> Result<OrderSnapshot, HandlerError> {
        order_key(ctx.key())?;
        let mut ledger = object::load(ctx).await?;
        ledger
            .forget(request.kind)
            .map_err(|error| Fault::invalid_input(error.to_string()))?;
        object::save(ctx, &ledger);
        Ok(ledger.snapshot(Freshness::Snapshot))
    }
}

/// The status, kind and external id of a slot or corrective entry.
fn entry_facts(
    config: &crate::config::Config,
    order: &crate::identity::OrderKey,
    ledger: &Ledger,
    target: &Target,
) -> Result<(SlotStatus, IssuedKind, ExternalId), Fault> {
    let slug = &config.account.slug;
    match target {
        Target::Slot(kind) => ledger
            .slot(*kind)
            .map(|slot| {
                (
                    slot.status.clone(),
                    IssuedKind::from(*kind),
                    ExternalId::for_slot(slug, order, *kind, slot.generation),
                )
            })
            .ok_or_else(|| Fault::invalid_input(format!("no {kind} slot"))),
        Target::Corrective(id) => ledger
            .corrective(id)
            .map(|entry| {
                (
                    entry.status.clone(),
                    IssuedKind::Corrective,
                    ExternalId::for_corrective(slug, order, entry.cseq),
                )
            })
            .ok_or_else(|| Fault::invalid_input(format!("no corrective for request {id}"))),
    }
}

/// The generation embedded in a slot or corrective external id.
fn external_generation(external_id: &ExternalId) -> u32 {
    external_id
        .as_str()
        .rsplit(':')
        .next()
        .and_then(|generation| generation.parse().ok())
        .unwrap_or(0)
}

/// What the storno re-send loop needs.
struct StornoSend<'a> {
    target: &'a Target,
    number: &'a str,
    storno_id: ExternalId,
    comment: Option<&'a str>,
    e_invoice: bool,
    /// The credit entries captured before the reversal wipes them.
    payments: Vec<rust_decimal::Decimal>,
}

/// Step 1 of the storno protocol: the entry carrying `number`, or the answer
/// when nothing is to be sent.
fn locate_for_storno(
    config: &crate::config::Config,
    order: &crate::identity::OrderKey,
    ledger: &Ledger,
    number: &str,
) -> Result<Result<Located, StornoResponse>, Fault> {
    let Some(target) = ledger.find_by_number(number) else {
        return Err(Fault::invalid_input(format!(
            "invoice {number} is not managed by this order (not_managed); use Szamlazz.Agent.storno"
        )));
    };
    if target == Target::Slot(DocumentKind::Proforma) {
        return Err(Fault::invalid_input(format!(
            "{number} is a proforma; it cannot be reversed, use delete_proforma"
        )));
    }
    if ledger.correctives().iter().any(|entry| {
        entry.corrected_number == number
            && matches!(
                entry.status,
                SlotStatus::Pending | SlotStatus::Committed | SlotStatus::ReversalUnverified
            )
    }) {
        return Ok(Err(StornoResponse::new(StornoOutcome::Rejected, number)
            .with_code("221")
            .with_message(
                "the invoice has a corrective invoice (has_corrective)",
            )));
    }
    let (status, kind, external_id) = entry_facts(config, order, ledger, &target)?;
    match &status {
        SlotStatus::Reversed { by, .. } => {
            let mut response = StornoResponse::new(StornoOutcome::Reversed, number);
            response.storno_number.clone_from(by);
            Ok(Err(response))
        }
        SlotStatus::Pending => Ok(Err(StornoResponse::new(StornoOutcome::Conflict, number)
            .with_conflict_reason(ConflictReason::Pending))),
        SlotStatus::Committed | SlotStatus::ReversalUnverified => Ok(Ok(Located {
            target,
            status,
            kind,
            external_id,
        })),
        other => Err(Fault::invalid_input(format!(
            "{number} is {other}; nothing to reverse"
        ))),
    }
}

/// The answer for a proforma slot that is neither `pending` nor `committed`.
fn undeletable(status: &SlotStatus) -> Result<DeleteProformaResponse, Fault> {
    match status {
        SlotStatus::Deleted => Ok(DeleteProformaResponse::deleted()),
        SlotStatus::Consumed { .. } => Ok(DeleteProformaResponse::not_deleted("proforma_consumed")),
        SlotStatus::Rejected { .. } | SlotStatus::Vacant => {
            Ok(DeleteProformaResponse::not_deleted("no_proforma"))
        }
        SlotStatus::Blocked { .. } => Ok(DeleteProformaResponse::not_deleted("blocked")),
        SlotStatus::Pending
        | SlotStatus::Committed
        | SlotStatus::Reversed { .. }
        | SlotStatus::ReversalUnverified => Err(Fault::invalid_input(format!(
            "ledger inconsistent: proforma slot is {status}"
        ))),
    }
}

/// The entry a storno request is about.
struct Located {
    target: Target,
    status: SlotStatus,
    kind: IssuedKind,
    external_id: ExternalId,
}
