//! The Restate services: the `Szamlazz.Order` Virtual Object and the stateless
//! `Szamlazz.Agent` service (design §5–§8).
//!
//! Both are thin adapters: every szamlazz.hu call runs inside `ctx.run` through
//! the [`Steps`] module, every ledger
//! transition is a pure [`Ledger`](crate::ledger::Ledger) method followed by a `ctx.set`, and domain
//! outcomes are returned as data. `TerminalError`s carry a
//! [`TerminalCode`](crate::contract::TerminalCode) and always mean "outcome
//! unknown — call again with the same request id".
//!
//! - [`Order`] — keyed by the order number; owns every document issued for it;
//!   registered as `Szamlazz.Order`.
//! - [`Agent`] — by-number operations (`query`, `set_payments`, `storno`)
//!   registered as `Szamlazz.Agent`.

use std::sync::Arc;

use szamlazz_agent::client::BuildError;

use crate::config::Config;
use crate::steps::Steps;

mod agent;
mod create;
mod handlers;
mod storno;
mod support;

pub use handlers::{AgentClient, AgentIngressClient, OrderClient, OrderIngressClient};

/// The `Order` Virtual Object: one instance per order number, owning every
/// document issued for that order. Registered as `Szamlazz.Order`.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The state is a single [`Ledger`](crate::ledger::Ledger) under the key `"ledger"`.
#[derive(Debug, Clone)]
pub struct Order {
    steps: Arc<Steps>,
    config: Arc<Config>,
}

impl Order {
    /// Builds the object for `config`, constructing the steps.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Arc<Config>) -> Result<Self, BuildError> {
        let steps = Arc::new(Steps::new(Arc::clone(&config))?);
        Ok(Self::from_parts(steps, config))
    }

    /// Builds the object over existing steps.
    #[must_use]
    pub fn from_parts(steps: Arc<Steps>, config: Arc<Config>) -> Self {
        Self { steps, config }
    }

    /// The durable step bodies.
    #[must_use]
    pub fn steps(&self) -> &Arc<Steps> {
        &self.steps
    }

    /// The deployment configuration.
    #[must_use]
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }
}

/// The stateless `Szamlazz.Agent` service: by-number operations over the
/// same steps as [`Order`].
#[derive(Debug, Clone)]
pub struct Agent {
    steps: Arc<Steps>,
    config: Arc<Config>,
}

impl Agent {
    /// Builds the service for `config`, constructing the steps.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Arc<Config>) -> Result<Self, BuildError> {
        let steps = Arc::new(Steps::new(Arc::clone(&config))?);
        Ok(Self::from_parts(steps, config))
    }

    /// Builds the service over existing steps.
    #[must_use]
    pub fn from_parts(steps: Arc<Steps>, config: Arc<Config>) -> Self {
        Self { steps, config }
    }

    /// The durable step bodies.
    #[must_use]
    pub fn steps(&self) -> &Arc<Steps> {
        &self.steps
    }
}

#[cfg(test)]
mod tests;
