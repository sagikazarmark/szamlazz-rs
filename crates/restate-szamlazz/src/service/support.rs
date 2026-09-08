//! Plumbing shared by the `Szamlazz.Order` and `Szamlazz.Agent` handlers: the
//! fault → `TerminalError` mapping, journaled runs, the validation of
//! documents found under our external ids and the account check of documents
//! found by number.

use std::ops::ControlFlow;

use restate_sdk::errors::{HandlerError, TerminalError};
use szamlazz_agent::Date;

use crate::account::Account;
use crate::contract::{IssuedKind, StornoOutcome, StornoResponse, TerminalCode};
use crate::gateway::{
    FoundDocument, QueryOutcome, StornoLookupOutcome, StornoOutcome as GatewayStornoOutcome,
};
use crate::identity::{ExternalId, Namespace, OrderKey};

pub(super) use self::journaled::Journaled;
#[cfg(test)]
pub(super) use self::journaled::journaled_types;

/// The `Journaled` trait, its seal and the one list of its implementors. A
/// module of its own so that the seal is nameable nowhere else: a type
/// becomes journalable by being added to the `journaled!` list below and in
/// no other way, and the list is what `service::journal`'s registry is
/// checked against.
mod journaled {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::gateway::{
        CreateOutcome, DeleteOutcome, LookupOutcome, ProbeOutcome, QueryOutcome,
        SetPaymentsOutcome, StornoLookupOutcome, StornoOutcome as GatewayStornoOutcome,
        TaxpayerOutcome,
    };
    use crate::identity::Namespace;
    use crate::service::prologue::Resolution;

    /// A type the services journal as the result of a `ctx.run`: the bound
    /// of `run_once`, `run_retrying` and `run_reading`, so this list is
    /// exactly what the journal can hold.
    ///
    /// Implementing it is a promise that the type's serde layout is
    /// **additive-only**, as the [`gateway`](crate::gateway) module docs
    /// state. The promise is checked by the fixtures under
    /// `tests/journal/<type>/` (`service::journal`), one per variant: a new
    /// implementor is pinned there before it is journaled, and a new variant
    /// of one of these enums fails to compile until it is named in the pins'
    /// `variants!` list, and fails the generator by name until it has a
    /// sample.
    ///
    /// Implemented through the `journaled!` list in this module only: the
    /// trait is sealed by a supertrait private to this module, so an `impl`
    /// anywhere else fails to compile, and the same list yields the
    /// implementors' names ([`journaled_types`]) that `service::journal`'s
    /// registry is held to, so a type journaled without pins fails that test
    /// by name.
    pub(in crate::service) trait Journaled:
        sealed::Sealed + Serialize + DeserializeOwned
    {
    }

    /// The seal on [`Journaled`]: a supertrait only this module can
    /// implement.
    mod sealed {
        pub trait Sealed {}
    }

    /// Implements [`Journaled`] (and its seal) for each listed type and
    /// writes their names into [`journaled_types`].
    macro_rules! journaled {
        ($($ty:ty),+ $(,)?) => {
            $(
                impl sealed::Sealed for $ty {}
                impl Journaled for $ty {}
            )+

            /// The names of every [`Journaled`] implementor (the `journaled!`
            /// list), as [`std::any::type_name`] writes them, so an alias
            /// (`GatewayStornoOutcome`) names its type: what
            /// `service::journal`'s registry is checked against.
            #[cfg(test)]
            pub(in crate::service) fn journaled_types() -> Vec<&'static str> {
                vec![$(::std::any::type_name::<$ty>()),+]
            }
        };
    }

    journaled!(
        Namespace,
        Resolution,
        QueryOutcome,
        LookupOutcome,
        CreateOutcome,
        StornoLookupOutcome,
        GatewayStornoOutcome,
        DeleteOutcome,
        SetPaymentsOutcome,
        ProbeOutcome,
        TaxpayerOutcome,
    );
}

pub(super) use crate::contract::Fault;

