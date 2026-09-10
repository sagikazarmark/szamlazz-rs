//! Run plumbing shared by the `Szamlazz.Order` and `Szamlazz.Agent`
//! handlers: the service-side [`Fault`] constructors and the fault →
//! `TerminalError` mapping, the [`Journaled`] marker with its one list, the
//! [`RunCtx`] trait over the SDK's three contexts and the `run_*` helpers
//! (once, retrying, reading, best effort) every durable step goes through,
//! the two journaled reads the handlers share (a verify by number, an
//! ownership lookup by one of our external ids) and the order key's parse.
//! No domain decision lives here:
//! the create protocol is `create`, the storno protocol `storno`, the
//! prologue `prologue`.

use std::error::Error as StdError;
use std::fmt;
use std::future::Future;
use std::sync::Arc;

use restate_sdk::context::{ContextSideEffects, RunFuture as _, RunRetryPolicy};
use restate_sdk::errors::{HandlerError, TerminalError};
use restate_sdk::prelude::{Context, ObjectContext, SharedObjectContext};
use restate_sdk::serde::Json;

use crate::account::BoxFuture;
use crate::contract::{IssuedKind, TerminalCode};
use crate::gateway::{
    FoundDocument, Gateway, OwnershipOutcome, QueryOutcome, SzamlazzAnswer, Unanswered,
};
use crate::identity::{ExternalId, Namespace, OrderKey};
use crate::service::prologue::Execution;

pub(super) use self::journaled::Journaled;
#[cfg(test)]
pub(super) use self::journaled::journaled_types;

/// The [`Journaled`] marker, its seal and the one list of its implementors.
///
/// Not a compatibility contract (ADR 0009: a journal entry is never decoded
/// by a later release). The list exists for one invariant: every type the
/// services can journal is one `service::journal` scans for the agent key and
/// the document body, since an entry is shown in the Restate UI for the
/// retention period. A type becomes journalable by being added here and in no
/// other way, and the same list is what the scan's registry is checked
/// against, so a type journaled without samples fails that test by name.
mod journaled {
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    use crate::gateway::{
        CreateOutcome, DeleteOutcome, LookupOutcome, OwnershipOutcome, ProbeOutcome, QueryOutcome,
        SetCreditEntriesOutcome, StornoLookupOutcome, StornoOutcome, TaxpayerOutcome,
    };
    use crate::identity::Namespace;
    use crate::service::prologue::Resolution;

    /// A type the services journal as the result of a `ctx.run`: the bound
    /// of the run helpers, so this list is exactly what the journal can hold.
    pub(in crate::service) trait Journaled:
        sealed::Sealed + Serialize + DeserializeOwned
    {
    }

    mod sealed {
        pub trait Sealed {}
    }

