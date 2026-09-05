//! The static resolver: accounts from deployment configuration, in one of
//! two mutually exclusive shapes.
//!
//! **Single-account** — one `[account]`, reachable unscoped; any scope is
//! unknown:
//!
//! ```toml
//! [account]
//! id = "acme"
//! agent_key = "..."
//! endpoint = "https://www.szamlazz.hu/szamla/"   # default
//! mode = "live"                                  # default
//! supplier_id = 972720                           # optional here
//!
//! [account.defaults]
//! currency = "HUF"
//!
//! [account.seller]
//! bank_account = "..."
//! ```
//!
//! **Multi-account** — a table of `[accounts.<scope>]`, each reachable under
//! its scope only; an unscoped request is unscoped (`unknown_account`):
//!
//! ```toml
//! [accounts.acme]
//! id = "acme"
//! agent_key = "..."
//! supplier_id = 972720                           # required here
//!
//! [accounts.beta_events]
//! id = "beta"
//! agent_key = "..."
//! supplier_id = 972721
//! ```
//!
//! Both present is a load error; there is no default account. The scope keys
//! are `[a-z0-9_]`, 1–[`MAX_SCOPE_LEN`] bytes — a strict subset of Restate's
//! scope format (`[a-zA-Z0-9_.-]`, at most 36 bytes) chosen so that
//! environment overrides can address them
//! (`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY`).
//!
//! The configuration types implement `Deserialize` only; the endpoint binary
//! chooses the file format and environment merging. [`StaticResolver`] is
//! built from a parsed [`StaticConfig`] with `TryFrom`, which validates what
//! `Deserialize` cannot — including the checkable half of the resolver's
//! safety contract in the multi-account shape: a supplier id on every
//! account, unique supplier ids, unique `(endpoint, agent_key)` pairs and
//! unique ids, so that no szamlazz.hu account is reachable under two scopes.
//! It implements both [`AccountResolver`] and [`CredentialStore`]: the agent
//! key is inline and the credential reference is the account id.

use std::collections::BTreeMap;
use std::fmt;

use serde::Deserialize;
use szamlazz_agent::Credentials;

use super::{
    Account, AccountId, AccountResolver, BoxFuture, CredentialRef, CredentialStore, Endpoint,
    FetchError, InvalidEndpoint, ResolveError,
};
use crate::config::{AccountMode, Defaults, Secret, SellerConfig};

/// The maximum length in bytes of an `[accounts.<scope>]` key: Restate's own
/// limit on a scope value (a dashed UUID is exactly 36).
pub const MAX_SCOPE_LEN: usize = 36;

/// The static resolver's configuration: one `[account]` **or** a table of
/// `[accounts.<scope>]`, never both.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct StaticConfig {
    /// The single-account shape: the account this deployment issues for,
    /// reachable unscoped.
    #[serde(default)]
    pub account: Option<StaticAccount>,
    /// The multi-account shape: the accounts by the scope each is reachable
    /// under.
    #[serde(default)]
    pub accounts: BTreeMap<String, StaticAccount>,
}

