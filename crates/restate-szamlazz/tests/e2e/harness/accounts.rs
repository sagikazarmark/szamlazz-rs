//! The accounts the two deployments serve and the resolver and store behind
//! them: the single-account phase over the static resolver as a deployment
//! would configure it, the multi-account phase over [`MutableAccounts`]
//! (accounts and keys the test changes while invocations are in flight, a
//! resolution it can fail per scope, and a fetch it can hold
//! ([`FetchHold`]) so a change lands between two executions in sequence, or
//! an invocation stands still where a scenario needs it), and the agent keys
//! the run puts on the wire, sentinels the leak scan looks for.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use restate_szamlazz::account::{
    Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
    ResolveError, StaticConfig, StaticResolver,
};
use restate_szamlazz::config::{ValidatedWorkerConfig, WorkerConfig};
use restate_szamlazz::{Agent, Order};
use serde_json::json;
use szamlazz_agent::Credentials;
use tokio::sync::watch;

/// The agent key of the test account (and of `acme` after the flag day): a
/// sentinel that must never appear in a journal entry.
pub(crate) const AGENT_KEY: &str = "e2e-agent-key-sentinel-7d1f4b";

/// The agent key of the `beta` account; a second sentinel.
pub(crate) const KEY_B: &str = "e2e-agent-key-b-sentinel-2e7f41";

/// `beta`'s key after the rotation scenario; a third sentinel.
pub(crate) const KEY_B_V2: &str = "e2e-agent-key-b-rotated-sentinel-5ba9c0";

/// Every agent key the run puts on the wire; none may be journaled.
pub(crate) const AGENT_KEYS: [&str; 3] = [AGENT_KEY, KEY_B, KEY_B_V2];

/// `acme`'s seller bank account in the multi-account phase, and the value
/// the account-change scenario replaces it with.
pub(crate) const BANK_ACCOUNT: &str = "11111111-22222222-33333333";
pub(crate) const BANK_ACCOUNT_CHANGED: &str = "44444444-55555555-66666666";

/// The two services for the test account at `endpoint`, over the static
/// resolver's `[account]` shape exactly as a single-account deployment would
/// configure it, with short policies so that retries and exhaustion are
/// observable within the test. Nothing scripts this phase's resolver or
/// store: its scenarios run concurrently, and a script on a shared resolver
/// (which sees no order key) would fire on whichever invocation resolved
/// next; the scripted account steps run in phase 2, per scope.
pub(crate) fn services(endpoint: &str) -> (Order, Agent) {
    let accounts: StaticConfig = serde_json::from_value(json!({
        "account": {
            "id": "acct",
            "agent_key": AGENT_KEY,
            "endpoint": endpoint,
        },
    }))
    .expect("config");
    let resolver = Arc::new(StaticResolver::try_from(accounts).expect("resolver"));
    let accounts = Accounts::new(
        Arc::clone(&resolver) as Arc<dyn AccountResolver>,
        resolver as Arc<dyn CredentialStore>,
    );
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts, worker_config());
    (order, agent)
}

/// The deployment-level settings of both phases (the flag day keeps the
/// namespace) with short policies: two executions of the create step one
/// second apart, so exhaustion and re-execution are observable within the
/// test; three executions of a read step one second apart, so a retried and
/// an exhausted read are observable; and a one-second resolve policy so a
/// scripted outage is retried within it.
///
/// **One second is not a free parameter.** The scenarios that assert a run
/// retry read what `sys_invocation` shows while the invoker waits out a run
/// retry delay in place (status `backing-off`, `retry_count`, `last_failure`,
/// `last_failure_related_command_name`; the harness's `watch`). Under vqueues
/// a run retry delay at or above the server's `invocation_yield_threshold`
/// (2 s in 1.7.8) is retried via the scheduler instead: the invoker drops its
/// status row and the entry waits in the vqueue inbox, so `sys_invocation`
/// shows `ready` with none of those columns, `retry_count` starts over at 1
/// on the next execution, and every such assertion fails. Verified end to end with a
/// 2 s read delay (the invocation was picked up again after about 1.2 s;
/// `docs/research/2026-09-06-pretix-invoice-sync/raw/02-restate.md`, section
/// 7, has the source reading). Widening the delays to make the windows easier
/// to catch is therefore not available; what makes a window catchable is the
/// sampler's 100 ms interval (ten samples per window), and a missed one is
/// diagnosable by the sample count in `Retries`.
///
/// Built with `ValidatedWorkerConfig::unchecked` (the `test-util` feature),
/// never through `WorkerConfig::validate`: the 1 s issue delay is under the
/// floor `validate` holds a deployment to (`IssueConfig::MIN_INITIAL_DELAY`,
/// the client timeout plus a margin), which `from_parts` demands and this
/// suite (whose szamlazz.hu is a scripted mock that answers at once) has no
/// use for.
fn worker_config() -> ValidatedWorkerConfig {
    let config: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct",
        "issue": {
            "max_attempts": 2,
            "initial_delay": "1s",
            "factor": 2.0,
            "max_delay": "2s",
            "max_duration": "1m",
        },
        "read": {
            "max_attempts": 3,
            "initial_delay": "1s",
            "factor": 1.0,
            "max_delay": "1s",
            "max_duration": "30s",
        },
        "resolve": {
            "initial_delay": "1s",
            "factor": 1.0,
            "max_delay": "1s",
            "max_duration": "30s",
        },
    }))
    .expect("config");
    ValidatedWorkerConfig::unchecked(config)
}

