//! The *Storno invoice* protocol (ADR 0007), shared by `Szamlazz.Order`'s
//! `storno_invoice` and `Szamlazz.Agent`'s `storno`: verify the original by
//! number, decide on it ([`StornoVerdict`]), build the intent from what the
//! verify found ([`StornoIntent`]: the storno repeats the original's `telj`
//! and lifts its `eszamla`, never a caller's), look the storno external id
//! up, send the storno query-first under the issue policy, and answer from
//! data.
//!
//! The two handlers differ in their verdict (the order's handler acts on
//! nothing but its own documents and refuses what szamlazz.hu cannot reverse
//! before a send; the by-number one redirects a managed document to its
//! order and lets szamlazz.hu's echo tell), in the storno external id
//! (`{namespace}:{order}:storno:{number}` or
//! `{namespace}:by-number:{number}:storno`) and in the best-effort read that
//! names an existing storno (the order-number hint, or the by-number storno
//! lookup). Everything else is the one protocol below; the two `Execution`
//! methods at the end are its shells.

use std::ops::ControlFlow;
use std::sync::Arc;

use restate_sdk::errors::{HandlerError, TerminalError};
use restate_sdk::prelude::{Context, ObjectContext};
use szamlazz_agent::Date;

use super::prologue::Execution;
use super::support::{
    AnsweredCode, Fault, RunCtx, run_best_effort, run_reading, run_retrying, verified_document,
    verify,
};
use crate::account::Account;
use crate::contract::{ConflictReason, IssuedKind, StornoOutcome, StornoRequest, StornoResponse};
use crate::gateway::{self, FoundDocument, QueryOutcome, StornoLookupOutcome, StornoStepRequest};
use crate::identity::{ExternalId, Namespace, OrderKey};

/// What the storno step sends, built from what the verify step found.
/// Shared by `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`,
/// whose storno external ids differ.
#[derive(Debug, Clone)]
struct StornoIntent {
    /// The invoice to reverse.
    number: String,
    /// `{namespace}:{order}:storno:{number}` or
    /// `{namespace}:by-number:{number}:storno`.
    storno_id: ExternalId,
    comment: Option<String>,
    /// The verified document's `eszamla` when known, else the account
    /// default: an open code set for which the account's own default is a
    /// legitimate choice.
    e_invoice: bool,
    /// The verified document's `telj`, which the storno repeats as its
    /// `teljesitesDatum`: a fiscal fact of the document for which
    /// no default can be right, so it is never defaulted.
    fulfillment_date: Date,
}

impl StornoIntent {
    /// The intent for reversing the verified `found` (`number`, as the
    /// caller named it) under `storno_id`: `e_invoice` lifted from the
    /// document with `account`'s default as fallback, `fulfillment_date` the
    /// document's own `telj`. A pure function of the journaled verify result,
    /// so every execution rebuilds the same request.
    ///
    /// # Errors
    ///
    /// [`Fault::missing_fulfillment_date`] when the document carries no
    /// `telj`; the callers raise it after every answer that needs no send.
    fn from_verified(
        found: &FoundDocument,
        account: &Account,
        number: String,
        storno_id: ExternalId,
        comment: Option<String>,
    ) -> Result<Self, Fault> {
        let fulfillment_date = found
            .fulfillment_date
            .ok_or_else(|| Fault::missing_fulfillment_date(&number))?;
        Ok(Self {
            e_invoice: found.e_invoice().unwrap_or(account.defaults.e_invoice),
            number,
            storno_id,
            comment,
            fulfillment_date,
        })
    }
}

/// What a storno handler does next with the document its verify found,
/// decided before anything else is read or sent. Both storno protocols
/// answer in this shape (`Szamlazz.Order.storno_invoice`'s [`storno_verdict`],
/// `Szamlazz.Agent.storno`'s [`unmanaged_storno_verdict`]), and the handler
/// dispatches on it: proceed, read the storno number, or answer.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StornoVerdict {
    /// A live document the handler may reverse: on to the intent and the
    /// lookup step.
    Proceed,
    /// Already reversed, by anyone: the answer is `reversed`, and the storno
    /// number is what the handler's best-effort read names
    /// ([`reversed_response`]). The one verdict that needs a further read.
    AlreadyReversed,
    /// Answered without a send: `conflict{not_managed}` and
    /// `rejected{not_stornoable}` at the order's handler,
    /// `managed_by_order` at the by-number one.
    Answered(StornoResponse),
}

/// Step 1's decision on the verified document, for `storno_invoice` on
/// `order`: `number` is the invoice as the caller named it, which every
/// answer echoes. In the order of the answers: a document that does not
/// carry this order's number is `conflict{not_managed}` (the order's handler
/// acts on nothing but its own, and reads nothing further); one already
/// reversed, by anyone, is [`StornoVerdict::AlreadyReversed`] (the storno
/// number is the hint's, which the handler reads best effort); a `tipus`
/// szamlazz.hu cannot reverse (a proforma, a delivery note, a storno) is
/// `rejected{not_stornoable}`; a live `SZ`, `ES`, `VS` or `HS` proceeds.
fn storno_verdict(found: &FoundDocument, order: &OrderKey, number: &str) -> StornoVerdict {
    if !found.carries_order(order) {
        return StornoVerdict::Answered(
            StornoResponse::new(StornoOutcome::Conflict, number)
                .with_conflict_reason(ConflictReason::NotManaged),
        );
    }
    if !found.is_live() {
        return StornoVerdict::AlreadyReversed;
    }
    if !found.is_stornoable() {
        return StornoVerdict::Answered(StornoResponse::not_stornoable(
            number,
            "the document cannot be reversed: only invoices can be stornoed",
        ));
    }
    StornoVerdict::Proceed
}

