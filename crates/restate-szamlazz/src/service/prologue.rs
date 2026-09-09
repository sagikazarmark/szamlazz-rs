//! The prologue every handler runs after parsing its key: pin the
//! namespace, resolve the account, fetch its credentials, open the gateway.
//! This module holds the decisions of those steps: functions of their inputs
//! whose only effect is a log line, which is what can be unit-tested (the SDK
//! has no mock context; the durable behaviour is asserted end to end), and
//! the prologue's two calls into an embedder's trait objects, each under the
//! worker's deadline ([`CALL_DEADLINE`]): the resolve that is the body of the
//! `account` step's closure, and the credential fetch, the one step that runs
//! outside the journal. The durable steps themselves are
//! `support::run_prologue`, generic over `support::RunCtx`.

use std::borrow::Cow;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use restate_sdk::errors::TerminalError;
use serde::{Deserialize, Serialize};
use szamlazz_agent::Credentials;

use super::support::Fault;
use crate::account::{Account, Accounts, BoxError, FetchError, ResolveError};
use crate::config::{WorkerConfig, format_duration};
use crate::gateway::Gateway;

/// What one handler execution runs on: the gateway opened for this execution
/// and the deployment settings with the namespace pinned by the journal.
///
/// Built by the prologue, dropped with the execution: no gateway or client
/// outlives one handler execution.
#[derive(Debug)]
pub(super) struct Execution {
    pub(super) gateway: Arc<Gateway>,
    pub(super) config: WorkerConfig,
}

/// The span every handler execution runs in, from the handler's first line to
/// its answer: `execution{scope, order, restate.invocation.id, account.id}`.
///
/// Opened before the prologue's first step and left after the handler's last,
/// so every log line the execution emits (the prologue's own warnings, the
/// gateway steps' spans and events, the paging `credentials_rejected` warning)
/// is attributable to a scope, an account and an invocation from the
/// worker's log alone. `scope` is what the SDK saw (`<unscoped>` when
/// the request carried none, as the start-up log prints it); `order` is the
/// Virtual Object key, absent on the stateless `Szamlazz.Agent`; `account.id`
/// is [`tracing::field::Empty`] until the `account` step has answered and
/// [`record_account`] fills it. `restate.invocation.id` is the value the
/// ingress returns as `x-restate-id`, the caller's handle on the invocation.
/// Never the key: the account id is journaled and shown in the Restate UI
/// already, so logging it leaks nothing.
pub(super) fn execution_span(
    scope: Option<&str>,
    order: Option<&str>,
    invocation_id: &str,
) -> tracing::Span {
    tracing::info_span!(
        "execution",
        scope = %scope.unwrap_or("<unscoped>"),
        // `None` records nothing, like `Empty`.
        order = order.map(tracing::field::display),
        restate.invocation.id = %invocation_id,
        account.id = tracing::field::Empty,
    )
}

/// Records the resolved account's id on the current execution span: a no-op
/// outside one, since the field is declared there only.
pub(super) fn record_account(account: &Account) {
    tracing::Span::current().record("account.id", tracing::field::display(&account.id));
}

/// The journaled answer of the `account` step: the account, or the reason the
/// request names none. Data, so that unscoped and unknown are settled by the
/// journal and never retried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Resolution {
    /// The account the request names.
    Account(Box<Account>),
    /// The request carries no scope and the deployment serves no unscoped
    /// account.
    Unscoped,
    /// No account is reachable under the scope.
    Unknown {
        /// The scope the request carried.
        scope: String,
    },
}

/// The bound on each call into the account resolver or the credential store
/// (every `resolve`, every `fetch` attempt), after which the call is dropped
/// and answered as unavailable. The worker's own constant, not a setting: the
/// static resolver is in memory and never reaches it, and a database-backed
/// one that has not answered in ten seconds is not going to; waiting on would
/// only hold the execution until the handler's inactivity timeout, spending an
/// invocation attempt on a wait the resolve policy or the fetch loop is there
/// to retry.
pub(super) const CALL_DEADLINE: Duration = Duration::from_secs(10);