/// A resolver and store whose accounts and keys the test can change while
/// invocations are in flight: what a database-backed deployment looks like
/// to the worker, and what the rotation, account-change, resolve-policy and
/// kill scenarios drive. Seeded from the static resolver's multi-account
/// shape, so the shape is exercised end to end too. Every script is **per
/// scope** (a resolution failure) or **per credential reference** (a held
/// fetch), so a scenario under `beta` disturbs nothing under `acme`.
#[derive(Debug)]
pub(crate) struct MutableAccounts {
    /// The accounts by the scope each is reachable under.
    accounts: Mutex<BTreeMap<String, Account>>,
    /// The credentials by credential reference.
    keys: Mutex<BTreeMap<String, Credentials>>,
    /// Per scope: how many of its next resolutions fail with `unavailable`
    /// ([`Self::fail_next_resolutions`]).
    resolver_failures: Mutex<BTreeMap<String, u32>>,
    /// Per scope: how many times the resolver was asked.
    resolutions: Mutex<BTreeMap<String, u32>>,
    /// The fetches a scenario holds ([`Self::hold_fetch`]).
    holds: Mutex<Vec<Arc<Hold>>>,
}

/// Where a held fetch stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoldState {
    /// Armed: the fetches before the held one pass.
    Armed,
    /// The held fetch arrived and is parked.
    Reached,
    /// Released: the parked fetch answers, later fetches pass.
    Released,
}

/// The store's side of a [`FetchHold`]: which reference's fetch is held, how
/// many fetches of it pass first, and where the hold stands.
#[derive(Debug)]
struct Hold {
    credential_ref: String,
    /// Fetches of the reference still to let through before the held one.
    remaining: AtomicU32,
    state: watch::Sender<HoldState>,
}

/// A hold on one fetch of a credential reference ([`MutableAccounts::hold_fetch`]):
/// the store parks that fetch until [`release`](Self::release), so a scenario
/// can act **between two executions** of a handler with the second one
/// waiting at its fetch (after it replayed its journal, before it opens its
/// gateway) instead of racing the run retry delay before it, or hold an
/// invocation still where it needs one standing (the kill scenario). Dropping
/// the hold releases it. Bounded by the prologue's fetch deadline: a hold
/// released later than that is a fetch that timed out.
pub(crate) struct FetchHold {
    hold: Arc<Hold>,
}

impl FetchHold {
    /// How long [`reached`](Self::reached) waits for the held fetch before
    /// failing the scenario: the execution that should reach it is a run
    /// retry away.
    const REACHED_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30);

    /// Resolves once the held fetch has arrived and is parked; fails the
    /// scenario when none arrives within [`Self::REACHED_DEADLINE`].
    pub(crate) async fn reached(&self) {
        let mut state = self.hold.state.subscribe();
        let reached = state.wait_for(|state| *state != HoldState::Armed);
        tokio::time::timeout(Self::REACHED_DEADLINE, reached)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "no fetch of {} reached the hold within {:?}",
                    self.hold.credential_ref,
                    Self::REACHED_DEADLINE
                )
            })
            .expect("the hold outlives its receivers");
    }

    /// Lets the parked fetch answer.
    pub(crate) fn release(&self) {
        self.hold.state.send_replace(HoldState::Released);
    }
}

impl Drop for FetchHold {
    fn drop(&mut self) {
        self.release();
    }
}

impl MutableAccounts {
    async fn seeded_from(resolver: &StaticResolver) -> Self {
        let mut accounts = BTreeMap::new();
        let mut keys = BTreeMap::new();
        for (scope, account) in resolver.accounts() {
            let scope = scope.expect("the multi-account shape scopes every account");
            let credentials = resolver
                .fetch(&account.credential_ref)
                .await
                .expect("the inline key");
            keys.insert(account.credential_ref.to_string(), credentials);
            accounts.insert(scope.to_owned(), account.clone());
        }
        Self {
            accounts: Mutex::new(accounts),
            keys: Mutex::new(keys),
            resolver_failures: Mutex::new(BTreeMap::new()),
            resolutions: Mutex::new(BTreeMap::new()),
            holds: Mutex::new(Vec::new()),
        }
    }