    macro_rules! journaled {
        ($($ty:ty),+ $(,)?) => {
            $(
                impl sealed::Sealed for $ty {}
                impl Journaled for $ty {}
            )+

            /// The implementors' names, as [`std::any::type_name`] writes them.
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
        OwnershipOutcome,
        LookupOutcome,
        CreateOutcome,
        StornoLookupOutcome,
        StornoOutcome,
        DeleteOutcome,
        SetCreditEntriesOutcome,
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
    pub(super) fn szamlazz_error(answer: SzamlazzAnswer) -> Self {
        Self::new(
            TerminalCode::SzamlazzError,
            format!("szamlazz.hu error {answer}"),
        )
        .with_szamlazz_code(answer.code)
    }

    /// [`Fault::szamlazz_error`] with what the handler was doing named before
    /// szamlazz.hu's answer (`the credit entries on invoice SZ-1 were
    /// refused: 259: …`), so the caller reads the subject first; the code
    /// travels in `szamlazz_code` as on every pass-through.
    pub(super) fn szamlazz_error_on(subject: impl fmt::Display, answer: SzamlazzAnswer) -> Self {
        Self::new(
            TerminalCode::SzamlazzError,
            format!("{subject}: szamlazz.hu error {answer}"),
        )
        .with_szamlazz_code(answer.code)
    }

    pub(super) fn unavailable(message: impl Into<String>) -> Self {
        Self::new(TerminalCode::Unavailable, message)
    }

    /// szamlazz.hu answered a read with a code the handler cannot conclude a
    /// document from (neither 7 nor a credential code). An answer, so it is
    /// journaled and never retried by the read policy; still a fault, since
    /// nothing may be concluded from it.
    pub(super) fn inconclusive_answer(answer: SzamlazzAnswer) -> Self {
        Self::unavailable(format!(
            "szamlazz.hu answered the query with code {answer}; nothing may be concluded; retry with a new Idempotency-Key or read get"
        ))
        .with_szamlazz_code(answer.code)
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
    /// (3, 135, 136 or 164). The message claims the outcome is not known,
    /// nothing more: szamlazz.hu answers these codes before acting, so the
    /// request it rejected was not acted on, but the rejection may be a
    /// post-send re-query's after a send with an open code, and an earlier
    /// execution's send may have landed. Pure; the warning that pages the
    /// operator is [`AnsweredCode::into_fault`]'s, the one way a handler
    /// raises this fault.
    fn credentials_rejected(answer: SzamlazzAnswer) -> Self {
        Self::new(
            TerminalCode::CredentialsRejected,
            format!(
                "szamlazz.hu rejected the agent credentials (code {answer}); the outcome is not known; fix the account's agent key, then retry with a new Idempotency-Key or read get"
            ),
        )
        .with_szamlazz_code(answer.code)
    }
}

/// szamlazz.hu answered a step with a code rather than a document, as the
/// handler reads it: the `CredentialsRejected` and `Api` variants every
/// gateway outcome carries, lifted out of the outcome at the site that
/// decides on it, with what the handler makes of another code. The one input
/// of [`AnsweredCode::into_fault`], so the code → fault mapping (and the
/// paging warning a credential code carries) is written once. Never
/// journaled: the outcome is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AnsweredCode {
    /// A credential code (3, 135, 136, 164): the worker's key is wrong, not
    /// the request; `credentials_rejected` (503).
    CredentialsRejected(SzamlazzAnswer),
    /// Another code the handler cannot conclude a document from (every read
    /// and write step of `Szamlazz.Order`, the verifies, the storno
    /// protocol): `unavailable` (503) naming it.
    Inconclusive(SzamlazzAnswer),
    /// Another code the handler passes through rather than concludes from
    /// (`Szamlazz.Agent.query`, `query_taxpayer`): `szamlazz_error` (422)
    /// with szamlazz.hu's message.
    PassedThrough(SzamlazzAnswer),
}

impl AnsweredCode {
    /// The fault for the code: the one mapping from a szamlazz.hu answer that
    /// is not a document onto a fault, and the home of the warning that pages
    /// the operator on a credential code (tagged with the namespace and the
    /// code, never the key), emitted here and nowhere else. The mapping is
    /// the side effect: calling it on a credential code pages, whether or not
    /// the fault is then raised, and nothing else does, so the `Fault`
    /// constructors stay pure and a handler that reads a credential code
    /// pages exactly once, at the site that decides on it. The caller attaches
    /// the document identity it knows ([`Fault::about`]).
    pub(super) fn into_fault(self, namespace: &Namespace) -> Fault {
        match self {
            Self::CredentialsRejected(answer) => {
                tracing::warn!(
                    namespace = %namespace,
                    code = %answer.code,
                    "szamlazz.hu rejected the agent credentials; fix the account's agent key"
                );
                Fault::credentials_rejected(answer)
            }
            Self::Inconclusive(answer) => Fault::inconclusive_answer(answer),
            Self::PassedThrough(answer) => Fault::szamlazz_error(answer),
        }
    }
}

/// A fault that cannot be represented as an SDK terminal error.
///
/// An unknown public fault code has no inferred HTTP status. This error is
/// an internal conversion failure, not a classification of that fault or
/// advice to retry the request that originally produced it.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FaultConversionError {
    /// This version does not know the code's HTTP status.
    #[error("cannot emit fault with unknown HTTP status for code {code}")]
    UnknownStatus {
        /// The unclassified code, preserved verbatim.
        code: TerminalCode,
    },
    /// The fault could not be encoded as JSON.
    #[error("cannot encode fault JSON: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// The SDK's terminal error carrying a known fault: the code's status and
/// the complete fault JSON as the message. Unknown codes fail conversion;
/// no terminal status is invented for a fault decoded from a newer worker.
impl TryFrom<Fault> for TerminalError {
    type Error = FaultConversionError;

    fn try_from(fault: Fault) -> Result<Self, Self::Error> {
        let Some(status) = fault.status() else {
            return Err(FaultConversionError::UnknownStatus { code: fault.code });
        };
        let body = serde_json::to_string(&fault)?;
        Ok(Self::new_with_code(status, body))
    }
}

/// Service constructors emit known codes. If that invariant is broken,
/// surface a retryable internal conversion failure to Restate rather than
/// completing the invocation with a fabricated terminal status.
impl From<Fault> for HandlerError {
    fn from(fault: Fault) -> Self {
        match TerminalError::try_from(fault) {
            Ok(terminal) => terminal.into(),
            Err(internal) => internal.into(),
        }
    }
}

/// The fault of a read step that ended without an answer: the read policy is
/// exhausted (500, carrying the last `Unanswered`'s message) or the
/// invocation was cancelled (409). Both are the `unavailable` fault (the
/// fault vocabulary is the seven codes, and a cancelled read sent nothing,
/// so nothing is unknown about the document), but the message tells them
/// apart: an exhausted read says to retry, a cancelled one does not, since
/// Restate's guidance on a cancellation is that the caller does not retry it.
/// The caller attaches the document it was reading about when it knows one.
pub(super) fn read_exhausted(step: &str, error: &TerminalError) -> Fault {
    if let Some(fault) = initialization_fault(
        error,
        "retry with a new Idempotency-Key, query the document or read get",
    ) {
        return fault;
    }
    if is_cancelled(error) {
        return Fault::unavailable(format!(
            "the {step} read was cancelled ({}) before szamlazz.hu answered; nothing was sent",
            error.code()
        ));
    }
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
/// cancellation from an exhausted policy. The SDK exports no constant for it;
/// the e2e's cancellation mid-send
/// (`a_cancellation_mid_send_is_outcome_unknown_and_releases_the_key`) is
/// what holds this one to a real cancelled run.
const CANCELLED: u16 = 409;

/// Whether the `TerminalError` a run ended with is the SDK's cancellation
/// ([`CANCELLED`]) rather than an exhausted retry policy. What every mapping
/// of a run's error onto a fault asks first, so a cancelled invocation is
/// never told to retry as if its policy had run out.
pub(super) fn is_cancelled(error: &TerminalError) -> bool {
    error.code() == CANCELLED
}

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
    if is_cancelled(&error) {
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
/// verify cannot conclude from, a credential code as `credentials_rejected`
/// ([`AnsweredCode::into_fault`]). Shared by every verify: `Szamlazz.Order`'s
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
        QueryOutcome::Api(answer) => Err(AnsweredCode::Inconclusive(answer).into_fault(namespace)),
        QueryOutcome::CredentialsRejected(answer) => {
            Err(AnsweredCode::CredentialsRejected(answer).into_fault(namespace))
        }
    }
}

/// The one thing the helpers below need of a Restate context: its scope, its
/// key, its invocation id and a journaled run. Implemented for the SDK's
/// three context types, so the helpers are plain generic fns rather than
/// three stamps of a macro.
///
/// Why a trait of our own and not the SDK's `ContextSideEffects`: its `run`
/// returns an `impl RunFuture` from a trait method, whose `Send`-ness a
/// generic caller cannot see (rust-lang/rust#100013, "`Send` is not general
/// enough" inside the handler dispatcher). [`RunCtx::run`] returns a boxed
/// `Send` future, which is `Send` for every caller.
///
/// **Step names.** A run's name is what the Restate UI, `sys_journal` and the
/// e2e's step-name table show: kebab-case, `{verb}-{object}[-{parameter}]`
/// (`lookup-invoice`, `create-proforma`, `verify-original-{number}`,
/// `storno-{number}`, `lookup-taxpayer-{prefix}`, `set-credit-entries-{number}`)
/// or a bare noun where the step is the handler's one read of that thing
/// (`namespace`, `account`, `probe`, `query`). The verb is the gateway's
/// question: `lookup-{kind}` is every ownership read of one of the order's
/// external ids, whatever the handler then decides (an exclusivity check, a
/// proforma link, `get`'s four reads, the lookup step; the table lists which
/// handler journals it where), `verify-*` a read by number, `hint-*` a
/// best-effort read, `create-*` / `storno-*` / `delete-*` / `set-*` a write. A
/// `{number}` / `{prefix}` parameter is bounded (the contract's
/// `InvoiceNumber`, the `TaxpayerPrefix`) so the name is. The table in
/// `tests/e2e/harness/run_names.rs` lists every name in order.
pub(in crate::service) trait RunCtx<'ctx>: Sync {
    /// The scope the request arrived under, `None` when unscoped.
    fn scope(&self) -> Option<&str>;
    /// The Virtual Object key on an object context; `None` on the stateless
    /// service's.
    fn key(&self) -> Option<&str>;
    /// The invocation id, as the ingress returns it in `x-restate-id`.
    fn invocation_id(&self) -> &str;
    /// Journals the result of `f` under `name`, re-executing it under `policy`
    /// while it fails with a retryable error; the SDK's `ctx.run` with a JSON
    /// result.
    fn run<T, F, Fut>(
        &self,
        name: String,
        policy: RunRetryPolicy,
        f: F,
    ) -> BoxFuture<'ctx, Result<T, TerminalError>>
    where
        T: Journaled + Send + 'static,
        F: FnOnce() -> Fut + Send + 'ctx,
        Fut: Future<Output = Result<T, HandlerError>> + Send + 'ctx;
}

macro_rules! run_ctx {
    ($ctx:ident, |$this:ident| $key:expr) => {
        impl<'ctx> RunCtx<'ctx> for $ctx<'ctx> {
            fn scope(&self) -> Option<&str> {
                $ctx::scope(self)
            }

            fn key(&self) -> Option<&str> {
                let $this = self;
                $key
            }

            fn invocation_id(&self) -> &str {
                $ctx::invocation_id(self)
            }

            fn run<T, F, Fut>(
                &self,
                name: String,
                policy: RunRetryPolicy,
                f: F,
            ) -> BoxFuture<'ctx, Result<T, TerminalError>>
            where
                T: Journaled + Send + 'static,
                F: FnOnce() -> Fut + Send + 'ctx,
                Fut: Future<Output = Result<T, HandlerError>> + Send + 'ctx,
            {
                let run = ContextSideEffects::run(self, || async move { Ok(Json(f().await?)) })
                    .name(name)
                    .retry_policy(policy);
                Box::pin(async move {
                    let Json(value) = run.await?;
                    Ok(value)
                })
            }
        }
    };
}

