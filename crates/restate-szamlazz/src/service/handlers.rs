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
    DocumentKind, ForgetRequest, GetRequest, OrderSnapshot, QueryRequest, QueryResponse,
    RecordReversalRequest, SetPaymentsRequest, SetPaymentsResponse, StornoRequest, StornoResponse,
};

/// The `Szamlazz.Order` Virtual Object, keyed by the order number
/// (`rendelésszám`).
///
/// Issuing handlers take a caller-supplied `request_id` as their retry
/// identity: the same id returns the entry's current state forever. Every
/// handler that calls szamlazz.hu kills the invocation after five attempts
/// (ADR 0004); the `pending` slot written before the first call makes that
/// safe.
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
        idempotency_retention = "7d"
    )]
    async fn create_proforma(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_slot(&ctx, DocumentKind::Proforma, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the invoice (`számla`) of the order, optionally converting its
    /// proforma.
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
        idempotency_retention = "7d"
    )]
    async fn create_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_slot(&ctx, DocumentKind::Invoice, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the prepayment invoice (`előlegszámla`) of the order; one per
    /// order.
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
        idempotency_retention = "7d"
    )]
    async fn create_prepayment(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_slot(&ctx, DocumentKind::Prepayment, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues the final invoice (`végszámla`) settling the order's committed
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
        idempotency_retention = "7d"
    )]
    async fn create_final(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        Box::pin(self.issue_slot(&ctx, DocumentKind::Final, request.into_inner()))
            .await
            .map(Json)
    }

    /// Issues a corrective invoice (`helyesbítő számla`) for an invoice managed
    /// by this order. A new request id issues a new corrective.
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
        idempotency_retention = "7d"
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

    /// Reverses (`sztornó`) an invoice managed by this order; idempotent.
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
        idempotency_retention = "7d"
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
        idempotency_retention = "7d"
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

    /// The order's ledger as recorded, or — with `verify` — after checking
    /// every committed document against szamlazz.hu. Read-only: never writes
    /// state, so it runs concurrently with the exclusive handlers.
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
        idempotency_retention = "7d"
    )]
    async fn get(
        &self,
        ctx: SharedObjectContext<'_>,
        request: Json<GetRequest>,
    ) -> HandlerResult<Json<OrderSnapshot>> {
        self.snapshot(&ctx, request.into_inner()).await.map(Json)
    }

    /// Operator assertion about a recorded document (private: not reachable
    /// from the ingress).
    #[handler(ingress_private = true)]
    async fn record_reversal(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<RecordReversalRequest>,
    ) -> HandlerResult<Json<OrderSnapshot>> {
        self.record(&ctx, request.into_inner()).await.map(Json)
    }

    /// Operator drop of a slot whose document szamlazz.hu no longer knows
    /// (private: not reachable from the ingress).
    #[handler(ingress_private = true)]
    async fn forget(
        &self,
        ctx: ObjectContext<'_>,
        request: Json<ForgetRequest>,
    ) -> HandlerResult<Json<OrderSnapshot>> {
        self.forget_slot(&ctx, request.into_inner()).await.map(Json)
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
        idempotency_retention = "7d"
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
        idempotency_retention = "7d"
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