/// The service-side constructors of the contract's [`Fault`], one per way the
/// handlers fail: each names its [`TerminalCode`] and writes the message the
/// caller reads. The wire shape is the contract's; the conversion to the
/// SDK's `TerminalError` hands it the code's status and the fault JSON as
/// the message, which the ingress wraps in its envelope.
impl Fault {
    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::InvalidInput, message)
    }

    /// The document the request names by number is not known to szamlazz.hu
    /// (code 7). Nothing was sent.
    pub(super) fn not_found(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::NotFound, message)
    }

    /// szamlazz.hu answered with an error code the handler passes through
    /// rather than concludes from: the `szamlazz_error` fault (422) with the
    /// code in `szamlazz_code` and a message that repeats szamlazz.hu's.
    pub(super) fn szamlazz_error(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        Self::new(
            TerminalCode::SzamlazzError,
            format!("szamlazz.hu error {code}: {}", message.into()),
        )
        .with_szamlazz_code(code)
    }

    pub(super) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::Unavailable, message)
    }

    /// szamlazz.hu answered a read with a code the handler cannot conclude a
    /// document from (neither 7 nor a credential code). An answer, so it is
    /// journaled and never retried by the read policy; still a fault, since
    /// nothing may be concluded from it.
    pub(super) fn inconclusive_answer(code: impl Into<String>, message: impl Into<String>) -> Self {
        let code = code.into();
        Self::unavailable(format!(
            "szamlazz.hu answered the query with code {code}: {}; nothing may be concluded; retry with a new Idempotency-Key or read get",
            message.into()
        ))
        .with_szamlazz_code(code)
    }

    /// szamlazz.hu reported unavailability (`szlahu_down`) to a write step's
    /// leading query, before anything was sent. An answer, so it is
    /// journaled and never re-executed under the issue policy, which is sized
    /// for the post-send window; a fault, since nothing may be concluded from
    /// it. No `szamlazz_code`: `szlahu_down` is a header, not a code.
    pub(super) fn szlahu_down_answer(message: impl Into<String>) -> Self {
        Self::unavailable(format!(
            "szamlazz.hu reported unavailability (szlahu_down) to the query: {}; nothing was sent; retry with a new Idempotency-Key or read get",
            message.into()
        ))
    }

    /// The verified original of a storno carries no `telj`.
    /// szamlazz.hu's query schema has the element mandatory (the legal "no
    /// separate date" case is an equal `telj`, never an absent one), so this
    /// is szamlazz.hu breaking its own schema: the same class as an
    /// inconclusive answer, and answered the same way. The storno must repeat
    /// that date and no default can be right, so nothing is sent.
    pub(super) fn missing_fulfillment_date(number: &str) -> Self {
        Self::unavailable(format!(
            "szamlazz.hu returned invoice {number} without a fulfillment date (telj), which the storno must repeat; nothing was sent, so retry with a new Idempotency-Key, or query the invoice"
        ))
    }

    pub(super) fn outcome_unknown(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::OutcomeUnknown, message)
    }

    /// The request names no account of this deployment (unscoped where
    /// accounts are scoped, or an unknown scope).
    pub(super) fn unknown_account(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::UnknownAccount, message)
    }

    /// szamlazz.hu rejected the account's agent credentials with `code`
    /// (3, 135, 136 or 164). Logs the warning that pages the operator (tagged
    /// with the namespace and the code, never the key), and builds the fault.
    /// The message claims the outcome is not known, nothing more: szamlazz.hu
    /// answers these codes before acting, so the request it rejected was not
    /// acted on, but the rejection may be a post-send re-query's after a send
    /// with an open code, and an earlier execution's send may have landed.
    pub(super) fn credentials_rejected(
        namespace: &Namespace,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let code = code.into();
        let message = message.into();
        tracing::warn!(
            namespace = %namespace,
            code = %code,
            "szamlazz.hu rejected the agent credentials; fix the account's agent key"
        );
        Self::new(
            TerminalCode::CredentialsRejected,
            format!(
                "szamlazz.hu rejected the agent credentials (code {code}: {message}); the outcome is not known; fix the account's agent key, then retry with a new Idempotency-Key or read get"
            ),
        )
        .with_szamlazz_code(code)
    }

    /// The SDK's terminal error carrying this fault: the code's status and
    /// the fault JSON as the message.
    fn into_terminal(self) -> TerminalError {
        let status = self.status();
        let body = serde_json::to_string(&self)
            .unwrap_or_else(|_| format!("{{\"code\":\"{}\"}}", self.code));
        TerminalError::new_with_code(status, body)
    }
}

