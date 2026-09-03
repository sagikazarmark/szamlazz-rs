//! Restate services for issuing and managing szamlazz.hu documents with durable, idempotent
//! execution.
//!
//! The `Order` Virtual Object — keyed by the order number — owns every document issued for one
//! order and serialises issuing per key so that a caller can say "issue the invoice for order X"
//! and get exactly one legal document under retries, crashes, concurrent callers and reversals;
//! the stateless `SzamlaAgent` service exposes by-number operations (query, credit entries,
//! storno of unmanaged documents) over the same low-level layer. Both are projections of the
//! Számla Agent model: deployment constants live in [`Config`], line totals are computed, and
//! domain outcomes are returned as data.
//!
//! - [`contract`] — the request/response types.
//! - [`config`] — the deployment configuration.
//! - [`identity`] — order keys, external ids and payload fingerprints.
//! - [`ledger`] — the `Order` state and its pure transitions.
//! - [`szamla_agent`] — the low-level layer over the Számla Agent client, outcome as data.
//! - [`service`] — the Restate adapters.
//!
//! ## Features
//!
//! - `schemars` — `JsonSchema` derives on every [`contract`] type, so the Restate discovery
//!   manifest and the `OpenAPI` export carry typed request and response schemas.
//!
//! # Services
//!
//! [`Order`] is a Restate Virtual Object registered as `Order` and [`SzamlaAgentService`] a
//! stateless service registered as `SzamlaAgent`. Bind both to an endpoint:
//!
//! ```no_run
//! # async fn serve(config: std::sync::Arc<restate_szamlazz::Config>) -> Result<(), Box<dyn std::error::Error>> {
//! use restate_sdk::prelude::{Endpoint, HttpServer};
//! use restate_szamlazz::{Order, SzamlaAgentService};
//!
//! let order = Order::new(std::sync::Arc::clone(&config))?;
//! let agent = SzamlaAgentService::from_parts(std::sync::Arc::clone(order.agent()), config);
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
//! [`contract::TerminalCode`] and always means "outcome unknown — call again with the same
//! `request_id`, or read `Order.get`", never "no document exists".
//!
//! See `docs/design/restate-szamlazz.md` in the repository for the design.

pub mod config;
pub mod contract;
pub mod identity;
pub mod ledger;
pub mod service;
pub mod szamla_agent;

pub use config::Config;
pub use contract::{CreateRequest, CreateResponse, DocumentKind, RequestId};
pub use identity::{ExternalId, OrderKey};
pub use ledger::Ledger;
pub use service::{Order, OrderClient, SzamlaAgentService, SzamlaAgentServiceClient};
pub use szamla_agent::SzamlaAgent;
