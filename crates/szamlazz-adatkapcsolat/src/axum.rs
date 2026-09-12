//! axum integration: a [`Router`] that runs the whole push protocol (key
//! verification, root-element dispatch, ack rendering) around a [`Handler`].
//!
//! Works on native servers and on `wasm32`/Cloudflare Workers. axum requires
//! handler futures to be `Send`, which futures holding JS objects can never
//! be; on wasm targets this module wraps the handler future and state in
//! `send_wrapper::SendWrapper`, the same single-thread `Send` assertion
//! `#[worker::send]` makes. This is sound on single-threaded executors
//! (Workers, browsers); a hypothetical multi-threaded wasm runtime would
//! panic at the wrapper's thread check instead of causing undefined behavior.

use std::convert::Infallible;
use std::future::{Future, ready};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, FromRequest as _, Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use http::{StatusCode, Uri, header};
use tower::util::ServiceExt as _;

use crate::KEY_HEADER;
use crate::ack::{ControlCode, InvoiceAck, InvoiceDirection};
use crate::document::{Document, RootKind};
use crate::handler::{Handler, MaybeSend, MaybeSync};
use crate::key::keys_match;

#[cfg(not(target_arch = "wasm32"))]
type AppState<R> = Arc<Receiver<R>>;
#[cfg(target_arch = "wasm32")]
type AppState<R> = send_wrapper::SendWrapper<Arc<Receiver<R>>>;

/// Resolves a presented Adatkapcsolat key to the [`Handler`] of the
/// *connection* it identifies: one registered Adatkapcsolat connection (one
/// szamlazz.hu account pushing to this URL under one key) when several share
/// the receiver.
///
/// The returned handler is the authenticated connection's context, so its
/// fields are directly available while handling the document. The three
/// answers are protocol decisions, not just lookup results:
///
/// - `Ok(Some(handler))`: the key is known; the push is handled.
/// - `Ok(None)`: the key is **definitely** unknown; the router answers the
///   protocol's `KEY_ERR` Ack, and szamlazz.hu **never resends** a bank
///   transaction or receipt answered that way (an invoice only when it next
///   changes). Return it only from a lookup that actually completed.
/// - `Err(_)`: the key **could not be checked** (a database or secrets
///   service timed out, …); the router answers `503` with no Ack, so the
///   record stays in szamlazz.hu's 72-hour retry window. The error is not
///   echoed to szamlazz.hu; with the `tracing` feature the router logs it at
///   `warn`, without it the error is dropped, so log it yourself.
///
/// Resolution is async so a resolver that does I/O need not block.
/// Implementations can be written as `async fn`; the `MaybeSend` bound keeps
/// the trait implementable on Cloudflare Workers, where futures are `!Send`.
/// Compare keys that are secrets with [`keys_match`], in constant time; a key
/// that is an opaque id you look up needs no more than the lookup.
///
/// The matched handler is **owned** (`Arc`), so a resolver may hand out a
/// handler it holds (an `Arc::clone`, what the fixed-key router does) or
/// build one per request from what the lookup found, a connection context a
/// borrow could not express:
///
/// ```
/// use std::sync::Arc;
///
/// use szamlazz_adatkapcsolat::axum::KeyResolver;
/// # use szamlazz_adatkapcsolat::{Ack, BankTransaction, Handler, InvoiceAck, InvoiceDocument, ReceiptBatch};
///
/// /// The connection a key resolved to, built per request.
/// struct Connection {
///     name: String,
///     // a database handle, a queue producer, …
/// }
/// # impl Handler for Connection {
/// #     type Error = std::convert::Infallible;
/// #     async fn outgoing_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
/// #         Ok(InvoiceAck::accept(invoice.info.id))
/// #     }
/// #     async fn incoming_invoice(&self, invoice: InvoiceDocument) -> Result<InvoiceAck, Self::Error> {
/// #         Ok(InvoiceAck::accept(invoice.info.id))
/// #     }
/// #     async fn bank_transaction(&self, _: BankTransaction) -> Result<Ack, Self::Error> {
/// #         Ok(Ack::accept())
/// #     }
/// #     async fn receipts(&self, _: ReceiptBatch) -> Result<Ack, Self::Error> {
/// #         Ok(Ack::accept())
/// #     }
/// # }
///
/// struct Directory { /* a pool */ }
/// # impl Directory {
/// #     async fn connection_for(&self, _key: &str) -> Result<Option<String>, std::io::Error> {
/// #         Ok(Some("acme".to_owned()))
/// #     }
/// # }
///
/// impl KeyResolver for Directory {
///     type Handler = Connection;
///     type Error = std::io::Error;
///
///     async fn resolve(&self, key: &str) -> Result<Option<Arc<Connection>>, Self::Error> {
///         // A lookup that fails is `Err` (503, retried), never `Ok(None)`.
///         let name = self.connection_for(key).await?;
///         Ok(name.map(|name| Arc::new(Connection { name })))
///     }
/// }
/// ```
///
/// Breaking change in 0.4: `resolve` answered `Option<&Self::Handler>`, a
/// borrow of the resolver, and `Error` was bound by `Display`.
pub trait KeyResolver {
    /// The handler, the connection's context, selected for an authenticated
    /// key.
    type Handler: Handler;

