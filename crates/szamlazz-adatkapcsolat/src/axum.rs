//! axum integration: a [`Router`] that runs the whole push protocol — key
//! verification, root-element dispatch, ack rendering — around a [`Handler`].
//!
//! Works on native servers and on `wasm32`/Cloudflare Workers. axum requires
//! handler futures to be `Send`, which futures holding JS objects can never
//! be; on wasm targets this module wraps the handler future and state in
//! `send_wrapper::SendWrapper` — the same single-thread `Send` assertion
//! `#[worker::send]` makes. This is sound on single-threaded executors
//! (Workers, browsers); a hypothetical multi-threaded wasm runtime would
//! panic at the wrapper's thread check instead of causing undefined behavior.

use std::future::Future;
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use http::{HeaderMap, StatusCode, Uri, header};
use tower::util::ServiceExt as _;

use crate::KEY_HEADER;
use crate::ack::{Ack, InvoiceAck, InvoiceDirection};
use crate::document::Document;
use crate::handler::{Handler, MaybeSend, MaybeSync};

#[cfg(not(target_arch = "wasm32"))]
type AppState<R> = Arc<Receiver<R>>;
#[cfg(target_arch = "wasm32")]
type AppState<R> = send_wrapper::SendWrapper<Arc<Receiver<R>>>;

/// Resolves a presented Adatkapcsolat key to tenant-specific business logic.
///
/// The returned [`Handler`] is the authenticated tenant context, so its fields
/// are directly available while handling the document. Returning `None`
/// produces the protocol's matching `KEY_ERR` ack. Implementations should use
/// constant-time key comparison when keys are secrets rather than opaque IDs.
pub trait KeyResolver {
    /// Handler/context selected for an authenticated key.
    type Handler: Handler;

    /// Authenticates `presented_key` and returns its tenant handler/context.
    fn resolve(&self, presented_key: &str) -> Option<&Self::Handler>;
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
/// The layer handles the protocol before your [`Handler`] runs:
/// - requests whose `X-Szamlazzhu-Key` header does not match `key` are
///   answered `200` + `KEY_ERR` (per protocol) without invoking the handler;
/// - requests without the header are answered `401` instead of `KEY_ERR`:
///   szamlazz.hu always sends it, so its absence means transport damage
///   (e.g. a proxy stripping it), and a non-200 keeps retries alive rather
///   than halting delivery;
/// - unparsable bodies are answered `400` (szamlazz.hu retries, surfacing
///   the misconfiguration in its logs);
/// - handler errors are answered `500`, making szamlazz.hu retry for up to
///   72 hours.
///
/// Számlázz.hu publishes no maximum size and receipt batches are unbounded, so
/// this router disables axum's default body limit. Use
/// [`router_with_body_limit`] to apply a deployment-specific cap.
pub fn router<H>(key: impl Into<String>, handler: H) -> Router
where
    H: Handler + MaybeSend + MaybeSync + 'static,
    H::Error: MaybeSend,
{
    build_router(
        FixedKey {
            key: key.into(),
            handler,
        },
        None,
    )
}

/// Fixed-key convenience router with a caller-selected request-body limit.
pub fn router_with_body_limit<H>(key: impl Into<String>, handler: H, body_limit: usize) -> Router
where
    H: Handler + MaybeSend + MaybeSync + 'static,
    H::Error: MaybeSend,
{
    build_router(
        FixedKey {
            key: key.into(),
            handler,
        },
        Some(body_limit),
    )
}

/// Multi-customer router without a request-body limit.
pub fn router_with_resolver<R>(resolver: R) -> Router
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    build_router(resolver, None)
}

/// Multi-customer router with a caller-selected request-body limit.
pub fn router_with_resolver_and_body_limit<R>(resolver: R, body_limit: usize) -> Router
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    build_router(resolver, Some(body_limit))
}

fn build_router<R>(resolver: R, body_limit: Option<usize>) -> Router
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
        Some(limit) => router.layer(DefaultBodyLimit::max(limit)),
        None => router.layer(DefaultBodyLimit::disable()),
    }
}

/// Nests a receiver at both `path` and its trailing-slash form.
///
/// axum treats a nested root route at `/push` and `/push/` as distinct paths.
/// This helper installs both exact routes without a redirect. `path` may be
/// passed with or without its trailing slash. Both forms also accept the
/// optional identification-key suffix configured by Adatkapcsolat's
/// `addkeytourl` setting — as its own segment (`{path}/{key}`), so the
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

struct FixedKey<H> {
    key: String,
    handler: H,
}

impl<H: Handler> KeyResolver for FixedKey<H> {
    type Handler = H;

