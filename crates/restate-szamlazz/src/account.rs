//! The account model and the two pluggable traits that produce it.
//!
//! An [`Account`] is one szamlazz.hu account as the worker knows it — its
//! resolver-owned id, mode, supplier pin, endpoint, document defaults, seller
//! block and a reference to its credentials. Never the agent key: the account
//! is resolved once per invocation and journaled, and the journal is visible in
//! the Restate UI for the retention period.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use http::Uri;
use serde::{Deserialize, Serialize};
use szamlazz_agent::Credentials;

use crate::config::{AccountMode, Config, Defaults, SellerConfig};

pub mod static_resolver;

pub use static_resolver::{StaticAccount, StaticConfig, StaticConfigError, StaticResolver};

/// One szamlazz.hu account as the worker knows it — never the agent key.
///
/// Resolved once per invocation by an account resolver and journaled, so an
/// invocation finishes on the account it started on. Everything account-shaped
/// the service layer reads is here: the ownership-validation pins (`mode`,
/// `supplier_id`), the endpoint, the document defaults and the seller block.
/// The credentials are fetched separately, by [`Account::credential_ref`],
/// on every handler execution.
///
/// # Journal compatibility
///
/// The type is **additive-only**: a new field gets a `#[serde(default)]`,
/// and no field is renamed or removed, so a journaled account written by an
/// earlier version reads back under a later one. `id` and `credential_ref`
/// are the only required fields. The struct is `#[non_exhaustive]` for the
/// same reason; build one with [`Account::new`] and set the rest.
///
/// # Ownership validation
///
/// A document found under one of our external ids is ours only when it
/// carries the order number, the `tipus` of the kind, `teszt` equal to
/// [`mode`](Self::mode) and — when both are known — the account's
/// [`supplier_id`](Self::supplier_id). The mode defaults to live and is
/// always checked: a test account configured as live fails on its first
/// found document instead of issuing on the wrong account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Account {
    /// The resolver's identifier of the account; opaque to the worker.
    pub id: AccountId,
    /// Whether the account is live or a test account; validated against
    /// `teszt` on every document found under our external ids. Default live.
    #[serde(default)]
    pub mode: AccountMode,
    /// The account's supplier id (`szállító/id`). Optional pin; when set it is
    /// validated against every document found under our external ids.
    #[serde(default)]
    pub supplier_id: Option<u64>,
    /// The Számla Agent endpoint. Default: production.
    #[serde(default)]
    pub endpoint: Endpoint,
    /// Document defaults that per-call overrides may change.
    #[serde(default)]
    pub defaults: Defaults,
    /// The seller block; account data is used where absent.
    #[serde(default)]
    pub seller: SellerConfig,
    /// What the credential store fetches the agent key by. Opaque to the
    /// worker; resolver-owned like the id.
    pub credential_ref: CredentialRef,
}

impl Account {
    /// An account with `id` and `credential_ref`, live, unpinned, on the
    /// production endpoint, with default document settings.
    pub fn new(id: impl Into<AccountId>, credential_ref: impl Into<CredentialRef>) -> Self {
        Self {
            id: id.into(),
            mode: AccountMode::default(),
            supplier_id: None,
            endpoint: Endpoint::default(),
            defaults: Defaults::default(),
            seller: SellerConfig::default(),
            credential_ref: credential_ref.into(),
        }
    }
}

/// The legacy path: the account of a [`Config`], whose one identifier is the
/// namespace — it becomes both the id and the credential reference.
impl TryFrom<&Config> for Account {
    type Error = InvalidEndpoint;

    fn try_from(config: &Config) -> Result<Self, Self::Error> {
        let namespace = config.account.slug.as_str();
        let mut account = Self::new(namespace, namespace);
        account.mode = config.account.mode;
        account.supplier_id = config.account.supplier_id;
        account.endpoint = match &config.account.endpoint {
            Some(endpoint) => Endpoint::parse(endpoint)?,
            None => Endpoint::production(),
        };
        account.defaults.clone_from(&config.defaults);
        account.seller.clone_from(&config.seller);
        Ok(account)
    }
}

macro_rules! opaque_string {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// The value as a string slice.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }
    };
}

opaque_string! {
    /// The resolver's identifier of an [`Account`]: opaque to the worker,
    /// meaningful to the operator. Never a resolution input — the scope is.
    AccountId
}

opaque_string! {
    /// What a credential store fetches an account's credentials by. Chosen
    /// by the resolver, understood by the store, opaque to the worker.
    CredentialRef
}