/// Which of the two calls into an embedder's trait objects the worker bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoundedCall {
    /// `AccountResolver::resolve`.
    Resolver,
    /// `CredentialStore::fetch`.
    Store,
}

impl BoundedCall {
    /// The subject of the timeout's display text.
    fn subject(self) -> &'static str {
        match self {
            Self::Resolver => "the account resolver",
            Self::Store => "the credential store",
        }
    }
}

/// A bounded call that had not answered at [`CALL_DEADLINE`]; its future was
/// dropped. Names the deadline, so that a fault built from it says what
/// happened; names nothing of the account.
#[derive(Debug, thiserror::Error)]
#[error("{} did not answer within {}", call.subject(), format_duration(CALL_DEADLINE))]
pub(super) struct TimedOut {
    /// The call that outlived the bound.
    call: BoundedCall,
}

/// Awaits `future` (one `call` into an embedder's trait object) for at most
/// [`CALL_DEADLINE`], dropping it at the deadline.
///
/// # Errors
///
/// [`TimedOut`] when the deadline passed without an answer.
async fn bounded<T>(call: BoundedCall, future: impl Future<Output = T>) -> Result<T, TimedOut> {
    tokio::time::timeout(CALL_DEADLINE, future)
        .await
        .map_err(|_elapsed| TimedOut { call })
}