run_ctx!(ObjectContext, |ctx| Some(ctx.key()));
run_ctx!(SharedObjectContext, |ctx| Some(ctx.key()));
run_ctx!(Context, |_ctx| None);

/// Journals the result of `f` under `name`, executing it at most once per
/// journal entry (`RunRetryPolicy::max_attempts(1)`): the pure `namespace`
/// pin returns its outcome as data, so a closure failure is a bug, not a retry.
/// One-shot writes use [`run_retrying`] with one execution so their sites can
/// map a run cancellation to an operation-specific fault. Reads go through
/// [`run_reading`].
pub(in crate::service) async fn run_once<'ctx, C, T, F, Fut>(
    ctx: &C,
    name: impl Into<String>,
    f: F,
) -> Result<T, HandlerError>
where
    C: RunCtx<'ctx>,
    F: FnOnce() -> Fut + Send + 'ctx,
    Fut: Future<Output = T> + Send + 'ctx,
    T: Journaled + Send + 'static,
{
    let value = ctx
        .run(
            name.into(),
            RunRetryPolicy::new().max_attempts(1),
            || async move { Ok(f().await) },
        )
        .await?;
    Ok(value)
}

/// Journals the result of `f` under `name`, re-executing it under `policy`
/// while it fails with `E`, the step's own "not settled" error, which the SDK
/// treats as retryable. The whole handler replays to this entry after the
/// policy's delay, so the closure begins again from its first line.
///
/// # Errors
///
/// The `TerminalError` the run ends with: exhaustion of the policy (500,
/// carrying the last `E`'s message) or cancellation (409). The caller decides
/// what it means.
pub(in crate::service) async fn run_retrying<'ctx, C, T, E, F, Fut>(
    ctx: &C,
    name: impl Into<String>,
    policy: RunRetryPolicy,
    f: F,
) -> Result<T, TerminalError>
where
    C: RunCtx<'ctx>,
    F: FnOnce() -> Fut + Send + 'ctx,
    Fut: Future<Output = Result<T, E>> + Send + 'ctx,
    T: Journaled + Send + 'static,
    E: StdError + Send + Sync + 'static,
{
    ctx.run(name.into(), policy, || async move { Ok(f().await?) })
        .await
}