/// `Szamlazz.Agent.storno`'s decision on the verified document, `number` as
/// the caller named it: one carrying an order number (`rendelesszam`
/// trimmed; an empty or whitespace-only element is none) is
/// `Szamlazz.Order`'s, answered as `managed_by_order` with that number as the
/// `order_key`, and this service never calls into it; one already reversed,
/// by anyone, is [`StornoVerdict::AlreadyReversed`] (the storno number is the
/// by-number lookup's, which the handler reads best effort); a live
/// unmanaged document proceeds. No document type pre-check: szamlazz.hu's
/// echo tells.
fn unmanaged_storno_verdict(found: &FoundDocument, number: &str) -> StornoVerdict {
    if let Some(order) = &found.order_number {
        return StornoVerdict::Answered(
            StornoResponse::new(StornoOutcome::ManagedByOrder, number).with_order_key(order),
        );
    }
    if !found.is_live() {
        return StornoVerdict::AlreadyReversed;
    }
    StornoVerdict::Proceed
}

/// The `reversed` answer of both storno handlers: `storno_number` as the
/// read that named it did, absent when a best-effort read could not.
fn reversed_response(number: &str, storno_number: Option<String>) -> StornoResponse {
    let mut response = StornoResponse::new(StornoOutcome::Reversed, number);
    response.storno_number = storno_number;
    response
}

/// Step 2 of both storno protocols: what the storno lookup step settled,
/// before the storno step. `Break(response)` when the `SS` reversing `number`
/// already holds the storno external id (a storno of ours was issued:
/// `reversed{storno_number}`, nothing sent); `Continue(())` when nothing
/// does.
///
/// # Errors
///
/// Rejected credentials (`credentials_rejected`), and another code
/// (`unavailable`: nothing may be concluded from it, and nothing was sent).
/// The caller attaches the identity it knows.
fn after_storno_lookup(
    outcome: StornoLookupOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<ControlFlow<StornoResponse>, Fault> {
    match outcome {
        StornoLookupOutcome::Absent => Ok(ControlFlow::Continue(())),
        StornoLookupOutcome::AlreadyReversed { storno_number } => Ok(ControlFlow::Break(
            reversed_response(number, Some(storno_number)),
        )),
        StornoLookupOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
        StornoLookupOutcome::Api(answer) => {
            Err(AnsweredCode::Inconclusive(answer).into_fault(namespace))
        }
    }
}

/// The settled storno step as the handlers' `StornoResponse`: reversed (now
/// or already), not stornoable, or rejected.
///
/// # Errors
///
/// The faults a settled step can still be: rejected credentials (the
/// warning tagged with `namespace`), and the leading query answered with
/// another code or `szlahu_down` (`unavailable` at once; nothing was sent).
/// The caller attaches the identity it knows.
fn storno_response(
    outcome: gateway::StornoOutcome,
    number: String,
    namespace: &Namespace,
) -> Result<StornoResponse, Fault> {
    Ok(match outcome {
        gateway::StornoOutcome::Reversed(storno) => {
            StornoResponse::new(StornoOutcome::Reversed, number).with_storno_number(storno.number)
        }
        gateway::StornoOutcome::AlreadyReversed { storno_number } => {
            StornoResponse::new(StornoOutcome::Reversed, number).with_storno_number(storno_number)
        }
        gateway::StornoOutcome::NotStornoable => StornoResponse::not_stornoable(
            number,
            "szamlazz.hu echoed the document unchanged: it cannot be reversed (only invoices can be stornoed)",
        ),
        gateway::StornoOutcome::Rejected(rejection) => {
            StornoResponse::new(StornoOutcome::Rejected, number)
                .with_code(rejection.code)
                .with_message(rejection.message)
        }
        gateway::StornoOutcome::CredentialsRejected(answer) => {
            return Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace));
        }
        gateway::StornoOutcome::Api(answer) => {
            return Err(AnsweredCode::Inconclusive(answer).into_fault(namespace));
        }
        gateway::StornoOutcome::Unavailable { message } => {
            return Err(Fault::szlahu_down_answer(message));
        }
    })
}

/// The fault of a storno step whose run ended without a settled outcome: the
/// issue policy exhausted (500, carrying the last `Unconfirmed`'s display) or
/// the invocation cancelled (409). `outcome_unknown`; nothing is recorded,
/// and the next call's verify and lookup find whatever landed. `next` is
/// what the caller does about it (`retry with a new Idempotency-Key`, `call
/// storno again`).
fn storno_outcome_unknown(error: &TerminalError, next: &str) -> Fault {
    Fault::outcome_unknown(format!(
        "the storno step ended without a confirmed outcome ({}): {}; {next}",
        error.code(),
        error.message()
    ))
}