/// The `account` step's one error: the resolver could not answer. Retryable
/// to the SDK, so the resolve policy re-executes the step; its display never
/// echoes the resolver's own message (it becomes `last_failure`, and the
/// exhausted step's fault text) but does tell a resolver that reported itself
/// unavailable from one the worker gave up waiting on.
#[derive(Debug, thiserror::Error)]
pub(super) enum ResolverUnavailable {
    /// The resolver answered `ResolveError::Unavailable`.
    #[error("the account resolver is unavailable")]
    Reported(#[source] BoxError),
    /// The resolver had not answered at [`CALL_DEADLINE`].
    #[error(transparent)]
    TimedOut(TimedOut),
}

/// Resolves the request's scope through `accounts` under [`CALL_DEADLINE`]:
/// the body of the `account` step's closure. Every answer of the resolver is
/// data; its unavailability, reported or by the deadline, is the retryable
/// error.
///
/// # Errors
///
/// [`ResolverUnavailable`]: the resolver answered `Unavailable`, or had not
/// answered at the deadline. Retryable: the resolve policy re-executes the
/// step.
pub(super) async fn resolve(
    accounts: &Accounts,
    scope: Option<&str>,
) -> Result<Resolution, ResolverUnavailable> {
    match bounded(BoundedCall::Resolver, accounts.resolve(scope)).await {
        Ok(result) => resolution(result),
        Err(timed_out) => Err(ResolverUnavailable::TimedOut(timed_out)),
    }
}

/// The `account` step's closure result: every answer of the resolver as
/// data, its unavailability as the retryable error.
///
/// Runs inside the step's closure, so it runs once per resolution and not on
/// a replay. A pure function of its input: the account carries no pin the
/// worker could advise on here; whether the key under a scope opens the
/// account the scope names is the operator's go-live check, not a runtime
/// signal.
pub(super) fn resolution(
    result: Result<Account, ResolveError>,
) -> Result<Resolution, ResolverUnavailable> {
    match result {
        Ok(account) => Ok(Resolution::Account(Box::new(account))),
        Err(ResolveError::Unscoped) => Ok(Resolution::Unscoped),
        Err(ResolveError::Unknown { scope }) => Ok(Resolution::Unknown { scope }),
        Err(ResolveError::Unavailable(source)) => Err(ResolverUnavailable::Reported(source)),
    }
}

/// The account of a journaled [`Resolution`]: unscoped and unknown are the
/// terminal fault `unknown_account` (HTTP 400): the request named no account
/// of this deployment, and no retry with the same request changes that.
pub(super) fn account_of(resolution: Resolution) -> Result<Account, Fault> {
    match resolution {
        Resolution::Account(account) => Ok(*account),
        Resolution::Unscoped => Err(Fault::unknown_account(
            "no account is reachable unscoped: this deployment serves accounts by scope, send the request under the account's scope (/restate/scope/{scope}/call/…)",
        )),
        Resolution::Unknown { scope } => Err(Fault::unknown_account(format!(
            "no account is reachable under scope {scope:?}"
        ))),
    }
}

/// The fault of an `account` step that ended without a resolution: the
/// resolve policy is exhausted (500) or the invocation was cancelled (409).
pub(super) fn resolve_exhausted(error: &TerminalError) -> Fault {
    Fault::unavailable(format!(
        "the account could not be resolved ({}): {}; retry with a new Idempotency-Key",
        error.code(),
        error.message()
    ))
}

/// Fetches of the credential store per handler execution, including the
/// first. Short by design: a prolonged store outage is a terminal
/// `unavailable`, not a handler retry.
const FETCH_ATTEMPTS: u32 = 3;
/// The pause before each re-fetch.
const FETCH_PAUSE: Duration = Duration::from_millis(200);

/// How one fetch of the store ended short of credentials: the store's own
/// answer, or the worker's deadline on the call.
#[derive(Debug, thiserror::Error)]
pub(super) enum FetchFailure {
    /// The store answered with its error.
    #[error(transparent)]
    Store(FetchError),
    /// The store had not answered at [`CALL_DEADLINE`].
    #[error(transparent)]
    TimedOut(TimedOut),
}

impl FetchFailure {
    /// Whether the next attempt of the loop may answer differently: a store
    /// that is unavailable or silent may recover; a reference the store does
    /// not know is settled.
    fn is_retryable(&self) -> bool {
        match self {
            Self::Store(FetchError::Gone { .. }) => false,
            Self::Store(FetchError::Unavailable(_)) | Self::TimedOut(_) => true,
        }
    }
}

/// Fetches the account's credentials outside the journal, on every
/// execution, with a short in-process retry of an unavailable store; each
/// attempt is one call bounded by [`CALL_DEADLINE`], so the loop ends within
/// `FETCH_ATTEMPTS × CALL_DEADLINE` plus the pauses.
///
/// # Errors
///
/// The terminal `unavailable` fault: the store is gone for this reference
/// or stayed unavailable (reporting so, or not answering in time) through
/// the retries. Terminal by decision: a retryable error would route a
/// prolonged store outage into the handler's kill-on-five and an unstructured
/// 500, whereas this is structured and immediate. The cost (an outage during
/// a replay of an invocation whose create already landed surfaces as
/// `unavailable` although the document exists) is reconciled by `get` or a
/// retry with a new `Idempotency-Key`.
pub(super) async fn fetch_credentials(
    accounts: &Accounts,
    account: &Account,
) -> Result<Credentials, Fault> {
    let mut attempt = 1;
    loop {
        let failure = match bounded(BoundedCall::Store, accounts.fetch(account)).await {
            Ok(Ok(credentials)) => return Ok(credentials),
            Ok(Err(error)) => FetchFailure::Store(error),
            Err(timed_out) => FetchFailure::TimedOut(timed_out),
        };
        if !failure.is_retryable() || attempt >= FETCH_ATTEMPTS {
            return Err(fetch_fault(account, &failure));
        }
        tracing::warn!(
            account = %account.id,
            attempt,
            error = %failure,
            "credential store unavailable; retrying"
        );
        attempt += 1;
        tokio::time::sleep(FETCH_PAUSE).await;
    }
}

/// The terminal fault of a failed credential fetch. The operator's warning
/// names the account and the reference; the caller's message names neither
/// (no response names the account, and a store's reference may be internal
/// topology, a secret path), and never echoes the store's own message. It
/// does tell the causes apart: a reference the store does not know is
/// configuration, an unavailable store is an outage, a store silent past the
/// deadline is the worker giving up on it.
pub(super) fn fetch_fault(account: &Account, failure: &FetchFailure) -> Fault {
    tracing::warn!(
        account = %account.id,
        credential_ref = %account.credential_ref,
        error = %failure,
        "credentials could not be fetched"
    );
    let cause: Cow<'static, str> = match failure {
        FetchFailure::Store(FetchError::Gone { .. }) => {
            "the credential store has no credentials under the account's reference".into()
        }
        FetchFailure::Store(FetchError::Unavailable(_)) => {
            "the credential store is unavailable".into()
        }
        FetchFailure::TimedOut(timed_out) => timed_out.to_string().into(),
    };
    Fault::unavailable(format!(
        "the account's credentials could not be fetched ({cause}); retry with a new Idempotency-Key"
    ))
}