    /// Why a lookup could not complete. Not sent to szamlazz.hu (the `503`
    /// alone drives the retry), so it may carry internal detail.
    type Error: std::error::Error;

    /// Authenticates `presented_key` and returns its connection's handler.
    fn resolve(
        &self,
        presented_key: &str,
    ) -> impl Future<Output = Result<Option<Arc<Self::Handler>>, Self::Error>> + MaybeSend;
}

/// The request-body cap a receiver router applies.
///
/// The receiver sits on the public internet and buffers each push before it
/// can authenticate it, so a cap is the default: [`BodyLimit::DEFAULT`] is
/// 64 MiB, far above any observed push, where Számlázz.hu publishes no
/// maximum. Requests over the cap are answered `413`, which szamlazz.hu
/// retries like any non-200. Receipt batches are unbounded in principle;
/// raise the cap, or pass [`BodyLimit::Unlimited`], as a deliberate,
/// deployment-level choice; nothing selects it for you.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BodyLimit {
    /// Reject bodies over this many bytes with `413`.
    Max(usize),
    /// No cap; the whole body is buffered whatever its size.
    Unlimited,
}

impl BodyLimit {
    /// The cap [`router`] and [`router_with_resolver`] apply: 64 MiB.
    pub const DEFAULT: Self = Self::Max(64 * 1024 * 1024);
}

