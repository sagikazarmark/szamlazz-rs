//! The `#[restate_sdk::object]` / `#[restate_sdk::service]` handler surfaces.
//!
//! Kept in their own module so the `missing_docs` allowance covers only the
//! macro-generated clients; the handler logic lives in the sibling modules.

#![allow(
    missing_docs,
    reason = "the macro-generated `OrderClient` / `AgentClient` carry no documentation"
)]

use restate_sdk::errors::HandlerResult;
use restate_sdk::prelude::{Context, ObjectContext, SharedObjectContext};
use restate_sdk::serde::Json;

use super::{Agent, Order};
use crate::contract::{
    CorrectRequest, CreateRequest, CreateResponse, DeleteProformaRequest, DeleteProformaResponse,
    DocumentKind, OrderStatus, QueryRequest, QueryResponse, SetPaymentsRequest,
    SetPaymentsResponse, StornoRequest, StornoResponse,
};

/// The `Szamlazz.Order` Virtual Object, keyed by the order number
/// (`rendelésszám`).
///
/// Keeps no state: every handler answers from szamlazz.hu through the order's
/// deterministic external ids. The retry identity of a request is Restate's
/// ingress `Idempotency-Key`. Issuing is two durable steps — a read-only
/// lookup and a query-first create under the issue policy's run retry policy
/// (design §5) — and every handler that calls szamlazz.hu kills the invocation
/// after five attempts (ADR 0004); the external-id query inside the create
/// step is what makes both safe.
#[restate_sdk::object(name = "Szamlazz.Order")]
impl Order {
    /// Issues the proforma (`díjbekérő`) of the order.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_proforma(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_kind(&ctx, DocumentKind::Proforma, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the invoice (`számla`) of the order, converting its live
    /// proforma unless told otherwise (`options.proforma`).
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_kind(&ctx, DocumentKind::Invoice, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the prepayment invoice (`előlegszámla`) of the order; one per
    /// order.
    ///
    /// Takes no `options.proforma` (anything but `auto` is `invalid_input`)
    /// and runs no proforma lookup: the Agent cannot carry
    /// `dijbekeroSzamlaszam` on a prepayment invoice, and szamlazz.hu
    /// converts the order's live proforma by shared order number regardless
    /// (`docs/szamlazz-hu-behaviour.md`, "Proformas: conversion,
    /// auto-linking, deletion"). `get` reports the proforma as `consumed`
    /// once the link landed.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_prepayment(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_kind(&ctx, DocumentKind::Prepayment, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the final invoice (`végszámla`) settling the order's live
    /// prepayment invoice.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_final(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_kind(&ctx, DocumentKind::Final, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues a corrective invoice (`helyesbítő számla`) for an invoice of
    /// this order. A new `correction_id` issues a new corrective.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn correct_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CorrectRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.correct(&ctx, request.into_inner()))
            .await
            .map(Json)
    }

    /// Reverses (`sztornó`) an invoice of this order; idempotent.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn storno_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<StornoRequest>,
    ) -> HandlerResult<Json<StornoResponse>> {
        Box::pin(self.storno(&ctx, request.into_inner()))
            .await
            .map(Json)
    }

    /// Deletes the order's proforma.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn delete_proforma(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<DeleteProformaRequest>,
    ) -> HandlerResult<Json<DeleteProformaResponse>> {
        Box::pin(self.delete(&ctx, request.into_inner()))
            .await
            .map(Json)
    }

    /// What szamlazz.hu holds under the order's external ids right now: four
    /// queries, no state. Read-only, so it runs concurrently with the
    /// exclusive handlers.
    #[handler(invocation_retry_policy(max_attempts = 3, on_max_attempts = "kill"))]
    async fn get(&self, ctx: SharedObjectContext<'_>) -> HandlerResult<Json<OrderStatus>> {
        self.status(&ctx).await.map(Json)
    }
}

/// The `Szamlazz.Agent` service: query, credit entries and storno by document
/// number. Never calls into `Order`; a document that carries an order number
/// is reported as `managed_by_order` instead.
#[restate_sdk::service(name = "Szamlazz.Agent")]
impl Agent {
    /// Queries a document by number, order number or external id.
    #[handler(invocation_retry_policy(
        initial_interval = "10s",
        factor = 2.0,
        max_interval = "1m",
        max_attempts = 3,
        on_max_attempts = "kill"
    ))]
    async fn query(
        &self,
        ctx: Context<'_>,
        request: Json<QueryRequest>,
    ) -> HandlerResult<Json<QueryResponse>> {
        self.query_request(&ctx, request.into_inner())
            .await
            .map(Json)
    }

    /// Registers credit entries (`jóváírás`) on an invoice.
    #[handler(
        invocation_retry_policy(max_attempts = 2, on_max_attempts = "kill"),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn set_payments(
        &self,
        ctx: Context<'_>,
        request: Json<SetPaymentsRequest>,
    ) -> HandlerResult<Json<SetPaymentsResponse>> {
        self.set_payments_request(&ctx, request.into_inner())
            .await
            .map(Json)
    }

    /// Reverses an invoice that no `Order` manages.
    #[handler(
        invocation_retry_policy(max_attempts = 2, on_max_attempts = "kill"),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn storno(
        &self,
        ctx: Context<'_>,
        request: Json<StornoRequest>,
    ) -> HandlerResult<Json<StornoResponse>> {
        self.storno_request(&ctx, request.into_inner())
            .await
            .map(Json)
    }
}