/// Opens the gateway for this execution over a fresh client.
pub(super) fn open(account: Account, credentials: Credentials) -> Result<Arc<Gateway>, Fault> {
    Gateway::open(account, credentials)
        .map(Arc::new)
        .map_err(|error| {
            Fault::unavailable(format!(
                "the szamlazz.hu client could not be built: {error}"
            ))
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    use serde_json::json;

    use super::*;
    use crate::account::{AccountId, AccountResolver, BoxFuture, CredentialRef, CredentialStore};
    use crate::contract::TerminalCode;

    fn account() -> Account {
        Account::new(AccountId::from("acct"), CredentialRef::from("acct"))
    }

    fn fault_body(fault: Fault) -> (u16, serde_json::Value) {
        let error = TerminalError::from(fault);
        let body = serde_json::from_str(error.message()).expect("json body");
        (error.code(), body)
    }

    /// A resolver whose calls never complete (a database-backed embedder's
    /// pool that never answers), counting how often it was asked.
    #[derive(Default)]
    struct Hung {
        resolves: AtomicU32,
    }

    impl AccountResolver for Hung {
        fn resolve<'a>(
            &'a self,
            _scope: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Account, ResolveError>> {
            self.resolves.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::pending())
        }
    }

    /// An [`Accounts`] over a [`Hung`] resolver; a resolve never reaches the
    /// store, so an empty script stands in.
    fn hung() -> (Arc<Hung>, Accounts) {
        let hung = Arc::new(Hung::default());
        let accounts = Accounts::new(hung.clone(), Arc::new(Scripted::new([])));
        (hung, accounts)
    }

    /// One answer of a [`Scripted`] store to a fetch.
    #[derive(Debug, Clone, Copy)]
    enum Fetch {
        /// The credentials, [`SCRIPTED_KEY`].
        Credentials,
        /// `FetchError::Unavailable`, caused by [`STORE_CAUSE`].
        Unavailable,
        /// `FetchError::Gone` for the reference asked.
        Gone,
        /// No answer, ever: the call is the worker's to drop.
        Hang,
    }

    /// The agent key a [`Scripted`] store answers with.
    const SCRIPTED_KEY: &str = "scripted-agent-key";

    /// The cause a [`Scripted`] store's unavailability carries: what a
    /// database-backed store would say, and what no fault may echo.
    const STORE_CAUSE: &str = "connection refused to db.internal:5432 (secret-dsn)";

    /// A store that answers its script in order, one entry per fetch, and
    /// counts the fetches; a fetch past the script's end is the test's
    /// mistake and panics.
    struct Scripted {
        script: std::sync::Mutex<std::collections::VecDeque<Fetch>>,
        fetches: AtomicU32,
    }

    impl Scripted {
        fn new(script: impl IntoIterator<Item = Fetch>) -> Self {
            Self {
                script: std::sync::Mutex::new(script.into_iter().collect()),
                fetches: AtomicU32::new(0),
            }
        }

        fn fetches(&self) -> u32 {
            self.fetches.load(Ordering::SeqCst)
        }
    }

    impl CredentialStore for Scripted {
        fn fetch<'a>(
            &'a self,
            credential_ref: &'a CredentialRef,
        ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
            self.fetches.fetch_add(1, Ordering::SeqCst);
            let next = self
                .script
                .lock()
                .expect("script")
                .pop_front()
                .expect("a fetch past the end of the script");
            Box::pin(async move {
                match next {
                    Fetch::Credentials => Ok(Credentials::agent_key(SCRIPTED_KEY)),
                    Fetch::Unavailable => {
                        Err(FetchError::unavailable(std::io::Error::other(STORE_CAUSE)))
                    }
                    Fetch::Gone => Err(FetchError::Gone {
                        credential_ref: credential_ref.clone(),
                    }),
                    Fetch::Hang => std::future::pending().await,
                }
            })
        }
    }

    /// An [`Accounts`] over a [`Scripted`] store; `Accounts::fetch` reaches
    /// the store only, so a hung resolver stands in.
    fn scripted(script: impl IntoIterator<Item = Fetch>) -> (Arc<Scripted>, Accounts) {
        let store = Arc::new(Scripted::new(script));
        let accounts = Accounts::new(Arc::new(Hung::default()), store.clone());
        (store, accounts)
    }

    #[test]
    fn a_resolved_account_is_the_resolution() {
        assert_eq!(
            resolution(Ok(account())).expect("data"),
            Resolution::Account(Box::new(account()))
        );
        assert_eq!(
            account_of(Resolution::Account(Box::new(account()))).expect("account"),
            account()
        );
    }

    #[test]
    fn unscoped_and_unknown_are_data_and_the_unknown_account_fault() {
        let unscoped = resolution(Err(ResolveError::Unscoped)).expect("data");
        assert_eq!(unscoped, Resolution::Unscoped);
        let (status, body) = fault_body(account_of(unscoped).expect_err("fault"));
        assert_eq!(status, 400);
        assert_eq!(body["code"], TerminalCode::UnknownAccount.as_str());
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("unscoped"),
            "{body}"
        );

        let unknown = resolution(Err(ResolveError::Unknown {
            scope: "acme-events".to_owned(),
        }))
        .expect("data");
        assert_eq!(
            unknown,
            Resolution::Unknown {
                scope: "acme-events".to_owned()
            }
        );
        let (status, body) = fault_body(account_of(unknown).expect_err("fault"));
        assert_eq!(status, 400);
        assert_eq!(body["code"], "unknown_account");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("acme-events"),
            "{body}"
        );
    }

    #[test]
    fn an_unavailable_resolver_is_the_retryable_error_and_never_echoes_its_cause() {
        let error = resolution(Err(ResolveError::unavailable(std::io::Error::other(
            "connection refused to db.internal:5432 (secret-dsn)",
        ))))
        .expect_err("retryable");
        let rendered = error.to_string();
        assert_eq!(rendered, "the account resolver is unavailable");
        assert!(!rendered.contains("secret-dsn"));
        // The cause stays reachable for logs.
        assert!(std::error::Error::source(&error).is_some());
    }

    /// A resolver that never answers is bounded by the worker, not by the
    /// handler's inactivity timeout: at the deadline the `account` step's
    /// closure answers the same retryable error an unavailable resolver
    /// does, so the resolve policy re-executes the step, and its text names
    /// the deadline, so the exhausted step's fault says what happened.
    #[tokio::test(start_paused = true)]
    async fn a_resolver_that_never_answers_is_unavailable_at_the_deadline() {
        let (hung, accounts) = hung();
        let started = tokio::time::Instant::now();

        let error = resolve(&accounts, Some("acme"))
            .await
            .expect_err("retryable");

        assert_eq!(started.elapsed(), CALL_DEADLINE);
        assert_eq!(hung.resolves.load(Ordering::SeqCst), 1);
        assert!(
            matches!(error, ResolverUnavailable::TimedOut(_)),
            "{error:?}"
        );
        let rendered = error.to_string();
        assert!(rendered.contains("did not answer within"), "{rendered}");
        assert!(rendered.contains("10s"), "{rendered}");
        // The `resolution` decision is untouched by the bound: an answer that
        // arrives in time is still the answer.
        assert_eq!(
            resolution(Ok(account())).expect("data"),
            Resolution::Account(Box::new(account()))
        );
    }

    /// A store that never answers is bounded per attempt: the fetch loop
    /// gives each of its attempts the deadline, pauses between them, and ends
    /// in the terminal `unavailable` fault within `attempts × deadline` plus
    /// the pauses, never in the handler's inactivity timeout. The fault's
    /// text names the deadline and neither the account nor the credential
    /// reference.
    #[tokio::test(start_paused = true)]
    async fn a_store_that_never_answers_is_the_terminal_fault_after_its_attempts() {
        const ACCOUNT: &str = "acct-8e1f";
        const REF: &str = "secrets/kv/accounts/acme/szamlazz";
        let account = Account::new(AccountId::from(ACCOUNT), CredentialRef::from(REF));
        let (store, accounts) = scripted([Fetch::Hang; FETCH_ATTEMPTS as usize]);
        let started = tokio::time::Instant::now();

        let fault = fetch_credentials(&accounts, &account)
            .await
            .expect_err("terminal");

        let elapsed = started.elapsed();
        let deadlines = CALL_DEADLINE * FETCH_ATTEMPTS;
        let pauses = FETCH_PAUSE * (FETCH_ATTEMPTS - 1);
        assert!(elapsed >= deadlines, "{elapsed:?} < {deadlines:?}");
        assert!(
            elapsed <= deadlines + pauses,
            "{elapsed:?} > {:?}",
            deadlines + pauses
        );
        assert_eq!(store.fetches(), FETCH_ATTEMPTS);

        let (status, body) = fault_body(fault);
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("did not answer within"), "{message}");
        assert!(message.contains("10s"), "{message}");
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
        assert!(!message.contains(ACCOUNT), "{message}");
        assert!(!message.contains(REF), "{message}");
    }

    /// A store that reports itself unavailable is asked `FETCH_ATTEMPTS`
    /// times, `FETCH_PAUSE` apart, and then the fetch is the terminal
    /// `unavailable` fault (never a Restate retry): within the one execution,
    /// with no deadline spent (every answer came at once), naming the cause
    /// and neither the store's message, the account nor the reference.
    #[tokio::test(start_paused = true)]
    async fn a_store_that_stays_unavailable_is_the_terminal_fault_after_three_fetches() {
        const ACCOUNT: &str = "acct-8e1f";
        const REF: &str = "secrets/kv/accounts/acme/szamlazz";
        let account = Account::new(AccountId::from(ACCOUNT), CredentialRef::from(REF));
        let (store, accounts) = scripted([Fetch::Unavailable; FETCH_ATTEMPTS as usize]);
        let started = tokio::time::Instant::now();

        let fault = fetch_credentials(&accounts, &account)
            .await
            .expect_err("terminal");

        assert_eq!(store.fetches(), FETCH_ATTEMPTS);
        assert_eq!(
            started.elapsed(),
            FETCH_PAUSE * (FETCH_ATTEMPTS - 1),
            "one pause between each pair of attempts, no deadline spent"
        );

        let (status, body) = fault_body(fault);
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(
            message.contains("credential store is unavailable"),
            "{message}"
        );
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
        assert!(!message.contains(STORE_CAUSE), "{message}");
        assert!(!message.contains(ACCOUNT), "{message}");
        assert!(!message.contains(REF), "{message}");
    }

    /// A reference the store does not know is settled: no attempt of the
    /// loop would answer differently, so the fetch is the terminal fault at
    /// once, after one fetch and no pause.
    #[tokio::test(start_paused = true)]
    async fn a_gone_reference_is_the_terminal_fault_after_one_fetch() {
        const REF: &str = "secrets/kv/accounts/acme/szamlazz";
        let account = Account::new(AccountId::from("acct-8e1f"), CredentialRef::from(REF));
        let (store, accounts) = scripted([Fetch::Gone]);
        let started = tokio::time::Instant::now();

        let fault = fetch_credentials(&accounts, &account)
            .await
            .expect_err("terminal");

        assert_eq!(store.fetches(), 1, "gone is not retried");
        assert_eq!(started.elapsed(), Duration::ZERO);

        let (status, body) = fault_body(fault);
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("no credentials"), "{message}");
        assert!(!message.contains(REF), "{message}");
    }

    /// A store that recovers within the attempts answers the credentials: a
    /// reported unavailability and a silent attempt dropped at the deadline
    /// each spend one attempt and one pause, and the answer of the last
    /// attempt is the execution's credentials, as the store gave them.
    #[tokio::test(start_paused = true)]
    async fn a_store_that_recovers_within_its_attempts_answers_the_credentials() {
        let (store, accounts) = scripted([Fetch::Unavailable, Fetch::Hang, Fetch::Credentials]);
        let started = tokio::time::Instant::now();

        let credentials = fetch_credentials(&accounts, &account())
            .await
            .expect("the third attempt's answer");

        assert_eq!(store.fetches(), FETCH_ATTEMPTS);
        assert_eq!(
            started.elapsed(),
            FETCH_PAUSE + CALL_DEADLINE + FETCH_PAUSE,
            "the reported failure at once, the silent one at the deadline, a pause after each"
        );
        let Credentials::AgentKey(key) = credentials else {
            panic!("the store answers an agent key");
        };
        assert_eq!(key.expose(), SCRIPTED_KEY);
    }

    #[test]
    fn an_exhausted_resolve_step_is_unavailable() {
        let (status, body) = fault_body(resolve_exhausted(&TerminalError::new_with_code(
            500,
            "the account resolver is unavailable",
        )));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        assert!(
            body["message"]
                .as_str()
                .expect("message")
                .contains("retry with a new Idempotency-Key"),
            "{body}"
        );
    }

    /// The `unavailable` fault of a failed credential fetch tells the caller
    /// what to do and nothing about the deployment: neither the store's own
    /// message (a vault token, a DSN), nor the credential reference (a
    /// database-backed store's ref is internal topology, a secret path), nor
    /// the account id (no response names the account). Both cases (the store
    /// gone for the reference, the store unavailable) are told apart in the
    /// text. The operator's warning carries the account and the reference.
    #[test]
    fn a_failed_credential_fetch_is_unavailable_and_names_neither_the_account_nor_the_ref() {
        use crate::test_support::LogCapture;

        const ACCOUNT: &str = "acct-8e1f";
        const REF: &str = "secrets/kv/accounts/acme/szamlazz";
        let account = Account::new(AccountId::from(ACCOUNT), CredentialRef::from(REF));

        let capture = LogCapture::default();
        let guard = capture.subscribe();
        drop(fetch_fault(
            &Account::new(AccountId::from("warmup"), CredentialRef::from("warmup")),
            &FetchFailure::Store(FetchError::unavailable(std::io::Error::other("warm-up"))),
        ));
        LogCapture::rebuild_interest();

        let (status, body) = fault_body(fetch_fault(
            &account,
            &FetchFailure::Store(FetchError::unavailable(std::io::Error::other(
                "vault token v.abc123 rejected",
            ))),
        ));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(
            message.contains("credential store is unavailable"),
            "{message}"
        );
        assert!(
            message.contains("retry with a new Idempotency-Key"),
            "{message}"
        );
        assert!(!message.contains("abc123"), "{message}");
        assert!(!message.contains(ACCOUNT), "{message}");
        assert!(!message.contains(REF), "{message}");

        let (status, body) = fault_body(fetch_fault(
            &account,
            &FetchFailure::Store(FetchError::Gone {
                credential_ref: CredentialRef::from(REF),
            }),
        ));
        assert_eq!(status, 503);
        assert_eq!(body["code"], "unavailable");
        let message = body["message"].as_str().expect("message");
        assert!(message.contains("no credentials"), "{message}");
        assert!(!message.contains(ACCOUNT), "{message}");
        assert!(!message.contains(REF), "{message}");
        drop(guard);

        let logs = capture.logs();
        let warnings: Vec<&str> = logs
            .lines()
            .filter(|line| line.contains("could not be fetched") && !line.contains("warmup"))
            .collect();
        assert_eq!(warnings.len(), 2, "{logs}");
        for line in warnings {
            assert!(line.contains("WARN"), "{line}");
            assert!(line.contains(&format!("account={ACCOUNT}")), "{line}");
            assert!(line.contains(&format!("credential_ref={REF}")), "{line}");
        }
    }

    #[test]
    fn a_resolution_round_trips_through_json() {
        let resolution = Resolution::Account(Box::new(account()));
        let json = serde_json::to_value(&resolution).expect("json");
        assert_eq!(json["Account"]["id"], json!("acct"));
        assert_eq!(
            serde_json::from_value::<Resolution>(json).expect("back"),
            resolution
        );
    }
}