impl Default for BodyLimit {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A router answering the Adatkapcsolat push protocol at `/` and, when the
/// registration enables `addkeytourl`, at `/{identification-key}`.
///
/// `addkeytourl` appends the key to the registered URL by literal string
/// concatenation, so the key only forms the path segment these routes match
/// when the registered URL ends with `/` (as every official example does).
/// When enabling `addkeytourl`, register the receiver URL with a trailing
/// slash; a URL like `https://host/push` would receive pushes at
/// `/push{key}`, which no route matches.
///
/// Nest it wherever your receiver URL is registered:
///
/// ```no_run
/// use axum::Router;
/// use szamlazz_adatkapcsolat::axum::{nest_at, router};
/// # use szamlazz_adatkapcsolat::{Handler, MaybeSend, MaybeSync};
/// # fn app<H>(key: String, handler: H) -> Router
/// # where
/// #     H: Handler + MaybeSend + MaybeSync + 'static,
/// #     <H as Handler>::Error: MaybeSend,
/// # {
/// let receiver = router(key, handler);
/// let app = nest_at(Router::new(), "/szamlazz/push", receiver);
/// # app
/// # }
/// ```
///
/// # What each answer means to szamlazz.hu
///
/// A push is delivered at most a bounded number of times, and szamlazz.hu
/// reads the body of a `200` only: a `200` ends the delivery, any other status
/// is retried for up to 72 hours whatever its body says. Two `200` bodies
/// carry *control codes* and end more than the delivery: `KEY_ERR` ("this
/// key is wrong") makes szamlazz.hu stop sending under the key, and **a bank
/// transaction or receipt answered `KEY_ERR` is never resent**, an invoice
/// only when it next changes. So the layer reserves `KEY_ERR` for a definite
/// mismatch and answers every uncertain case with a status that keeps the
/// retry window alive, in this order, before your [`Handler`] runs:
///
/// - **`401`**: no (or an undecodable) `X-Szamlazzhu-Key` header. szamlazz.hu
///   always sends it, so its absence is transport damage (a proxy stripping
///   it). Answered before the body is read: an unauthenticated client gets no
///   parsing out of the receiver.
/// - **`413`**: the body is over the [`BodyLimit`].
/// - **`400`**: the root element is not a known document (or the body is not
///   UTF-8 / XML at all). Only UTF-8 validity and the root element are
///   checked at this point.
/// - **`200` + `KEY_ERR`**, in the Ack shape of the pushed kind: the
///   [`KeyResolver`] completed and knows no such key ([`Ok(None)`]). With
///   `router(key, …)` that is a header not equal to `key`. Retries stop.
/// - **`503`**: the resolver could not check the key ([`Err`]): the record
///   stays retryable instead of being dropped.
/// - **`400`**: an authenticated push whose body is not the pushed document
///   at all: an element outside the document's namespace, XML the typed
///   parse cannot read (truncated, an `alap/id` missing, an `<osszeg>` that
///   is not a number). Never a document that merely omits what the XSD
///   requires, carries an unknown `irany`, a date that is not a date or a
///   PDF that does not decode; [`Document::parse`]
///   reads those leniently and the push is Acked, because szamlazz.hu retries
///   a `400` identically for 72 hours and then drops the record. A receiver
///   that wants the XSD's verdict calls [`Document::validate`] from its
///   handler.
/// - **`500`**: the handler failed; szamlazz.hu retries for up to 72 hours.
///
/// Retry-keeping: `401`, `413`, `400`, `503`, `500`. Final: `200`, with or
/// without a control code.
///
/// # Body limit
///
/// This router applies [`BodyLimit::DEFAULT`] (64 MiB). Use
/// [`router_with_body_limit`] to raise it or (as an explicit choice) to
/// lift it with [`BodyLimit::Unlimited`].
///
/// # Panics
///
/// Panics at construction if the configured key is empty.
///
/// [`Ok(None)`]: KeyResolver::resolve
/// [`Err`]: KeyResolver::resolve
pub fn router<H>(key: impl Into<String>, handler: H) -> Router
where
    H: Handler + MaybeSend + MaybeSync + 'static,
    H::Error: MaybeSend,
{
    router_with_body_limit(key, handler, BodyLimit::DEFAULT)
}

/// Fixed-key router with a caller-selected request-body limit; see [`router`]
/// for the protocol it answers and [`BodyLimit`] for the default it replaces.
///
/// # Panics
///
/// Panics at construction if the configured key is empty.
pub fn router_with_body_limit<H>(
    key: impl Into<String>,
    handler: H,
    body_limit: BodyLimit,
) -> Router
where
    H: Handler + MaybeSend + MaybeSync + 'static,
    H::Error: MaybeSend,
{
    let key = key.into();
    assert!(!key.is_empty(), "Adatkapcsolat key must not be empty");
    router_with_resolver_and_body_limit(
        FixedKey {
            key,
            handler: Arc::new(handler),
        },
        body_limit,
    )
}

/// Multi-connection router: the [`KeyResolver`] maps each presented key to its
/// connection's [`Handler`], and says when it could not ([`Err`] → `503`,
/// never `KEY_ERR`). Applies [`BodyLimit::DEFAULT`]; see [`router`] for the
/// protocol it answers.
///
/// [`Err`]: KeyResolver::resolve
pub fn router_with_resolver<R>(resolver: R) -> Router
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    router_with_resolver_and_body_limit(resolver, BodyLimit::DEFAULT)
}

/// Multi-connection router with a caller-selected request-body limit; see
/// [`router_with_resolver`] and [`BodyLimit`].
pub fn router_with_resolver_and_body_limit<R>(resolver: R, body_limit: BodyLimit) -> Router
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    let receiver = Arc::new(Receiver { resolver });
    #[cfg(not(target_arch = "wasm32"))]
    let state: AppState<R> = receiver;
    #[cfg(target_arch = "wasm32")]
    let state: AppState<R> = send_wrapper::SendWrapper::new(receiver);
    let router = Router::new()
        .route("/", post(receive::<R>))
        .route("/{appended_key}", post(receive::<R>))
        .with_state(state);

    match body_limit {
        BodyLimit::Max(limit) => router.layer(DefaultBodyLimit::max(limit)),
        BodyLimit::Unlimited => router.layer(DefaultBodyLimit::disable()),
    }
}

