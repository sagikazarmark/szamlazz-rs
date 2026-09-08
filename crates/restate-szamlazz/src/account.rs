//! The account model and the two pluggable traits that produce it.
//!
//! An [`Account`] is one szamlazz.hu account as the worker knows it: its
//! resolver-owned id, endpoint, document defaults, seller block and a
//! reference to its credentials. Never the agent key: the account
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

use szamlazz_agent::ops::invoice::{Seller, SellerEmail};

pub mod static_resolver;

pub use static_resolver::{
    AccountTable, InvalidScope, MAX_SCOPE_LEN, Secret, StaticAccount, StaticConfig,
    StaticConfigError, StaticDefaults, StaticResolver, StaticSeller, StaticSellerEmail,
};

/// One szamlazz.hu account as the worker knows it, never the agent key.
///
/// Resolved once per invocation by an account resolver and journaled, so an
/// invocation finishes on the account it started on. Everything account-shaped
/// the service layer reads is here: the endpoint, the document defaults and
/// the seller block. The credentials are fetched separately, by
/// [`Account::credential_ref`], on every handler execution.
///
/// # Journal compatibility
///
/// The type is **additive-only**, like every type the services journal (the
/// [`gateway`](crate::gateway) module docs state the rule once): a new field
/// gets a `#[serde(default)]`, and no field is renamed or removed, so a
/// journaled account written by an earlier version reads back under a later
/// one. `id` and `credential_ref` are the only required fields. The struct is
/// `#[non_exhaustive]` for the same reason; build one with [`Account::new`]
/// and set the rest. Its journaled shape is pinned under
/// `tests/journal/resolution/`.
///
/// # No account pin
///
/// The account carries **nothing the worker checks a found document
/// against**. Ownership validation is about the *document*: under one of our
/// external ids a document is ours when it carries the order number and the
/// `tipus` of the kind, and found by number it must carry this order's number
/// (`Szamlazz.Order`'s verifies); nothing about the account. 0.3 pinned two
/// fields of a queried document, `szallito/id` (`supplier_id`) and `teszt`
/// (`mode`); both were dropped. Neither is in a create response (a create's
/// reply is a number and totals), so neither
/// could fire before the first document of a fresh order was issued: a key
/// configured under the wrong scope issued into the wrong account and answered
/// `issued`, and the pin tripped on the *next* found document. A tripwire with
/// that blind spot, on fields the operator had to read off the very account
/// being checked (`szallito/id`, undocumented) or that only tell test from
/// live (`teszt`), was not worth a fault code and a configuration field.
///
/// So **the right key under the right scope is the resolver's guarantee**,
/// and the deployment's to verify: under each scope, at go-live and after
/// every key rotation, `Szamlazz.Agent.query` a document known to be the
/// account's and read `<teszt>` and the seller block (name, tax number) on
/// the answer. A key pasted into the wrong scope issues that scope's
/// documents in another company's name (or on a test account, or on a live
/// one from staging), with nothing in the worker failing. The endpoint
/// README's deploy checklist carries the check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Account {
    /// The resolver's identifier of the account; opaque to the worker.
    pub id: AccountId,
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
    /// An account with `id` and `credential_ref` on the production endpoint,
    /// with default document settings.
    pub fn new(id: impl Into<AccountId>, credential_ref: impl Into<CredentialRef>) -> Self {
        Self {
            id: id.into(),
            endpoint: Endpoint::default(),
            defaults: Defaults::default(),
            seller: SellerConfig::default(),
            credential_ref: credential_ref.into(),
        }
    }
}