/// The storno number a **best-effort** order-number hint names for a document
/// of the order the verify already saw reversed: the hint when it is the `SS`
/// referencing `number`, unknown when it is any other document (something
/// newer was issued under the order), nothing (code 7) or another code
/// (nothing may be concluded from it, and the handler's answer, `reversed`,
/// is known). Rejected credentials stay the fault they are on every step.
///
/// # Errors
///
/// `credentials_rejected`; the caller attaches the storno's identity.
fn storno_number_from_hint(
    outcome: QueryOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<Option<String>, Fault> {
    match outcome {
        QueryOutcome::Found(found) if found.is_storno_of(number) => Ok(Some(found.number)),
        QueryOutcome::Found(_) | QueryOutcome::NotFound | QueryOutcome::Api(_) => Ok(None),
        QueryOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
    }
}

/// The storno number a **best-effort** storno lookup names for a document the
/// verify already saw reversed: the `SS` under the storno external id when the
/// storno was ours, unknown when nothing is under the id (a reversal from the
/// UI leaves nothing there) or another code answered (nothing may be concluded
/// from it, and the handler's answer, `reversed`, is known). Rejected
/// credentials stay the fault they are on every step.
///
/// # Errors
///
/// `credentials_rejected`.
fn storno_number_from_lookup(
    outcome: StornoLookupOutcome,
    namespace: &Namespace,
) -> Result<Option<String>, Fault> {
    match outcome {
        StornoLookupOutcome::AlreadyReversed { storno_number } => Ok(Some(storno_number)),
        StornoLookupOutcome::Absent | StornoLookupOutcome::Api(_) => Ok(None),
        StornoLookupOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
    }
}

// ----- the durable steps ---------------------------------------------------

/// The storno lookup step: one read-only journaled query of the storno
/// external id, under the read policy.
async fn lookup_storno<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    intent: &StornoIntent,
) -> Result<StornoLookupOutcome, Fault> {
    let gateway = Arc::clone(&exec.gateway);
    let external_id = intent.storno_id.clone();
    let number = intent.number.clone();
    run_reading(
        ctx,
        format!("lookup-storno-{number}"),
        exec,
        move || async move { gateway.lookup_storno(&external_id, &number).await },
    )
    .await
}

/// The storno step: one durable step under the issue policy's run retry
/// policy, query-first on every execution (the query is inside the closure: a
/// separate journaled query would replay its stale "nothing" on the retry and
/// re-send). The request is rebuilt from the intent on every execution (the
/// date included), so every send is byte-identical.
///
/// # Errors
///
/// The `TerminalError` the run ends with: exhaustion (500) or cancellation
/// (409); the caller maps it to `outcome_unknown` about its document. Nothing
/// is recorded: the next call's lookup finds whatever landed.
async fn storno_step<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    intent: &StornoIntent,
) -> Result<gateway::StornoOutcome, TerminalError> {
    let gateway = Arc::clone(&exec.gateway);
    let number = intent.number.clone();
    let external_id = intent.storno_id.clone();
    let comment = intent.comment.clone();
    let e_invoice = intent.e_invoice;
    let fulfillment_date = intent.fulfillment_date;
    run_retrying(
        ctx,
        format!("storno-{}", intent.number),
        exec.config.issue.run_retry_policy(),
        move || async move {
            gateway
                .storno(StornoStepRequest {
                    invoice_number: &number,
                    external_id: &external_id,
                    comment: comment.as_deref(),
                    e_invoice,
                    fulfillment_date,
                })
                .await
        },
    )
    .await
}

/// The storno number of a reversed document of `order`, when the
/// order-number hint is the `SS` referencing it (step `hint-storno-{number}`,
/// a best-effort read under the read policy, [`run_best_effort`]). Rejected
/// credentials are a fault about the storno (`storno_id`); everything else
/// the hint can answer is data ([`storno_number_from_hint`]).
async fn storno_number_of<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    order: &OrderKey,
    number: &str,
    storno_id: &ExternalId,
) -> Result<Option<String>, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let hinted = order.clone();
    let Some(outcome) = run_best_effort(
        ctx,
        format!("hint-storno-{number}"),
        exec,
        move || async move { gateway.hint(&hinted).await },
    )
    .await?
    else {
        return Ok(None);
    };
    storno_number_from_hint(outcome, number, &exec.config.namespace)
        .map_err(|fault| fault.about(order, None, storno_id).into())
}

/// The storno number of a reversed document no `Order` manages, when a storno
/// of ours holds `{namespace}:by-number:{number}:storno` (step
/// `lookup-storno-{number}`, the same entry the storno protocol's lookup step
/// writes, which this path never reaches; a best-effort read under the read
/// policy, [`run_best_effort`]). The only read that can name an unmanaged
/// document's storno: it carries no order number for the hint. Rejected
/// credentials are a fault; everything else is data
/// ([`storno_number_from_lookup`]).
async fn storno_number_of_unmanaged<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    number: &str,
) -> Result<Option<String>, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let external_id = ExternalId::for_unmanaged_storno(&exec.config.namespace, number);
    let looked_up = number.to_owned();
    let Some(outcome) = run_best_effort(
        ctx,
        format!("lookup-storno-{number}"),
        exec,
        move || async move { gateway.lookup_storno(&external_id, &looked_up).await },
    )
    .await?
    else {
        return Ok(None);
    };
    storno_number_from_lookup(outcome, &exec.config.namespace).map_err(Into::into)
}