    /// Replaces the credentials under `credential_ref`: a rotation.
    pub(crate) fn rotate(&self, credential_ref: &str, agent_key: &str) {
        self.keys
            .lock()
            .expect("keys")
            .insert(credential_ref.to_owned(), Credentials::agent_key(agent_key));
    }

    /// Changes the account reachable under `scope` in place.
    pub(crate) fn update(&self, scope: &str, change: impl FnOnce(&mut Account)) {
        let mut accounts = self.accounts.lock().expect("accounts");
        change(
            accounts
                .get_mut(scope)
                .unwrap_or_else(|| panic!("no account under scope {scope}")),
        );
    }

    /// The next `count` resolutions of `scope` fail with `unavailable` (the
    /// resolver's own message never reaching the worker's fault), the ones
    /// after answer: what the resolve policy re-executes the `account` step
    /// on. Resolutions of other scopes pass.
    pub(crate) fn fail_next_resolutions(&self, scope: &str, count: u32) {
        self.resolver_failures
            .lock()
            .expect("failures")
            .insert(scope.to_owned(), count);
    }

    /// How many times the resolver was asked for `scope`.
    pub(crate) fn resolutions(&self, scope: &str) -> u32 {
        self.resolutions
            .lock()
            .expect("resolutions")
            .get(scope)
            .copied()
            .unwrap_or_default()
    }

    /// Holds the `nth` fetch of `credential_ref` from now on (`1` is the next
    /// one): it parks until the hold is released; the fetches before it and
    /// after it pass. A retry of the held fetch (the prologue's deadline cut
    /// it) parks again: the hold is on the position, not on one future. One
    /// hold per reference at a time: a second one would count the fetches
    /// passing under the first as its own.
    pub(crate) fn hold_fetch(&self, credential_ref: &str, nth: u32) -> FetchHold {
        assert!(nth >= 1, "the first fetch is 1");
        let mut holds = self.holds.lock().expect("holds");
        holds.retain(|hold| *hold.state.borrow() != HoldState::Released);
        assert!(
            !holds
                .iter()
                .any(|hold| hold.credential_ref == credential_ref),
            "a hold on {credential_ref} is in place already: one per reference at a time"
        );
        let (state, _) = watch::channel(HoldState::Armed);
        let hold = Arc::new(Hold {
            credential_ref: credential_ref.to_owned(),
            remaining: AtomicU32::new(nth - 1),
            state,
        });
        holds.push(Arc::clone(&hold));
        FetchHold { hold }
    }

    /// Parks when this fetch of `credential_ref` is a held one.
    async fn park_if_held(&self, credential_ref: &str) {
        let hold = self
            .holds
            .lock()
            .expect("holds")
            .iter()
            .find(|hold| {
                hold.credential_ref == credential_ref && *hold.state.borrow() != HoldState::Released
            })
            .cloned();
        let Some(hold) = hold else {
            return;
        };
        let passes = hold
            .remaining
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                remaining.checked_sub(1)
            })
            .is_ok();
        if passes {
            return;
        }
        hold.state.send_if_modified(|state| {
            if *state == HoldState::Armed {
                *state = HoldState::Reached;
                true
            } else {
                false
            }
        });
        let mut state = hold.state.subscribe();
        state
            .wait_for(|state| *state == HoldState::Released)
            .await
            .expect("the hold outlives its receivers");
    }

    /// Counts the resolution of `scope` and says whether it is a scripted
    /// failure.
    fn scripted_failure(&self, scope: &str) -> bool {
        *self
            .resolutions
            .lock()
            .expect("resolutions")
            .entry(scope.to_owned())
            .or_default() += 1;
        let mut failures = self.resolver_failures.lock().expect("failures");
        match failures.get_mut(scope) {
            Some(outstanding) if *outstanding > 0 => {
                *outstanding -= 1;
                true
            }
            _ => false,
        }
    }
}

impl AccountResolver for MutableAccounts {
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            let Some(scope) = scope else {
                return Err(ResolveError::Unscoped);
            };
            if self.scripted_failure(scope) {
                return Err(ResolveError::unavailable(std::io::Error::other(
                    "scripted resolver outage",
                )));
            }
            self.accounts
                .lock()
                .expect("accounts")
                .get(scope)
                .cloned()
                .ok_or_else(|| ResolveError::Unknown {
                    scope: scope.to_owned(),
                })
        })
    }
}

impl CredentialStore for MutableAccounts {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            self.park_if_held(credential_ref.as_str()).await;
            self.keys
                .lock()
                .expect("keys")
                .get(credential_ref.as_str())
                .cloned()
                .ok_or_else(|| FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                })
        })
    }
}

