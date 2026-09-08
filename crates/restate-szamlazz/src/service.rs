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
//! account resolver and the credential store), and a [`WorkerConfig`] with the
//! deployment-level settings (the namespace of the external ids; the issue,
//! read and resolve policies). Every handler runs the same prologue after parsing
//! its key: **pin** the namespace in a pure durable step, **resolve** the
//! request's scope to its account in a durable step named `account` under the
//! resolve policy, **fetch** the account's credentials outside the journal on
//! every execution, **open** the gateway for this execution over a fresh
//! client. The handler body then runs on that execution (`prologue::Execution`);
//! nothing of it (gateway, client, credentials) outlives the execution.
//!
//! Every durable step runs over the `Runner` seam (`runner`): an
//! object-safe trait the three SDK context types implement, so the
//! `#[restate_sdk]` handlers (`handlers`) are one line each over their context
//! and the handler bodies (`entry`, `create`, `storno`, `agent`, `durable`)
//! are written once, and the offline suite (`paths`) drives them over a fake
//! runner without a server.
//!
//! - [`Order`]: keyed by the order number; its per-key lock serialises
//!   issuing per order; registered as `Szamlazz.Order`.
//! - [`Agent`]: by-number operations (`query`, `set_payments`, `storno`), the
//!   NAV taxpayer lookup (`query_taxpayer`) and the read-only `check_account`
//!   probe, registered as `Szamlazz.Agent`.

use crate::account::Accounts;
use crate::config::WorkerConfig;

mod agent;
mod body;
mod create;
mod durable;
mod entry;
mod handlers;
#[cfg(test)]
mod journal;
#[cfg(test)]
mod paths;
mod prologue;
#[cfg(test)]
pub(crate) mod run_names;
pub(crate) mod runner;
mod storno;
mod support;

pub use body::Body;
pub use handlers::{AgentClient, AgentIngressClient, OrderClient, OrderIngressClient};

use entry::{AgentHandlers, OrderHandlers};
use prologue::Opener;
use runner::Runner;

/// The `Order` Virtual Object: one instance per order number. Registered as
/// `Szamlazz.Order`.
///
/// Same-key handlers run one at a time, which serialises issuing per order.
/// The object holds no state.
#[derive(Debug, Clone)]
pub struct Order {
    accounts: Accounts,
    config: WorkerConfig,
    opener: Opener,
}

impl Order {
    /// Builds the object over the account resolver and credential store in
    /// `accounts` and the deployment-level `config`.
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: WorkerConfig) -> Self {
        Self {
            accounts,
            config,
            opener: Opener::default(),
        }
    }

    /// The same object opening every execution's gateway through `opener`
    /// instead of `Gateway::open`: the offline suite's, over a client that
    /// loads no root certificates.
    #[cfg(test)]
    pub(crate) fn with_opener(mut self, opener: Opener) -> Self {
        self.opener = opener;
        self
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

    /// The object's handlers over `runner`: what each `#[restate_sdk]`
    /// handler does with its context, and what the offline suite drives over
    /// a fake.
    fn over<'a>(&'a self, runner: &'a dyn Runner) -> OrderHandlers<'a> {
        OrderHandlers::new(self, runner)
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
    accounts: Accounts,
    config: WorkerConfig,
    opener: Opener,
}

impl Agent {
    /// Builds the service over the account resolver and credential store in
    /// `accounts` and the deployment-level `config`.
    #[must_use]
    pub fn from_parts(accounts: Accounts, config: WorkerConfig) -> Self {
        Self {
            accounts,
            config,
            opener: Opener::default(),
        }
    }

    /// The same service opening every execution's gateway through `opener`
    /// instead of `Gateway::open`; see [`Order::with_opener`].
    #[cfg(test)]
    pub(crate) fn with_opener(mut self, opener: Opener) -> Self {
        self.opener = opener;
        self
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

    /// The service's handlers over `runner`; see [`Order::over`].
    fn over<'a>(&'a self, runner: &'a dyn Runner) -> AgentHandlers<'a> {
        AgentHandlers::new(self, runner)
    }
}

#[cfg(test)]
mod tests;