// ----- the two shells --------------------------------------------------------

impl Execution {
    /// `Szamlazz.Order.storno_invoice`, on the `order` the handler parsed from
    /// its key: the verify (`verify-storno-{number}`) decided by
    /// [`storno_verdict`], the intent, the storno lookup, the storno step, and
    /// the answer from data. Every fault after the verify is about this
    /// storno (`{namespace}:{order}:storno:{number}`).
    pub(super) async fn storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: OrderKey,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;
        let number = String::from(number);
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
        let kind = IssuedKind::for_document_type(&found.document_type);
        // Every fault from here on is about this storno.
        let about = |fault: Fault| fault.about(&order, kind, &storno_id);
        // The intent is a pure function of the verified document: a `telj`
        // it does not carry is a fault after every answer that needs no
        // send.
        let intent = StornoIntent::from_verified(
            &found,
            self.gateway.account(),
            number.clone(),
            storno_id.clone(),
            comment,
        )
        .map_err(about)?;

        // Step 2: lookup: a storno of ours already under the id.
        let looked_up = lookup_storno(ctx, self, &intent).await.map_err(about)?;
        if let ControlFlow::Break(response) =
            after_storno_lookup(looked_up, &number, namespace).map_err(about)?
        {
            return Ok(response);
        }

        // Step 3: the storno step, under the issue policy. Any `Err` from the
        // run (exhaustion (500) or cancellation (409)) is `outcome_unknown`
        // about this storno: nothing is recorded, the next invocation's
        // verify and lookup find whatever landed.
        let outcome = storno_step(ctx, self, &intent).await.map_err(|error| {
            about(storno_outcome_unknown(
                &error,
                "retry with a new Idempotency-Key",
            ))
        })?;

