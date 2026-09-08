//! The handlers over a [`Runner`]: everything a handler does once it has its
//! context, decode the body, parse the key, run the prologue, run its
//! operation, written once against the seam, so that the `#[restate_sdk]`
//! handlers (`handlers`) are one line each over the SDK context and the
//! offline suite (`paths`) drives the very same fns over a `FakeRunner`.
//!
//! Every handler with an input takes it as a [`Body`] and decodes it first
//! (a malformed body is the `invalid_input` fault before anything is
//! journaled), then, on `Szamlazz.Order`, parses its key (an invalid or
//! untrimmed key is `invalid_input` likewise, before the prologue), then runs
//! its execution ([`durable::execute`]: the span, the prologue, the body).

use std::future::Future;

use restate_sdk::errors::HandlerError;
use szamlazz_agent::ops::taxpayer::TaxpayerPrefix;

use super::agent::taxpayer_prefix;
use super::prologue::Execution;
use super::runner::Runner;
use super::support::{Fault, order_key};
use super::{Agent, Body, Order, durable};
use crate::contract::{
    CheckAccountResponse, CorrectRequest, CreateRequest, CreateResponse, DeleteProformaRequest,
    DeleteProformaResponse, DocumentKind, OrderStatus, QueryRequest, QueryResponse,
    QueryTaxpayerRequest, QueryTaxpayerResponse, SetPaymentsRequest, SetPaymentsResponse,
    StornoRequest, StornoResponse,
};
use crate::identity::OrderKey;

/// The `Szamlazz.Order` handlers over one runner.
pub(super) struct OrderHandlers<'a> {
    order: &'a Order,
    runner: &'a dyn Runner,
}

impl<'a> OrderHandlers<'a> {
    /// `order`'s handlers over `runner`.
    pub(super) fn new(order: &'a Order, runner: &'a dyn Runner) -> Self {
        Self { order, runner }
    }

    /// The order the runner's key names, as every `Szamlazz.Order` handler
    /// parses it before its prologue ([`order_key`]).
    fn key(&self) -> Result<OrderKey, Fault> {
        // An object runner always carries a key; a service runner driving
        // these handlers is a wiring error, answered as a fault rather than a
        // panic (a panic on the SDK's connection task takes every in-flight
        // invocation down with it, #64).
        let key = self.runner.key().ok_or_else(|| {
            Fault::invalid_input(
                "no Virtual Object key: the Szamlazz.Order handlers run on a keyed context",
            )
        })?;
        order_key(key)
    }

    /// Runs the execution: the prologue (pin → resolve → fetch → open), then
    /// `body` on the execution it built, inside the execution span carrying
    /// the scope, the key, the invocation id and the account id.
    async fn execute<T, F, Fut>(&self, body: F) -> Result<T, HandlerError>
    where
        F: FnOnce(Execution) -> Fut + Send,
        Fut: Future<Output = Result<T, HandlerError>> + Send,
    {
        durable::execute(
            self.runner,
            &self.order.accounts,
            &self.order.config,
            &self.order.opener,
            body,
        )
        .await
    }

    /// `create_proforma`, `create_invoice`, `create_prepayment` and
    /// `create_final`: one shape, `kind` apart.
    async fn issue(
        &self,
        kind: DocumentKind,
        request: Body<CreateRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        let request = request.into_request()?;
        let order = self.key()?;
        let runner = self.runner;
        self.execute(|execution| async move {
            Box::pin(execution.issue_kind(runner, order, kind, request)).await
        })
        .await
    }

    /// `Szamlazz.Order.create_proforma`.
    pub(super) async fn create_proforma(
        &self,
        request: Body<CreateRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        self.issue(DocumentKind::Proforma, request).await
    }

    /// `Szamlazz.Order.create_invoice`.
    pub(super) async fn create_invoice(
        &self,
        request: Body<CreateRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        self.issue(DocumentKind::Invoice, request).await
    }

    /// `Szamlazz.Order.create_prepayment`.
    pub(super) async fn create_prepayment(
        &self,
        request: Body<CreateRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        self.issue(DocumentKind::Prepayment, request).await
    }

    /// `Szamlazz.Order.create_final`.
    pub(super) async fn create_final(
        &self,
        request: Body<CreateRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        self.issue(DocumentKind::Final, request).await
    }

    /// `Szamlazz.Order.correct_invoice`: the body decoded, the key parsed,
    /// then the corrective protocol on the execution.
    pub(super) async fn correct_invoice(
        &self,
        request: Body<CorrectRequest>,
    ) -> Result<CreateResponse, HandlerError> {
        let request = request.into_request()?;
        let order = self.key()?;
        let runner = self.runner;
        self.execute(|execution| async move {
            Box::pin(execution.correct(runner, order, request)).await
        })
        .await
    }

