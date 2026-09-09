//! The router half of the shared test support: a handler and a resolver with
//! scripted failures, and one request through a router as status plus body.

use std::future::ready;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use szamlazz_adatkapcsolat::{
    Ack, BankTransaction, Handler, InvoiceAck, InvoiceDocument, KEY_HEADER, MaybeSend, ReceiptBatch,
};
use tower::util::ServiceExt as _;

/// A handler's or a resolver's failure with a message the tests look for in
/// the response (never) and, under `tracing`, in the log (once), wrapping a
/// cause so the log's `source()` chain is observable too.
#[derive(Debug)]
pub struct Failure(pub &'static str, pub std::io::Error);

impl Failure {
    pub fn new(message: &'static str) -> Self {
        Self(
            message,
            std::io::Error::new(std::io::ErrorKind::TimedOut, "connection timed out"),
        )
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for Failure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.1)
    }
}

/// Accepts everything by default; `fail` answers every invoice with an error,
/// `invalid_ack` with a registration number that cannot be rendered.
#[derive(Clone, Default)]
pub struct TestHandler {
    pub fail: bool,
    pub invalid_ack: bool,
}

impl Handler for TestHandler {
    type Error = Failure;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Failure>> + MaybeSend {
        if self.fail {
            return ready(Err(Failure::new("database down")));
        }
        let registration = if self.invalid_ack {
            "invalid\0registration"
        } else {
            "IKT-1"
        };
        ready(Ok(
            InvoiceAck::accept(invoice.info.id).with_registration_number(registration)
        ))
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Failure>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, Failure>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, Failure>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

pub fn request(key: Option<&str>, body: impl AsRef<[u8]>) -> Request<Body> {
    request_at("/", key, body)
}

pub fn request_at(path: &str, key: Option<&str>, body: impl AsRef<[u8]>) -> Request<Body> {
    let mut builder = Request::post(path).header("content-type", "application/xml");
    if let Some(key) = key {
        builder = builder.header(KEY_HEADER, key);
    }
    builder
        .body(Body::from(body.as_ref().to_vec()))
        .expect("request")
}

pub async fn call(key: Option<&str>, body: impl AsRef<[u8]>, fail: bool) -> (StatusCode, String) {
    call_at("/", key, body, fail).await
}

pub async fn call_at(
    path: &str,
    key: Option<&str>,
    body: impl AsRef<[u8]>,
    fail: bool,
) -> (StatusCode, String) {
    let app = szamlazz_adatkapcsolat::axum::router(
        "secret-key",
        TestHandler {
            fail,
            ..TestHandler::default()
        },
    );
    send(app, request_at(path, key, body)).await
}

/// Runs one request through `app` and returns the status with the body as text.
pub async fn send(app: Router, request: Request<Body>) -> (StatusCode, String) {
    let response = app.oneshot(request).await.expect("response");
    let status = response.status();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// A database-shaped resolver whose lookup is down: it cannot tell a wrong key
/// from a right one, so it must not answer either way. Written as an
/// `async fn` that awaits, the way a real lookup would.
pub struct UnavailableResolver;

impl szamlazz_adatkapcsolat::axum::KeyResolver for UnavailableResolver {
    type Handler = TestHandler;
    type Error = Failure;

    async fn resolve(&self, _key: &str) -> Result<Option<Arc<Self::Handler>>, Failure> {
        tokio::task::yield_now().await;
        Err(Failure::new("key store timed out"))
    }
}
