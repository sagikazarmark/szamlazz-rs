//! The accounts the two deployments serve and the resolver and store behind
//! them: the single-account phase over [`ScriptedAccounts`] (a static resolver
//! whose next resolutions can fail or hang and whose store can be taken
//! down), the multi-account phase over [`MutableAccounts`] (accounts and keys
//! the test changes while invocations are in flight), and the agent keys the
//! run puts on the wire — sentinels the leak scan looks for.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use restate_szamlazz::account::{
    Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
    ResolveError, StaticConfig, StaticResolver,
};
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::{Agent, Order};
use serde_json::json;
use szamlazz_agent::Credentials;

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

/// The static resolver and store behind a script: the resolver fails the
/// next N resolutions with `unavailable` or hangs the next N forever, and the
/// store can be taken down. What the prologue's e2e drives — a resolver that
/// fails then succeeds, one that never answers, a store that always fails —
/// on the one deployment the harness registers.
#[derive(Debug)]
pub(crate) struct ScriptedAccounts {
    inner: StaticResolver,
    /// Resolutions left to fail with `unavailable`.
    resolver_failures: AtomicU32,
    /// Resolutions left to hang — a future that never completes, so the
    /// invocation is stuck in its `account` step until it is killed.
    resolver_hangs: AtomicU32,
    /// How many times the resolver was asked.
    resolutions: AtomicU32,
    /// Whether every fetch fails with `unavailable`.
    store_down: AtomicBool,
    /// How many times the store was asked.
    fetches: AtomicU32,
}

impl ScriptedAccounts {
    fn new(inner: StaticResolver) -> Self {
        Self {
            inner,
            resolver_failures: AtomicU32::new(0),
            resolver_hangs: AtomicU32::new(0),
            resolutions: AtomicU32::new(0),
            store_down: AtomicBool::new(false),
            fetches: AtomicU32::new(0),
        }
    }

    pub(crate) fn fail_next_resolutions(&self, count: u32) {
        self.resolver_failures.store(count, Ordering::SeqCst);
    }

    pub(crate) fn hang_next_resolutions(&self, count: u32) {
        self.resolver_hangs.store(count, Ordering::SeqCst);
    }

    pub(crate) fn set_store_down(&self, down: bool) {
        self.store_down.store(down, Ordering::SeqCst);
    }

    pub(crate) fn resolutions(&self) -> u32 {
        self.resolutions.load(Ordering::SeqCst)
    }

    pub(crate) fn fetches(&self) -> u32 {
        self.fetches.load(Ordering::SeqCst)
    }
}

impl AccountResolver for ScriptedAccounts {
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            self.resolutions.fetch_add(1, Ordering::SeqCst);
            let hanging = self.resolver_hangs.load(Ordering::SeqCst);
            if hanging > 0 {
                self.resolver_hangs.store(hanging - 1, Ordering::SeqCst);
                std::future::pending::<()>().await;
            }
            let outstanding = self.resolver_failures.load(Ordering::SeqCst);
            if outstanding > 0 {
                self.resolver_failures
                    .store(outstanding - 1, Ordering::SeqCst);
                return Err(ResolveError::unavailable(std::io::Error::other(
                    "scripted resolver outage",
                )));
            }
            self.inner.resolve(scope).await
        })
    }
}

impl CredentialStore for ScriptedAccounts {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            self.fetches.fetch_add(1, Ordering::SeqCst);
            if self.store_down.load(Ordering::SeqCst) {
                return Err(FetchError::unavailable(std::io::Error::other(
                    "scripted store outage",
                )));
            }
            self.inner.fetch(credential_ref).await
        })
    }
}

/// The two services for the test account at `endpoint`, over the scripted
/// resolver and store, with short policies so that retries and exhaustion are
/// observable within the test.
pub(crate) fn services(endpoint: &str) -> (Arc<ScriptedAccounts>, Order, Agent) {
    let accounts: StaticConfig = serde_json::from_value(json!({
        "account": {
            "id": "acct",
            "agent_key": AGENT_KEY,
            "endpoint": endpoint,
        },
    }))
    .expect("config");
    // The static resolver behind the script, and a short resolve policy so a
    // scripted outage is retried within the test.
    let scripted = Arc::new(ScriptedAccounts::new(
        StaticResolver::try_from(accounts).expect("resolver"),
    ));
    let accounts = Accounts::new(
        Arc::clone(&scripted) as Arc<dyn AccountResolver>,
        Arc::clone(&scripted) as Arc<dyn CredentialStore>,
    );
    let order = Order::from_parts(accounts.clone(), worker_config());
    let agent = Agent::from_parts(accounts, worker_config());
    (scripted, order, agent)
}

/// The deployment-level settings of both phases — the flag day keeps the
/// namespace — with short policies: two executions of the create step one
/// second apart, so exhaustion and re-execution are observable within the
/// test; three executions of a read step one second apart, so a retried and
/// an exhausted read are observable within the `watch` window; and a
/// one-second resolve policy so a scripted outage is retried within it.
///
/// Built in Rust and handed to `from_parts`, never through
/// `WorkerConfig::validate`: the 1 s issue delay is under the floor `validate`
/// holds a deployment to (`IssueConfig::MIN_INITIAL_DELAY`, the client timeout
/// plus a margin), which the endpoint's loader enforces and this suite — whose
/// szamlazz.hu is a scripted mock that answers at once — has no use for.
fn worker_config() -> WorkerConfig {
    serde_json::from_value(json!({
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
    .expect("config")
}

/// A resolver and store whose accounts and keys the test can change while
/// invocations are in flight — what a database-backed deployment looks like
/// to the worker, and what the rotation and account-change scenarios drive.
/// Seeded from the static resolver's multi-account shape, so the shape is
/// exercised end to end too.
#[derive(Debug)]
pub(crate) struct MutableAccounts {
    /// The accounts by the scope each is reachable under.
    accounts: Mutex<BTreeMap<String, Account>>,
    /// The credentials by credential reference.
    keys: Mutex<BTreeMap<String, Credentials>>,
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
/// szamlazz.hu account of the single-account phase (same key — the flag day
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