/// Document defaults; [`DocumentOverrides`](crate::contract::DocumentOverrides)
/// may change the first seven per call.
///
/// Journaled inside the [`Account`], so
/// additive-only and `#[non_exhaustive]`: start from [`Default::default`]
/// and set fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct Defaults {
    /// Issue e-invoices (`e-számla`). Default `false`.
    pub e_invoice: bool,
    /// Document language code. Default `hu`.
    pub language: String,
    /// Currency code. Default `HUF`.
    pub currency: String,
    /// Quoting bank for non-HUF documents without an explicit rate. Default
    /// `MNB`.
    pub exchange_rate_bank: String,
    /// PDF template token.
    pub template: Option<String>,
    /// Whether szamlazz.hu should email documents to buyers.
    pub send_email: Option<bool>,
    /// Invoice number prefix (`számlaszám előtag`).
    pub number_prefix: Option<String>,
    /// Additional logo token configured on the account.
    pub extra_logo: Option<String>,
    /// Aggregator identifier for contracted integrations; not overridable per
    /// call.
    pub aggregator: Option<String>,
    /// Guardian processing flag for contracted integrations; not overridable
    /// per call.
    pub guardian: Option<bool>,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            e_invoice: false,
            language: "hu".to_owned(),
            currency: "HUF".to_owned(),
            exchange_rate_bank: "MNB".to_owned(),
            template: None,
            send_email: None,
            number_prefix: None,
            extra_logo: None,
            aggregator: None,
            guardian: None,
        }
    }
}

/// The seller (`eladó`) block; the account's own data is used where absent.
///
/// Journaled inside the [`Account`], so
/// additive-only and `#[non_exhaustive]`: start from [`Default::default`]
/// and set fields.
///
/// Deliberately not the agent crate's [`Seller`], although the fields mirror
/// it: the account's journal shape is this crate's contract with every
/// in-flight invocation, and a crate-owned type keeps a `Seller`
/// change in `szamlazz-agent` (a field renamed, retyped, or made required)
/// from altering what an `account` entry replays as. The same reason
/// `Szamlazz.Agent.query_taxpayer` journals the crate-owned
/// `QueryTaxpayerResponse` projection rather than the agent crate's
/// `TaxpayerInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct SellerConfig {
    /// Bank name.
    pub bank: Option<String>,
    /// Bank account number.
    pub bank_account: Option<String>,
    /// Name of the signer shown on documents.
    pub signer_name: Option<String>,
    /// The notification email szamlazz.hu sends to buyers.
    pub email: SellerEmailConfig,
}

impl SellerConfig {
    /// The Agent seller block. The email block is present only when at least
    /// one of its fields is set.
    #[must_use]
    pub fn to_seller(&self) -> Seller {
        Seller {
            bank: self.bank.clone(),
            bank_account: self.bank_account.clone(),
            signer_name: self.signer_name.clone(),
            email: self.email.to_seller_email(),
        }
    }
}

/// Settings of the notification email szamlazz.hu sends to buyers.
///
/// Journaled inside the [`Account`] through
/// [`SellerConfig`], so additive-only and `#[non_exhaustive]`: start from
/// [`Default::default`] and set fields.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct SellerEmailConfig {
    /// Reply-to address.
    pub reply_to: Option<String>,
    /// Subject.
    pub subject: Option<String>,
    /// Body; supports `BBCode`.
    pub body: Option<String>,
}

