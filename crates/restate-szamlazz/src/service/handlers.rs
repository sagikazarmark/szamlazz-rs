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

use super::agent::taxpayer_prefix;
use super::support::order_key;
use super::{Agent, Body, Order};
use crate::contract::{
    CheckAccountResponse, CorrectRequest, CreateRequest, CreateResponse, DeleteProformaRequest,
    DeleteProformaResponse, DocumentKind, OrderStatus, QueryRequest, QueryResponse,
    QueryTaxpayerRequest, QueryTaxpayerResponse, SetCreditEntriesRequest, SetCreditEntriesResponse,
    StornoRequest, StornoResponse,
};

/// The `Szamlazz.Order` Virtual Object, keyed by the order number
/// (`rendelésszám`).
///
/// Keeps only unresolved-write uncertainty: document status comes from szamlazz.hu through the order's
/// deterministic external ids. The retry identity of a request is Restate's
/// ingress `Idempotency-Key`. Every handler with an input takes it as a
/// [`Body`] and decodes it first (a malformed body is the `invalid_input`
/// fault before anything is journaled), then parses its key (an invalid or
/// untrimmed key is `invalid_input` likewise, before the prologue), then runs
/// its execution inside one span (`execution{scope, order,
/// restate.invocation.id, account.id}`, so every log line it emits is
/// attributable): the prologue (pin the namespace, resolve the request's
/// scope to its account, journaled once per invocation) and then its operation.
/// Credentials and a fresh gateway are acquired inside the first operation
/// run that executes; completed runs replay without consulting the store.
/// After validation, a target ownership lookup (`lookup-{kind}`) settles a
/// live target, a collision or a reversal without reissue before prerequisites.
/// Reversed non-correctives take a best-effort `hint-storno-{number}`; an
/// absent target or explicit reissue proceeds through prerequisites, then
/// the full lookup (also `lookup-{kind}`) and query-first create under the
/// protected write protocol. Write handlers pause after five invocation attempts;
/// `get` kills after three. A completed arm never grants permission on replay.
#[restate_sdk::object(name = "Szamlazz.Order")]
impl Order {
    /// Issues the proforma (`díjbekérő`) of the order.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_proforma(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        // Reborrowed so that the `async move` body captures the reference,
        // not the context; every handler below does the same.
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.issue_kind(ctx, order, DocumentKind::Proforma, request)).await
        })
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
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.issue_kind(ctx, order, DocumentKind::Invoice, request)).await
        })
        .await
        .map(Json)
    }

    /// Issues the prepayment invoice (`előlegszámla`) of the order; one per
    /// order.
    ///
    /// Converts the order's live proforma unless told otherwise
    /// (`options.proforma`, exactly as `create_invoice` takes it): the create
    /// carries `dijbekeroSzamlaszam`, so the link does not rest on
    /// szamlazz.hu's own linking by shared order number, which happens
    /// regardless, and is why `none` is `conflict{proforma_live}`
    /// while a live proforma of ours exists. `get` reports the proforma as
    /// `consumed` once the link landed.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_prepayment(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.issue_kind(ctx, order, DocumentKind::Prepayment, request)).await
        })
        .await
        .map(Json)
    }

    /// Issues the final invoice (`végszámla`) settling the order's live
    /// prepayment invoice.
    ///
    /// szamlazz.hu links the prepayment invoice (the create carries
    /// `elolegSzamlaszam`) but does **not** net it into the final invoice's
    /// totals: the caller's `document` lists the full performance and deducts
    /// the prepayment as a negative line item at the same VAT rate (verified
    /// on the test account). Takes no `options.proforma` (anything but `auto`
    /// is `invalid_input`): the order's proforma was consumed by the
    /// prepayment invoice.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn create_final(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<CreateRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.issue_kind(ctx, order, DocumentKind::Final, request)).await
        })
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
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn correct_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<CorrectRequest>,
    ) -> HandlerResult<Json<CreateResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.correct(ctx, order, request)).await
        })
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
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn storno_invoice(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<StornoRequest>,
    ) -> HandlerResult<Json<StornoResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.storno(ctx, order, request)).await
        })
        .await
        .map(Json)
    }

    /// Deletes the order's expected proforma; a replacement is never selected.
    /// A lost answer or cancellation of the one-shot write is structured
    /// `outcome_unknown`: reconcile using `get` and the expected number, then
    /// deliberately renew with a new `Idempotency-Key` and the same expected
    /// number if still intended. The deletion may already have landed.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            factor = 2.0,
            max_interval = "10m",
            max_attempts = 5,
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn delete_proforma(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<DeleteProformaRequest>,
    ) -> HandlerResult<Json<DeleteProformaResponse>> {
        let request = request.into_request()?;
        let order = order_key(ctx.key())?;
        super::recovery::guard(&ctx).await?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            Box::pin(execution.delete(ctx, order, request)).await
        })
        .await
        .map(Json)
    }

    /// A non-atomic observation of the order's four external ids: separately
    /// journaled reads that can mix observation times and replay ages. Runs
    /// alongside exclusive writes, so absence does not prove an in-flight
    /// create will never land. Use a fresh invocation/key for each new poll;
    /// a retained `Idempotency-Key` can replay the completed answer. Unspecified
    /// discovery retention does not disable deduplication. Use Restate's
    /// attach/output to learn whether a particular invocation completed.
    /// The journal is retained a day for inspection. The retry policy is the reads'
    /// (`10s → 1m`, three attempts, the same as `Szamlazz.Agent.query`'s):
    /// pinned like every other handler's, so no server default leaks through.
    /// The timeouts are the reads': a read step is one round trip bounded by
    /// the 60 s client timeout, and szamlazz.hu has been seen to stall for a
    /// minute and still answer, so the server's 1 m default would suspend
    /// exactly such a read.
    #[handler(
        invocation_retry_policy(
            initial_interval = "10s",
            factor = 2.0,
            max_interval = "1m",
            max_attempts = 3,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "1d"
    )]
    async fn get(&self, ctx: SharedObjectContext<'_>) -> HandlerResult<Json<OrderStatus>> {
        let order = order_key(ctx.key())?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.status(ctx, order).await
        })
        .await
        .map(Json)
    }

    /// Operator-only shared observation of unresolved Order uncertainty.
    #[handler]
    async fn observe_unresolved(
        &self,
        ctx: SharedObjectContext<'_>,
    ) -> HandlerResult<Json<crate::contract::recovery::UnresolvedObservation>> {
        self.observe_marker(&ctx).await.map(Json)
    }

    /// Operator-only exclusive evidence-carrying settlement of the exact marker.
    #[handler(
        invocation_retry_policy(
            initial_interval = "10s",
            factor = 2.0,
            max_interval = "1m",
            max_attempts = 3,
            on_max_attempts = "pause"
        ),
        inactivity_timeout = "4m",
        abort_timeout = "3m",
        journal_retention = "30d"
    )]
    async fn recover(
        &self,
        ctx: ObjectContext<'_>,
        request: Body<crate::contract::recovery::RecoveryRequest>,
    ) -> HandlerResult<Json<crate::contract::recovery::RecoveryResponse>> {
        self.recover_marker(&ctx, request.into_request()?)
            .await
            .map(Json)
    }
}