/// Nests a receiver at both `path` and its trailing-slash form.
///
/// axum treats a nested root route at `/push` and `/push/` as distinct paths.
/// This helper installs both exact routes without a redirect. `path` may be
/// passed with or without its trailing slash. Both forms also accept the
/// optional identification-key suffix configured by Adatkapcsolat's
/// `addkeytourl` setting, as its own segment (`{path}/{key}`), so the
/// receiver URL must be registered with a trailing slash (see [`router`]);
/// authentication still uses `X-Szamlazzhu-Key`.
///
/// # Panics
///
/// Panics if `path` is empty or contains only `/` characters, or if axum
/// rejects a generated route because the path is invalid or conflicts with an
/// existing route.
pub fn nest_at(app: Router, path: &str, receiver: Router) -> Router {
    let path = path.trim_end_matches('/');
    assert!(!path.is_empty(), "receiver path must not be the root");
    let trailing_receiver = receiver.clone();
    let trailing_route = post(move |mut request: Request| {
        let receiver = trailing_receiver.clone();
        async move {
            *request.uri_mut() = Uri::from_static("/");
            receiver
                .oneshot(request)
                .await
                .unwrap_or_else(|error| match error {})
        }
    });

    app.nest(path, receiver)
        .route(&format!("{path}/"), trailing_route)
}

/// The one-connection resolver behind [`router`]: one key, one handler, held
/// once and handed out by `Arc::clone` (no allocation per request).
struct FixedKey<H> {
    key: String,
    handler: Arc<H>,
}

impl<H: Handler + MaybeSend + MaybeSync> KeyResolver for FixedKey<H> {
    type Handler = H;
    type Error = Infallible;

    fn resolve(
        &self,
        presented_key: &str,
    ) -> impl Future<Output = Result<Option<Arc<H>>, Infallible>> + MaybeSend {
        ready(Ok(
            keys_match(presented_key, &self.key).then(|| Arc::clone(&self.handler))
        ))
    }
}

struct Receiver<R> {
    resolver: R,
}

/// The axum handler: hands the `!Send`-tolerant inner future to axum, with
/// the wasm `Send` assertion applied where needed.
fn receive<R>(state: State<AppState<R>>, request: Request) -> impl Future<Output = Response> + Send
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    let future = receive_inner(state, request);

    #[cfg(target_arch = "wasm32")]
    {
        send_wrapper::SendWrapper::new(future)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        future
    }
}