impl From<Fault> for TerminalError {
    fn from(fault: Fault) -> Self {
        fault.into_terminal()
    }
}

impl From<Fault> for HandlerError {
    fn from(fault: Fault) -> Self {
        TerminalError::from(fault).into()
    }
}

/// The fault of a read step that ended without an answer: the read policy is
/// exhausted (500, carrying the last `Unanswered`'s message) or the
/// invocation was cancelled (409). The caller attaches the document it was
/// reading about when it knows one.
pub(super) fn read_exhausted(step: &str, error: &TerminalError) -> Fault {
    Fault::unavailable(format!(
        "the {step} read ended without an answer from szamlazz.hu ({}): {}; retry with a new Idempotency-Key or read get",
        error.code(),
        error.message()
    ))
}

/// The status the SDK ends a run with when the invocation was cancelled
/// (`restate-sdk` 0.12, `endpoint/context.rs`: `TerminalFailure { code: 409,
/// message: "cancelled" }`). A closure's own error never reaches a run's
/// `TerminalError` with this code (`run_retrying` turns it into a retryable
/// failure and exhaustion is 500), so on a run's error the code alone tells a
/// cancellation from an exhausted policy.
const CANCELLED: u16 = 409;

/// What a **best-effort** read makes of a run that ended without an answer:
/// the storno-number hint after a verify found the document already reversed,
/// and `Szamlazz.Agent.storno`'s storno lookup in the same situation: reads
/// whose handler already knows its answer (`reversed`) and only lacks the
/// storno number. An exhausted read policy is swallowed (logged at `warn`
/// naming the step and the last failure), and the number is reported as
/// unknown, rather than failing a handler whose answer is known. A
/// cancellation is never swallowed: the invocation was told to stop, and a
/// cancelled invocation must not complete as `reversed` as if nothing had
/// happened; it is propagated as it came, so the SDK reports the
/// cancellation.
///
/// # Errors
///
/// The cancellation, unchanged.
pub(super) fn best_effort(step: &str, error: TerminalError) -> Result<(), TerminalError> {
    if error.code() == CANCELLED {
        return Err(error);
    }
    tracing::warn!(
        step,
        last_failure = %error.message(),
        "the storno number could not be read; reporting the reversal without it"
    );
    Ok(())
}

/// Parses the Virtual Object key as an [`OrderKey`].
///
/// The key must arrive trimmed. Restate's per-key lock is on the *raw* key,
/// so `ORD-1` and ` ORD-1` would be two instances with two locks that map to
/// one szamlazz.hu order and identical external ids: two concurrent creates
/// under them would both pass their lookup and both send, leaving
/// szamlazz.hu's order-number-repetition toggle as the only guard. A key
/// whose trimmed form differs from the raw one is therefore refused as
/// `invalid_input` naming the rule; [`OrderKey::parse`] itself stays lenient
/// for the places that parse an order number rather than a key.
pub(super) fn order_key(key: &str) -> Result<OrderKey, Fault> {
    if key.trim() != key {
        return Err(Fault::invalid_input(format!(
            "invalid order key {key:?}: the order key must not have leading or trailing whitespace; Restate locks on the raw key, so trim it before calling"
        )));
    }
    OrderKey::parse(key)
        .map_err(|error| Fault::invalid_input(format!("invalid order key: {error}")))
}