/// A Számla Agent endpoint URL: an `http` or `https` URI with a host.
///
/// Validated when parsed and when deserialized, so an [`Account`] never
/// carries an endpoint the client cannot post to. The text is kept as
/// written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Endpoint(String);

impl Endpoint {
    /// The production endpoint, `https://www.szamlazz.hu/szamla/`.
    pub const PRODUCTION: &str = szamlazz_agent::wire::ENDPOINT;

    /// The production endpoint.
    #[must_use]
    pub fn production() -> Self {
        Self(Self::PRODUCTION.to_owned())
    }

    /// Parses and validates an endpoint URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the text is not a URI, its scheme is neither
    /// `http` nor `https`, or it has no host.
    pub fn parse(value: &str) -> Result<Self, InvalidEndpoint> {
        let uri: Uri = value.parse()?;
        match uri.scheme_str().map(str::to_ascii_lowercase).as_deref() {
            Some("http" | "https") => {}
            _ => return Err(InvalidEndpoint::Scheme),
        }
        if uri.host().is_none_or(str::is_empty) {
            return Err(InvalidEndpoint::Host);
        }
        Ok(Self(value.to_owned()))
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for Endpoint {
    fn default() -> Self {
        Self::production()
    }
}

impl FromStr for Endpoint {
    type Err = InvalidEndpoint;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

impl TryFrom<String> for Endpoint {
    type Error = InvalidEndpoint;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for Endpoint {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Serializes as the plain string.
impl Serialize for Endpoint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Deserializes from a string, rejecting invalid endpoints.
impl<'de> Deserialize<'de> for Endpoint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(&String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// A string that is not a valid [`Endpoint`]. Does not echo the text: an
/// endpoint URL may carry userinfo.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InvalidEndpoint {
    /// The text is not a URI.
    #[error("endpoint is not a valid URI: {0}")]
    Uri(#[from] http::uri::InvalidUri),
    /// The scheme is neither `http` nor `https`.
    #[error("endpoint must be an http or https URL")]
    Scheme,
    /// The URI has no host.
    #[error("endpoint has no host")]
    Host,
}

/// The future the two traits return: boxed so that the traits are
/// object-safe and the services can hold them as `dyn`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Maps the Restate scope of a request to the [`Account`] it names.
///
/// The worker calls [`resolve`](Self::resolve) once per invocation, inside a
/// durable step, and journals the result: the scope is the only resolution
/// input — never a header, a body field or the Virtual Object key.
///
/// # Safety contract
///
/// The worker cannot check these at runtime; a resolver guarantees them.
///
/// - **One account under exactly one scope, no fan-in.** Unscoped counts as
///   a scope value. Two scopes reaching one szamlazz.hu account would split
///   an order's per-key lock across two Virtual Objects. The static resolver
///   satisfies this by construction with one account and checks it at load
///   time once it holds several; a database-backed resolver must guarantee it
///   itself — `check_account` only echoes the configuration.
/// - **Append-only mapping.** Moving traffic to another account means a new
///   scope; a scope's account is never changed in place. A running
///   invocation stays on the account it journaled either way.
///
/// Unscoped and unknown are answers, not faults of the resolver: the request
/// names no account, and the worker reports that to the caller as a terminal
/// fault. `Unavailable` is the resolver's own fault, retryable, and journals
/// nothing. A resolver may cache internally.
pub trait AccountResolver: fmt::Debug + Send + Sync {
    /// The account reachable under `scope`; `None` is the unscoped request.
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>>;
}

/// Fetches an account's credentials by its [`CredentialRef`].
///
/// # Safety contract
///
/// - **Fetched on every handler execution, never journaled.** The worker
///   calls [`fetch`](Self::fetch) outside the journal every time a handler
///   executes, including replays, and holds the result only for that
///   execution — so a rotation is picked up on the next execution of every
///   in-flight invocation, and no agent key is written into Restate. The
///   return type is the Számla Agent crate's [`Credentials`], which has no
///   serde implementation: the compiler rejects any attempt to journal it.
/// - A `Gone` reference is an answer (the account's credentials were
///   removed); `Unavailable` is a fault of the store.
pub trait CredentialStore: fmt::Debug + Send + Sync {
    /// The credentials under `credential_ref`.
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a CredentialRef,
    ) -> BoxFuture<'a, Result<Credentials, FetchError>>;
}

/// [`AccountResolver::resolve`] failure.
///
/// `Unavailable` carries its cause for `source()` but its display text never
/// echoes the cause's message: the text may become a fault body or a journal
/// entry, and the cause is the resolver's own business.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResolveError {
    /// The request carries no scope and this deployment serves no unscoped
    /// account.
    #[error("no account is reachable unscoped")]
    Unscoped,
    /// No account is reachable under the scope.
    #[error("no account is reachable under scope {scope:?}")]
    Unknown {
        /// The scope the request carried.
        scope: String,
    },
    /// The resolver could not answer; retryable.
    #[error("the account resolver is unavailable")]
    Unavailable(#[source] BoxError),
}

impl ResolveError {
    /// An `Unavailable` error caused by `source`.
    pub fn unavailable(source: impl Into<BoxError>) -> Self {
        Self::Unavailable(source.into())
    }
}

/// [`CredentialStore::fetch`] failure. Same display rule as
/// [`ResolveError`]: `Unavailable` never echoes its cause.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FetchError {
    /// The store has no credentials under the reference.
    #[error("no credentials under reference {credential_ref}")]
    Gone {
        /// The reference the account carried.
        credential_ref: CredentialRef,
    },
    /// The store could not answer; retryable.
    #[error("the credential store is unavailable")]
    Unavailable(#[source] BoxError),
}

impl FetchError {
    /// An `Unavailable` error caused by `source`.
    pub fn unavailable(source: impl Into<BoxError>) -> Self {
        Self::Unavailable(source.into())
    }
}

/// The cause an `Unavailable` error carries.
pub type BoxError = Box<dyn std::error::Error + Send + Sync + 'static>;

/// The bundle of resolver and store both Restate services share.
///
/// Trait objects rather than type parameters, so that the services' types —
/// and the SDK-generated clients — do not change with the deployment's
/// choice of resolver.
#[derive(Debug, Clone)]
pub struct Accounts {
    resolver: Arc<dyn AccountResolver>,
    store: Arc<dyn CredentialStore>,
}

impl Accounts {
    /// Bundles `resolver` and `store`.
    #[must_use]
    pub fn new(resolver: Arc<dyn AccountResolver>, store: Arc<dyn CredentialStore>) -> Self {
        Self { resolver, store }
    }

    /// The account reachable under `scope`; see
    /// [`AccountResolver::resolve`].
    ///
    /// # Errors
    ///
    /// The resolver's error.
    pub async fn resolve(&self, scope: Option<&str>) -> Result<Account, ResolveError> {
        self.resolver.resolve(scope).await
    }

    /// The credentials of `account`, fetched by its
    /// [`credential_ref`](Account::credential_ref); see
    /// [`CredentialStore::fetch`].
    ///
    /// # Errors
    ///
    /// The store's error.
    pub async fn fetch(&self, account: &Account) -> Result<Credentials, FetchError> {
        self.store.fetch(&account.credential_ref).await
    }
}

/// The static resolver is both halves of the bundle.
impl From<StaticResolver> for Accounts {
    fn from(resolver: StaticResolver) -> Self {
        let resolver = Arc::new(resolver);
        Self::new(Arc::clone(&resolver) as Arc<dyn AccountResolver>, resolver)
    }
}

/// The adapter from the library [`Config`]: its single account through the
/// static resolver. Goes with `Config` in #31.
impl TryFrom<&Config> for Accounts {
    type Error = InvalidEndpoint;

    fn try_from(config: &Config) -> Result<Self, Self::Error> {
        StaticResolver::try_from(config).map(Self::from)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use static_assertions::assert_not_impl_any;
    use szamlazz_agent::AgentKey;

    use super::*;

    // The compile-time guard against journaling credentials: neither type
    // can be serialized or deserialized, so a `ctx.run` closure cannot
    // return one.
    assert_not_impl_any!(Credentials: serde::Serialize, serde::Deserialize<'static>);
    assert_not_impl_any!(AgentKey: serde::Serialize, serde::Deserialize<'static>);

    /// Everything the worker may print or journal about an account — the
    /// account itself, the bundle, an opened gateway, the resolver's and the
    /// store's errors — renders without the agent key.
    #[tokio::test]
    async fn renderings_of_account_accounts_gateway_and_errors_carry_no_secret() {
        const KEY: &str = "sentinel-agent-key-4b8c2e";
        let config: StaticConfig = serde_json::from_value(json!({
            "account": {
                "id": "acme",
                "agent_key": KEY,
                "endpoint": "http://127.0.0.1:1/",
                "mode": "test",
                "supplier_id": 972_720,
            },
        }))
        .expect("config");
        let accounts = Accounts::from(StaticResolver::try_from(config).expect("resolver"));
        let account = accounts.resolve(None).await.expect("account");
        let credentials = accounts.fetch(&account).await.expect("credentials");
        assert!(
            matches!(&credentials, Credentials::AgentKey(key) if key.expose() == KEY),
            "the key is really in play"
        );
        let gateway = crate::gateway::Gateway::open(account.clone(), credentials).expect("gateway");

        let renderings = [
            ("Account Debug", format!("{account:?}")),
            (
                "Account JSON",
                serde_json::to_string(&account).expect("json"),
            ),
            ("Accounts Debug", format!("{accounts:?}")),
            ("Gateway Debug", format!("{gateway:?}")),
            ("Gateway account Debug", format!("{:?}", gateway.account())),
        ];
        for (label, rendering) in renderings {
            assert!(!rendering.contains(KEY), "{label}: {rendering}");
        }

        let unknown = accounts.resolve(Some("x")).await.expect_err("unknown");
        let gone = accounts
            .fetch(&Account::new("other", "other"))
            .await
            .expect_err("gone");
        let unavailable = ResolveError::unavailable(std::io::Error::other("connection reset"));
        let store_unavailable = FetchError::unavailable(std::io::Error::other("timed out"));
        for (label, rendering) in [
            ("Unknown Debug", format!("{unknown:?}")),
            ("Unknown Display", unknown.to_string()),
            ("Unscoped Display", ResolveError::Unscoped.to_string()),
            ("Gone Debug", format!("{gone:?}")),
            ("Gone Display", gone.to_string()),
            ("Unavailable Debug", format!("{unavailable:?}")),
            ("Unavailable Display", unavailable.to_string()),
            ("store Unavailable Debug", format!("{store_unavailable:?}")),
            ("store Unavailable Display", store_unavailable.to_string()),
        ] {
            assert!(!rendering.contains(KEY), "{label}: {rendering}");
        }
    }

    #[test]
    fn account_journals_as_json_without_a_secret_and_reads_back() {
        let mut account = Account::new("acme", "acme-key");
        account.mode = AccountMode::Test;
        account.supplier_id = Some(972_720);
        account.endpoint = Endpoint::parse("http://127.0.0.1:1/").expect("endpoint");
        account.seller.bank_account = Some("11111111-22222222".to_owned());

        let json = serde_json::to_value(&account).expect("serialize");
        assert_eq!(json["id"], "acme");
        assert_eq!(json["credential_ref"], "acme-key");
        assert_eq!(json["mode"], "test");
        assert_eq!(json["supplier_id"], 972_720);
        assert_eq!(json["endpoint"], "http://127.0.0.1:1/");
        assert_eq!(json["seller"]["bank_account"], "11111111-22222222");
        assert_eq!(json["defaults"]["currency"], "HUF");
        assert!(
            json.as_object()
                .expect("object")
                .keys()
                .all(|key| !key.contains("key") && !key.contains("secret")),
            "{json}"
        );

        let back: Account = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, account);
    }

    /// Additive-only: a journaled account written before a field existed
    /// reads back with that field's default. `id` and `credential_ref` are the
    /// only required fields; the mode is live unless said otherwise.
    #[test]
    fn account_is_additive_only_with_live_mode_and_production_endpoint_by_default() {
        let account: Account =
            serde_json::from_value(json!({ "id": "acme", "credential_ref": "acme" }))
                .expect("deserialize");
        assert_eq!(account, Account::new("acme", "acme"));
        assert_eq!(account.mode, AccountMode::Live);
        assert_eq!(account.supplier_id, None);
        assert_eq!(account.endpoint, Endpoint::production());
        assert_eq!(account.endpoint.as_str(), "https://www.szamlazz.hu/szamla/");
        assert_eq!(account.defaults, crate::config::Defaults::default());
        assert_eq!(account.seller, crate::config::SellerConfig::default());
    }

    #[test]
    fn endpoint_requires_an_http_or_https_uri_with_a_host() {
        for valid in [
            "http://127.0.0.1:1234",
            "http://127.0.0.1:1234/",
            "https://www.szamlazz.hu/szamla/",
            "HTTPS://example.com/x?y=1",
        ] {
            let endpoint = Endpoint::parse(valid).unwrap_or_else(|e| panic!("{valid}: {e}"));
            assert_eq!(endpoint.as_str(), valid, "the text is kept as written");
            assert_eq!(endpoint.to_string(), valid);
        }
        for (invalid, is_expected) in [
            (
                "",
                (|e: &InvalidEndpoint| matches!(e, InvalidEndpoint::Uri(_))) as fn(&_) -> bool,
            ),
            ("not a url", |e| matches!(e, InvalidEndpoint::Uri(_))),
            ("http:///x", |e| matches!(e, InvalidEndpoint::Uri(_))),
            ("localhost", |e| matches!(e, InvalidEndpoint::Scheme)),
            ("/szamla/", |e| matches!(e, InvalidEndpoint::Scheme)),
            ("ftp://example.com/", |e| {
                matches!(e, InvalidEndpoint::Scheme)
            }),
        ] {
            let error = Endpoint::parse(invalid).expect_err(invalid);
            assert!(is_expected(&error), "{invalid}: {error:?}");
        }
        assert!(
            serde_json::from_value::<Endpoint>(json!("localhost")).is_err(),
            "deserialization validates too"
        );
    }

    /// A resolver and a store that a downstream build might plug in: the
    /// traits are object-safe and the bundle routes through them. The store
    /// holds [`Credentials`], whose `Debug` is redacted — a store that held
    /// raw strings would print them through `Accounts`' `Debug`.
    #[derive(Debug)]
    struct Table {
        accounts: Vec<(Option<&'static str>, Account)>,
        keys: Vec<(&'static str, Credentials)>,
    }

    impl AccountResolver for Table {
        fn resolve<'a>(
            &'a self,
            scope: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Account, ResolveError>> {
            Box::pin(async move {
                let Some(scope) = scope else {
                    return self
                        .accounts
                        .iter()
                        .find(|(s, _)| s.is_none())
                        .map(|(_, account)| account.clone())
                        .ok_or(ResolveError::Unscoped);
                };
                self.accounts
                    .iter()
                    .find(|(s, _)| *s == Some(scope))
                    .map(|(_, account)| account.clone())
                    .ok_or_else(|| ResolveError::Unknown {
                        scope: scope.to_owned(),
                    })
            })
        }
    }

    impl CredentialStore for Table {
        fn fetch<'a>(
            &'a self,
            credential_ref: &'a CredentialRef,
        ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
            Box::pin(async move {
                self.keys
                    .iter()
                    .find(|(r, _)| *r == credential_ref.as_str())
                    .map(|(_, credentials)| credentials.clone())
                    .ok_or_else(|| FetchError::Gone {
                        credential_ref: credential_ref.clone(),
                    })
            })
        }
    }

    #[tokio::test]
    async fn accounts_bundle_routes_through_the_plugged_in_resolver_and_store() {
        let table = std::sync::Arc::new(Table {
            accounts: vec![
                (Some("a"), Account::new("acme", "acme-key")),
                (Some("b"), Account::new("beta", "beta-key")),
            ],
            keys: vec![("acme-key", Credentials::agent_key("key-a"))],
        });
        let accounts = Accounts::new(table.clone(), table);

        let account = accounts.resolve(Some("a")).await.expect("resolves");
        assert_eq!(account.id.as_str(), "acme");
        assert!(matches!(
            accounts.resolve(None).await,
            Err(ResolveError::Unscoped)
        ));
        assert!(matches!(
            accounts.resolve(Some("zzz")).await,
            Err(ResolveError::Unknown { scope }) if scope == "zzz"
        ));

        let credentials = accounts.fetch(&account).await.expect("fetches");
        assert!(matches!(credentials, Credentials::AgentKey(key) if key.expose() == "key-a"));
        let beta = accounts.resolve(Some("b")).await.expect("resolves");
        assert!(matches!(
            accounts.fetch(&beta).await,
            Err(FetchError::Gone { credential_ref }) if credential_ref.as_str() == "beta-key"
        ));

        let debug = format!("{accounts:?}");
        assert!(debug.contains("Table"), "{debug}");
        assert!(!debug.contains("key-a"), "{debug}");
    }

    /// The unavailable errors carry their cause for the operator's logs but
    /// never put its message into their own text: the text may end up in a
    /// fault body or a journal entry.
    #[test]
    fn unavailable_errors_never_echo_their_source_in_display() {
        let cause = || std::io::Error::other("db down at 10.0.0.7 as user svc");
        let resolve = ResolveError::unavailable(cause());
        let fetch = FetchError::unavailable(cause());
        for text in [resolve.to_string(), fetch.to_string()] {
            assert!(!text.contains("10.0.0.7"), "{text}");
            assert!(!text.contains("svc"), "{text}");
            assert!(text.contains("unavailable"), "{text}");
        }
        assert!(
            std::error::Error::source(&resolve)
                .is_some_and(|source| source.to_string().contains("10.0.0.7")),
            "the cause is reachable through source()"
        );
        assert!(std::error::Error::source(&fetch).is_some());
        assert_eq!(
            ResolveError::Unknown {
                scope: "x".to_owned()
            }
            .to_string(),
            "no account is reachable under scope \"x\""
        );
    }
}