        // Step 4: branch on data.
        storno_response(outcome, number, namespace).map_err(|fault| about(fault).into())
    }

    /// Step 1 of the order's storno protocol: the document must be known,
    /// carry this order's number and be a live invoice kind. One read (the
    /// verify), then [`storno_verdict`] on what it found; `Break(response)` is
    /// the answer for anything that stops the storno before it is sent (not
    /// managed, already reversed, not stornoable): a domain outcome, not a
    /// fault.
    async fn verify_for_storno(
        &self,
        ctx: &ObjectContext<'_>,
        order: &OrderKey,
        number: &str,
        storno_id: &ExternalId,
    ) -> Result<ControlFlow<StornoResponse, Box<FoundDocument>>, HandlerError> {
        let namespace = &self.config.namespace;
        let about = |fault: Fault| fault.about(order, None, storno_id);
        let found = verify(ctx, self, format!("verify-storno-{number}"), number)
            .await
            .map_err(about)?;
        let found = verified_document(found, number, namespace).map_err(about)?;
        match storno_verdict(&found, order, number) {
            StornoVerdict::Proceed => Ok(ControlFlow::Continue(found)),
            StornoVerdict::Answered(response) => Ok(ControlFlow::Break(response)),
            StornoVerdict::AlreadyReversed => {
                // Idempotent: already reversed by anyone. The storno number is
                // best effort; a cancelled invocation propagates as such.
                let storno_number = storno_number_of(ctx, self, order, number, storno_id).await?;
                Ok(ControlFlow::Break(reversed_response(number, storno_number)))
            }
        }
    }

    /// `Szamlazz.Agent.storno`: the verify (`verify-{number}`) decided by
    /// [`unmanaged_storno_verdict`], then (for a document carrying no order
    /// number) the intent, the lookup and the storno step under the by-number
    /// storno external id (`{namespace}:by-number:{number}:storno`). A
    /// document carrying an order number is answered as `managed_by_order`;
    /// one already reversed is `reversed` with the storno number the
    /// by-number storno lookup names, best effort (ours when we issued the
    /// storno, unknown otherwise). By-number faults carry no order identity.
    pub(super) async fn storno_by_number(
        &self,
        ctx: &Context<'_>,
        request: StornoRequest,
    ) -> Result<StornoResponse, HandlerError> {
        let StornoRequest {
            invoice_number: number,
            comment,
        } = request;
        let number = String::from(number);
        let namespace = &self.config.namespace;

        // Step 1: verify the document.
        let found = verify(ctx, self, format!("verify-{number}"), &number).await?;
        let found = verified_document(found, &number, namespace)?;
        match unmanaged_storno_verdict(&found, &number) {
            StornoVerdict::Proceed => {}
            StornoVerdict::Answered(response) => return Ok(response),
            StornoVerdict::AlreadyReversed => {
                // Idempotent: already reversed by anyone. The storno number is
                // best effort: ours when a storno of ours holds the by-number
                // storno id, unknown otherwise; a cancelled invocation
                // propagates as such.
                let storno_number = storno_number_of_unmanaged(ctx, self, &number).await?;
                return Ok(reversed_response(&number, storno_number));
            }
        }
        // The intent is a pure function of the verified document: a `telj`
        // it does not carry is a fault after every answer that needs no send.
        let intent = StornoIntent::from_verified(
            &found,
            self.gateway.account(),
            number.clone(),
            ExternalId::for_unmanaged_storno(namespace, &number),
            comment,
        )?;

        // Step 2: lookup: a storno of ours already under the id.
        let looked_up = lookup_storno(ctx, self, &intent).await?;
        if let ControlFlow::Break(response) = after_storno_lookup(looked_up, &number, namespace)? {
            return Ok(response);
        }

        // Step 3: the storno step, under the issue policy: query-first on
        // every execution; any `Err` from the run (exhaustion or
        // cancellation) is `outcome_unknown`, and the next call's lookup finds
        // whatever landed.
        let outcome = storno_step(ctx, self, &intent)
            .await
            .map_err(|error| storno_outcome_unknown(&error, "call storno again"))?;

        // Step 4: branch on data.
        storno_response(outcome, number, namespace).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;

    use super::*;
    use crate::contract::IssuedKind;
    use crate::gateway::SzamlazzAnswer;
    use crate::test_support::{Doc, ORIGINAL_TELJ};

    fn ord_1() -> OrderKey {
        OrderKey::parse("ORD-1").expect("order")
    }

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// Step 1 of the storno protocol on the verified document: this order's
    /// live invoice kind (`SZ`, `ES`, `VS`, `HS`) proceeds; a document that
    /// does not carry this order's number (another order's, none, an empty
    /// or whitespace `rendelesszam`, as rendered and as a parsed value the
    /// worker reads on its own) is `conflict{not_managed}` before
    /// anything else is read; one already reversed, by anyone, is
    /// `AlreadyReversed` (the answer is known, the storno number is the
    /// hint's); a `tipus` szamlazz.hu cannot reverse (a proforma, a delivery
    /// note, a storno) is `rejected{not_stornoable}`. The order of the checks
    /// is the order of the answers: another order's reversed proforma is
    /// `not_managed`, this order's reversed proforma is `AlreadyReversed`.
    #[test]
    fn the_storno_verdict_refuses_before_it_reverses() {
        let order = ord_1();
        let verdict = |doc: &Doc| storno_verdict(&doc.parse(), &order, doc.number);

        for tipus in ["SZ", "ES", "VS", "HS"] {
            assert_eq!(
                verdict(&Doc::new("X-1", tipus)),
                StornoVerdict::Proceed,
                "{tipus}"
            );
        }

        for not_ours in [Some("ORD-2"), None, Some(""), Some("  ")] {
            let StornoVerdict::Answered(response) = verdict(&Doc {
                order: not_ours,
                reversed: true,
                ..Doc::new("D-1", "D")
            }) else {
                panic!("rendelesszam {not_ours:?} is answered");
            };
            assert_eq!(response.outcome, StornoOutcome::Conflict, "{not_ours:?}");
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "{not_ours:?}"
            );
            assert_eq!(response.invoice_number, "D-1", "{not_ours:?}");
            assert_eq!(response.storno_number, None, "{not_ours:?}");
        }
        // The projection's own reading of the parsed value, with the parser's
        // normalisation out of the way.
        for raw in ["", "   ", "ORD-2"] {
            let StornoVerdict::Answered(response) =
                storno_verdict(&Doc::default().assigned_order(Some(raw)), &order, "SZ-1")
            else {
                panic!("order_number {raw:?} is answered");
            };
            assert_eq!(
                response.conflict_reason,
                Some(ConflictReason::NotManaged),
                "order_number {raw:?}: the worker's own reading"
            );
        }
        assert_eq!(
            storno_verdict(
                &Doc::default().assigned_order(Some(" ORD-1 ")),
                &order,
                "SZ-1"
            ),
            StornoVerdict::Proceed,
            "a padded order number carries the order"
        );

        for tipus in ["SZ", "D"] {
            assert_eq!(
                verdict(&Doc {
                    reversed: true,
                    ..Doc::new("X-1", tipus)
                }),
                StornoVerdict::AlreadyReversed,
                "{tipus}: reversed is answered before the kind is checked"
            );
        }

        for (number, tipus) in [("D-1", "D"), ("SL-1", "SL"), ("SS-1", "SS")] {
            let StornoVerdict::Answered(response) = verdict(&Doc {
                referenced_invoice: (tipus == "SS").then_some("SZ-1"),
                ..Doc::new(number, tipus)
            }) else {
                panic!("{tipus} is answered");
            };
            assert_eq!(response.outcome, StornoOutcome::Rejected, "{tipus}");
            assert_eq!(
                response.code.as_deref(),
                Some(StornoResponse::NOT_STORNOABLE),
                "{tipus}"
            );
            assert_eq!(response.invoice_number, number, "{tipus}");
            assert_eq!(response.conflict_reason, None, "{tipus}");
            assert!(
                response
                    .message
                    .as_deref()
                    .is_some_and(|message| message.contains("only invoices can be stornoed")),
                "{tipus}: {response:?}"
            );
        }
    }

    /// `Szamlazz.Agent.storno`'s verdict on the verified document: one
    /// carrying an order number is `managed_by_order` with that number,
    /// trimmed, as the `order_key` to call `Szamlazz.Order.storno_invoice`
    /// on; an empty or whitespace-only `rendelesszam` is no order number (as
    /// rendered, and as a parsed value the worker reads on its own), and the
    /// document is this service's to reverse; one already reversed, by
    /// anyone, is `AlreadyReversed` (the answer is known, the storno number
    /// is the by-number lookup's); a live unmanaged document proceeds,
    /// whatever its `tipus`: there is no kind pre-check here, szamlazz.hu's
    /// echo tells.
    #[test]
    fn the_unmanaged_storno_verdict_redirects_managed_documents_and_proceeds_on_the_rest() {
        let verdict = |doc: &Doc| unmanaged_storno_verdict(&doc.parse(), doc.number);

        for managed in [Some("ORD-1"), Some("  ORD-1 ")] {
            let StornoVerdict::Answered(response) = verdict(&Doc {
                order: managed,
                reversed: true,
                ..Doc::default()
            }) else {
                panic!("rendelesszam {managed:?} is answered");
            };
            assert_eq!(
                response.outcome,
                StornoOutcome::ManagedByOrder,
                "{managed:?}"
            );
            assert_eq!(
                response.order_key.as_deref(),
                Some("ORD-1"),
                "{managed:?}: the trimmed order number is the key"
            );
            assert_eq!(response.invoice_number, "SZ-1", "{managed:?}");
            assert_eq!(response.storno_number, None, "{managed:?}");
            assert_eq!(response.conflict_reason, None, "{managed:?}");
        }

        for (unmanaged, tipus) in [
            (None, "SZ"),
            (Some(""), "SZ"),
            (Some("  "), "SZ"),
            (None, "D"),
        ] {
            assert_eq!(
                verdict(&Doc {
                    order: unmanaged,
                    ..Doc::new("X-1", tipus)
                }),
                StornoVerdict::Proceed,
                "rendelesszam {unmanaged:?}, {tipus}"
            );
        }

        // The projection's own reading of the parsed value, with the parser's
        // normalisation out of the way.
        for raw in ["", "   "] {
            assert_eq!(
                unmanaged_storno_verdict(&Doc::default().assigned_order(Some(raw)), "SZ-1"),
                StornoVerdict::Proceed,
                "order_number {raw:?}: the worker's own reading"
            );
        }
        let StornoVerdict::Answered(response) =
            unmanaged_storno_verdict(&Doc::default().assigned_order(Some(" ORD-1 ")), "SZ-1")
        else {
            panic!("a padded order number is answered");
        };
        assert_eq!(response.order_key.as_deref(), Some("ORD-1"));

        assert_eq!(
            verdict(&Doc {
                order: None,
                reversed: true,
                ..Doc::default()
            }),
            StornoVerdict::AlreadyReversed
        );
    }

    /// `Szamlazz.Agent.storno`'s verify: code 7 is 404 `not_found` naming the
    /// invoice, with no order identity (a by-number fault).
    #[test]
    fn storno_answers_an_unknown_invoice_as_not_found() {
        let (status, body) = fault_body(
            verified_document(QueryOutcome::NotFound, "SZ-9", &namespace()).expect_err("a fault"),
        );
        assert_eq!(status, 404, "{body}");
        assert_eq!(body["code"], "not_found", "{body}");
        assert!(
            body["message"].as_str().expect("message").contains("SZ-9"),
            "{body}"
        );
        assert_eq!(body.get("order"), None, "a by-number fault: {body}");
    }

    /// The storno intent both storno handlers build from the verified
    /// original: the storno repeats the original's `telj`, lifts `eszamla`
    /// from the document (the appearance cases are
    /// [`the_storno_intent_lifts_eszamla_from_the_original_not_the_default`]),
    /// and an original szamlazz.hu returned without a `telj` (its schema has
    /// the element mandatory) is the `unavailable` fault naming the invoice,
    /// never a send without the date or with a default.
    #[test]
    fn the_storno_intent_repeats_the_originals_fulfillment_date() {
        let mut account = Account::new("acct", "acct");
        account.defaults.e_invoice = false;
        let storno_id = || ExternalId::new("acct:ORD-1:storno:SZ-1");

        // `telj` present: the intent carries it; `eszamla = 3` is an e-invoice
        // where the account's default is paper.
        let intent = StornoIntent::from_verified(
            &Doc {
                eszamla: Some(3),
                ..Doc::default()
            }
            .parse(),
            &account,
            "SZ-1".to_owned(),
            storno_id(),
            Some("wrong buyer".to_owned()),
        )
        .expect("an intent");
        assert_eq!(intent.fulfillment_date, ORIGINAL_TELJ);
        assert_eq!(intent.number, "SZ-1");
        assert_eq!(intent.storno_id, storno_id());
        assert_eq!(intent.comment.as_deref(), Some("wrong buyer"));
        assert!(intent.e_invoice, "lifted from the document");

        // `telj` empty (parsed as absent): the fault, 503 `unavailable`,
        // naming the invoice; `.about(..)` attaches the storno identity as
        // every fault.
        let fault = StornoIntent::from_verified(
            &Doc {
                fulfillment_date: None,
                alap_extra: "<telj></telj>",
                ..Doc::default()
            }
            .parse(),
            &account,
            "SZ-1".to_owned(),
            storno_id(),
            None,
        )
        .expect_err("a fault");
        let (status, body) = fault_body(fault.clone());
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("SZ-1"), "{message}");
        assert!(message.contains("fulfillment date"), "{message}");
        assert!(message.contains("nothing was sent"), "{message}");
        assert!(message.contains("Idempotency-Key"), "{message}");
        assert_eq!(body.get("order"), None);

        let (_, body) = fault_body(fault.about(
            &ord_1(),
            Some(IssuedKind::Invoice),
            &ExternalId::new("acct:ORD-1:storno:SZ-1"),
        ));
        assert_eq!(body["code"], "unavailable");
        assert_eq!(body["order"], "ORD-1");
        assert_eq!(body["kind"], "invoice");
        assert_eq!(body["external_id"], "acct:ORD-1:storno:SZ-1");
    }

    /// The storno's `eszamla` is the verified original's appearance, and the
    /// account default only where the code is not an invoice appearance.
    /// szamlazz.hu does not require a storno's form to match its original's:
    /// a mismatch is accepted silently and the storno document takes the
    /// *request's* flag (P73), so the intent, not the server, keeps a
    /// reversal in its original's form: `1` (paper) is `false` under an
    /// e-invoice default, `3` (the code szamlazz.hu reports for an invoice
    /// created with `eszamla=true`) and `2` are `true` under a paper default,
    /// and `0` (a proforma) is whatever the account default says.
    #[test]
    fn the_storno_intent_lifts_eszamla_from_the_original_not_the_default() {
        let mut account = Account::new("acct", "acct");
        let intent = |eszamla: i64, account: &Account| {
            StornoIntent::from_verified(
                &Doc {
                    eszamla: Some(eszamla),
                    ..Doc::default()
                }
                .parse(),
                account,
                "SZ-1".to_owned(),
                ExternalId::new("acct:ORD-1:storno:SZ-1"),
                None,
            )
            .expect("an intent")
            .e_invoice
        };

        account.defaults.e_invoice = true;
        assert!(!intent(1, &account), "paper, whatever the account default");
        assert!(intent(0, &account), "not an invoice: the account default");

        account.defaults.e_invoice = false;
        assert!(
            intent(3, &account),
            "e-invoice, whatever the account default"
        );
        assert!(
            intent(2, &account),
            "e-invoice, whatever the account default"
        );
        assert!(!intent(0, &account), "not an invoice: the account default");
    }

    /// Step 2 of both storno protocols, decided once for both services: a
    /// storno of ours already under the storno external id answers `reversed`
    /// with its number before anything is sent; nothing under the id proceeds
    /// to the storno step; rejected credentials are the `credentials_rejected`
    /// fault and another code the `unavailable` one (nothing may be concluded,
    /// nothing was sent), each carrying the szamlazz.hu code beside the token,
    /// never in it.
    #[test]
    fn the_storno_lookup_answers_an_existing_storno_and_faults_on_a_code() {
        let namespace = namespace();

        assert_eq!(
            after_storno_lookup(StornoLookupOutcome::Absent, "SZ-1", &namespace).expect("data"),
            ControlFlow::Continue(()),
            "nothing under the id: on to the storno step"
        );

        let ControlFlow::Break(response) = after_storno_lookup(
            StornoLookupOutcome::AlreadyReversed {
                storno_number: "SS-1".to_owned(),
            },
            "SZ-1",
            &namespace,
        )
        .expect("data") else {
            panic!("a storno of ours under the id is the answer");
        };
        assert_eq!(response.outcome, StornoOutcome::Reversed);
        assert_eq!(response.invoice_number, "SZ-1");
        assert_eq!(response.storno_number.as_deref(), Some("SS-1"));
        assert_eq!(response.conflict_reason, None);

        let (status, body) = fault_body(
            after_storno_lookup(
                StornoLookupOutcome::CredentialsRejected(SzamlazzAnswer::new(
                    "3",
                    "Sikertelen bejelentkezés.",
                )),
                "SZ-1",
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "credentials_rejected", "{body}");
        assert_eq!(body["szamlazz_code"], "3", "{body}");

        let (status, body) = fault_body(
            after_storno_lookup(
                StornoLookupOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
                "SZ-1",
                &namespace,
            )
            .expect_err("a fault"),
        );
        assert_eq!(status, 503, "{body}");
        assert_eq!(body["code"], "unavailable", "{body}");
        assert_eq!(body["szamlazz_code"], "57", "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("code 57"), "{message}");
        assert!(message.contains("Hibás számlaszám."), "{message}");
        assert!(message.contains("nothing may be concluded"), "{message}");
    }

    /// The settled storno step as the response, for the two answers the
    /// leading query can settle it with before anything is sent (#63):
    /// another code is `unavailable` naming it (the shape the storno lookup
    /// gives the same code), and `szlahu_down` is `unavailable` without a
    /// `szamlazz_code`. Both services share this mapping.
    #[test]
    fn a_settled_storno_step_maps_its_leading_query_answers_onto_faults() {
        let respond = |outcome: gateway::StornoOutcome| {
            storno_response(outcome, "SZ-1".to_owned(), &namespace())
        };

        let response = respond(gateway::StornoOutcome::AlreadyReversed {
            storno_number: "SS-1".to_owned(),
        })
        .expect("data");
        assert_eq!(response.storno_number.as_deref(), Some("SS-1"));

        let (status, body) = fault_body(
            respond(gateway::StornoOutcome::Api(SzamlazzAnswer::new(
                "57",
                "Hibás XML.",
            )))
            .expect_err("a fault"),
        );
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        assert_eq!(body["szamlazz_code"], "57");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("code 57"), "{message}");

        let (status, body) = fault_body(
            respond(gateway::StornoOutcome::Unavailable {
                message: "maintenance".to_owned(),
            })
            .expect_err("a fault"),
        );
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        assert_eq!(body.get("szamlazz_code"), None, "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("szlahu_down"), "{message}");
        assert!(message.contains("nothing was sent"), "{message}");

        let (_, body) = fault_body(
            respond(gateway::StornoOutcome::CredentialsRejected(
                SzamlazzAnswer::new("3", "login"),
            ))
            .expect_err("a fault"),
        );
        assert_eq!(body["code"], "credentials_rejected");
    }

    /// The `outcome_unknown` fault of an exhausted or cancelled storno step
    /// names the run's status and last failure, and what the caller does
    /// next, as each shell words it.
    #[test]
    fn an_unsettled_storno_step_is_outcome_unknown_naming_the_next_step() {
        let last = TerminalError::new_with_code(500, "transport failure: connection reset");
        let (status, body) = fault_body(storno_outcome_unknown(&last, "call storno again"));
        assert_eq!(status, 500, "{body}");
        assert_eq!(body["code"], "outcome_unknown", "{body}");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("500"), "{message}");
        assert!(message.contains("connection reset"), "{message}");
        assert!(message.ends_with("call storno again"), "{message}");
    }

    /// What the two best-effort reads make of an answer, for a document the
    /// verify already saw reversed: `Szamlazz.Order.storno_invoice`'s
    /// order-number hint names the storno when it is the `SS` referencing the
    /// invoice; any other document under the order, nothing, or another code
    /// is unknown; `Szamlazz.Agent.storno`'s by-number storno lookup names it
    /// when a storno of ours holds the id (#65); nothing under it (a reversal
    /// from the UI) or another code is unknown. Rejected credentials stay the
    /// fault on both.
    #[test]
    fn the_best_effort_reads_name_the_storno_only_from_its_own_document() {
        let namespace = namespace();
        let rejected_body = |fault: Fault| {
            let (status, body) = fault_body(fault);
            assert_eq!(status, 503);
            assert_eq!(body["code"], "credentials_rejected", "{body}");
            assert_eq!(body["szamlazz_code"], "3", "{body}");
        };
        let rejected = || SzamlazzAnswer::new("3", "Sikertelen bejelentkezés.");

        let storno = Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SS")
        };
        assert_eq!(
            storno_number_from_hint(QueryOutcome::Found(storno.boxed()), "SZ-1", &namespace)
                .expect("data"),
            Some("SS-1".to_owned())
        );
        for not_its_storno in [
            // The reversed invoice itself is the newest document under the
            // order.
            QueryOutcome::Found(Doc::default().boxed()),
            // Another invoice's storno.
            QueryOutcome::Found(
                Doc {
                    referenced_invoice: Some("SZ-9"),
                    ..Doc::new("SS-9", "SS")
                }
                .boxed(),
            ),
            QueryOutcome::NotFound,
            QueryOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
        ] {
            assert_eq!(
                storno_number_from_hint(not_its_storno.clone(), "SZ-1", &namespace).expect("data"),
                None,
                "{not_its_storno:?}"
            );
        }
        rejected_body(
            storno_number_from_hint(
                QueryOutcome::CredentialsRejected(rejected()),
                "SZ-1",
                &namespace,
            )
            .expect_err("a fault"),
        );

        assert_eq!(
            storno_number_from_lookup(
                StornoLookupOutcome::AlreadyReversed {
                    storno_number: "SS-1".to_owned(),
                },
                &namespace,
            )
            .expect("data"),
            Some("SS-1".to_owned())
        );
        for unknown in [
            StornoLookupOutcome::Absent,
            StornoLookupOutcome::Api(SzamlazzAnswer::new("57", "Hibás számlaszám.")),
        ] {
            assert_eq!(
                storno_number_from_lookup(unknown.clone(), &namespace).expect("data"),
                None,
                "{unknown:?}"
            );
        }
        rejected_body(
            storno_number_from_lookup(
                StornoLookupOutcome::CredentialsRejected(rejected()),
                &namespace,
            )
            .expect_err("a fault"),
        );
    }

    /// The `reversed` answer both storno handlers give once a verify found
    /// the document already reversed: the storno number as the best-effort
    /// read named it, or absent when that read could not name one.
    #[test]
    fn the_reversed_answer_carries_the_storno_number_when_known() {
        let known = reversed_response("SZ-1", Some("SS-1".to_owned()));
        assert_eq!(known.outcome, StornoOutcome::Reversed);
        assert_eq!(known.invoice_number, "SZ-1");
        assert_eq!(known.storno_number.as_deref(), Some("SS-1"));

        let unknown = reversed_response("SZ-1", None);
        assert_eq!(unknown.outcome, StornoOutcome::Reversed);
        assert_eq!(unknown.invoice_number, "SZ-1");
        assert_eq!(unknown.storno_number, None);
        assert_eq!(unknown.conflict_reason, None);
        assert_eq!(unknown.code, None);
    }
}
