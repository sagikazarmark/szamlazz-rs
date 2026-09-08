//! Restate services for issuing and managing szamlazz.hu documents with durable, idempotent
//! execution.
//!
//! The `Order` Virtual Object (keyed by the order number) serialises issuing per key so that a
//! caller can say "issue the invoice for order X" and get exactly one legal document under
//! retries, crashes, concurrent callers and reversals. It keeps **no state**: szamlazz.hu is the
//! source of truth, reached through deterministic external ids (`{namespace}:{order}:{kind}`), so
//! any invocation can find what an earlier one issued. The stateless `Szamlazz.Agent` service
//! exposes by-number operations (query, credit entries, storno of unmanaged documents), the NAV
//! taxpayer lookup by tax number (`query_taxpayer`) and the read-only `check_account` probe over
//! the same gateway. Both are projections of the Számla Agent model: account constants live on the
//! resolved [`Account`], deployment constants in [`WorkerConfig`], line totals are computed, and
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
//! resolver's configuration and bind both to an endpoint:
//!
//! ```no_run
//! # async fn serve(
//! #     accounts: restate_szamlazz::account::StaticConfig,
//! #     worker: restate_szamlazz::WorkerConfig,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! use restate_sdk::prelude::{Endpoint, HttpServer};
//! use restate_szamlazz::account::StaticResolver;
//! use restate_szamlazz::{Accounts, Agent, Order};
//!
//! let worker = worker.validate()?;
//! let accounts = Accounts::from(StaticResolver::try_from(accounts)?);
//! let order = Order::from_parts(accounts.clone(), worker.clone());
//! let agent = Agent::from_parts(accounts, worker);
//! let endpoint = Endpoint::builder().bind(order).bind(agent).build();
//! HttpServer::new(endpoint)
//!     .listen_and_serve("0.0.0.0:9080".parse()?)
//!     .await;
//! # Ok(())
//! # }
//! ```
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
//!     .build();
//! HttpServer::new(endpoint)
//!     .listen_and_serve("0.0.0.0:9080".parse()?)
//!     .await;
//! # Ok(())
//! # }
//! ```
//!
//! Domain outcomes (`issued`, `already_issued`, `reconciled`, `reversed`, `rejected`,
//! `conflict{reason}`) are returned as data with HTTP 200. A `TerminalError` is a fault, whose body
//! is a [`contract::Fault`] with a [`contract::TerminalCode`]. Three of the seven codes mean
//! "outcome unknown: retry with a new `Idempotency-Key`, or read `Szamlazz.Order.get`", never "no
//! document exists": `outcome_unknown`, `unavailable`, `credentials_rejected`. The other four are
//! settled and are not retried as they are: `invalid_input`, `unknown_account` and `not_found` are
//! the caller's request (fix it), `szamlazz_error` is szamlazz.hu's own answer passed through.
//! [`contract::TerminalCode`] says which is which.

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
