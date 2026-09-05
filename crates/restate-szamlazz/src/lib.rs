//! Restate services for issuing and managing szamlazz.hu documents with durable, idempotent
//! execution.
//!
//! The `Order` Virtual Object — keyed by the order number — serialises issuing per key so that a
//! caller can say "issue the invoice for order X" and get exactly one legal document under
//! retries, crashes, concurrent callers and reversals. It keeps **no state**: szamlazz.hu is the
//! source of truth, reached through deterministic external ids (`{namespace}:{order}:{kind}`), so
//! any invocation can find what an earlier one issued. The stateless `Szamlazz.Agent` service
//! exposes by-number operations (query, credit entries, storno of unmanaged documents) and the
//! read-only `check_account` probe over the
//! same gateway. Both are projections of the Számla Agent model: account constants live on the
//! resolved [`Account`], deployment constants in [`WorkerConfig`], line totals are computed, and
//! domain outcomes are returned as data.
//!
//! - [`contract`] — the request/response types.
//! - [`config`] — the deployment-level configuration (namespace, issue and resolve policies).
//! - [`account`] — the account model, the account resolver and credential store traits, and
//!   the static resolver over deployment configuration.
//! - [`identity`] — order keys and external ids.
//! - [`gateway`] — the module that speaks to szamlazz.hu for one account, outcome as data.
//! - [`service`] — the Restate adapters.
//!
//! ## Features
//!
//! - `schemars` — `JsonSchema` derives on every [`contract`] type, so the Restate discovery
//!   manifest and the `OpenAPI` export carry typed request and response schemas.
//!
//! # Services
//!
//! [`Order`] is a Restate Virtual Object registered as `Szamlazz.Order` and [`Agent`] a
//! stateless service registered as `Szamlazz.Agent`. Both hold the [`Accounts`] bundle (the
//! account resolver and the credential store) and a [`WorkerConfig`]; every handler resolves
//! its account and opens a [`Gateway`] for its own execution. Build the bundle from the static
//! resolver's configuration (or your own resolver and store) and bind both to an endpoint:
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
//! worker.validate()?;
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
//! Domain outcomes (`issued`, `already_issued`, `reconciled`, `reversed`, `rejected`,
//! `conflict{reason}`) are returned as data with HTTP 200. A `TerminalError` carries a
//! [`contract::TerminalCode`] and always means "outcome unknown — retry with a new
//! `Idempotency-Key`, or read `Szamlazz.Order.get`", never "no document exists".
//!
//! See `docs/design/restate-szamlazz.md` in the repository for the design.

pub mod account;
pub mod config;
pub mod contract;
pub mod gateway;
pub mod identity;
pub mod service;

pub use account::{Account, AccountResolver, Accounts, CredentialStore};
pub use config::WorkerConfig;
pub use contract::{CorrectionId, CreateRequest, CreateResponse, DocumentKind};
pub use gateway::Gateway;
pub use identity::{ExternalId, OrderKey};
pub use service::{Agent, AgentClient, Order, OrderClient};
