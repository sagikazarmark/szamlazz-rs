//! The Restate services: the `Order` Virtual Object and the stateless
//! `SzamlaAgent` service (design §5–§8).
//!
//! Both are thin adapters: every szamlazz.hu call runs inside `ctx.run` through
//! the [`SzamlaAgent`] module, every ledger
//! transition is a pure [`Ledger`](crate::ledger::Ledger) method followed by a `ctx.set`, and domain
//! outcomes are returned as data. `TerminalError`s carry a
//! [`TerminalCode`](crate::contract::TerminalCode) and always mean "outcome
//! unknown — call again with the same request id".
//!
//! - [`Order`] — keyed by the order number; owns every document issued for it.
//! - [`SzamlaAgentService`] — by-number operations (`query`, `set_payments`,
//!   `storno`) registered as `SzamlaAgent`.

use std::sync::Arc;

use szamlazz_agent::client::BuildError;

use crate::config::Config;
use crate::szamla_agent::SzamlaAgent;

mod agent;
mod create;
mod handlers;
mod storno;
mod support;

pub use handlers::{
    OrderClient, OrderIngressClient, SzamlaAgentServiceClient, SzamlaAgentServiceIngressClient,
};

/// The `Order` Virtual Object: one instance per order number, owning every
/// document issued for that order.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The state is a single [`Ledger`](crate::ledger::Ledger) under the key `"ledger"`.
#[derive(Debug, Clone)]
pub struct Order {
    agent: Arc<SzamlaAgent>,
    config: Arc<Config>,
}

impl Order {
    /// Builds the object for `config`, constructing the low-level layer.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Arc<Config>) -> Result<Self, BuildError> {
        let agent = Arc::new(SzamlaAgent::new(Arc::clone(&config))?);
        Ok(Self::from_parts(agent, config))
    }

    /// Builds the object over an existing low-level layer.
    #[must_use]
    pub fn from_parts(agent: Arc<SzamlaAgent>, config: Arc<Config>) -> Self {
        Self { agent, config }
    }

    /// The low-level layer.
    #[must_use]
    pub fn agent(&self) -> &Arc<SzamlaAgent> {
        &self.agent
    }

    /// The deployment configuration.
    #[must_use]
    pub fn config(&self) -> &Arc<Config> {
        &self.config
    }
}

/// The stateless `SzamlaAgent` service: by-number operations over the same
/// low-level layer as [`Order`].
///
/// The Rust type is named `SzamlaAgentService` to leave the module type
/// [`SzamlaAgent`] its name; the Restate
/// service is registered as `SzamlaAgent`.
#[derive(Debug, Clone)]
pub struct SzamlaAgentService {
    agent: Arc<SzamlaAgent>,
    config: Arc<Config>,
}

impl SzamlaAgentService {
    /// Builds the service for `config`, constructing the low-level layer.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP client cannot be constructed.
    pub fn new(config: Arc<Config>) -> Result<Self, BuildError> {
        let agent = Arc::new(SzamlaAgent::new(Arc::clone(&config))?);
        Ok(Self::from_parts(agent, config))
    }

    /// Builds the service over an existing low-level layer.
    #[must_use]
    pub fn from_parts(agent: Arc<SzamlaAgent>, config: Arc<Config>) -> Self {
        Self { agent, config }
    }

    /// The low-level layer.
    #[must_use]
    pub fn agent(&self) -> &Arc<SzamlaAgent> {
        &self.agent
    }
}

#[cfg(test)]
mod tests;