/// The `Szamlazz.Agent` service: query, credit entries and storno by document
/// number, the NAV taxpayer lookup by tax number, and the `check_account`
/// probe. Never calls into `Order`; a document that carries an order number
/// is reported as `managed_by_order` when it is a supported key, otherwise
/// `unsupported_order_number` with the reported string and reconciliation
/// guidance. Neither sends a storno.
/// Unkeyed: invocations run concurrently, so two by-number writes on one
/// invoice are not serialised here; that is the caller's (see [`Agent`]).
#[restate_sdk::service(name = "Szamlazz.Agent")]
impl Agent {
    /// Proves, for the scope the request arrived under, that it reaches the
    /// worker, resolves to the configured account and the account's agent key
    /// works, with one read-only query of a sentinel external id, issuing
    /// nothing. For onboarding and deploy pipelines; also the deploy-time
    /// canary for the experimental Restate flags (`scope: null` under a
    /// scoped call means the server did not forward the scope). No input.
    /// It does not identify the seller or test/live account. Verify those
    /// with `examples/verify_seller.rs` and the deployed `Accounts` bundle.
    /// The journal is retained a day so that the leak assertion can scan it.
    /// The timeouts are the reads' 2m / 2m: one 60 s round trip plus
    /// the margin a stalling szamlazz.hu needs.
    #[handler(
        invocation_retry_policy(
            initial_interval = "10s",
            factor = 2.0,
            max_interval = "1m",
            max_attempts = 3,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "1d"
    )]
    async fn check_account(&self, ctx: Context<'_>) -> HandlerResult<Json<CheckAccountResponse>> {
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.check_account_request(ctx).await
        })
        .await
        .map(Json)
    }

    /// Queries a document by number, order number or external id, returning
    /// a projection that deliberately omits the seller block. The
    /// journal is retained a day for inspection; completed reads replay, and
    /// a retained idempotency key can replay the completed answer. The
    /// timeouts are the reads' 2m / 2m: one
    /// 60 s round trip plus the margin a stalling szamlazz.hu needs.
    #[handler(
        invocation_retry_policy(
            initial_interval = "10s",
            factor = 2.0,
            max_interval = "1m",
            max_attempts = 3,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "1d"
    )]
    async fn query(
        &self,
        ctx: Context<'_>,
        request: Body<QueryRequest>,
    ) -> HandlerResult<Json<QueryResponse>> {
        let request = request.into_request()?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.query_request(ctx, request).await
        })
        .await
        .map(Json)
    }

    /// Looks a Hungarian taxpayer up through NAV (`xmltaxpayer`) by tax
    /// number (the bare eight-digit stem or the full `NNNNNNNN-N-NN` form)
    /// on the account the request's scope resolves to, so an embedder needs
    /// no second credential path for this one read. Read-only, one step
    /// under the read policy; `valid: false` is data. Not cached here: the
    /// caller caches, with a TTL on the order of a day. The journal is
    /// retained a day so that it can be inspected; there is nothing to
    /// replay. The timeouts are the reads' 2m / 2m: one 60 s round
    /// trip plus the margin a stalling szamlazz.hu needs.
    #[handler(
        invocation_retry_policy(
            initial_interval = "10s",
            factor = 2.0,
            max_interval = "1m",
            max_attempts = 3,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "1d"
    )]
    async fn query_taxpayer(
        &self,
        ctx: Context<'_>,
        request: Body<QueryTaxpayerRequest>,
    ) -> HandlerResult<Json<QueryTaxpayerResponse>> {
        // Both refusals precede the prologue: nothing journaled, nothing sent.
        let request = request.into_request()?;
        let prefix = taxpayer_prefix(&request)?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.query_taxpayer_request(ctx, prefix).await
        })
        .await
        .map(Json)
    }

    /// Registers credit entries (`jóváírás`) on an invoice.
    ///
    /// With `additive: true` this is **at-least-once**: a lost reply or
    /// cancellation of the one-shot write is the structured
    /// `outcome_unknown`, and the one retry after a crash re-sends the same
    /// entries, each of which appends a second copy. The retry waits out the
    /// 60 s client timeout (never the server's ~500 ms default) so that it
    /// cannot re-send while the first send is still in flight; a caller that
    /// sees `outcome_unknown` queries the invoice before sending only missing
    /// entries with a new `Idempotency-Key`. A replacing call queries first
    /// too, then sends the current intended snapshot if still wanted.
    ///
    /// Not serialised per invoice: the service is unkeyed, so two concurrent
    /// replacing calls on one invoice race and the last send to land wins
    /// (see [`Agent`]). The caller serialises per invoice, or sends
    /// `additive: true` and lets szamlazz.hu sum.
    #[handler(
        invocation_retry_policy(
            initial_interval = "2m",
            max_attempts = 2,
            on_max_attempts = "kill"
        ),
        inactivity_timeout = "2m",
        abort_timeout = "2m",
        journal_retention = "3d",
        idempotency_retention = "30d"
    )]
    async fn set_credit_entries(
        &self,
        ctx: Context<'_>,
        request: Body<SetCreditEntriesRequest>,
    ) -> HandlerResult<Json<SetCreditEntriesResponse>> {
        let request = request.into_request()?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.set_credit_entries_request(ctx, request).await
        })
        .await
        .map(Json)
    }

    /// Reverses an invoice that no `Order` manages. The storno step is the
    /// same closure `Szamlazz.Order` runs (query, send, re-query at 60 s
    /// each), so the retry policy and the timeouts are `Szamlazz.Order`'s:
    /// invocation attempts are spent only on worker-side failures
    /// (a crash, a rollout cutting the connection), and every re-dispatch is
    /// query-first, so nothing about an unmanaged storno justifies a shorter
    /// budget; the retry interval waits out the 60 s client timeout (never
    /// the server's ~500 ms default), so that the leading query cannot look
    /// before a cut send has landed.
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
    async fn storno(
        &self,
        ctx: Context<'_>,
        request: Body<StornoRequest>,
    ) -> HandlerResult<Json<StornoResponse>> {
        let request = request.into_request()?;
        let ctx = &ctx;
        self.execute(ctx, |execution| async move {
            execution.storno_by_number(ctx, request).await
        })
        .await
        .map(Json)
    }
}
