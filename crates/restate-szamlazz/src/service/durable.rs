//! The durable steps of both services over the [`Runner`] seam, written once:
//! the execution ([`execute`]: the span, the prologue, the body), the
//! prologue's four steps, the typed run helpers ([`run_once`],
//! [`run_retrying`], [`run_reading`], [`run_best_effort`]; every value that
//! crosses the seam is a [`Journaled`] type, encoded as the SDK's `Json<T>`
//! would), and the read and storno steps `Szamlazz.Order` and
//! `Szamlazz.Agent` share ([`verify`], [`lookup`], [`lookup_storno`],
//! [`storno_step`], the two best-effort storno-number reads).
//!
//! Until #133 these were a `journal_helpers!` macro stamped out per SDK
//! context type (`object`, `shared`, `service`), because a generic `async fn`
//! over the SDK's sealed context traits trips rust-lang/rust#100013; the
//! trait object sidesteps it (`runner`), and the offline suite (`paths`)
//! drives every fn here over a `FakeRunner`.

use std::convert::Infallible;
use std::error::Error as StdError;
use std::future::Future;
use std::sync::Arc;

use restate_sdk::errors::{HandlerError, TerminalError};
use tracing::Instrument as _;

use super::prologue::{self as decisions, Execution, Opener};
use super::runner::{BoxError, Runner, Step};
use super::support::{
    Fault, Journaled, Lookup, StornoIntent, best_effort, read_exhausted, storno_number_from_hint,
    storno_number_from_lookup,
};
use crate::account::Accounts;
use crate::config::{StepPolicy, WorkerConfig};
use crate::contract::{IssuedKind, Selector};
use crate::gateway::{
    QueryOutcome, StornoLookupOutcome, StornoOutcome as GatewayStornoOutcome, StornoStepRequest,
    Unanswered,
};
use crate::identity::{ExternalId, OrderKey};

/// How a run ended short of a value.
#[derive(Debug)]
pub(in crate::service) enum RunError {
    /// The run's own terminal error: exhaustion of its policy (500, carrying
    /// the last failure's message) or cancellation (409).
    Terminal(TerminalError),
    /// The journaled bytes do not decode as the step's type: a journal this
    /// deployment is not additive to (ADR 0005). Retryable, as the SDK's own
    /// deserialisation failure is: the invocation replays into the same
    /// failure until its attempts are spent, which leaves a rollback its
    /// window; never a fault, which would settle an invocation on an entry
    /// nobody could read.
    Undecodable(serde_json::Error),
}

impl RunError {
    /// The handler error: the fault `fault` builds from the run's terminal
    /// error, or an undecodable entry as the retryable error it is.
    pub(in crate::service) fn or_fault(
        self,
        fault: impl FnOnce(&TerminalError) -> Fault,
    ) -> HandlerError {
        match self {
            Self::Terminal(error) => fault(&error).into(),
            Self::Undecodable(error) => HandlerError::from(error),
        }
    }
}

impl From<RunError> for HandlerError {
    /// The run's terminal error as it came, or an undecodable entry as the
    /// retryable error it is.
    fn from(error: RunError) -> Self {
        match error {
            RunError::Terminal(error) => error.into(),
            RunError::Undecodable(error) => Self::from(error),
        }
    }
}