/// The protocol in the order [`router`] documents: header, body limit, root
/// scan, key, and (for authenticated pushes only) the full parse and the
/// handler. Unauthenticated work stays minimal; every uncertain answer stays
/// retryable.
async fn receive_inner<R>(State(receiver): State<AppState<R>>, request: Request) -> Response
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    // Szamlazz.hu sends the key header with every push, so a missing (or
    // undecodable) header is transport damage (typically a proxy stripping
    // it), not an unknown key. KEY_ERR would make szamlazz.hu stop resending
    // (bank transactions and receipts permanently); a non-200 keeps the
    // 72-hour retry window alive while the deployment is fixed. Answered
    // before the body is buffered: a client without the header gets no work
    // out of the receiver.
    let Some(presented_key) = request
        .headers()
        .get(KEY_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
    else {
        return (StatusCode::UNAUTHORIZED, "missing X-Szamlazzhu-Key header").into_response();
    };

    // Buffers the body under the router's `DefaultBodyLimit` (413 over it).
    let body = match Bytes::from_request(request, &()).await {
        Ok(body) => body,
        Err(rejection) => return rejection.into_response(),
    };

    // Identify the root before authentication (a KEY_ERR Ack takes the shape
    // of the pushed kind), but read no XML past its start tag.
    let root = match Document::identify(&body) {
        Ok(root) => root,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    let handler = match receiver.resolver.resolve(&presented_key).await {
        Ok(Some(handler)) => handler,
        Ok(None) => {
            // Per protocol: answer 200 with a KEY_ERR Ack matching the pushed
            // document type, so szamlazz.hu stops sending until the key
            // changes. Reserved for a lookup that completed and found no
            // connection; bank transactions and receipts answered this way
            // are never resent.
            return xml_response(ControlCode::KeyUnknown.to_xml(root));
        }
        // The resolver could not check the key. KEY_ERR would permanently
        // drop the record; a non-200 keeps the 72-hour retry window alive.
        // The 503 carries no Ack; the root kind only names the push in the
        // log.
        Err(error) => return resolver_unavailable(root, &error),
    };

    // Authenticated: the per-element namespace pass and the typed parse.
    // Shape only: a body that is not the pushed document. Content is read
    // leniently: a 400 here is retried identically for 72 hours and then
    // dropped, so it must not be the answer to a missing element.
    let document = match Document::parse_identified(&body, root) {
        Ok(document) => document,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match document {
        Document::OutgoingInvoice(invoice) => {
            let id = invoice.info.id;

            match handler.outgoing_invoice(invoice).await {
                Ok(ack) => invoice_xml_response(&ack.for_document(id), InvoiceDirection::Outgoing),
                Err(error) => handler_error(root, &error),
            }
        }
        Document::IncomingInvoice(invoice) => {
            let id = invoice.info.id;

            match handler.incoming_invoice(invoice).await {
                Ok(ack) => invoice_xml_response(&ack.for_document(id), InvoiceDirection::Incoming),
                Err(error) => handler_error(root, &error),
            }
        }
        Document::BankTransaction(transaction) => {
            match handler.bank_transaction(transaction).await {
                Ok(ack) => xml_response(ack.to_bank_transaction_xml()),
                Err(error) => handler_error(root, &error),
            }
        }
        Document::Receipts(batch) => match handler.receipts(batch).await {
            Ok(ack) => xml_response(ack.to_receipts_xml()),
            Err(error) => handler_error(root, &error),
        },
    }
}

fn invoice_xml_response(ack: &InvoiceAck, direction: InvoiceDirection) -> Response {
    match ack.to_xml(direction) {
        Ok(body) => xml_response(body),
        // The handler produced an Ack the protocol cannot carry: its
        // failure, answered like one.
        Err(error) => handler_error(direction.into(), &error),
    }
}

/// Answers a resolver that could not check the key with a bare 503. Like a
/// handler failure, the error is not echoed (it may carry internal detail),
/// and the status alone keeps szamlazz.hu retrying; under the `tracing`
/// feature it is logged at `warn`.
fn resolver_unavailable(root: RootKind, error: &dyn std::error::Error) -> Response {
    log_warn(
        root,
        error,
        "key resolver could not check the key; answering 503 so szamlazz.hu retries",
    );
    (StatusCode::SERVICE_UNAVAILABLE, "key resolver unavailable").into_response()
}

fn xml_response(body: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Answers a handler failure with a bare 500. The handler's error is
/// deliberately not echoed to szamlazz.hu: it may carry internal detail, and
/// the status alone drives the 72-hour retry. Under the `tracing` feature it
/// is logged at `warn`; without it, it is dropped here, and the handler logs
/// its own.
fn handler_error(root: RootKind, error: &dyn std::error::Error) -> Response {
    log_warn(
        root,
        error,
        "handler failed; answering 500 so szamlazz.hu retries",
    );
    (StatusCode::INTERNAL_SERVER_ERROR, "handler error").into_response()
}

/// The one place the router speaks about an error: a `warn` event under the
/// `tracing` feature, nothing without it. The response never carries it.
/// The `error` field is the error with its [`source`](std::error::Error::source)
/// chain (`outer: cause: root cause`), so a wrapped cause is not lost.
#[cfg_attr(not(feature = "tracing"), allow(unused_variables))]
fn log_warn(root: RootKind, error: &dyn std::error::Error, message: &'static str) {
    #[cfg(feature = "tracing")]
    tracing::warn!(kind = %root, error = %ErrorChain(error), "{message}");
}

/// An error and its `source()` chain, colon-separated, for the log.
#[cfg(feature = "tracing")]
struct ErrorChain<'a>(&'a dyn std::error::Error);

#[cfg(feature = "tracing")]
impl std::fmt::Display for ErrorChain<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)?;
        let mut source = self.0.source();
        while let Some(cause) = source {
            write!(f, ": {cause}")?;
            source = cause.source();
        }
        Ok(())
    }
}