/// The two services of the multi-account phase at `endpoint`: `acme` is the
/// szamlazz.hu account of the single-account phase (same key; the flag day
/// changes configuration, not the account), `beta` is a second one. Reachable
/// by scope only.
pub(crate) async fn multi_account_services(endpoint: &str) -> (Arc<MutableAccounts>, Order, Agent) {
    let config: StaticConfig = serde_json::from_value(json!({
        "accounts": {
            "acme": {
                "id": "acme",
                "agent_key": AGENT_KEY,
                "endpoint": endpoint,
                "seller": { "bank_account": BANK_ACCOUNT },
            },
            "beta": {
                "id": "beta",
                "agent_key": KEY_B,
                "endpoint": endpoint,
            },
        },
    }))
    .expect("config");
    let resolver = StaticResolver::try_from(config).expect("resolver");
    assert!(resolver.is_scoped());
    let mutable = Arc::new(MutableAccounts::seeded_from(&resolver).await);
    let accounts = Accounts::new(
        Arc::clone(&mutable) as Arc<dyn AccountResolver>,
        Arc::clone(&mutable) as Arc<dyn CredentialStore>,
    );
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts, worker_config());
    (mutable, order, agent)
}

// ----- the scripts, against the resolver and store alone ------------------------

/// The agent key a fetched credential carries.
#[cfg(test)]
fn key_of(credentials: &Credentials) -> &str {
    match credentials {
        Credentials::AgentKey(key) => key.expose(),
        _ => panic!("an agent key"),
    }
}

/// `hold_fetch(ref, 2)` lets the first fetch of the reference pass, parks the
/// second until the hold is released (the scenario acts in between: here a
/// rotation, which the parked fetch then answers), lets the third pass at
/// once, and never touches another reference's fetches.
#[tokio::test]
async fn a_held_fetch_parks_until_released_and_answers_what_was_changed_meanwhile() {
    let (store, _, _) = multi_account_services("http://127.0.0.1:1/").await;
    let beta = CredentialRef::from("beta");
    let acme = CredentialRef::from("acme");
    let hold = store.hold_fetch("beta", 2);

    // The first fetch passes with the seeded key.
    let first = store.fetch(&beta).await.expect("the first fetch");
    assert_eq!(key_of(&first), KEY_B);
    // Another reference is not held.
    assert_eq!(
        key_of(&store.fetch(&acme).await.expect("acme's fetch")),
        AGENT_KEY
    );

    // The second parks: it does not answer before the release.
    let store_for_fetch = Arc::clone(&store);
    let mut second = tokio::spawn(async move {
        store_for_fetch
            .fetch(&CredentialRef::from("beta"))
            .await
            .expect("the held fetch")
    });
    hold.reached().await;
    tokio::select! {
        biased;
        answered = &mut second => panic!("the held fetch answered before the release: {answered:?}"),
        () = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }
    store.rotate("beta", KEY_B_V2);
    hold.release();
    let second = second.await.expect("the held fetch task");
    assert_eq!(
        key_of(&second),
        KEY_B_V2,
        "the parked fetch answers the rotated key"
    );

    // The third passes at once.
    let third = store.fetch(&beta).await.expect("the third fetch");
    assert_eq!(key_of(&third), KEY_B_V2);
}

/// `fail_next_resolutions(scope, 2)` fails the scope's next two resolutions
/// with `unavailable` (the resolver's own message inside, for the worker to
/// never echo), answers the third, counts all three, and leaves another
/// scope's resolutions alone; a scope no account is under stays unknown, and
/// no scope is unscoped.
#[tokio::test]
async fn scripted_resolutions_fail_per_scope_then_answer() {
    let (resolver, _, _) = multi_account_services("http://127.0.0.1:1/").await;
    resolver.fail_next_resolutions("beta", 2);

    for attempt in 1..=2 {
        let error = resolver
            .resolve(Some("beta"))
            .await
            .expect_err("a scripted failure");
        assert!(
            matches!(error, ResolveError::Unavailable(_)),
            "attempt {attempt}: {error:?}"
        );
    }
    let account = resolver.resolve(Some("beta")).await.expect("the answer");
    assert_eq!(account.id.as_str(), "beta");
    assert_eq!(
        resolver.resolutions("beta"),
        3,
        "two failures and the answer"
    );

    let acme = resolver
        .resolve(Some("acme"))
        .await
        .expect("acme is not scripted");
    assert_eq!(acme.id.as_str(), "acme");
    assert_eq!(resolver.resolutions("acme"), 1);
    assert!(matches!(
        resolver.resolve(Some("gamma")).await,
        Err(ResolveError::Unknown { .. })
    ));
    assert!(matches!(
        resolver.resolve(None).await,
        Err(ResolveError::Unscoped)
    ));
}