/// Runs one handler execution: the prologue, then `body` on the
/// execution it built, the whole inside the execution span
/// (`prologue::execution_span`), so every log line from the
/// prologue's first step to the handler's answer carries the
/// scope, the key, the invocation id and, once resolved, the
/// account id. The key is the runner's: the Virtual Object key on the object
/// contexts, `None` on the stateless service. The body takes the
/// execution by value: nothing of it outlives the call.
pub(in crate::service) async fn execute<T, F, Fut>(
    runner: &dyn Runner,
    accounts: &Accounts,
    config: &WorkerConfig,
    opener: &Opener,
    body: F,
) -> Result<T, HandlerError>
where
    F: FnOnce(Execution) -> Fut + Send,
    Fut: Future<Output = Result<T, HandlerError>> + Send,
{
    let span = decisions::execution_span(runner.scope(), runner.key(), runner.invocation_id());
    async move {
        let execution = prologue(runner, accounts, config, opener).await?;
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
/// 4. **Open** the gateway for this execution over a fresh client
///    (`opener`; a deployment's is `Gateway::open`).
async fn prologue(
    runner: &dyn Runner,
    accounts: &Accounts,
    config: &WorkerConfig,
    opener: &Opener,
) -> Result<Execution, HandlerError> {
    // 1. Pin.
    let pinned = {
        let namespace = config.namespace.clone();
        run_once(runner, "namespace", move || async move { namespace }).await?
    };
    let config = WorkerConfig {
        namespace: pinned,
        ..config.clone()
    };

    // 2. Resolve.
    let scope = runner.scope().map(str::to_owned);
    let resolution = {
        let accounts = accounts.clone();
        run_retrying(
            runner,
            "account",
            config.resolve.step_policy(),
            move || async move { decisions::resolve(&accounts, scope.as_deref()).await },
        )
        .await
        .map_err(|error| error.or_fault(decisions::resolve_exhausted))?
    };
    let account = decisions::account_of(resolution)?;
    decisions::record_account(&account);

    // 3. Fetch, outside the journal.
    let credentials = decisions::fetch_credentials(accounts, &account).await?;

    // 4. Open.
    let gateway = opener.open(account, credentials)?;
    Ok(Execution { gateway, config })
}

/// The bytes a step journals for `value`: `serde_json::to_vec`, exactly what
/// the SDK's `Json<T>` writes, so an entry is byte-identical to one the SDK
/// serialised itself. A failure (a `Serialize` impl that errors; none of the
/// journaled types' can) is the step's retryable failure, as the SDK's own
/// serialisation error fails the step.
fn encode<T: Journaled>(value: &T) -> Result<Vec<u8>, BoxError> {
    serde_json::to_vec(value).map_err(BoxError::from)
}

/// `f` as a [`Step`]: its value encoded, its error boxed.
fn step<T, E, F, Fut>(f: F) -> Step
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Journaled + Send + 'static,
    E: StdError + Send + Sync + 'static,
{
    Box::new(move || {
        Box::pin(async move {
            let value = f().await.map_err(|error| BoxError::from(Box::new(error)))?;
            encode(&value)
        })
    })
}

/// Journals the result of `f` under `name`, executing it at most
/// once per journal entry ([`StepPolicy::ONCE`]):
/// the pure `namespace` pin and the write steps that have no
/// retry of their own (`delete-proforma-*`, `set-payments-*`)
/// return every outcome as data, so a closure failure is a bug,
/// not a retry. Reads go through [`run_reading`].
///
/// # Errors
///
/// The run's terminal error (a cancellation; the policy has no retry to
/// exhaust), or the retryable error of an entry the current types cannot
/// decode ([`RunError::Undecodable`]).
pub(in crate::service) async fn run_once<T, F, Fut>(
    runner: &dyn Runner,
    name: impl Into<String>,
    f: F,
) -> Result<T, HandlerError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = T> + Send + 'static,
    T: Journaled + Send + 'static,
{
    run_retrying(runner, name, StepPolicy::ONCE, move || async move {
        Ok::<T, Infallible>(f().await)
    })
    .await
    .map_err(HandlerError::from)
}

/// Journals the result of `f` under `name`, re-executing it under
/// `policy` while it fails with `E`, the step's own "not
/// settled" error, which the SDK treats as retryable. The whole
/// handler replays to this entry after the policy's delay, so the
/// closure begins again from its first line.
///
/// The value crosses the `Runner` seam as the bytes `serde_json::to_vec`
/// writes and `serde_json::from_slice` reads, what the SDK's `Json<T>` does
/// itself, so the journal entry is byte-identical to one the SDK serialised.
///
/// # Errors
///
/// The `TerminalError` the run ends with: exhaustion of the
/// policy (500, carrying the last `E`'s message) or cancellation
/// (409); the caller decides what it means. Or the journaled bytes not
/// decoding as `T`, which is retryable ([`RunError::Undecodable`]).
pub(in crate::service) async fn run_retrying<T, E, F, Fut>(
    runner: &dyn Runner,
    name: impl Into<String>,
    policy: StepPolicy,
    f: F,
) -> Result<T, RunError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, E>> + Send + 'static,
    T: Journaled + Send + 'static,
    E: StdError + Send + Sync + 'static,
{
    let bytes = runner
        .run(name.into(), policy, step(f))
        .await
        .map_err(RunError::Terminal)?;
    serde_json::from_slice(&bytes).map_err(RunError::Undecodable)
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
/// naming the step and the last failure, with `about` attaching the
/// document the caller was reading about (`identity` when it knows none).
/// Or the retryable error of an entry the current types cannot decode.
pub(in crate::service) async fn run_reading<T, F, Fut>(
    runner: &dyn Runner,
    name: impl Into<String>,
    exec: &Execution,
    about: impl FnOnce(Fault) -> Fault,
    f: F,
) -> Result<T, HandlerError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Unanswered>> + Send + 'static,
    T: Journaled + Send + 'static,
{
    let name = name.into();
    run_retrying(runner, name.clone(), exec.config.read.step_policy(), f)
        .await
        .map_err(|error| error.or_fault(|terminal| about(read_exhausted(&name, terminal))))
}

/// A **best-effort** read under the read policy: [`run_reading`]
/// for a step whose handler already knows its answer and only
/// lacks a detail: the answer of `f` as `Some`, or `None` when the
/// read policy is exhausted (logged at `warn` naming the step;
/// [`best_effort`]).
///
/// # Errors
///
/// A cancellation of the invocation, as it came: never swallowed,
/// so a cancelled invocation does not complete as if nothing had
/// happened. Or the retryable error of an entry the current types cannot
/// decode, likewise never swallowed.
pub(in crate::service) async fn run_best_effort<T, F, Fut>(
    runner: &dyn Runner,
    name: impl Into<String>,
    exec: &Execution,
    f: F,
) -> Result<Option<T>, HandlerError>
where
    F: FnOnce() -> Fut + Send + 'static,
    Fut: Future<Output = Result<T, Unanswered>> + Send + 'static,
    T: Journaled + Send + 'static,
{
    let name = name.into();
    match run_retrying(runner, name.clone(), exec.config.read.step_policy(), f).await {
        Ok(value) => Ok(Some(value)),
        Err(RunError::Terminal(error)) => best_effort(&name, error)
            .map(|()| None)
            .map_err(HandlerError::from),
        Err(error @ RunError::Undecodable(_)) => Err(error.into()),
    }
}

/// Journaled query of document `number` (a verify), under the read
/// policy; a fault carries what `about` attaches.
pub(in crate::service) async fn verify(
    runner: &dyn Runner,
    exec: &Execution,
    name: impl Into<String>,
    number: &str,
    about: impl FnOnce(Fault) -> Fault,
) -> Result<QueryOutcome, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let number = number.to_owned();
    run_reading(runner, name, exec, about, move || async move {
        gateway.verify(&number).await
    })
    .await
}

/// Journaled query by one of our external ids, under the read
/// policy, validated against the identity the document should
/// have. A fault carries that identity.
pub(in crate::service) async fn lookup(
    runner: &dyn Runner,
    exec: &Execution,
    name: impl Into<String>,
    external_id: &ExternalId,
    order: &OrderKey,
    kind: IssuedKind,
) -> Result<Lookup, HandlerError> {
    let about = |fault: Fault| fault.about(order, Some(kind), external_id.as_str());
    let gateway = Arc::clone(&exec.gateway);
    let selector = Selector::ExternalId(external_id.as_str().to_owned());
    let outcome = run_reading(runner, name, exec, about, move || async move {
        gateway.query(&selector).await
    })
    .await?;
    Lookup::classify(outcome, &exec.config.namespace, order, kind)
        .map_err(|fault| about(fault).into())
}

/// The storno lookup step: one read-only
/// journaled query of the storno external id, under the read
/// policy; a fault carries what `about` attaches.
pub(in crate::service) async fn lookup_storno(
    runner: &dyn Runner,
    exec: &Execution,
    intent: &StornoIntent,
    about: impl FnOnce(Fault) -> Fault,
) -> Result<StornoLookupOutcome, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let external_id = intent.storno_id.clone();
    let number = intent.number.clone();
    run_reading(
        runner,
        format!("lookup-storno-{number}"),
        exec,
        about,
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
/// The run's terminal error, exhaustion (500) or
/// cancellation (409), which the caller maps to `outcome_unknown`
/// about its document ([`RunError::or_fault`]); nothing is recorded: the
/// next call's lookup finds whatever landed. Or an undecodable entry,
/// retryable.
pub(in crate::service) async fn storno_step(
    runner: &dyn Runner,
    exec: &Execution,
    intent: &StornoIntent,
) -> Result<GatewayStornoOutcome, RunError> {
    let gateway = Arc::clone(&exec.gateway);
    let number = intent.number.clone();
    let external_id = intent.storno_id.clone();
    let comment = intent.comment.clone();
    let e_invoice = intent.e_invoice;
    let fulfillment_date = intent.fulfillment_date;
    run_retrying(
        runner,
        format!("storno-{}", intent.number),
        exec.config.issue.step_policy(),
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
/// answer is data ([`storno_number_from_hint`]).
pub(in crate::service) async fn storno_number_of(
    runner: &dyn Runner,
    exec: &Execution,
    order: &OrderKey,
    number: &str,
    storno_id: &ExternalId,
) -> Result<Option<String>, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let hinted = order.clone();
    let Some(outcome) = run_best_effort(
        runner,
        format!("hint-storno-{number}"),
        exec,
        move || async move { gateway.hint(&hinted).await },
    )
    .await?
    else {
        return Ok(None);
    };
    storno_number_from_hint(outcome, number, &exec.config.namespace)
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
/// ([`storno_number_from_lookup`]).
pub(in crate::service) async fn storno_number_of_unmanaged(
    runner: &dyn Runner,
    exec: &Execution,
    number: &str,
) -> Result<Option<String>, HandlerError> {
    let gateway = Arc::clone(&exec.gateway);
    let external_id = ExternalId::for_unmanaged_storno(&exec.config.namespace, number);
    let looked_up = number.to_owned();
    let Some(outcome) = run_best_effort(
        runner,
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