impl SellerEmailConfig {
    /// The Agent email block, or `None` when nothing is configured.
    #[must_use]
    pub fn to_seller_email(&self) -> Option<SellerEmail> {
        if self.reply_to.is_none() && self.subject.is_none() && self.body.is_none() {
            return None;
        }
        Some(SellerEmail {
            reply_to: self.reply_to.clone(),
            subject: self.subject.clone(),
            body: self.body.clone(),
        })
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
    /// meaningful to the operator. Never a resolution input; the scope is.
    AccountId
}

opaque_string! {
    /// What a credential store fetches an account's credentials by. Chosen
    /// by the resolver, understood by the store, opaque to the worker.
    CredentialRef
}

/// A Számla Agent endpoint URL: an `http` or `https` URI with a host and no
/// userinfo.
///
/// Validated when parsed and when deserialized, so an [`Account`] never
/// carries an endpoint the client cannot post to, or one that would leak:
/// the endpoint is journaled with the account and printed in the start-up
/// log, so a `user:password@` in it would be shown in the Restate UI for the
/// retention period, and it is refused. Plain `http` stays allowed (a
/// local mock or a proxy is a legitimate target, and the type cannot tell a
/// test deployment from production), but the agent key travels in the request
/// body, so `http` to a host other than loopback sends it in cleartext;
/// [`Endpoint::is_cleartext`] reports that case for the start-up log to warn
/// about. The text is kept as written.
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
    /// `http` nor `https`, it has no host, or its authority carries userinfo
    /// (`user:password@host`).
    pub fn parse(value: &str) -> Result<Self, InvalidEndpoint> {
        let uri: Uri = value.parse()?;
        match uri.scheme_str().map(str::to_ascii_lowercase).as_deref() {
            Some("http" | "https") => {}
            _ => return Err(InvalidEndpoint::Scheme),
        }
        if uri.host().is_none_or(str::is_empty) {
            return Err(InvalidEndpoint::Host);
        }
        if uri
            .authority()
            .is_some_and(|authority| authority.as_str().contains('@'))
        {
            return Err(InvalidEndpoint::Userinfo);
        }
        Ok(Self(value.to_owned()))
    }

    /// The URL as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether posting to this endpoint sends the agent key in cleartext:
    /// the scheme is `http` and the host is not loopback (`localhost`,
    /// `127.0.0.0/8`, `::1`). A local mock is not cleartext in any sense that
    /// matters; anything else on `http` is.
    #[must_use]
    pub fn is_cleartext(&self) -> bool {
        // Validated on construction, so this parses.
        let Ok(uri) = self.0.parse::<Uri>() else {
            return false;
        };
        if uri.scheme_str().map(str::to_ascii_lowercase).as_deref() != Some("http") {
            return false;
        }
        let host = uri.host().unwrap_or_default();
        let host = host.trim_start_matches('[').trim_end_matches(']');
        let loopback = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback());
        !loopback
    }

    /// The endpoint as the comparison key of the safety contract's fan-in
    /// rule (unique `(endpoint, credentials)` pairs): two endpoints with
    /// equal keys reach one server, so one agent key under both is one
    /// szamlazz.hu account under two scopes.
    ///
    /// Folds what a URL may spell two ways for one target: the scheme's and
    /// the host's case, the scheme's default port written out (`:443` on
    /// `https`, `:80` on `http`) and the path's trailing slashes, so
    /// `https://www.szamlazz.hu/szamla/` (the default) and
    /// `https://www.szamlazz.hu/szamla` (typed) are one endpoint. The rule
    /// errs toward refusing: a pair refused at load time costs the operator a
    /// configuration fix, a pair admitted costs duplicate documents. Nothing
    /// else is folded (a path's case and a query are kept as written), and
    /// the endpoint itself is untouched: what the client posts to, what is
    /// journaled and what the start-up log prints stay the text as written.
    #[must_use]
    pub fn normalized(&self) -> NormalizedEndpoint {
        // Validated on construction, so this parses; the fallback keeps the
        // comparison textual rather than panicking.
        let Ok(uri) = self.0.parse::<Uri>() else {
            return NormalizedEndpoint(self.0.clone());
        };
        let scheme = uri.scheme_str().unwrap_or_default().to_ascii_lowercase();
        let host = uri.host().unwrap_or_default().to_ascii_lowercase();
        let default_port = if scheme == "https" { 443 } else { 80 };
        let port = uri
            .port_u16()
            .filter(|port| *port != default_port)
            .map(|port| format!(":{port}"))
            .unwrap_or_default();
        let path = uri.path().trim_end_matches('/');
        let query = uri
            .query()
            .map(|query| format!("?{query}"))
            .unwrap_or_default();
        NormalizedEndpoint(format!("{scheme}://{host}{port}{path}{query}"))
    }
}

