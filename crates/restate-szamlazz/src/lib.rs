//! Restate services for issuing and managing szamlazz.hu documents with durable, idempotent
//! execution.
//!
//! The `Order` Virtual Object (keyed by the order number) serialises issuing per key so that a
//! caller can say "issue the invoice for order X". A durable unresolved-write marker and
//! one-use execution-local send permission protect retries and later mutations. szamlazz.hu is the
//! source of truth, reached through deterministic external ids (`{namespace}:{order}:{kind}`), so
//! any invocation can find what an earlier one issued. The stateless `Szamlazz.Agent` service
//! exposes by-number operations (query, credit entries, storno of unmanaged documents), the NAV
//! taxpayer lookup by tax number (`query_taxpayer`) and the read-only `check_account` probe over
//! the same gateway. Both are projections of the Számla Agent model: account constants live on the
//! resolved [`Account`], deployment constants in [`WorkerConfig`], line totals are computed or validated, and
//! domain outcomes are returned as data.
//!
//! - [`contract`]: the request/response types.
//! - [`config`]: the deployment-level configuration (the namespace; the issue, read and resolve
//!   policies).
//! - [`account`]: the account model, the account resolver and credential store traits, and
//!   the static resolver over deployment configuration.
//! - [`identity`]: order keys and external ids.
//! - [`gateway`]: the module that speaks to szamlazz.hu for one account, outcome as data.
//! - [`service`]: the Restate adapters.
//!
//! ## Features
//!
//! - `schemars`: `JsonSchema` derives on every [`contract`] type, so the Restate discovery
//!   manifest and the `OpenAPI` export carry typed request and response schemas. It enables
//!   `restate-sdk/schemars` too, and Cargo unifies features per build: with it on, every
//!   `Json<T>` handler on the same endpoint (your own included) needs `T: JsonSchema`.
//!   Faults are outside the success-output discovery schema; generated ingress
//!   clients use [`service::decode_fault`] in addition to their success types.
//!
//! ## Compatibility
//!
//! The crate re-exports the two crates it is built on, [`restate_sdk`] and [`szamlazz_agent`],
//! and the two Számla Agent types the [`CredentialStore`] trait is written in, [`Credentials`]
//! and [`AgentKey`], so an embedder pins one version of each. The coupling is
//! restate-szamlazz 0.x ⇔ szamlazz-agent 0.x (same minor) ⇔ restate-sdk 0.12 ⇔ Restate server
//! 1.7.8 with protocol v7. The SDK's `#[restate_sdk::service]` macro expands to `::restate_sdk`
//! paths, so a crate that defines services of its own also names `restate-sdk` as a direct
//! dependency, at the same minor; binding only [`Order`] and [`Agent`] needs the re-export alone.
//!
//! # Services
//!
//! [`Order`] is a Restate Virtual Object registered as `Szamlazz.Order` and [`Agent`] a
//! stateless service registered as `Szamlazz.Agent`. Both hold the [`Accounts`] bundle (the
//! account resolver and the credential store) and a [`WorkerConfig`]; every handler resolves
//! its account and opens a [`Gateway`] for its own execution. Build the bundle from the static
//! resolver's configuration and bind both to an endpoint, with the two things Restate's own
//! guidance asks of an endpoint: the SDK's replay-aware log filter, and the request identity key:
//!
//! ```no_run
//! # async fn serve(
//! #     accounts: restate_szamlazz::account::StaticConfig,
//! #     worker: restate_szamlazz::WorkerConfig,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! use restate_sdk::filter::ReplayAwareFilter;
//! use restate_sdk::prelude::{Endpoint, HttpServer};
//! use restate_szamlazz::account::StaticResolver;
//! use restate_szamlazz::{Accounts, Agent, Order};
//! use tracing_subscriber::layer::SubscriberExt as _;
//! use tracing_subscriber::util::SubscriberInitExt as _;
//! use tracing_subscriber::{EnvFilter, Layer as _};
//!
//! // Logs: `RUST_LOG` selects, and the SDK's `ReplayAwareFilter` drops what a
//! // replayed handler emits again. Every handler execution of this crate runs
//! // in an `execution{scope, order, restate.invocation.id, account.id}` span
//! // so events outside completed durable steps would otherwise repeat.
//! // Keep the SDK and execution INFO spans enabled, e.g.
//! // RUST_LOG=info,restate_szamlazz=debug.
//! tracing_subscriber::registry()
//!     .with(
//!         tracing_subscriber::fmt::layer()
//!             .with_filter(EnvFilter::from_default_env())
//!             .with_filter(ReplayAwareFilter),
//!     )
//!     .init();
//!
//! let worker = worker.validate()?;
//! let accounts = Accounts::from(StaticResolver::try_from(accounts)?);
//! let order = Order::from_parts(accounts.clone(), worker.clone());
//! let agent = Agent::from_parts(accounts, worker);
//! let endpoint = Endpoint::builder()
//!     .bind(order)
//!     .bind(agent)
//!     // Restate's request identity: with a key registered, the endpoint
//!     // refuses every request the runtime did not sign. Required wherever
//!     // anything but the runtime can reach this port, and always in the
//!     // multi-account shape: the scope that selects the account is protocol
//!     // data inside the request, so an unsigned request could invoke either
//!     // service under any scope. The value is the runtime's public key, which
//!     // the server logs at start-up when it holds the private half
//!     // (`RESTATE_REQUEST_IDENTITY_PRIVATE_KEY_PEM_FILE`); it is not a secret.
//!     .identity_key("publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f")?
//!     .build();
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:9080").await?;
//! #[cfg(unix)]
//! let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
//! let shutdown = async move {
//!     #[cfg(unix)]
//!     tokio::select! {
//!         _ = tokio::signal::ctrl_c() => {},
//!         _ = terminate.recv() => {},
//!     }
//!     #[cfg(not(unix))]
//!     let _ = tokio::signal::ctrl_c().await;
//! };
//! HttpServer::new(endpoint).serve_with_cancel(listener, shutdown).await;
//! # Ok(())
//! # }
//! ```
//!
//! These serving examples assume process/runtime termination after serving returns.
//! SDK 0.12 waits up to ten seconds for connection drain but does not abort and join
//! remaining connection/handler tasks. In an embedded host that keeps its runtime
//! alive, return is not a task-completion barrier. Component-level shutdown requires
//! owning, draining, cancelling and joining serving tasks before dependency teardown;
//! external effects still require reconciliation.
//!
//! SDK 0.12.0 can suppress fresh events when replay finishes under a child span.
//! These services retain the SDK endpoint span and clear its replay flag only
//! when a run closure actually executes; the execution span still supplies scope,
//! account and invocation correlation. The usual subscriber above is sufficient.
//! This mitigation is local to these services. Remove it after the minimum SDK
//! version retains its own span and updates it before executing fresh closures;
//! see the repository's `docs/research/2026-09-10-replay-logging.md` (#202).
//!
//! Register the endpoint with the server, then call a handler through the ingress; the
//! ingress URL grammar is Restate's, `/{service}/{key}/{handler}` for a Virtual Object,
//! `/{service}/{handler}` for a service, and `/restate/scope/{scope}/call/…` under a scope
//! (the multi-account shape; see [`account`]):
//!
//! ```text
//! restate deployments register http://worker:9080
//!
//! # Single-account shape: unscoped. The key is the order number, already trimmed.
//! curl -X POST http://localhost:8080/Szamlazz.Order/ORD-1/create_invoice \
//!   -H 'Idempotency-Key: 4c8f5a1e-…' -H 'content-type: application/json' \
//!   -d '{"document": {"buyer": {"name": "Kovács Bt.", "zip": "2030", "city": "Érd",
//!                               "address": "Tárnoki út 23."},
//!                     "items": [{"name": "Jegy", "quantity": "1", "unit": "db",
//!                                "unit_price": "1000", "vat_rate": "27"}],
//!                     "fulfillment_date": "2026-09-03", "due_date": "2026-09-11",
//!                     "payment_method": "transfer"}}'
//!
//! # Multi-account shape: the account's scope on every call.
//! curl -X POST http://localhost:8080/restate/scope/acme/call/Szamlazz.Order/ORD-1/create_invoice …
//! curl -X POST http://localhost:8080/restate/scope/acme/call/Szamlazz.Agent/check_account
//! ```
//!
//! ## Restate's words and this crate's
//!
//! The crate's documentation uses a few words of its own beside Restate's; where they meet:
//!
//! - **Fault**: a `TerminalError` one of the services raises, whose message is a
//!   [`contract::Fault`] JSON body with a [`contract::TerminalCode`]; a domain result that is
//!   not a fault (`rejected`, `conflict{…}`) is a 200 with an *outcome*, never an error.
//! - **Execution**: one run of a handler on one deployment, the unit a run retry re-executes
//!   with replay; Restate's *attempt* counts the server's re-dispatches of the invocation, which
//!   the handlers' `invocation_retry_policy` bounds. A *step* is one named `ctx.run`.
//! - **Prologue**: the first lines of every handler (pin the namespace, resolve the account,
//!   with credential fetch and gateway open deferred to an executing operation), not an interception layer; nothing in the SDK
//!   corresponds to it.
//! - **Deployment**: Restate's word, a registered endpoint revision (ADR 0009); the crate never
//!   uses it for anything else. What both services hold in common (the accounts and the
//!   validated configuration) is their *parts* (`from_parts`).
//! - **Account**, **scope**: an account is one szamlazz.hu account; the Restate *scope* of a
//!   request is the caller's identifier for it, resolved by the [`AccountResolver`]. Neither is
//!   a Restate *service* or *key*.
//!
//! ## Your own resolver and store
//!
//! A deployment that keeps its accounts in a database and its agent keys in a credential store
//! of its own implements [`AccountResolver`] and [`CredentialStore`] itself (both are
//! object-safe traits returning a [`BoxFuture`](account::BoxFuture)) and bundles them with
//! [`Accounts::new`]. Their safety contracts are on the traits. Two things the compiler will
//! otherwise tell you about: `account::Endpoint` (the Számla Agent URL an [`Account`] carries)
//! and `restate_sdk::prelude::Endpoint` (the Restate endpoint) share a name, so alias one; and
//! when one value is both resolver and store, `db.clone()` coerces to `Arc<dyn AccountResolver>`
//! at the argument but `Arc::clone(&db)` does not (the expected type makes it
//! `Arc::<dyn AccountResolver>::clone`, whose argument no longer matches); call `.clone()` on
//! the value.
//!
//! ```no_run
//! use std::sync::Arc;
//!
//! use restate_sdk::prelude::*;
//! use restate_szamlazz::account::{
//!     Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore,
//!     Endpoint as AgentEndpoint, FetchError, ResolveError,
//! };
//! use restate_szamlazz::{Agent, Credentials, Order, WorkerConfig};
//!
//! /// Your database handle; `account_row` is its query.
//! struct Db {
//!     // pool: sqlx::PgPool, …
//! }
//! # struct Row { id: String, credential_ref: String }
//! # impl Db {
//! #     async fn account_row(&self, _scope: &str) -> Result<Option<Row>, std::io::Error> {
//! #         unimplemented!()
//! #     }
//! # }
//!
//! impl AccountResolver for Db {
//!     fn resolve<'a>(
//!         &'a self,
//!         scope: Option<&'a str>,
//!     ) -> BoxFuture<'a, Result<Account, ResolveError>> {
//!         Box::pin(async move {
//!             let scope = scope.ok_or(ResolveError::Unscoped)?;
//!             // SELECT id, credential_ref FROM accounts WHERE scope = $1
//!             let row = self.account_row(scope).await.map_err(ResolveError::unavailable)?;
//!             let Some(row) = row else {
//!                 return Err(ResolveError::Unknown { scope: scope.to_owned() });
//!             };
//!             let mut account = Account::new(row.id, row.credential_ref);
//!             account.endpoint = AgentEndpoint::production();
//!             Ok(account)
//!         })
//!     }
//! }
//!
//! /// Your credential store's client; `agent_key` is its read.
//! struct Keys {
//!     // client: …
//! }
//! # impl Keys {
//! #     async fn agent_key(&self, _credential_ref: &str) -> Result<Option<String>, std::io::Error> {
//! #         unimplemented!()
//! #     }
//! # }
//!
//! impl CredentialStore for Keys {
//!     fn fetch<'a>(
//!         &'a self,
//!         credential_ref: &'a CredentialRef,
//!     ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
//!         Box::pin(async move {
//!             let key = self.agent_key(credential_ref.as_str()).await.map_err(FetchError::unavailable)?;
//!             key.map(Credentials::agent_key).ok_or_else(|| FetchError::Gone {
//!                 credential_ref: credential_ref.clone(),
//!             })
//!         })
//!     }
//! }
//!
//! /// A service of your own on the same endpoint.
//! struct Backoffice;
//!
//! #[restate_sdk::service]
//! impl Backoffice {
//!     #[handler]
//!     async fn ping(&self, _ctx: Context<'_>) -> HandlerResult<String> {
//!         Ok("pong".to_owned())
//!     }
//! }
//!
//! # async fn serve(db: Db, keys: Keys, worker: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
//! let worker = worker.validate()?;
//! let accounts = Accounts::new(Arc::new(db), Arc::new(keys));
//! let endpoint = Endpoint::builder()
//!     .bind(Order::from_parts(accounts.clone(), worker.clone()))
//!     .bind(Agent::from_parts(accounts, worker))
//!     .bind(Backoffice)
//!     // A resolver of your own is the multi-account shape; the identity key
//!     // is what keeps the scope trustworthy (see the quick start).
//!     .identity_key("publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f")?
//!     .build();
//! let listener = tokio::net::TcpListener::bind("0.0.0.0:9080").await?;
//! #[cfg(unix)]
//! let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
//! let shutdown = async move {
//!     #[cfg(unix)]
//!     tokio::select! {
//!         _ = tokio::signal::ctrl_c() => {},
//!         _ = terminate.recv() => {},
//!     }
//!     #[cfg(not(unix))]
//!     let _ = tokio::signal::ctrl_c().await;
//! };
//! HttpServer::new(endpoint).serve_with_cancel(listener, shutdown).await;
//! # Ok(())
//! # }
//! ```
//!
//! Domain outcomes (`issued`, `already_issued`, `reconciled`, `reversed`, `rejected`,
//! `conflict{reason}`) are returned as data with HTTP 200. A `TerminalError` is a fault, whose body
//! is a [`contract::Fault`] with a [`contract::TerminalCode`]. Three of the eight codes mean
//! "outcome unknown: reconcile before deliberately renewing", never "no document exists":
//! `outcome_unknown`, `unavailable`, `credentials_rejected`. Empty queries, elapsed time and kill do not
//! prove an earlier send cannot still land. Keep the original key while unfinished; use a new key only
//! after uncertainty is settled and renewal is intended. The other four are
//! settled and are not retried as they are: `invalid_input`, `unknown_account` and `not_found` are
//! the caller's request (fix it), `szamlazz_error` is szamlazz.hu's own answer passed through.
//! Intentional read cancellation is `cancelled` (409); a cancelled write keeps
//! `outcome_unknown` with `cause: cancelled`. Check [`contract::Fault::is_cancelled`]
//! before considering retry: reconcile a cancelled write before deliberately
//! renewing it. [`contract::TerminalCode`] says which is which.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod account;
pub mod config;
pub mod contract;
pub mod gateway;
pub mod identity;
pub mod service;
#[cfg(test)]
pub(crate) mod test_support;

pub use restate_sdk;
pub use szamlazz_agent;
pub use szamlazz_agent::{AgentKey, Credentials};

pub use account::{Account, AccountResolver, Accounts, CredentialStore};
pub use config::{ValidatedWorkerConfig, WorkerConfig};
pub use contract::{CorrectionId, CreateRequest, CreateResponse, DocumentKind, InvoiceNumber};
pub use gateway::Gateway;
pub use identity::{ExternalId, OrderKey};
pub use service::{Agent, AgentClient, Order, OrderClient};
