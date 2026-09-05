//! The Restate services: the `Szamlazz.Order` Virtual Object and the stateless
//! `Szamlazz.Agent` service (design §4–§7).
//!
//! Both are thin adapters: every szamlazz.hu call runs inside `ctx.run` through
//! the [`Gateway`] and domain outcomes are returned as data. Neither keeps
//! state — szamlazz.hu is the source of truth, reached through the order's
//! deterministic external ids. `TerminalError`s carry a
//! [`TerminalCode`](crate::contract::TerminalCode) and always mean "outcome
//! unknown — retry with a new `Idempotency-Key`".
//!
//! Each service holds exactly two things: the gateway, through which it reads
//! everything about the account ([`Gateway::account`]), and a
//! [`WorkerConfig`] with the deployment-level settings — the namespace of the
//! external ids and the issue policy — that are not account-shaped.
//!
//! - [`Order`] — keyed by the order number; its per-key lock serialises
//!   issuing per order; registered as `Szamlazz.Order`.
//! - [`Agent`] — by-number operations (`query`, `set_payments`, `storno`)
//!   registered as `Szamlazz.Agent`.

use std::sync::Arc;

use crate::config::{Config, WorkerConfig};
use crate::gateway::{Gateway, OpenError};

mod agent;
mod create;
mod handlers;
mod storno;
mod support;

pub use handlers::{AgentClient, AgentIngressClient, OrderClient, OrderIngressClient};

/// The `Order` Virtual Object: one instance per order number. Registered as
/// `Szamlazz.Order`.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The object holds no state.
#[derive(Debug, Clone)]
pub struct Order {
    gateway: Arc<Gateway>,
    config: WorkerConfig,
}

impl Order {
    /// Builds the object for `config`, opening the gateway.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint is not an http(s) URL or the HTTP
    /// client cannot be constructed.
    pub fn new(config: &Config) -> Result<Self, OpenError> {
        let gateway = Arc::new(Gateway::new(config)?);
        Ok(Self::from_parts(gateway, WorkerConfig::from(config)))
    }

    /// Builds the object over an existing gateway.
    #[must_use]
    pub fn from_parts(gateway: Arc<Gateway>, config: WorkerConfig) -> Self {
        Self { gateway, config }
    }

    /// The gateway to szamlazz.hu.
    #[must_use]
    pub fn gateway(&self) -> &Arc<Gateway> {
        &self.gateway
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }
}

/// The stateless `Szamlazz.Agent` service: by-number operations over the
/// same gateway as [`Order`].
#[derive(Debug, Clone)]
pub struct Agent {
    gateway: Arc<Gateway>,
    config: WorkerConfig,
}

impl Agent {
    /// Builds the service for `config`, opening the gateway.
    ///
    /// # Errors
    ///
    /// Returns an error when the endpoint is not an http(s) URL or the HTTP
    /// client cannot be constructed.
    pub fn new(config: &Config) -> Result<Self, OpenError> {
        let gateway = Arc::new(Gateway::new(config)?);
        Ok(Self::from_parts(gateway, WorkerConfig::from(config)))
    }

    /// Builds the service over an existing gateway.
    #[must_use]
    pub fn from_parts(gateway: Arc<Gateway>, config: WorkerConfig) -> Self {
        Self { gateway, config }
    }

    /// The gateway to szamlazz.hu.
    #[must_use]
    pub fn gateway(&self) -> &Arc<Gateway> {
        &self.gateway
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests;