/// One account as configured statically: the [`Account`] fields plus the
/// agent key inline.
#[derive(Debug, Clone, Deserialize)]
pub struct StaticAccount {
    /// The account's identifier; also its credential reference.
    pub id: AccountId,
    /// The Agent key (`számlaagentkulcs`). A number is accepted: keys may be
    /// all digits, and an unquoted one is a number to TOML and YAML.
    pub agent_key: Secret,
    /// The Számla Agent endpoint; the production URL when absent. Validated
    /// when the resolver is built.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Whether the account is live or a test account. Default live; always
    /// validated against `teszt`.
    #[serde(default)]
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`), an ownership pin: optional
    /// in the single-account shape, required in the multi-account shape.
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// Document defaults that per-call overrides may change.
    #[serde(default)]
    pub defaults: Defaults,
    /// The seller block; account data is used where absent.
    #[serde(default)]
    pub seller: SellerConfig,
}

/// Where an account sits in the configuration: `account` in the
/// single-account shape, `accounts.<scope>` in the multi-account shape.
/// Names the account in a [`StaticConfigError`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Table(Option<String>);

impl Table {
    /// The `[account]` table.
    #[must_use]
    pub const fn account() -> Self {
        Self(None)
    }

    /// The `[accounts.<scope>]` table.
    #[must_use]
    pub fn accounts(scope: impl Into<String>) -> Self {
        Self(Some(scope.into()))
    }

    /// The scope of an `[accounts.<scope>]` table; `None` for `[account]`.
    #[must_use]
    pub fn scope(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            None => f.write_str("account"),
            Some(scope) => write!(f, "accounts.{scope}"),
        }
    }
}

/// A [`StaticConfig`] that parsed but violates an invariant.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum StaticConfigError {
    /// Both `[account]` and `[accounts]` are present.
    #[error("[account] and [accounts.<scope>] are mutually exclusive; configure one shape")]
    BothShapes,
    /// Neither `[account]` nor a non-empty `[accounts]` is present.
    #[error("no account is configured: set [account] or at least one [accounts.<scope>]")]
    NoAccount,
    /// An `[accounts.<scope>]` key is not a valid scope.
    #[error("accounts.{scope:?}: {source}")]
    InvalidScope {
        /// The key as written.
        scope: String,
        /// Why it is invalid.
        source: InvalidScope,
    },
    /// The account's `id` is empty or blank.
    #[error("{table}.id must not be empty")]
    EmptyId {
        /// The account's table.
        table: Table,
    },
    /// The account's `agent_key` is empty or blank.
    #[error("{table}.agent_key must not be empty (account {id})")]
    EmptyAgentKey {
        /// The account's table.
        table: Table,
        /// The account.
        id: AccountId,
    },
    /// The account's `endpoint` is not an http(s) URL.
    #[error("{table}.endpoint (account {id}): {source}")]
    InvalidEndpoint {
        /// The account's table.
        table: Table,
        /// The account.
        id: AccountId,
        /// Why the endpoint is invalid.
        source: InvalidEndpoint,
    },
    /// An account of the multi-account shape has no `supplier_id`, which is
    /// the only server-side identity the worker can validate against.
    #[error("{table}.supplier_id is required in the multi-account shape (account {id})")]
    MissingSupplierId {
        /// The account's table.
        table: Table,
        /// The account.
        id: AccountId,
    },
    /// Two accounts share an `id`, which is also their credential reference.
    #[error("{first} and {second} share the id {id}")]
    DuplicateId {
        /// The shared id.
        id: AccountId,
        /// The first account's table.
        first: Table,
        /// The second account's table.
        second: Table,
    },
    /// Two accounts share a `supplier_id`: one szamlazz.hu account would be
    /// reachable under two scopes.
    #[error("{first} and {second} share the supplier id {supplier_id}")]
    DuplicateSupplierId {
        /// The shared supplier id.
        supplier_id: u64,
        /// The first account's table.
        first: Table,
        /// The second account's table.
        second: Table,
    },
    /// Two accounts share an `(endpoint, agent_key)` pair: one szamlazz.hu
    /// account would be reachable under two scopes. The key is not echoed.
    #[error("{first} and {second} share an endpoint and agent key")]
    DuplicateCredentials {
        /// The first account's table.
        first: Table,
        /// The second account's table.
        second: Table,
    },
}

/// A string that is not a valid `[accounts.<scope>]` key.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidScope {
    /// The key is empty.
    #[error("scope must not be empty")]
    Empty,
    /// The key exceeds [`MAX_SCOPE_LEN`] bytes.
    #[error("scope is {0} bytes long, at most {MAX_SCOPE_LEN} are allowed")]
    TooLong(usize),
    /// A character is outside `[a-z0-9_]`.
    #[error(
        "scope may only contain lowercase ASCII letters, digits and '_' (so that environment overrides can address it), found {0:?}"
    )]
    InvalidChar(char),
}

/// Validates an `[accounts.<scope>]` key: `[a-z0-9_]`, 1–[`MAX_SCOPE_LEN`]
/// bytes.
fn validate_scope(scope: &str) -> Result<(), InvalidScope> {
    if scope.is_empty() {
        return Err(InvalidScope::Empty);
    }
    if scope.len() > MAX_SCOPE_LEN {
        return Err(InvalidScope::TooLong(scope.len()));
    }
    if let Some(invalid) = scope
        .chars()
        .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_'))
    {
        return Err(InvalidScope::InvalidChar(invalid));
    }
    Ok(())
}

/// One configured account: what the resolver answers and what the store
/// fetches.
#[derive(Debug, Clone)]
struct Entry {
    account: Account,
    credentials: Credentials,
}

impl Entry {
    /// Validates one account and builds its [`Account`]; the shape-level
    /// rules (supplier id, uniqueness) are the caller's.
    fn build(table: &Table, config: StaticAccount) -> Result<Self, StaticConfigError> {
        let StaticAccount {
            id,
            agent_key,
            endpoint,
            mode,
            supplier_id,
            defaults,
            seller,
        } = config;
        if id.as_str().trim().is_empty() {
            return Err(StaticConfigError::EmptyId {
                table: table.clone(),
            });
        }
        if agent_key.expose().trim().is_empty() {
            return Err(StaticConfigError::EmptyAgentKey {
                table: table.clone(),
                id,
            });
        }
        let endpoint = match endpoint {
            Some(endpoint) => {
                Endpoint::parse(&endpoint).map_err(|source| StaticConfigError::InvalidEndpoint {
                    table: table.clone(),
                    id: id.clone(),
                    source,
                })?
            }
            None => Endpoint::production(),
        };
        let mut account = Account::new(id.clone(), CredentialRef::from(id.as_str()));
        account.mode = mode;
        account.supplier_id = supplier_id;
        account.endpoint = endpoint;
        account.defaults = defaults;
        account.seller = seller;
        Ok(Self {
            account,
            credentials: Credentials::agent_key(agent_key.expose()),
        })
    }
}

/// Which shape the resolver was built from.
#[derive(Debug, Clone)]
enum Shape {
    /// `[account]`: the one account, reachable unscoped. Boxed: an entry is
    /// large next to the map.
    Single(Box<Entry>),
    /// `[accounts.<scope>]`: the accounts by scope.
    Multi(BTreeMap<String, Entry>),
}

/// Accounts from deployment configuration; the resolver and the credential
/// store of a static deployment, one struct.
#[derive(Debug, Clone)]
pub struct StaticResolver {
    shape: Shape,
}

impl StaticResolver {
    /// Whether the accounts are reachable by scope (the multi-account shape)
    /// rather than unscoped (the single-account shape).
    #[must_use]
    pub fn is_scoped(&self) -> bool {
        matches!(self.shape, Shape::Multi(_))
    }

    /// The configured accounts with the scope each is reachable under:
    /// `None` for the single-account shape's one account.
    pub fn accounts(&self) -> impl Iterator<Item = (Option<&str>, &Account)> {
        let (single, multi) = match &self.shape {
            Shape::Single(entry) => (Some((None, &entry.account)), None),
            Shape::Multi(entries) => (
                None,
                Some(
                    entries
                        .iter()
                        .map(|(scope, entry)| (Some(scope.as_str()), &entry.account)),
                ),
            ),
        };
        single.into_iter().chain(multi.into_iter().flatten())
    }

    fn entries(&self) -> impl Iterator<Item = &Entry> {
        let (single, multi) = match &self.shape {
            Shape::Single(entry) => (Some(entry.as_ref()), None),
            Shape::Multi(entries) => (None, Some(entries.values())),
        };
        single.into_iter().chain(multi.into_iter().flatten())
    }

    /// Builds the multi-account shape, enforcing the checkable half of the
    /// safety contract.
    fn multi(accounts: BTreeMap<String, StaticAccount>) -> Result<Self, StaticConfigError> {
        let mut entries = BTreeMap::new();
        let mut ids: BTreeMap<&str, Table> = BTreeMap::new();
        let mut suppliers: BTreeMap<u64, Table> = BTreeMap::new();
        let mut credentials: BTreeMap<(&str, &str), Table> = BTreeMap::new();
        // Built first so that the uniqueness maps can borrow from them.
        let built: Vec<(Table, Entry, Secret)> = accounts
            .into_iter()
            .map(|(scope, account)| {
                validate_scope(&scope).map_err(|source| StaticConfigError::InvalidScope {
                    scope: scope.clone(),
                    source,
                })?;
                let table = Table::accounts(scope);
                let agent_key = account.agent_key.clone();
                let entry = Entry::build(&table, account)?;
                if entry.account.supplier_id.is_none() {
                    return Err(StaticConfigError::MissingSupplierId {
                        table,
                        id: entry.account.id,
                    });
                }
                Ok((table, entry, agent_key))
            })
            .collect::<Result<_, _>>()?;
        for (table, entry, agent_key) in &built {
            let account = &entry.account;
            if let Some(first) = ids.insert(account.id.as_str(), table.clone()) {
                return Err(StaticConfigError::DuplicateId {
                    id: account.id.clone(),
                    first,
                    second: table.clone(),
                });
            }
            let supplier_id = account
                .supplier_id
                .expect("checked when the entry was built");
            if let Some(first) = suppliers.insert(supplier_id, table.clone()) {
                return Err(StaticConfigError::DuplicateSupplierId {
                    supplier_id,
                    first,
                    second: table.clone(),
                });
            }
            if let Some(first) = credentials.insert(
                (account.endpoint.as_str(), agent_key.expose()),
                table.clone(),
            ) {
                return Err(StaticConfigError::DuplicateCredentials {
                    first,
                    second: table.clone(),
                });
            }
        }
        for (table, entry, _) in built {
            let scope = table
                .scope()
                .expect("multi-account tables carry their scope")
                .to_owned();
            entries.insert(scope, entry);
        }
        Ok(Self {
            shape: Shape::Multi(entries),
        })
    }
}

impl TryFrom<StaticConfig> for StaticResolver {
    type Error = StaticConfigError;

    /// Validates the configuration: exactly one shape; per account a
    /// non-blank id and agent key and an http(s) endpoint; and in the
    /// multi-account shape valid scope keys, a supplier id on every account,
    /// and unique ids, supplier ids and `(endpoint, agent_key)` pairs.
    fn try_from(config: StaticConfig) -> Result<Self, Self::Error> {
        match (config.account, config.accounts) {
            (Some(_), accounts) if !accounts.is_empty() => Err(StaticConfigError::BothShapes),
            (Some(account), _) => Ok(Self {
                shape: Shape::Single(Box::new(Entry::build(&Table::account(), account)?)),
            }),
            (None, accounts) if accounts.is_empty() => Err(StaticConfigError::NoAccount),
            (None, accounts) => Self::multi(accounts),
        }
    }
}

impl AccountResolver for StaticResolver {
    /// Single-account shape: unscoped → the account, any scope → unknown.
    /// Multi-account shape: unscoped → unscoped, a scope → its account or
    /// unknown.
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>> {
        Box::pin(async move {
            match (&self.shape, scope) {
                (Shape::Single(entry), None) => Ok(entry.account.clone()),
                (Shape::Single(_), Some(scope)) => Err(ResolveError::Unknown {
                    scope: scope.to_owned(),
                }),
                (Shape::Multi(_), None) => Err(ResolveError::Unscoped),
                (Shape::Multi(entries), Some(scope)) => entries
                    .get(scope)
                    .map(|entry| entry.account.clone())
                    .ok_or_else(|| ResolveError::Unknown {
                        scope: scope.to_owned(),
                    }),
            }
        })
    }
}

impl CredentialStore for StaticResolver {
    /// The inline key under the account's id; anything else is gone.
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
        Box::pin(async move {
            self.entries()
                .find(|entry| entry.account.credential_ref == *credential_ref)
                .map(|entry| entry.credentials.clone())
                .ok_or_else(|| FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// The single-account shape as the endpoint binary would hand it over
    /// after merging its file format and environment overrides.
    fn single() -> StaticConfig {
        serde_json::from_value(json!({
            "account": {
                "id": "acme",
                "agent_key": "key-acme",
                "endpoint": "http://127.0.0.1:1/",
                "mode": "test",
                "supplier_id": 972_720,
                "defaults": { "currency": "EUR", "e_invoice": true },
                "seller": { "bank_account": "11111111-22222222" },
            },
        }))
        .expect("config")
    }

    /// The multi-account shape: two accounts on the same endpoint with
    /// distinct keys and supplier ids.
    fn multi() -> serde_json::Value {
        json!({
            "accounts": {
                "acme": {
                    "id": "acme",
                    "agent_key": "key-acme",
                    "endpoint": "http://127.0.0.1:1/",
                    "mode": "test",
                    "supplier_id": 972_720,
                    "seller": { "bank_account": "11111111-22222222" },
                },
                "beta_events": {
                    "id": "beta",
                    "agent_key": "key-beta",
                    "endpoint": "http://127.0.0.1:1/",
                    "mode": "test",
                    "supplier_id": 972_721,
                },
            },
        })
    }

    fn resolver(config: serde_json::Value) -> Result<StaticResolver, StaticConfigError> {
        StaticResolver::try_from(serde_json::from_value::<StaticConfig>(config).expect("config"))
    }

    async fn key(resolver: &StaticResolver, credential_ref: &str) -> String {
        match resolver.fetch(&CredentialRef::from(credential_ref)).await {
            Ok(Credentials::AgentKey(key)) => key.expose().to_owned(),
            other => panic!("expected an agent key under {credential_ref}, got {other:?}"),
        }
    }

    #[test]
    fn single_account_shape_parses_and_builds_the_account() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        assert!(!resolver.is_scoped());
        let accounts: Vec<_> = resolver.accounts().collect();
        assert_eq!(accounts.len(), 1);
        let (scope, account) = accounts[0];
        assert_eq!(scope, None, "reachable unscoped");
        assert_eq!(account.id.as_str(), "acme");
        assert_eq!(
            account.credential_ref.as_str(),
            "acme",
            "credential_ref = id"
        );
        assert_eq!(account.mode, AccountMode::Test);
        assert_eq!(account.supplier_id, Some(972_720));
        assert_eq!(account.endpoint.as_str(), "http://127.0.0.1:1/");
        assert_eq!(account.defaults.currency, "EUR");
        assert!(account.defaults.e_invoice);
        assert_eq!(
            account.defaults.language, "hu",
            "unset defaults keep their spec value"
        );
        assert_eq!(
            account.seller.bank_account.as_deref(),
            Some("11111111-22222222")
        );
    }

    #[tokio::test]
    async fn mode_omitted_is_live_and_the_endpoint_defaults_to_production() {
        let config: StaticConfig =
            serde_json::from_value(json!({ "account": { "id": "acme", "agent_key": "k" } }))
                .expect("config");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        let account = resolver.resolve(None).await.expect("unscoped");
        assert_eq!(account.mode, AccountMode::Live);
        assert_eq!(
            account.supplier_id, None,
            "the supplier id is optional in the single-account shape"
        );
        assert_eq!(account.endpoint.as_str(), "https://www.szamlazz.hu/szamla/");
    }

    #[tokio::test]
    async fn resolve_unscoped_is_the_account_and_any_scope_is_unknown() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        let account = resolver.resolve(None).await.expect("unscoped");
        assert_eq!(account.id.as_str(), "acme");
        assert!(
            matches!(
                resolver.resolve(Some("acme")).await,
                Err(ResolveError::Unknown { scope }) if scope == "acme"
            ),
            "even the account's own id is not a scope in the single shape"
        );
    }

    #[tokio::test]
    async fn fetch_by_the_account_id_is_the_inline_key_and_anything_else_is_gone() {
        let resolver = StaticResolver::try_from(single()).expect("resolver");
        assert_eq!(key(&resolver, "acme").await, "key-acme");
        assert!(matches!(
            resolver.fetch(&CredentialRef::from("other")).await,
            Err(FetchError::Gone { credential_ref }) if credential_ref.as_str() == "other"
        ));
    }

    #[test]
    fn invalid_endpoint_and_blank_key_and_blank_id_are_construction_errors() {
        let mut config = single();
        config.account.as_mut().expect("account").endpoint = Some("localhost".to_owned());
        assert!(matches!(
            StaticResolver::try_from(config),
            Err(StaticConfigError::InvalidEndpoint { table, id, .. })
                if table == Table::account() && id.as_str() == "acme"
        ));

        let mut config = single();
        config.account.as_mut().expect("account").agent_key = " ".into();
        let error = StaticResolver::try_from(config).expect_err("blank key");
        assert!(matches!(
            &error,
            StaticConfigError::EmptyAgentKey { table, id }
                if *table == Table::account() && id.as_str() == "acme"
        ));
        assert_eq!(
            error.to_string(),
            "account.agent_key must not be empty (account acme)"
        );

        let mut config = single();
        config.account.as_mut().expect("account").id = " ".into();
        let error = StaticResolver::try_from(config).expect_err("blank id");
        assert!(matches!(
            &error,
            StaticConfigError::EmptyId { table } if *table == Table::account()
        ));
        assert_eq!(error.to_string(), "account.id must not be empty");
    }

    /// szamlazz.hu agent keys may be all digits, and a TOML or YAML author
    /// writing one unquoted produces a number.
    #[tokio::test]
    async fn numeric_agent_key_is_accepted() {
        let config: StaticConfig = serde_json::from_value(json!({
            "account": { "id": "acme", "agent_key": 12_345_678 },
        }))
        .expect("config");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        assert_eq!(key(&resolver, "acme").await, "12345678");
    }

    #[test]
    fn debug_renderings_redact_the_agent_key() {
        const KEY: &str = "sentinel-agent-key-7d1e";
        let config: StaticConfig = serde_json::from_value(json!({
            "account": { "id": "acme", "agent_key": KEY },
        }))
        .expect("config");
        assert!(!format!("{config:?}").contains(KEY), "{config:?}");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        assert!(!format!("{resolver:?}").contains(KEY), "{resolver:?}");

        let mut multi = multi();
        multi["accounts"]["acme"]["agent_key"] = json!(KEY);
        let config: StaticConfig = serde_json::from_value(multi).expect("config");
        assert!(!format!("{config:?}").contains(KEY), "{config:?}");
        let resolver = StaticResolver::try_from(config).expect("resolver");
        assert!(!format!("{resolver:?}").contains(KEY), "{resolver:?}");
    }

    // ----- the multi-account shape ------------------------------------------

    /// `[accounts.<scope>]` parses; each account is reachable under its scope
    /// only, unscoped is unscoped, an unknown scope is unknown, and the store
    /// hands out each account's own key by its id.
    #[tokio::test]
    async fn multi_account_shape_resolves_each_account_by_its_scope() {
        let resolver = resolver(multi()).expect("resolver");
        assert!(resolver.is_scoped());

        let acme = resolver.resolve(Some("acme")).await.expect("acme");
        assert_eq!(acme.id.as_str(), "acme");
        assert_eq!(acme.credential_ref.as_str(), "acme", "credential_ref = id");
        assert_eq!(acme.supplier_id, Some(972_720));
        assert_eq!(acme.mode, AccountMode::Test);
        assert_eq!(
            acme.seller.bank_account.as_deref(),
            Some("11111111-22222222")
        );
        let beta = resolver.resolve(Some("beta_events")).await.expect("beta");
        assert_eq!(beta.id.as_str(), "beta", "the id need not equal the scope");
        assert_eq!(beta.supplier_id, Some(972_721));

        assert!(matches!(
            resolver.resolve(None).await,
            Err(ResolveError::Unscoped)
        ));
        assert!(matches!(
            resolver.resolve(Some("beta")).await,
            Err(ResolveError::Unknown { scope }) if scope == "beta"
        ));

        assert_eq!(key(&resolver, "acme").await, "key-acme");
        assert_eq!(key(&resolver, "beta").await, "key-beta");
        assert!(matches!(
            resolver.fetch(&CredentialRef::from("beta_events")).await,
            Err(FetchError::Gone { .. })
        ));

        let listed: Vec<_> = resolver
            .accounts()
            .map(|(scope, account)| (scope.map(str::to_owned), account.id.to_string()))
            .collect();
        assert_eq!(
            listed,
            [
                (Some("acme".to_owned()), "acme".to_owned()),
                (Some("beta_events".to_owned()), "beta".to_owned()),
            ]
        );
    }

    #[test]
    fn both_shapes_is_an_error_and_so_is_neither() {
        let mut both = multi();
        both["account"] = single()
            .account
            .map(|_| json!({ "id": "solo", "agent_key": "k", "supplier_id": 1 }))
            .expect("account");
        assert!(matches!(resolver(both), Err(StaticConfigError::BothShapes)));

        assert!(matches!(
            resolver(json!({})),
            Err(StaticConfigError::NoAccount)
        ));
        assert!(
            matches!(
                resolver(json!({ "accounts": {} })),
                Err(StaticConfigError::NoAccount)
            ),
            "an empty accounts table is no account"
        );
    }

    #[test]
    fn multi_account_shape_requires_a_supplier_id_on_every_account() {
        let mut config = multi();
        config["accounts"]["beta_events"]
            .as_object_mut()
            .expect("object")
            .remove("supplier_id");
        let error = resolver(config).expect_err("missing supplier id");
        assert!(matches!(
            &error,
            StaticConfigError::MissingSupplierId { table, id }
                if *table == Table::accounts("beta_events") && id.as_str() == "beta"
        ));
        assert_eq!(
            error.to_string(),
            "accounts.beta_events.supplier_id is required in the multi-account shape (account beta)"
        );
    }

    #[test]
    fn duplicate_supplier_id_is_an_error() {
        let mut config = multi();
        config["accounts"]["beta_events"]["supplier_id"] = json!(972_720);
        let error = resolver(config).expect_err("duplicate supplier id");
        assert!(matches!(
            &error,
            StaticConfigError::DuplicateSupplierId { supplier_id: 972_720, first, second }
                if *first == Table::accounts("acme") && *second == Table::accounts("beta_events")
        ));
        assert_eq!(
            error.to_string(),
            "accounts.acme and accounts.beta_events share the supplier id 972720"
        );
    }

    /// The same key on the same endpoint is one szamlazz.hu account under two
    /// scopes; the same key on another endpoint is not. The production
    /// endpoint is compared as such whether written or defaulted, and the
    /// error never echoes the key.
    #[test]
    fn duplicate_endpoint_and_agent_key_is_an_error() {
        const KEY: &str = "shared-sentinel-key-3c9e";
        let mut config = multi();
        config["accounts"]["acme"]["agent_key"] = json!(KEY);
        config["accounts"]["beta_events"]["agent_key"] = json!(KEY);
        let error = resolver(config).expect_err("duplicate credentials");
        assert!(matches!(
            &error,
            StaticConfigError::DuplicateCredentials { first, second }
                if *first == Table::accounts("acme") && *second == Table::accounts("beta_events")
        ));
        assert!(!error.to_string().contains(KEY), "{error}");
        assert!(!format!("{error:?}").contains(KEY), "{error:?}");

        let mut config = multi();
        config["accounts"]["acme"]["agent_key"] = json!(KEY);
        config["accounts"]["beta_events"]["agent_key"] = json!(KEY);
        config["accounts"]["beta_events"]["endpoint"] = json!("http://127.0.0.1:2/");
        resolver(config).expect("the same key on another endpoint is another account");

        let mut config = multi();
        config["accounts"]["acme"]["agent_key"] = json!(KEY);
        config["accounts"]["beta_events"]["agent_key"] = json!(KEY);
        config["accounts"]["acme"]["endpoint"] = json!(Endpoint::PRODUCTION);
        config["accounts"]["beta_events"]
            .as_object_mut()
            .expect("object")
            .remove("endpoint");
        assert!(
            matches!(
                resolver(config),
                Err(StaticConfigError::DuplicateCredentials { .. })
            ),
            "a written and a defaulted production endpoint are the same endpoint"
        );
    }

    /// Two scopes with one id would share a credential reference.
    #[test]
    fn duplicate_id_is_an_error() {
        let mut config = multi();
        config["accounts"]["beta_events"]["id"] = json!("acme");
        let error = resolver(config).expect_err("duplicate id");
        assert!(matches!(
            &error,
            StaticConfigError::DuplicateId { id, first, second }
                if id.as_str() == "acme"
                    && *first == Table::accounts("acme")
                    && *second == Table::accounts("beta_events")
        ));
        assert_eq!(
            error.to_string(),
            "accounts.acme and accounts.beta_events share the id acme"
        );
    }

    /// Scope keys are `[a-z0-9_]`, 1–36 bytes: what an environment override
    /// can address, and a strict subset of Restate's scope format.
    #[test]
    fn scope_key_outside_the_charset_is_an_error() {
        fn with_scope(scope: &str) -> serde_json::Value {
            json!({
                "accounts": {
                    scope: { "id": "acme", "agent_key": "k", "supplier_id": 1 },
                },
            })
        }

        let longest = "a".repeat(MAX_SCOPE_LEN);
        for accepted in ["acme", "acme_events", "a1", "0", "_", longest.as_str()] {
            resolver(with_scope(accepted)).unwrap_or_else(|error| panic!("{accepted}: {error}"));
        }

        let too_long = "a".repeat(MAX_SCOPE_LEN + 1);
        for (scope, expected) in [
            ("", InvalidScope::Empty),
            ("Acme", InvalidScope::InvalidChar('A')),
            ("acme-events", InvalidScope::InvalidChar('-')),
            ("acme.events", InvalidScope::InvalidChar('.')),
            ("acme events", InvalidScope::InvalidChar(' ')),
            ("ácme", InvalidScope::InvalidChar('á')),
            (too_long.as_str(), InvalidScope::TooLong(MAX_SCOPE_LEN + 1)),
        ] {
            let error = resolver(with_scope(scope)).expect_err(scope);
            assert!(
                matches!(
                    &error,
                    StaticConfigError::InvalidScope { scope: found, source }
                        if found == scope && *source == expected
                ),
                "{scope:?}: {error:?}"
            );
        }
        assert_eq!(
            resolver(with_scope("acme-events"))
                .expect_err("dash")
                .to_string(),
            "accounts.\"acme-events\": scope may only contain lowercase ASCII letters, digits and '_' (so that environment overrides can address it), found '-'"
        );
    }

    /// Per-account errors in the multi-account shape name the scope's table.
    #[test]
    fn per_account_errors_name_the_scope_table() {
        let mut config = multi();
        config["accounts"]["beta_events"]["endpoint"] = json!("localhost");
        let error = resolver(config).expect_err("invalid endpoint");
        assert!(matches!(
            &error,
            StaticConfigError::InvalidEndpoint { table, id, .. }
                if *table == Table::accounts("beta_events") && id.as_str() == "beta"
        ));
        assert!(
            error
                .to_string()
                .starts_with("accounts.beta_events.endpoint (account beta): "),
            "{error}"
        );

        let mut config = multi();
        config["accounts"]["acme"]["agent_key"] = json!("");
        assert_eq!(
            resolver(config).expect_err("blank key").to_string(),
            "accounts.acme.agent_key must not be empty (account acme)"
        );
    }
}