/// The operation boundary: initialization executes only when the SDK runs
/// the closure. Its sanitized terminal failure completes this Run command,
/// bypassing the operation's retry policy without replacing a recorded Run
/// with an Output command. It says nothing about prior executions' sends.
pub(in crate::service) async fn run_operating<'ctx, C, T, E, F, Fut>(
    ctx: &C,
    name: impl Into<String>,
    policy: RunRetryPolicy,
    exec: &Execution,
    f: F,
) -> Result<T, TerminalError>
where
    C: RunCtx<'ctx>,
    F: FnOnce(Arc<Gateway>) -> Fut + Send + 'ctx,
    Fut: Future<Output = Result<T, E>> + Send + 'ctx,
    T: Journaled + Send + 'static,
    E: StdError + Send + Sync + 'static,
{
    let gateway = exec.gateway();
    ctx.run(name.into(), policy, move || async move {
        let gateway = gateway.await.map_err(HandlerError::from)?;
        Ok(f(gateway).await?)
    })
    .await
}

/// Only initialization emits a terminal 503 from an operation closure;
/// exhaustion is 500 and cancellation is 409. Preserve its structured fault
/// instead of wrapping it as issue-policy exhaustion or a lost answer.
pub(super) fn initialization_fault(error: &TerminalError, next: &str) -> Option<Fault> {
    (error.code() == 503)
        .then(|| serde_json::from_str::<Fault>(error.message()).ok())
        .flatten()
        .filter(|fault| fault.code == TerminalCode::Unavailable)
        .map(|mut fault| {
            fault.message = format!("{}; {next}", fault.message);
            fault
        })
}