/// An [`Endpoint`] reduced to what identifies its target, for the fan-in
/// rule's `(endpoint, credentials)` comparison ([`Endpoint::normalized`]).
/// Compared, hashed and ordered; never displayed and never posted to: the
/// folding that makes two spellings equal (the trimmed trailing slash above
/// all) may produce a URL szamlazz.hu does not serve, so the type exposes no
/// text. Not journaled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NormalizedEndpoint(String);

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
/// endpoint URL may carry userinfo, the very thing one variant refuses.
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
    /// The authority carries userinfo (`user:password@host`), which the
    /// journal and the start-up log would show.
    #[error("endpoint must not carry userinfo (user:password@host)")]
    Userinfo,
}

/// The future the two traits return: boxed so that the traits are
/// object-safe and the services can hold them as `dyn`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Maps the Restate scope of a request to the [`Account`] it names.
///
/// The worker calls [`resolve`](Self::resolve) once per invocation, inside a
/// durable step, and journals the result: the scope is the only resolution
/// input, never a header, a body field or the Virtual Object key. Implement
/// it over a database, a configuration service or a table in memory; the
/// static resolver ([`StaticResolver`]) is the configuration-backed one.
///
/// # Safety contract
///
/// The worker cannot check these at runtime; a resolver guarantees them. The
/// static resolver enforces every checkable rule at load time
/// ([`StaticConfigError`] names them); a resolver of your own (a
/// database-backed one above all) guarantees them itself.
///
/// - **One account under exactly one scope, no fan-in.** Unscoped counts as
///   a scope value. Two scopes reaching one szamlazz.hu account would split
///   an order's per-key lock across two Virtual Objects. A database-backed
///   resolver must guarantee it itself: `check_account` only echoes the
///   configuration, and no runtime check can see two scopes at once.
/// - **Append-only mapping.** Moving traffic to another account means a new
///   scope; a scope's account is never changed in place. A running
///   invocation stays on the account it journaled either way.
/// - **Unique `(endpoint, credentials)` pairs.** The same agent key on the
///   same endpoint is one account, whatever its `id`, and the same endpoint
///   is the same server, however spelled: compare on
///   [`Endpoint::normalized`] (scheme and host case, a default port written
///   out, a trailing slash), as the static resolver does, so `…/szamla/`
///   beside `…/szamla` with one key is refused rather than admitted as two.
/// - **The right key under the right scope.** The worker holds no account
///   pin: nothing on a found document is checked against the account (see
///   [`Account`], *No account pin*), so a key that opens another szamlazz.hu
///   account than the scope names, or a test account where a live one is
///   meant (and the reverse), issues there with nothing failing. The
///   resolver guarantees it; the deployment verifies it at go-live and after
///   every rotation by querying a known document under each scope and
///   reading its `<teszt>` and seller block.
/// - **A stable `credential_ref` across rotations.** Rotate the value behind
///   the reference, never the reference: the reference is journaled with the
///   account and an in-flight invocation fetches by it on its next
///   execution.
/// - **Never cache `Unscoped` or `Unknown`.** They are the *answers* the
///   worker journals and reports to the caller as `unknown_account`; a
///   resolver that cached them would turn the appending of a scope into a
///   window of refusals. A resolver may cache resolved accounts internally.
/// - **Answer within seconds.** The worker bounds every `resolve` call at
///   ten seconds and drops the future at the deadline; a slow answer is
///   `unavailable`, retried under the resolve policy, then the terminal
///   fault. A resolver over a pool or a network sets its own, shorter
///   timeouts and answers `Unavailable` itself rather than letting a call
///   hang into the worker's bound.
///
/// Unscoped and unknown are answers, not faults of the resolver: the request
/// names no account, and the worker reports that to the caller as a terminal
/// fault. `Unavailable` is the resolver's own fault, retryable, and journals
/// nothing.
///
/// # Debug
///
/// The trait requires no `Debug`: a resolver often holds a connection pool
/// or a key table, and the crate never formats one: [`Accounts`]' own
/// `Debug` names the trait objects without descending into them.
pub trait AccountResolver: Send + Sync {
    /// The account reachable under `scope`; `None` is the unscoped request.
    fn resolve<'a>(
        &'a self,
        scope: Option<&'a str>,
    ) -> BoxFuture<'a, Result<Account, ResolveError>>;
}

