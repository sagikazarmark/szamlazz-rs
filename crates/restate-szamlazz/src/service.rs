//! The Restate services: the `Szamlazz.Order` Virtual Object and the stateless
//! `Szamlazz.Agent` service (design §4–§7).
//!
//! Both are thin adapters: every szamlazz.hu call runs inside `ctx.run` through
//! the [`Gateway`](crate::gateway::Gateway) and domain outcomes are returned as
//! data. Neither keeps state — szamlazz.hu is the source of truth, reached
//! through the order's deterministic external ids. `TerminalError`s carry a
//! [`TerminalCode`](crate::contract::TerminalCode) and always mean "outcome
//! unknown — retry with a new `Idempotency-Key`".
//!
//! Each service holds exactly two things: the [`Accounts`] bundle — the
//! account resolver and the credential store — and a [`WorkerConfig`] with the
//! deployment-level settings (the namespace of the external ids, the issue
//! and resolve policies). Every handler runs the same prologue after parsing
//! its key: **pin** the namespace in a pure durable step, **resolve** the
//! request's scope to its account in a durable step named `account` under the
//! resolve policy, **fetch** the account's credentials outside the journal on
//! every execution, **open** the gateway for this execution over a fresh
//! client. The handler body then runs on that execution (`prologue::Execution`);
//! nothing of it — gateway, client, credentials — outlives the execution.
//!
//! - [`Order`] — keyed by the order number; its per-key lock serialises
//!   issuing per order; registered as `Szamlazz.Order`.
//! - [`Agent`] — by-number operations (`query`, `set_payments`, `storno`) and
//!   the read-only `check_account` probe, registered as `Szamlazz.Agent`.

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{Context, ObjectContext, SharedObjectContext};

use crate::account::Accounts;
use crate::config::WorkerConfig;

mod agent;
mod create;
mod handlers;
mod prologue;
mod storno;
mod support;

pub use handlers::{AgentClient, AgentIngressClient, OrderClient, OrderIngressClient};

use prologue::Execution;

/// The `Order` Virtual Object: one instance per order number. Registered as
/// `Szamlazz.Order`.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The object holds no state.
#[derive(Debug, Clone)]
pub struct Order {
    accounts: Accounts,
    config: WorkerConfig,
}

impl Order {
    /// Builds the object over the account resolver and credential store in
    /// `accounts` and the deployment-level `config`.
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: WorkerConfig) -> Self {
        Self { accounts, config }
    }

    /// The account resolver and credential store.
    #[must_use]
    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// The prologue of an exclusive handler: pin → resolve → fetch → open.
    async fn prologue(&self, ctx: &ObjectContext<'_>) -> Result<Execution, HandlerError> {
        support::object::prologue(ctx, &self.accounts, &self.config).await
    }

    /// The prologue of a shared handler (`get`).
    async fn prologue_shared(
        &self,
        ctx: &SharedObjectContext<'_>,
    ) -> Result<Execution, HandlerError> {
        support::shared::prologue(ctx, &self.accounts, &self.config).await
    }
}

/// The stateless `Szamlazz.Agent` service: by-number operations over the
/// same accounts as [`Order`]. `query` and `storno` check the document they
/// find against the resolved account (`account_mismatch` on a mismatch);
/// `set_payments` finds none and is exempt.
#[derive(Debug, Clone)]
pub struct Agent {
    accounts: Accounts,
    config: WorkerConfig,
}

impl Agent {
    /// Builds the service over the account resolver and credential store in
    /// `accounts` and the deployment-level `config`.
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: WorkerConfig) -> Self {
        Self { accounts, config }
    }

    /// The account resolver and credential store.
    #[must_use]
    pub fn accounts(&self) -> &Accounts {
        &self.accounts
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &WorkerConfig {
        &self.config
    }

    /// The prologue of every handler: pin → resolve → fetch → open.
    async fn prologue(&self, ctx: &Context<'_>) -> Result<Execution, HandlerError> {
        support::service::prologue(ctx, &self.accounts, &self.config).await
    }
}

#[cfg(test)]
mod tests;