/// A read-only durable step under the read policy: journals the answer of
/// `f` under `name`, re-executing it while szamlazz.hu does not answer
/// (`Unanswered`). Every answer is data; a read writes nothing, so a
/// re-executed closure's answer is exactly as fresh as a first one.
///
/// # Errors
///
/// The `unavailable` fault of a read that ended without an answer (the read
/// policy exhausted or the invocation cancelled), naming the step and the
/// last failure. The caller attaches the document when it knows one.
/// Initialization failure is also `unavailable`, without spending the read policy.
pub(in crate::service) async fn run_reading<'ctx, C, T, F, Fut>(
    ctx: &C,
    name: impl Into<String>,
    exec: &Execution,
    f: F,
) -> Result<T, Fault>
where
    C: RunCtx<'ctx>,
    F: FnOnce(Arc<Gateway>) -> Fut + Send + 'ctx,
    Fut: Future<Output = Result<T, Unanswered>> + Send + 'ctx,
    T: Journaled + Send + 'static,
{
    let name = name.into();
    run_operating(
        ctx,
        name.clone(),
        exec.config.read.run_retry_policy(),
        exec,
        f,
    )
    .await
    .map_err(|error| read_exhausted(&name, &error))
}

/// A **best-effort** read under the read policy: [`run_reading`] for a step
/// whose handler already knows its answer and only lacks a detail: the answer
/// of `f` as `Some`, or `None` when initialization fails or the read policy is exhausted (logged at
/// `warn` naming the step; [`best_effort`]).
///
/// # Errors
///
/// A cancellation of the invocation, as it came: never swallowed, so a
/// cancelled invocation does not complete as if nothing had happened.
pub(in crate::service) async fn run_best_effort<'ctx, C, T, F, Fut>(
    ctx: &C,
    name: impl Into<String>,
    exec: &Execution,
    f: F,
) -> Result<Option<T>, TerminalError>
where
    C: RunCtx<'ctx>,
    F: FnOnce(Arc<Gateway>) -> Fut + Send + 'ctx,
    Fut: Future<Output = Result<T, Unanswered>> + Send + 'ctx,
    T: Journaled + Send + 'static,
{
    let name = name.into();
    match run_operating(
        ctx,
        name.clone(),
        exec.config.read.run_retry_policy(),
        exec,
        f,
    )
    .await
    {
        Ok(value) => Ok(Some(value)),
        Err(error) => best_effort(&name, error).map(|()| None),
    }
}

/// Journaled query of document `number` (a verify), under the read policy.
pub(in crate::service) async fn verify<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    name: impl Into<String>,
    number: &str,
) -> Result<QueryOutcome, Fault> {
    let number = number.to_owned();
    run_reading(ctx, name, exec, move |gateway| async move {
        gateway.verify(&number).await
    })
    .await
}

/// Journaled query by one of our external ids, validated against the
/// identity the document should have ([`Gateway::lookup_ours`]), under the
/// read policy. Every answer is data for the caller to decide on; an
/// exhausted read is the `unavailable` fault carrying that identity.
///
/// [`Gateway::lookup_ours`]: crate::gateway::Gateway::lookup_ours
pub(in crate::service) async fn lookup<'ctx, C: RunCtx<'ctx>>(
    ctx: &C,
    exec: &Execution,
    name: impl Into<String>,
    external_id: &ExternalId,
    order: &OrderKey,
    kind: IssuedKind,
) -> Result<OwnershipOutcome, Fault> {
    let id = external_id.clone();
    let looked_up = order.clone();
    run_reading(ctx, name, exec, move |gateway| async move {
        gateway.lookup_ours(&id, &looked_up, kind).await
    })
    .await
    .map_err(|fault| fault.about(order, Some(kind), external_id))
}

#[cfg(test)]
mod tests {
    use restate_sdk::errors::TerminalError;

    use super::*;
    use crate::test_support::LogCapture;

    fn namespace() -> Namespace {
        "acct".parse().expect("namespace")
    }