/// Fetches an account's credentials by its [`CredentialRef`].
///
/// The return type is the Számla Agent crate's [`Credentials`] (re-exported
/// at the crate root with [`AgentKey`](szamlazz_agent::AgentKey), so a store
/// needs no direct dependency on that crate): build one with
/// [`Credentials::agent_key`].
///
/// # Safety contract
///
/// - **Fetched on every handler execution, never journaled.** The worker
///   calls [`fetch`](Self::fetch) outside the journal every time a handler
///   executes, including replays, and holds the result only for that
///   execution, so a rotation is picked up on the next execution of every
///   in-flight invocation, and no agent key is written into Restate.
///   [`Credentials`] has no serde implementation: the compiler rejects any
///   attempt to journal it.
/// - **A stable reference across rotations.** The [`CredentialRef`] is the
///   resolver's, journaled with the [`Account`]; a store rotates the value
///   behind it, never the reference, so an in-flight invocation's next
///   execution fetches the new key by the reference it journaled.
/// - A `Gone` reference is an answer (the account's credentials were
///   removed); `Unavailable` is a fault of the store. Neither display text
///   echoes the store's own message.
/// - **Answer within seconds.** The worker bounds every `fetch` call at ten
///   seconds and drops the future at the deadline; a slow answer is
///   `unavailable`, retried in process like a reported `Unavailable`, then
///   the terminal fault. A store over a secrets service or a network sets its
///   own, shorter timeouts and answers `Unavailable` itself rather than
///   letting a call hang into the worker's bound.
///
/// # Debug
///
/// The trait requires no `Debug` (a store over a key map would print the
/// keys), and the crate never formats one: [`Accounts`]' own `Debug` names
/// the trait objects without descending into them. Should you derive `Debug`
/// on a store, hold [`Credentials`] rather than raw strings: its `Debug` is
/// redacted.
pub trait CredentialStore: Send + Sync {
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
/// Trait objects rather than type parameters, so that the services' types
/// (and the SDK-generated clients) do not change with the deployment's
/// choice of resolver. Build it with [`Accounts::new`] over your own
/// resolver and store, or with [`Accounts::from`] a [`StaticResolver`],
/// which is both.
///
/// Its `Debug`, and so that of [`Order`](crate::Order) and
/// [`Agent`](crate::Agent), which derive theirs over it, names the two
/// trait objects and never descends into them: a store that derives `Debug`
/// over a key map cannot print its keys through the services.
#[derive(Clone)]
pub struct Accounts {
    resolver: Arc<dyn AccountResolver>,
    store: Arc<dyn CredentialStore>,
}

impl fmt::Debug for Accounts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Accounts")
            .field("resolver", &format_args!("dyn AccountResolver"))
            .field("store", &format_args!("dyn CredentialStore"))
            .finish()
    }
}

impl Accounts {
    /// Bundles `resolver` and `store`.
    ///
    /// Both are `Arc<dyn …>`, and an `Arc<MyStore>` coerces at the argument
    /// (`Accounts::new(Arc::new(resolver), Arc::new(store))`). When one value
    /// is both, `db.clone()` coerces too, but `Arc::clone(&db)` does not: the
    /// expected type makes it `Arc::<dyn AccountResolver>::clone`, whose
    /// argument no longer matches. Call `.clone()` on the value.
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