    /// `Szamlazz.Order.storno_invoice`: the body decoded, the key parsed,
    /// then the storno protocol on the execution.
    pub(super) async fn storno_invoice(
        &self,
        request: Body<StornoRequest>,
    ) -> Result<StornoResponse, HandlerError> {
        let request = request.into_request()?;
        let order = self.key()?;
        let runner = self.runner;
        self.execute(
            |execution| async move { Box::pin(execution.storno(runner, order, request)).await },
        )
        .await
    }

    /// `Szamlazz.Order.delete_proforma`: the body decoded, the key parsed,
    /// then the delete on the execution.
    pub(super) async fn delete_proforma(
        &self,
        request: Body<DeleteProformaRequest>,
    ) -> Result<DeleteProformaResponse, HandlerError> {
        let request = request.into_request()?;
        let order = self.key()?;
        let runner = self.runner;
        self.execute(
            |execution| async move { Box::pin(execution.delete(runner, order, request)).await },
        )
        .await
    }

    /// `Szamlazz.Order.get`: no body; the key parsed, then the live view on
    /// the execution.
    pub(super) async fn get(&self) -> Result<OrderStatus, HandlerError> {
        let order = self.key()?;
        let runner = self.runner;
        self.execute(|execution| async move { execution.status(runner, order).await })
            .await
    }
}

/// The `Szamlazz.Agent` handlers over one runner.
pub(super) struct AgentHandlers<'a> {
    agent: &'a Agent,
    runner: &'a dyn Runner,
}

impl<'a> AgentHandlers<'a> {
    /// `agent`'s handlers over `runner`.
    pub(super) fn new(agent: &'a Agent, runner: &'a dyn Runner) -> Self {
        Self { agent, runner }
    }

    /// Runs the execution; see [`OrderHandlers::execute`].
    async fn execute<T, F, Fut>(&self, body: F) -> Result<T, HandlerError>
    where
        F: FnOnce(Execution) -> Fut + Send,
        Fut: Future<Output = Result<T, HandlerError>> + Send,
    {
        durable::execute(
            self.runner,
            &self.agent.accounts,
            &self.agent.config,
            &self.agent.opener,
            body,
        )
        .await
    }

    /// `Szamlazz.Agent.check_account`: no body; the probe on the execution.
    pub(super) async fn check_account(&self) -> Result<CheckAccountResponse, HandlerError> {
        let runner = self.runner;
        self.execute(|execution| async move { execution.check_account_request(runner).await })
            .await
    }

    /// `Szamlazz.Agent.query`: the body decoded, then the one read.
    pub(super) async fn query(
        &self,
        request: Body<QueryRequest>,
    ) -> Result<QueryResponse, HandlerError> {
        let request = request.into_request()?;
        let runner = self.runner;
        self.execute(|execution| async move { execution.query_request(runner, request).await })
            .await
    }

    /// `Szamlazz.Agent.query_taxpayer`: the body decoded and the tax number
    /// reduced to its prefix, both before the prologue, then the one read.
    pub(super) async fn query_taxpayer(
        &self,
        request: Body<QueryTaxpayerRequest>,
    ) -> Result<QueryTaxpayerResponse, HandlerError> {
        // Both refusals precede the prologue: nothing journaled, nothing sent.
        let request = request.into_request()?;
        let prefix: TaxpayerPrefix = taxpayer_prefix(&request)?;
        let runner = self.runner;
        self.execute(
            |execution| async move { execution.query_taxpayer_request(runner, prefix).await },
        )
        .await
    }

    /// `Szamlazz.Agent.set_payments`: the body decoded, then the one-shot
    /// write.
    pub(super) async fn set_payments(
        &self,
        request: Body<SetPaymentsRequest>,
    ) -> Result<SetPaymentsResponse, HandlerError> {
        let request = request.into_request()?;
        let runner = self.runner;
        self.execute(
            |execution| async move { execution.set_payments_request(runner, request).await },
        )
        .await
    }

    /// `Szamlazz.Agent.storno`: the body decoded, then the by-number storno
    /// protocol.
    pub(super) async fn storno(
        &self,
        request: Body<StornoRequest>,
    ) -> Result<StornoResponse, HandlerError> {
        let request = request.into_request()?;
        let runner = self.runner;
        self.execute(|execution| async move { execution.storno_request(runner, request).await })
            .await
    }
}