    fn resolve(&self, presented_key: &str) -> Option<&Self::Handler> {
        keys_match(presented_key, &self.key).then_some(&self.handler)
    }
}

struct Receiver<R> {
    resolver: R,
}

/// The axum handler: hands the `!Send`-tolerant inner future to axum, with
/// the wasm `Send` assertion applied where needed.
fn receive<R>(
    state: State<AppState<R>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl Future<Output = Response> + Send
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    let future = receive_inner(state, headers, body);

    #[cfg(target_arch = "wasm32")]
    {
        send_wrapper::SendWrapper::new(future)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        future
    }
}

async fn receive_inner<R>(
    State(receiver): State<AppState<R>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response
where
    R: KeyResolver + MaybeSend + MaybeSync + 'static,
    R::Handler: MaybeSend + MaybeSync + 'static,
    <R::Handler as Handler>::Error: MaybeSend,
{
    // Identify and validate the XML envelope before authentication, but defer
    // typed deserialization (including base64 PDF decoding) until afterwards.
    let root = match Document::preflight(&body) {
        Ok(root) => root,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    // Szamlazz.hu sends the key header with every push, so a missing (or
    // undecodable) header is transport damage — typically a proxy stripping
    // it — not an unknown key. KEY_ERR would make szamlazz.hu stop resending
    // (bank transactions and receipts permanently); a non-200 keeps the
    // 72-hour retry window alive while the deployment is fixed.
    let Some(presented_key) = headers.get(KEY_HEADER).and_then(|v| v.to_str().ok()) else {
        return (StatusCode::UNAUTHORIZED, "missing X-Szamlazzhu-Key header").into_response();
    };
    let Some(handler) = receiver.resolver.resolve(presented_key) else {
        // Per protocol: answer 200 with a KEY_ERR ack matching the pushed
        // document type, so szamlazz.hu stops sending until the key changes.
        return match root {
            crate::document::RootKind::OutgoingInvoice => {
                invoice_xml_response(&InvoiceAck::key_error(), InvoiceDirection::Outgoing)
            }
            crate::document::RootKind::IncomingInvoice => {
                invoice_xml_response(&InvoiceAck::key_error(), InvoiceDirection::Incoming)
            }
            crate::document::RootKind::BankTransaction => {
                xml_response(Ack::key_error().to_bank_transaction_xml())
            }
            crate::document::RootKind::Receipts => xml_response(Ack::key_error().to_receipts_xml()),
        };
    };

    let document = match Document::parse_preflighted(&body, root) {
        Ok(document) => document,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    match document {
        Document::OutgoingInvoice(invoice) => {
            let id = invoice.info.id;

            match handler.outgoing_invoice(invoice).await {
                Ok(ack) => invoice_xml_response(&ack.for_document(id), InvoiceDirection::Outgoing),
                Err(_) => handler_error(),
            }
        }
        Document::IncomingInvoice(invoice) => {
            let id = invoice.info.id;

            match handler.incoming_invoice(invoice).await {
                Ok(ack) => invoice_xml_response(&ack.for_document(id), InvoiceDirection::Incoming),
                Err(_) => handler_error(),
            }
        }
        Document::BankTransaction(transaction) => {
            match handler.bank_transaction(transaction).await {
                Ok(ack) => xml_response(ack.to_bank_transaction_xml()),
                Err(_) => handler_error(),
            }
        }
        Document::Receipts(batch) => match handler.receipts(batch).await {
            Ok(ack) => xml_response(ack.to_receipts_xml()),
            Err(_) => handler_error(),
        },
    }
}

fn invoice_xml_response(ack: &InvoiceAck, direction: InvoiceDirection) -> Response {
    match ack.to_xml(direction) {
        Ok(body) => xml_response(body),
        Err(_) => handler_error(),
    }
}

fn xml_response(body: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        body,
    )
        .into_response()
}

/// Compares the presented key against the configured one without an
/// early-exit on the first differing byte — the header is the connection's
/// only authentication, so leaking its content through timing must be avoided.
/// (The length comparison is not itself secret-dependent.)
fn keys_match(presented: &str, expected: &str) -> bool {
    let (presented, expected) = (presented.as_bytes(), expected.as_bytes());

    if presented.len() != expected.len() {
        return false;
    }
    let mut diff = 0u8;

    for (a, b) in presented.iter().zip(expected) {
        diff |= a ^ b;
    }

    diff == 0
}

/// Answers a handler failure with a bare 500. The handler's error is
/// deliberately not echoed to szamlazz.hu — it may carry internal detail, and
/// the status alone drives the 72-hour retry. Handlers should log their own
/// errors for diagnostics.
fn handler_error() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "handler error").into_response()
}