    #[test]
    fn faults_serialise_their_code_and_status() {
        let order = OrderKey::parse("ORD-1").expect("order");
        let fault = Fault::outcome_unknown("exhausted").about(
            &order,
            Some(IssuedKind::Invoice),
            &ExternalId::new("acct:ORD-1:invoice"),
        );
        let error = TerminalError::try_from(fault).expect("known fault");
        assert_eq!(error.code(), 500);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], TerminalCode::OutcomeUnknown.as_str());
        assert_eq!(body["order"], "ORD-1");
        assert_eq!(body["kind"], "invoice");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice");
        assert_eq!(body["gen"], serde_json::Value::Null);
        assert_eq!(body["request_id"], serde_json::Value::Null);

        let cases = [
            (Fault::invalid_input("x"), 400, "invalid_input"),
            (Fault::unavailable("x"), 503, "unavailable"),
            (Fault::missing_fulfillment_date("SZ-1"), 503, "unavailable"),
            (
                AnsweredCode::CredentialsRejected(SzamlazzAnswer::new("3", "x"))
                    .into_fault(&namespace()),
                503,
                "credentials_rejected",
            ),
            (Fault::unknown_account("x"), 400, "unknown_account"),
            (Fault::not_found("x"), 404, "not_found"),
            (
                Fault::szamlazz_error(SzamlazzAnswer::new("152", "x")),
                422,
                "szamlazz_error",
            ),
        ];
        for (fault, status, code) in cases {
            let error = TerminalError::try_from(fault).expect("known fault");
            assert_eq!(error.code(), status);
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            assert_eq!(body["code"], code);
            assert_eq!(body["order"], serde_json::Value::Null);
        }
    }

    #[test]
    fn every_known_fault_converts_without_changing_its_body() {
        for code in TerminalCode::KNOWN {
            let fault = Fault::new(code, "known fault").with_szamlazz_code("999");
            let expected_status = fault.status().expect("known status");
            let terminal = TerminalError::try_from(fault.clone()).expect("known fault");
            assert_eq!(terminal.code(), expected_status);
            assert_eq!(
                serde_json::from_str::<Fault>(terminal.message()).expect("fault"),
                fault
            );
            let handler = HandlerError::from(fault);
            let error: &(dyn StdError + 'static) = handler.as_ref();
            assert!(error.to_string().starts_with("Terminal error"), "{error}");
        }
    }

    #[test]
    fn an_unknown_decoded_fault_cannot_acquire_a_terminal_status() {
        let fault: Fault =
            serde_json::from_str(r#"{"code":"future_fault/é","message":"new worker decision"}"#)
                .expect("open fault");
        let error = TerminalError::try_from(fault.clone()).expect_err("no known status");
        assert!(matches!(
            error,
            FaultConversionError::UnknownStatus { ref code } if code == &fault.code
        ));
        let handler = HandlerError::from(fault);
        let source: &(dyn StdError + 'static) = handler.as_ref();
        assert!(
            source.to_string().starts_with("Retryable error"),
            "{source}"
        );
        assert!(matches!(
            source
                .source()
                .expect("conversion failure")
                .downcast_ref::<FaultConversionError>(),
            Some(FaultConversionError::UnknownStatus { .. })
        ));
    }

    /// A szamlazz.hu code never travels in `code` (that field carries a
    /// `TerminalCode` token), but in `szamlazz_code`, beside it, on every
    /// fault a szamlazz.hu answer caused; faults that no szamlazz.hu answer
    /// caused carry no `szamlazz_code` at all.
    #[test]
    fn a_szamlazz_code_travels_in_its_own_field() {
        let error = TerminalError::try_from(Fault::szamlazz_error(SzamlazzAnswer::new(
            "152",
            "Már létezik ilyen rendelésszámú számla.",
        )))
        .expect("known fault");
        assert_eq!(error.code(), 422);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "szamlazz_error");
        assert_eq!(body["szamlazz_code"], "152");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("152"), "{message}");
        assert!(
            message.contains("Már létezik ilyen rendelésszámú számla."),
            "{message}"
        );

        for fault in [
            Fault::invalid_input("x"),
            Fault::not_found("x"),
            Fault::unavailable("x"),
            Fault::szlahu_down_answer("x"),
            Fault::outcome_unknown("x"),
            Fault::unknown_account("x"),
            Fault::missing_fulfillment_date("SZ-1"),
        ] {
            let error = TerminalError::try_from(fault).expect("known fault");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            assert_eq!(body["szamlazz_code"], serde_json::Value::Null, "{body}");
        }
    }

    /// The one mapping from a szamlazz.hu code onto a fault, as a table: a
    /// credential code is `credentials_rejected` (503), another code the
    /// handler cannot conclude from is `unavailable` (503), another code the
    /// handler passes through is `szamlazz_error` (422); each carries the
    /// szamlazz.hu code in `szamlazz_code` and its message in the text, and
    /// the credential row's message tells the caller the outcome is not known
    /// (never that "this attempt issued nothing", which a post-send re-query
    /// can make false, #63). The warning that pages the operator is emitted
    /// by exactly the credential row, tagged with the namespace and the code,
    /// and by nothing else: the mapping pages, the `Fault` constructors do
    /// not (the warm-up below drops a credential fault and pages all the
    /// same, which is what it is for). Every site that decides on a gateway
    /// outcome routes its two answered variants here (the sites' own tests
    /// assert the routing).
    #[test]
    fn an_answered_code_maps_onto_its_fault_and_only_a_credential_code_pages() {
        let capture = LogCapture::default();
        let guard = capture.subscribe();
        drop(
            AnsweredCode::CredentialsRejected(SzamlazzAnswer::new("0", "warm-up"))
                .into_fault(&"warmup".parse().expect("namespace")),
        );
        LogCapture::rebuild_interest();

        let table = [
            (
                AnsweredCode::CredentialsRejected(SzamlazzAnswer::new(
                    "136",
                    "Bejelentkezés letiltva",
                )),
                503,
                TerminalCode::CredentialsRejected,
                "136",
                "the outcome is not known",
            ),
            (
                AnsweredCode::Inconclusive(SzamlazzAnswer::new("57", "Hibás XML.")),
                503,
                TerminalCode::Unavailable,
                "57",
                "nothing may be concluded",
            ),
            (
                AnsweredCode::PassedThrough(SzamlazzAnswer::new(
                    "OPERATION_FAILED",
                    "A NAV szolgáltatás nem elérhető.",
                )),
                422,
                TerminalCode::SzamlazzError,
                "OPERATION_FAILED",
                "szamlazz.hu error",
            ),
        ];
        for (code, status, terminal, szamlazz_code, phrase) in table {
            let label = format!("{code:?}");
            let error =
                TerminalError::try_from(code.into_fault(&namespace())).expect("known fault");
            assert_eq!(error.code(), status, "{label}");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            assert_eq!(body["code"], terminal.as_str(), "{label}: {body}");
            assert_eq!(body["szamlazz_code"], szamlazz_code, "{label}: {body}");
            let message = body["message"].as_str().expect("message");
            assert!(message.contains(szamlazz_code), "{label}: {message}");
            assert!(message.contains(phrase), "{label}: {message}");
            assert!(
                message.contains("Idempotency-Key") || terminal == TerminalCode::SzamlazzError,
                "{label}: {message}"
            );
            assert!(!message.contains("attempt"), "{label}: {message}");
            assert!(!message.contains("issued nothing"), "{label}: {message}");
            assert_eq!(
                body["order"],
                serde_json::Value::Null,
                "{label}: nothing attached"
            );
        }
        drop(guard);

        let logs = capture.logs();
        let warnings: Vec<&str> = logs
            .lines()
            .filter(|line| line.contains("WARN") && !line.contains("warmup"))
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "exactly the credential row pages: {logs}"
        );
        assert!(warnings[0].contains("namespace=acct"), "{}", warnings[0]);
        assert!(warnings[0].contains("code=136"), "{}", warnings[0]);
        assert!(
            warnings[0].contains("fix the account's agent key"),
            "{}",
            warnings[0]
        );
    }

    /// The document identity a handler attaches to an answered code's fault
    /// travels beside the code: order, kind and external id.
    #[test]
    fn credentials_rejected_fault_carries_the_document_when_attached() {
        let order = OrderKey::parse("ORD-1").expect("order");
        let fault =
            AnsweredCode::CredentialsRejected(SzamlazzAnswer::new("136", "Bejelentkezés letiltva"))
                .into_fault(&namespace())
                .about(
                    &order,
                    Some(IssuedKind::Invoice),
                    &ExternalId::new("acct:ORD-1:invoice"),
                );
        let error = TerminalError::try_from(fault).expect("known fault");
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "credentials_rejected");
        assert_eq!(body["szamlazz_code"], "136");
        assert_eq!(body["order"], "ORD-1");
        assert_eq!(body["kind"], "invoice");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice");
    }

    /// A read step that ended without an answer (the read policy exhausted
    /// (500 carrying the last `Unanswered`) or the invocation cancelled (409))
    /// is the `unavailable` fault naming the step and the last failure, about
    /// the document when the caller attaches one.
    #[test]
    fn an_exhausted_read_is_a_structured_unavailable() {
        let last = TerminalError::new_with_code(
            500,
            "transport failure: error decoding response body: empty response",
        );
        let fault = read_exhausted("lookup-invoice", &last);
        let error = TerminalError::try_from(fault.clone()).expect("known fault");
        assert_eq!(error.code(), 503);
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("lookup-invoice"), "{message}");
        assert!(
            message.contains("empty response"),
            "names the last failure: {message}"
        );
        assert!(message.contains("500"), "{message}");
        assert!(message.contains("Idempotency-Key"), "{message}");
        assert_eq!(
            body["order"],
            serde_json::Value::Null,
            "nothing attached yet"
        );

        let order = OrderKey::parse("ORD-1").expect("order");
        let about = fault.about(
            &order,
            Some(IssuedKind::Invoice),
            &ExternalId::new("acct:ORD-1:invoice"),
        );
        let error = TerminalError::try_from(about).expect("known fault");
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        assert_eq!(body["order"], "ORD-1");
        assert_eq!(body["kind"], "invoice");
        assert_eq!(body["external_id"], "acct:ORD-1:invoice");

        let cancelled = TerminalError::new_with_code(409, "cancelled");
        let error = TerminalError::try_from(read_exhausted("lookup-proforma", &cancelled))
            .expect("known fault");
        assert_eq!(error.code(), 503, "a cancellation is the same fault code");
        let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("lookup-proforma"), "{message}");
        assert!(
            message.contains("cancelled") && message.contains("(409)"),
            "names the cancellation: {message}"
        );
        assert!(
            !message.contains("retry"),
            "a cancelled request is not told to retry: {message}"
        );
    }

    /// The best-effort reads (the storno-number hint after a verify found the
    /// document already reversed, and `Szamlazz.Agent.storno`'s storno lookup in
    /// the same situation) swallow an exhausted read policy: the handler's
    /// answer (`reversed`) is already known, so the number is reported as unknown
    /// after a `warn` naming the step. They never swallow a cancellation: the SDK
    /// ends a cancelled run with 409, and an invocation told to stop must not
    /// answer `reversed` as if nothing had happened (#65); the error is
    /// propagated as it came.
    #[test]
    fn a_best_effort_read_swallows_exhaustion_but_propagates_a_cancellation() {
        let capture = LogCapture::default();
        let guard = capture.subscribe();
        drop(best_effort(
            "hint-storno-warmup",
            TerminalError::new_with_code(500, "warm-up"),
        ));
        LogCapture::rebuild_interest();

        let exhausted =
            TerminalError::new_with_code(500, "szamlazz.hu is unavailable: maintenance");
        best_effort("hint-storno-SZ-1", exhausted).expect("exhaustion is swallowed");

        let cancelled = TerminalError::new_with_code(409, "cancelled");
        let error =
            best_effort("hint-storno-SZ-1", cancelled).expect_err("a cancellation propagates");
        assert_eq!(error.code(), 409);
        assert_eq!(error.message(), "cancelled");
        drop(guard);

        let logs = capture.logs();
        let warnings: Vec<&str> = logs
            .lines()
            .filter(|line| line.contains("WARN") && !line.contains("warmup"))
            .collect();
        assert_eq!(
            warnings.len(),
            1,
            "the swallowed exhaustion warns, the cancellation does not: {logs}"
        );
        assert!(warnings[0].contains("hint-storno-SZ-1"), "{}", warnings[0]);
        assert!(warnings[0].contains("maintenance"), "{}", warnings[0]);
    }

    /// The Virtual Object key must arrive trimmed: Restate's per-key lock is on
    /// the *raw* key, so `ORD-1` and ` ORD-1` would be two instances with two
    /// locks mapping to one szamlazz.hu order and identical external ids; two
    /// concurrent creates under them would both pass their lookup and both send.
    /// The handler refuses a key whose trimmed form differs from the raw one as
    /// `invalid_input` naming the rule; [`OrderKey::parse`] itself stays lenient
    /// for the places that parse an order number rather than a key.
    #[test]
    fn the_order_key_must_arrive_trimmed() {
        let key = order_key("ORD-1").expect("a trimmed key");
        assert_eq!(key.as_str(), "ORD-1");
        let key = order_key("rendelés-42").expect("non-ASCII text in NFC is fine");
        assert_eq!(key.as_str(), "rendelés-42");

        for raw in [" ORD-1", "ORD-1 ", "\tORD-1", "ORD-1\n", "\u{a0}ORD-1"] {
            assert_eq!(
                OrderKey::parse(raw).expect("the type trims").as_str(),
                "ORD-1",
                "{raw:?}: OrderKey::parse stays lenient"
            );
            let fault = order_key(raw).expect_err("refused");
            let error = TerminalError::try_from(fault).expect("known fault");
            assert_eq!(error.code(), 400, "{raw:?}");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            assert_eq!(body["code"], "invalid_input", "{raw:?}");
            let message = body["message"].as_str().expect("message");
            assert!(
                message.contains("must not have leading or trailing whitespace"),
                "{raw:?}: names the rule: {message}"
            );
            assert_eq!(
                body["order"],
                serde_json::Value::Null,
                "{raw:?}: no order identity yet"
            );
        }

        // The type's own alphabet still applies to a trimmed key, with its
        // message naming the rule: no internal whitespace, no `:`, NFC, 40 bytes.
        let too_long = "x".repeat(OrderKey::MAX_LEN + 1);
        for (raw, rule) in [
            ("rendelés #42", "must not contain whitespace"),
            ("a\u{a0}b", "must not contain whitespace"),
            ("ORD:1", "must not contain ':'"),
            ("rendele\u{301}s-42", "must be in Unicode NFC"),
            (too_long.as_str(), "at most 40 are allowed"),
        ] {
            let fault = order_key(raw).expect_err(rule);
            let error = TerminalError::try_from(fault).expect("known fault");
            assert_eq!(error.code(), 400, "{raw:?}");
            let body: serde_json::Value = serde_json::from_str(error.message()).expect("json body");
            assert_eq!(body["code"], "invalid_input", "{raw:?}");
            assert!(
                body["message"].as_str().expect("message").contains(rule),
                "{raw:?}: names the rule: {body}"
            );
            assert_eq!(
                body["order"],
                serde_json::Value::Null,
                "{raw:?}: no order identity yet"
            );
        }
    }
}