/// The document a verify by number found, or the fault for anything else:
/// 404 `not_found` naming the invoice on code 7, `unavailable` on a code the
/// verify cannot conclude from (`Fault::inconclusive_answer`), a credential
/// code as `credentials_rejected`. Shared by every verify: `Szamlazz.Order`'s
/// attach the order identity to the fault ([`Fault::about`]),
/// `Szamlazz.Agent.storno`'s carries none.
///
/// # Errors
///
/// The fault for every outcome but `Found`.
pub(super) fn verified_document(
    outcome: QueryOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<Box<FoundDocument>, Fault> {
    match outcome {
        QueryOutcome::Found(found) => Ok(found),
        QueryOutcome::NotFound => Err(Fault::not_found(format!(
            "invoice {number} is not known to szamlazz.hu (code 7)"
        ))),
        QueryOutcome::Api { code, message } => Err(Fault::inconclusive_answer(code, message)),
        QueryOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
        }
    }
}

/// What the storno step sends, built from what the verify
/// step found. Shared by `Szamlazz.Order.storno_invoice` and
/// `Szamlazz.Agent.storno`, whose storno external ids differ.
#[derive(Debug, Clone)]
pub(super) struct StornoIntent {
    /// The invoice to reverse.
    pub(super) number: String,
    /// `{namespace}:{order}:storno:{number}` or
    /// `{namespace}:by-number:{number}:storno`.
    pub(super) storno_id: ExternalId,
    pub(super) comment: Option<String>,
    /// The verified document's `eszamla` when known, else the account
    /// default: an open code set for which the account's own default is a
    /// legitimate choice.
    pub(super) e_invoice: bool,
    /// The verified document's `telj`, which the storno repeats as its
    /// `teljesitesDatum`: a fiscal fact of the document for which
    /// no default can be right, so it is never defaulted.
    pub(super) fulfillment_date: Date,
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
    pub(super) fn from_verified(
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
/// answer in this shape (`Szamlazz.Order.storno_invoice`'s `storno_verdict`,
/// `Szamlazz.Agent.storno`'s `unmanaged_storno_verdict`), and the handler
/// dispatches on it: proceed, read the storno number, or answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum StornoVerdict {
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

/// The `reversed` answer of both storno handlers: `storno_number` as the
/// read that named it did, absent when a best-effort read could not.
pub(super) fn reversed_response(number: &str, storno_number: Option<String>) -> StornoResponse {
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
pub(super) fn after_storno_lookup(
    outcome: StornoLookupOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<ControlFlow<StornoResponse>, Fault> {
    match outcome {
        StornoLookupOutcome::Absent => Ok(ControlFlow::Continue(())),
        StornoLookupOutcome::AlreadyReversed { storno_number } => Ok(ControlFlow::Break(
            reversed_response(number, Some(storno_number)),
        )),
        StornoLookupOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
        }
        StornoLookupOutcome::Api { code, message } => {
            Err(Fault::inconclusive_answer(code, message))
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
pub(super) fn storno_response(
    outcome: GatewayStornoOutcome,
    number: String,
    namespace: &Namespace,
) -> Result<StornoResponse, Fault> {
    Ok(match outcome {
        GatewayStornoOutcome::Reversed(storno) => {
            StornoResponse::new(StornoOutcome::Reversed, number).with_storno_number(storno.number)
        }
        GatewayStornoOutcome::AlreadyReversed { storno_number } => {
            StornoResponse::new(StornoOutcome::Reversed, number)
                .with_storno_number(storno_number)
        }
        GatewayStornoOutcome::NotStornoable => StornoResponse::new(StornoOutcome::Rejected, number)
            .with_code("not_stornoable")
            .with_message(
                "szamlazz.hu echoed the document unchanged: it cannot be reversed (only invoices can be stornoed)",
            ),
        GatewayStornoOutcome::Rejected { code, message } => {
            StornoResponse::new(StornoOutcome::Rejected, number)
                .with_code(code)
                .with_message(message)
        }
        GatewayStornoOutcome::CredentialsRejected { code, message } => {
            return Err(Fault::credentials_rejected(namespace, code, message));
        }
        GatewayStornoOutcome::Api { code, message } => {
            return Err(Fault::inconclusive_answer(code, message));
        }
        GatewayStornoOutcome::Unavailable { message } => {
            return Err(Fault::szlahu_down_answer(message));
        }
    })
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
pub(super) fn storno_number_from_hint(
    outcome: QueryOutcome,
    number: &str,
    namespace: &Namespace,
) -> Result<Option<String>, Fault> {
    match outcome {
        QueryOutcome::Found(found) if found.is_storno_of(number) => Ok(Some(found.number)),
        QueryOutcome::Found(_) | QueryOutcome::NotFound | QueryOutcome::Api { .. } => Ok(None),
        QueryOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
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
pub(super) fn storno_number_from_lookup(
    outcome: StornoLookupOutcome,
    namespace: &Namespace,
) -> Result<Option<String>, Fault> {
    match outcome {
        StornoLookupOutcome::AlreadyReversed { storno_number } => Ok(Some(storno_number)),
        StornoLookupOutcome::Absent | StornoLookupOutcome::Api { .. } => Ok(None),
        StornoLookupOutcome::CredentialsRejected { code, message } => {
            Err(Fault::credentials_rejected(namespace, code, message))
        }
    }
}

/// What a query by one of our external ids found.
///
/// Every caller matches all three variants: an issuing handler refuses a
/// [`Lookup::Collision`] as `conflict{external_id_collision}` (the newest
/// holder may hide a document of ours), `delete_proforma` answers
/// `not_deleted{external_id_collision}`, and only `get` (a read that must not
/// fail) reports the slot as absent.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Lookup {
    /// szamlazz.hu holds nothing under the id (code 7).
    Absent,
    /// A document that passed validation: ours, live or reversed.
    Ours(Box<FoundDocument>),
    /// A document that fails validation: another order or kind. Never
    /// trusted.
    Collision(Box<FoundDocument>),
}

impl Lookup {
    /// Classifies an answered query: another szamlazz.hu code is
    /// `unavailable` (nothing may be concluded), rejected credentials are
    /// `credentials_rejected`. (A query szamlazz.hu did not answer never
    /// reaches here: it is the read's `Unanswered`, retried by the read
    /// policy.)
    pub(super) fn classify(
        outcome: QueryOutcome,
        namespace: &Namespace,
        order: &OrderKey,
        kind: IssuedKind,
    ) -> Result<Self, Fault> {
        match outcome {
            QueryOutcome::NotFound => Ok(Self::Absent),
            QueryOutcome::Api { code, message } => Err(Fault::inconclusive_answer(code, message)),
            QueryOutcome::CredentialsRejected { code, message } => {
                Err(Fault::credentials_rejected(namespace, code, message))
            }
            QueryOutcome::Found(found) => {
                if found.is_ours(order, kind) {
                    Ok(Self::Ours(found))
                } else {
                    tracing::warn!(number = %found.number, kind = %kind, "external id collision");
                    Ok(Self::Collision(found))
                }
            }
        }
    }
}

/// The context-bound helpers, stamped out per Restate context type.
///
/// They are deliberately not generic over the SDK's sealed context traits: a
/// generic `async fn` over them trips rust-lang/rust#100013 (`Send` is not
/// general enough) inside the macro-generated dispatcher.
macro_rules! journal_helpers {
    ($module:ident, $ctx:ident) => {
        #[allow(
            dead_code,
            unused_imports,
            reason = "each context type uses a different subset of the helpers"
        )]
        pub(in crate::service) mod $module {
            use std::error::Error as StdError;
            use std::future::Future;
            use std::sync::Arc;

            use super::{Fault, Journaled, Lookup, StornoIntent};
            use crate::account::Accounts;
            use crate::config::{ValidatedWorkerConfig, WorkerConfig};
            use crate::contract::{IssuedKind, Selector};
            use crate::gateway::{
                QueryOutcome, StornoLookupOutcome, StornoOutcome as GatewayStornoOutcome,
                StornoStepRequest, Unanswered,
            };
            use crate::identity::{ExternalId, OrderKey};
            use crate::service::prologue::{self as decisions, Execution};
            use restate_sdk::context::{ContextSideEffects as _, RunFuture as _, RunRetryPolicy};
            use restate_sdk::errors::{HandlerError, TerminalError};
            use restate_sdk::prelude::$ctx;
            use restate_sdk::serde::Json;
            use tracing::Instrument as _;

            /// Runs one handler execution: the prologue, then `body` on the
            /// execution it built, the whole inside the execution span
            /// (`prologue::execution_span`), so every log line from the
            /// prologue's first step to the handler's answer carries the
            /// scope, the key, the invocation id and, once resolved, the
            /// account id. `key` is the Virtual Object key on the object
            /// contexts, `None` on the stateless service. The body takes the
            /// execution by value: nothing of it outlives the call.
            pub(in crate::service) async fn execute<T, F, Fut>(
                ctx: &$ctx<'_>,
                key: Option<&str>,
                accounts: &Accounts,
                config: &ValidatedWorkerConfig,
                body: F,
            ) -> Result<T, HandlerError>
            where
                F: FnOnce(Execution) -> Fut + Send,
                Fut: Future<Output = Result<T, HandlerError>> + Send,
            {
                let span = decisions::execution_span(ctx.scope(), key, ctx.invocation_id());
                async move {
                    let execution = prologue(ctx, accounts, config).await?;
                    body(execution).await
                }
                .instrument(span)
                .await
            }

            /// The prologue of every handler: pin → resolve →
            /// fetch → open. Runs inside the execution span [`execute`]
            /// opened, on which it records the account id once resolved.
            ///
            /// 1. **Pin** the namespace in a pure durable step (`namespace`):
            ///    a redeploy with a changed namespace cannot make a running
            ///    invocation issue under a new id.
            /// 2. **Resolve** the request's scope to its account in a durable
            ///    step named `account` under the resolve policy: unscoped and
            ///    unknown are journaled as data and become the terminal
            ///    `unknown_account`; an unavailable resolver (reporting so,
            ///    or silent past `prologue::CALL_DEADLINE`) is retryable and
            ///    journals nothing; exhaustion is `unavailable`.
            /// 3. **Fetch** the account's credentials outside the journal
            ///    (on every execution, including replays) with a short
            ///    in-process retry, each attempt bounded by the same
            ///    deadline, then terminal `unavailable`.
            /// 4. **Open** the gateway for this execution over a fresh client.
            async fn prologue(
                ctx: &$ctx<'_>,
                accounts: &Accounts,
                config: &ValidatedWorkerConfig,
            ) -> Result<Execution, HandlerError> {
                // 1. Pin.
                let pinned = {
                    let namespace = config.namespace.clone();
                    run_once(ctx, "namespace", move || async move { namespace }).await?
                };
                // The pin replaces the namespace alone, which no policy
                // invariant reads: the execution's settings are the validated
                // ones with the journaled namespace.
                let config = WorkerConfig {
                    namespace: pinned,
                    ..WorkerConfig::clone(config)
                };

                // 2. Resolve.
                let scope = ctx.scope().map(str::to_owned);
                let resolution = {
                    let accounts = accounts.clone();
                    run_retrying(
                        ctx,
                        "account",
                        config.resolve.run_retry_policy(),
                        move || async move { decisions::resolve(&accounts, scope.as_deref()).await },
                    )
                    .await
                    .map_err(|error| decisions::resolve_exhausted(&error))?
                };
                let account = decisions::account_of(resolution)?;
                decisions::record_account(&account);

                // 3. Fetch, outside the journal.
                let credentials = decisions::fetch_credentials(accounts, &account).await?;

                // 4. Open.
                let gateway = decisions::open(account, credentials)?;
                Ok(Execution { gateway, config })
            }

            /// Journals the result of `f` under `name`, executing it at most
            /// once per journal entry (`RunRetryPolicy::max_attempts(1)`):
            /// the pure `namespace` pin and the write steps that have no
            /// retry of their own (`delete-proforma-*`, `set-payments-*`)
            /// return every outcome as data, so a closure failure is a bug,
            /// not a retry. Reads go through [`run_reading`].
            pub(in crate::service) async fn run_once<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                f: F,
            ) -> Result<T, HandlerError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = T> + Send + 'ctx,
                T: Journaled + Send + 'static,
            {
                let Json(value) = ctx
                    .run(|| async move { Ok(Json(f().await)) })
                    .name(name)
                    .retry_policy(RunRetryPolicy::new().max_attempts(1))
                    .await?;
                Ok(value)
            }

            /// Journals the result of `f` under `name`, re-executing it under
            /// `policy` while it fails with `E`, the step's own "not
            /// settled" error, which the SDK treats as retryable. The whole
            /// handler replays to this entry after the policy's delay, so the
            /// closure begins again from its first line.
            ///
            /// # Errors
            ///
            /// The `TerminalError` the run ends with: exhaustion of the
            /// policy (500, carrying the last `E`'s message) or cancellation
            /// (409). The caller decides what it means.
            pub(in crate::service) async fn run_retrying<'ctx, T, E, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                policy: RunRetryPolicy,
                f: F,
            ) -> Result<T, TerminalError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, E>> + Send + 'ctx,
                T: Journaled + Send + 'static,
                E: StdError + Send + Sync + 'static,
            {
                let Json(value) = ctx
                    .run(|| async move { Ok(Json(f().await?)) })
                    .name(name)
                    .retry_policy(policy)
                    .await?;
                Ok(value)
            }

            /// A read-only durable step under the read policy: journals the
            /// answer of `f` under `name`, re-executing it while szamlazz.hu
            /// does not answer (`Unanswered`). Every answer is data; a read
            /// writes nothing, so a re-executed closure's answer is exactly as
            /// fresh as a first one.
            ///
            /// # Errors
            ///
            /// The `unavailable` fault of a read that ended without an answer
            /// (the read policy exhausted or the invocation cancelled),
            /// naming the step and the last failure. The caller attaches the
            /// document when it knows one.
            pub(in crate::service) async fn run_reading<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                exec: &Execution,
                f: F,
            ) -> Result<T, Fault>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, Unanswered>> + Send + 'ctx,
                T: Journaled + Send + 'static,
            {
                let name = name.into();
                run_retrying(ctx, name.clone(), exec.config.read.run_retry_policy(), f)
                    .await
                    .map_err(|error| super::read_exhausted(&name, &error))
            }

            /// A **best-effort** read under the read policy: [`run_reading`]
            /// for a step whose handler already knows its answer and only
            /// lacks a detail: the answer of `f` as `Some`, or `None` when the
            /// read policy is exhausted (logged at `warn` naming the step;
            /// [`super::best_effort`]).
            ///
            /// # Errors
            ///
            /// A cancellation of the invocation, as it came: never swallowed,
            /// so a cancelled invocation does not complete as if nothing had
            /// happened.
            pub(in crate::service) async fn run_best_effort<'ctx, T, F, Fut>(
                ctx: &$ctx<'ctx>,
                name: impl Into<String>,
                exec: &Execution,
                f: F,
            ) -> Result<Option<T>, TerminalError>
            where
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, Unanswered>> + Send + 'ctx,
                T: Journaled + Send + 'static,
            {
                let name = name.into();
                match run_retrying(ctx, name.clone(), exec.config.read.run_retry_policy(), f).await
                {
                    Ok(value) => Ok(Some(value)),
                    Err(error) => super::best_effort(&name, error).map(|()| None),
                }
            }

            /// Journaled query of document `number` (a verify), under the read
            /// policy.
            pub(in crate::service) async fn verify(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                number: &str,
            ) -> Result<QueryOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let number = number.to_owned();
                run_reading(ctx, name, exec, move || async move {
                    gateway.verify(&number).await
                })
                .await
            }

            /// Journaled query by external id, under the read policy.
            pub(in crate::service) async fn query_external_id(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                external_id: &ExternalId,
            ) -> Result<QueryOutcome, Fault> {
                let gateway = Arc::clone(&exec.gateway);
                let selector = Selector::ExternalId(external_id.as_str().to_owned());
                run_reading(ctx, name, exec, move || async move {
                    gateway.query(&selector).await
                })
                .await
            }

            /// Journaled query by one of our external ids, under the read
            /// policy, validated against the identity the document should
            /// have. A fault carries that identity.
            pub(in crate::service) async fn lookup(
                ctx: &$ctx<'_>,
                exec: &Execution,
                name: impl Into<String>,
                external_id: &ExternalId,
                order: &OrderKey,
                kind: IssuedKind,
            ) -> Result<Lookup, Fault> {
                let about = |fault: Fault| fault.about(order, Some(kind), external_id.as_str());
                let outcome = query_external_id(ctx, exec, name, external_id)
                    .await
                    .map_err(about)?;
                Lookup::classify(outcome, &exec.config.namespace, order, kind).map_err(about)
            }

            /// The storno lookup step: one read-only
            /// journaled query of the storno external id, under the read
            /// policy.
            pub(in crate::service) async fn lookup_storno(
                ctx: &$ctx<'_>,
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

            /// The storno step: one durable step under the
            /// issue policy's run retry policy, query-first on every execution
            /// (the query is inside the closure: a separate journaled query
            /// would replay its stale "nothing" on the retry and re-send).
            /// The request is rebuilt from the intent on every execution
            /// (the date included), so every send is byte-identical.
            ///
            /// # Errors
            ///
            /// The `TerminalError` the run ends with: exhaustion (500) or
            /// cancellation (409); the caller maps it to `outcome_unknown`
            /// about its document. Nothing is recorded: the next call's
            /// lookup finds whatever landed.
            pub(in crate::service) async fn storno_step(
                ctx: &$ctx<'_>,
                exec: &Execution,
                intent: &StornoIntent,
            ) -> Result<GatewayStornoOutcome, TerminalError> {
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
            /// order-number hint is the `SS` referencing it (step
            /// `hint-storno-{number}`, a best-effort read under the read
            /// policy, [`run_best_effort`]). Rejected credentials are a fault
            /// about the storno (`storno_id`); everything else the hint can
            /// answer is data ([`super::storno_number_from_hint`]).
            pub(in crate::service) async fn storno_number_of(
                ctx: &$ctx<'_>,
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
                super::storno_number_from_hint(outcome, number, &exec.config.namespace)
                    .map_err(|fault| fault.about(order, None, storno_id.as_str()).into())
            }

            /// The storno number of a reversed document no `Order` manages,
            /// when a storno of ours holds `{namespace}:by-number:{number}:storno`
            /// (step `lookup-storno-{number}`, the same entry the storno
            /// protocol's lookup step writes, which this path never reaches;
            /// a best-effort read under the read policy, [`run_best_effort`]).
            /// The only read that can name an unmanaged document's storno: it
            /// carries no order number for the hint. Rejected credentials are
            /// a fault; everything else is data
            /// ([`super::storno_number_from_lookup`]).
            pub(in crate::service) async fn storno_number_of_unmanaged(
                ctx: &$ctx<'_>,
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
                super::storno_number_from_lookup(outcome, &exec.config.namespace)
                    .map_err(Into::into)
            }
        }
    };
}

journal_helpers!(object, ObjectContext);
journal_helpers!(shared, SharedObjectContext);
journal_helpers!(service, Context);
