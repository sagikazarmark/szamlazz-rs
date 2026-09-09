//! The Restate services: the `Szamlazz.Order` Virtual Object and the stateless
//! `Szamlazz.Agent` service.
//!
//! Both are thin adapters: every szamlazz.hu call runs inside `ctx.run` through
//! the [`Gateway`](crate::gateway::Gateway) and domain outcomes are returned as
//! data. Neither keeps state: szamlazz.hu is the source of truth, reached
//! through the order's deterministic external ids. `TerminalError`s carry a
//! [`TerminalCode`](crate::contract::TerminalCode): three of the codes mean
//! "outcome unknown: retry with a new `Idempotency-Key`, or read `get`"
//! (`outcome_unknown`, `unavailable`, `credentials_rejected`), the rest are
//! settled: the same request never succeeds, or szamlazz.hu's own answer is
//! passed through. On the wire a fault is the JSON string inside Restate's
//! ingress envelope (`{"code": <HTTP status>, "message": "<fault JSON>",
//! "source": "invocation"}`): the fault → `TerminalError` conversion in
//! `support` hands the SDK the status and the fault JSON as the message, and
//! the ingress wraps them in its envelope.
//!
//! Each service holds exactly two things: the [`Accounts`] bundle (the
//! account resolver and the credential store), and a [`ValidatedWorkerConfig`]
//! with the deployment-level settings (the namespace of the external ids; the
//! issue, read and resolve policies), validated because a service cannot be
//! built over an issue policy below its floor. Every handler runs the same prologue after parsing
//! its key: **pin** the namespace in a pure durable step, **resolve** the
//! request's scope to its account in a durable step named `account` under the
//! resolve policy, **fetch** the account's credentials outside the journal on
//! every execution, **open** the gateway for this execution over a fresh
//! client. The handler body then runs on that execution (`prologue::Execution`);
//! nothing of it (gateway, client, credentials) outlives the execution.
//!
//! - [`Order`]: keyed by the order number; its per-key lock serialises
//!   issuing per order; registered as `Szamlazz.Order`.
//! - [`Agent`]: by-number operations (`query`, `set_payments`, `storno`), the
//!   NAV taxpayer lookup (`query_taxpayer`) and the read-only `check_account`
//!   probe, registered as `Szamlazz.Agent`.

use std::future::Future;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{Context, ObjectContext, SharedObjectContext};

use crate::account::Accounts;
use crate::config::ValidatedWorkerConfig;

mod agent;
mod body;
mod create;
mod handlers;
#[cfg(test)]
mod journal;
mod prologue;
mod storno;
mod support;

pub use body::Body;
pub use handlers::{AgentClient, AgentIngressClient, OrderClient, OrderIngressClient};

use prologue::Execution;

/// What both services hold: the accounts bundle and the validated
/// deployment-level settings. One struct, since the two services are built
/// from the same parts and differ only in their handlers; what every
/// handler's execution starts from.
#[derive(Debug, Clone)]
pub(crate) struct Deployment {
    pub(crate) accounts: Accounts,
    pub(crate) config: ValidatedWorkerConfig,
}

/// The `Order` Virtual Object: one instance per order number. Registered as
/// `Szamlazz.Order`.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The object holds no state.
#[derive(Debug, Clone)]
pub struct Order {
    deployment: Deployment,
}

impl Order {
    /// Builds the object over the account resolver and credential store in
    /// `accounts` and the validated deployment-level `config`
    /// ([`WorkerConfig::validate`](crate::config::WorkerConfig::validate)).
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: ValidatedWorkerConfig) -> Self {
        Self {
            deployment: Deployment { accounts, config },
        }
    }

    /// The account resolver and credential store.
    #[must_use]
    pub fn accounts(&self) -> &Accounts {
        &self.deployment.accounts
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &ValidatedWorkerConfig {
        &self.deployment.config
    }

    /// Runs an exclusive handler's execution: the prologue (pin → resolve →
    /// fetch → open), then `body` on the execution it built, inside the
    /// execution span carrying the scope, the key, the invocation id and the
    /// account id.
    async fn execute<T, F, Fut>(&self, ctx: &ObjectContext<'_>, body: F) -> Result<T, HandlerError>
    where
        F: FnOnce(Execution) -> Fut + Send,
        Fut: Future<Output = Result<T, HandlerError>> + Send,
    {
        support::execute(ctx, Some(ctx.key()), &self.deployment, body).await
    }

    /// Runs a shared handler's (`get`) execution, as [`Order::execute`].
    async fn execute_shared<T, F, Fut>(
        &self,
        ctx: &SharedObjectContext<'_>,
        body: F,
    ) -> Result<T, HandlerError>
    where
        F: FnOnce(Execution) -> Fut + Send,
        Fut: Future<Output = Result<T, HandlerError>> + Send,
    {
        support::execute(ctx, Some(ctx.key()), &self.deployment, body).await
    }
}

/// The stateless `Szamlazz.Agent` service: by-number operations over the
/// same accounts as [`Order`], the taxpayer lookup and the `check_account`
/// probe. No handler compares what it finds with the account.
///
/// **Unkeyed.** A stateless service's invocations run concurrently, so two
/// by-number writes on one invoice (`set_payments`, `storno`) are not
/// serialised by the worker as [`Order`]'s handlers are by its per-key lock.
/// Two replacing `set_payments` (`additive: false`) race and the last send to
/// land wins, which under reordered webhook deliveries may be the older
/// snapshot; two `storno`s both send, and szamlazz.hu's idempotent storno
/// answers the repeat with the existing storno number (verified for a
/// sequential repeat; two sends in the same instant are unverified). A keyed
/// `Szamlazz.Document` object per invoice number was judged over-engineering
/// for two writes whose only hazard is a replace: the caller serialises per
/// invoice on its side, or sends `additive: true` and lets szamlazz.hu sum.
#[derive(Debug, Clone)]
pub struct Agent {
    deployment: Deployment,
}

impl Agent {
    /// Builds the service over the account resolver and credential store in
    /// `accounts` and the validated deployment-level `config`
    /// ([`WorkerConfig::validate`](crate::config::WorkerConfig::validate)).
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: ValidatedWorkerConfig) -> Self {
        Self {
            deployment: Deployment { accounts, config },
        }
    }

    /// The account resolver and credential store.
    #[must_use]
    pub fn accounts(&self) -> &Accounts {
        &self.deployment.accounts
    }

    /// The deployment-level settings.
    #[must_use]
    pub fn config(&self) -> &ValidatedWorkerConfig {
        &self.deployment.config
    }

    /// Runs a handler's execution: the prologue (pin → resolve → fetch →
    /// open), then `body` on the execution it built, inside the execution
    /// span carrying the scope, the invocation id and the account id.
    async fn execute<T, F, Fut>(&self, ctx: &Context<'_>, body: F) -> Result<T, HandlerError>
    where
        F: FnOnce(Execution) -> Fut + Send,
        Fut: Future<Output = Result<T, HandlerError>> + Send,
    {
        support::execute(ctx, None, &self.deployment, body).await
    }
}

#[cfg(test)]
mod tests;