    /// Everything the worker may print or journal about an account (the
    /// account itself, the bundle, an opened gateway, the resolver's and the
    /// store's errors) renders without the agent key.
    #[tokio::test]
    async fn renderings_of_account_accounts_gateway_and_errors_carry_no_secret() {
        const KEY: &str = "sentinel-agent-key-4b8c2e";
        let config: StaticConfig = serde_json::from_value(json!({
            "account": {
                "id": "acme",
                "agent_key": KEY,
                "endpoint": "http://127.0.0.1:1/",
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
        let gateway = crate::test_support::open_gateway(account.clone(), credentials);

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
        account.endpoint = Endpoint::parse("http://127.0.0.1:1/").expect("endpoint");
        account.seller.bank_account = Some("11111111-22222222".to_owned());

        let json = serde_json::to_value(&account).expect("serialize");
        assert_eq!(json["id"], "acme");
        assert_eq!(json["credential_ref"], "acme-key");
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
    /// only required fields.
    #[test]
    fn account_is_additive_only_with_the_production_endpoint_by_default() {
        let account: Account =
            serde_json::from_value(json!({ "id": "acme", "credential_ref": "acme" }))
                .expect("deserialize");
        assert_eq!(account, Account::new("acme", "acme"));
        assert_eq!(account.endpoint, Endpoint::production());
        assert_eq!(account.endpoint.as_str(), "https://www.szamlazz.hu/szamla/");
        assert_eq!(account.defaults, Defaults::default());
        assert_eq!(account.seller, SellerConfig::default());
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

    /// An endpoint with userinfo is refused: the endpoint is journaled
    /// with the account and printed in the start-up log, so a password in it
    /// would be shown in the Restate UI for the retention period. The error
    /// never echoes the text.
    #[test]
    fn endpoint_refuses_userinfo() {
        for invalid in [
            "https://u5er:s3cret@www.szamlazz.hu/szamla/",
            "https://u5er@example.com/",
            "http://:s3cret@127.0.0.1:1234/",
        ] {
            let error = Endpoint::parse(invalid).expect_err(invalid);
            assert!(
                matches!(error, InvalidEndpoint::Userinfo),
                "{invalid}: {error:?}"
            );
            assert!(!error.to_string().contains("s3cret"), "{error}");
            assert!(!error.to_string().contains("u5er"), "{error}");
        }
        assert!(
            serde_json::from_value::<Endpoint>(json!("https://u:p@example.com/")).is_err(),
            "deserialization refuses it too"
        );
    }

    /// Plain `http` stays allowed (a local mock or a proxy is a legitimate
    /// target and the type cannot tell a test deployment from production),
    /// but an `http` endpoint on a host other than loopback sends the agent
    /// key in cleartext, which the type reports so the start-up log can warn.
    #[test]
    fn endpoint_reports_cleartext_off_loopback() {
        for cleartext in [
            "http://szamlazz.internal/szamla/",
            "http://10.0.0.7:8080/",
            "http://host.docker.internal:1234/",
            "HTTP://EXAMPLE.COM/",
        ] {
            let endpoint = Endpoint::parse(cleartext).expect(cleartext);
            assert!(endpoint.is_cleartext(), "{cleartext}");
        }
        for safe in [
            "https://www.szamlazz.hu/szamla/",
            "https://szamlazz.internal/",
            "http://127.0.0.1:1234/",
            "http://127.1.2.3/",
            "http://localhost:1234/",
            "http://LOCALHOST/",
            "http://[::1]:1234/",
        ] {
            let endpoint = Endpoint::parse(safe).expect(safe);
            assert!(!endpoint.is_cleartext(), "{safe}");
        }
        assert!(!Endpoint::production().is_cleartext());
    }

    /// The fan-in rule compares endpoints on a normalised form: two spellings
    /// of one server are one endpoint, two servers stay two. The endpoint
    /// itself keeps its text, and the key is not a URL: it has no text to
    /// post to.
    #[test]
    fn endpoint_normalizes_for_comparison_only() {
        let same = [
            ("https://x/", "https://x"),
            ("https://x/szamla/", "https://x/szamla"),
            ("https://x/szamla/", "https://x/szamla//"),
            ("HTTPS://X/szamla/", "https://x/szamla/"),
            ("https://x:443/szamla/", "https://x/szamla/"),
            ("http://x:80/", "http://x/"),
            ("http://[::1]:1234/", "http://[::1]:1234"),
            ("https://x/a?b=1", "https://x/a/?b=1"),
            (Endpoint::PRODUCTION, "https://www.szamlazz.hu/szamla"),
        ];
        for (a, b) in same {
            let (a, b) = (Endpoint::parse(a).expect(a), Endpoint::parse(b).expect(b));
            assert_eq!(
                a.normalized(),
                b.normalized(),
                "{a} and {b} are one endpoint"
            );
            assert_ne!(a, b, "the endpoints themselves keep their spelling");
        }
        let different = [
            ("https://x/a", "https://x/b"),
            ("https://x/Szamla/", "https://x/szamla/"),
            ("http://x/", "https://x/"),
            ("https://x:8443/", "https://x/"),
            ("https://x:80/", "https://x/"),
            ("https://x/a?b=1", "https://x/a?b=2"),
            ("https://x/a?b=1", "https://x/a"),
            ("https://x/", "https://y/"),
        ];
        for (a, b) in different {
            let (a, b) = (Endpoint::parse(a).expect(a), Endpoint::parse(b).expect(b));
            assert_ne!(
                a.normalized(),
                b.normalized(),
                "{a} and {b} are two endpoints"
            );
        }
        let endpoint = Endpoint::parse("HTTPS://X:443/szamla/").expect("endpoint");
        assert_eq!(endpoint.as_str(), "HTTPS://X:443/szamla/");
        assert_eq!(
            serde_json::to_value(&endpoint).expect("json"),
            json!("HTTPS://X:443/szamla/"),
            "normalisation never reaches the journal"
        );
    }

    /// A resolver and a store that a downstream build might plug in: the
    /// traits are object-safe and the bundle routes through them. Neither
    /// trait asks for `Debug`, and the bundle's own `Debug` does not descend
    /// into them.
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

        // The bundle's `Debug` names the trait objects, not the `Table`
        // behind them (the leak property itself is asserted in
        // `service::tests`, with a store that does print its keys).
        let debug = format!("{accounts:?}");
        assert!(debug.contains("dyn AccountResolver"), "{debug}");
        assert!(!debug.contains("Table"), "{debug}");
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

    #[test]
    fn seller_config_projects_to_the_agent_seller_block() {
        let seller: SellerConfig = serde_json::from_value(json!({
            "bank": "Bank",
            "bank_account": "1234",
            "signer_name": "Signer",
            "email": {"reply_to": "r@e.hu", "subject": "S", "body": "B"},
        }))
        .expect("parse");
        let block = seller.to_seller();
        assert_eq!(block.bank.as_deref(), Some("Bank"));
        assert_eq!(block.bank_account.as_deref(), Some("1234"));
        assert_eq!(block.signer_name.as_deref(), Some("Signer"));
        let email = block.email.expect("email block");
        assert_eq!(email.reply_to.as_deref(), Some("r@e.hu"));
        assert_eq!(email.subject.as_deref(), Some("S"));
        assert_eq!(email.body.as_deref(), Some("B"));

        assert_eq!(
            SellerConfig::default().to_seller().email,
            None,
            "no email block unless a field is set"
        );
        assert_eq!(Defaults::default().language, "hu");
        assert_eq!(Defaults::default().currency, "HUF");
        assert_eq!(Defaults::default().exchange_rate_bank, "MNB");
        assert!(!Defaults::default().e_invoice);
    }
}
