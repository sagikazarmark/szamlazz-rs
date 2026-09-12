//! Protocol tests: fixture documents through the axum router, in the order
//! the router documents (header, body limit, root, key, parse, handler).
//! The parse itself is `tests/document.rs`, feature-free.

#![cfg(feature = "axum")]

mod common;

use axum::Router;
use axum::http::StatusCode;
use common::axum::{TestHandler, UnavailableResolver, call, call_at, request, request_at, send};
use common::{BANK_TRANSACTION, OUTGOING_INVOICE, RECEIPT_BATCH, incoming_invoice};
use std::convert::Infallible;
use std::future::ready;
use std::sync::{Arc, Mutex};
use szamlazz_adatkapcsolat::axum::BodyLimit;
use szamlazz_adatkapcsolat::{
    Ack, BankTransaction, Handler, InvoiceAck, InvoiceDocument, MaybeSend, ReceiptBatch,
};
use tower::util::ServiceExt as _;

#[test]
#[should_panic(expected = "Adatkapcsolat key must not be empty")]
fn empty_fixed_key_is_rejected_at_construction() {
    let _ = szamlazz_adatkapcsolat::axum::router("", MismatchedAck);
}

#[test]
#[should_panic(expected = "Adatkapcsolat key must not be empty")]
fn empty_fixed_key_with_body_limit_is_rejected_at_construction() {
    let _ = szamlazz_adatkapcsolat::axum::router_with_body_limit(
        "",
        MismatchedAck,
        BodyLimit::Unlimited,
    );
}

#[tokio::test]
async fn empty_presented_key_is_an_unknown_key_not_a_panic() {
    let (status, body) = call(Some(""), OUTGOING_INVOICE, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("KEY_ERR"));
}

#[tokio::test]
async fn numeric_lexical_shape_is_checked_only_after_authentication() {
    let xml = br#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>1</id><osszeg>1__2</osszeg></banktranz>"#;
    let (status, body) = call(Some("not-the-key"), xml, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("KEY_ERR"));
    let (status, body) = call(Some("secret-key"), xml, true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!body.contains("valasz"));
}

#[tokio::test]
async fn acks_document_with_valid_key() {
    let (status, body) = call(Some("secret-key"), OUTGOING_INVOICE, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<szamlavalasz"));
    assert!(body.contains("<iktatoszam>IKT-1</iktatoszam>"));
}

struct MismatchedAck;

impl Handler for MismatchedAck {
    type Error = Infallible;

    fn outgoing_invoice(
        &self,
        _invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Infallible>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(-1)))
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Infallible>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, Infallible>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, Infallible>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

#[tokio::test]
async fn normalizes_handler_ack_to_the_pushed_invoice_id() {
    let app = szamlazz_adatkapcsolat::axum::router("secret-key", MismatchedAck);
    let (status, body) = send(app, request(Some("secret-key"), OUTGOING_INVOICE)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<id>123456</id>"));
    assert!(!body.contains("<id>-1</id>"));
}

#[tokio::test]
async fn wrong_key_answers_key_err_without_handler() {
    let (status, body) = call(Some("not-the-key"), OUTGOING_INVOICE, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<hibakod>KEY_ERR</hibakod>"));
    assert!(body.contains("<szamlavalasz"));
}

// A PDF that does not decode is a content detail of a legal record, not a
// reason to refuse its delivery: under a wrong key the push is `KEY_ERR` as
// any other, under the right one it is Acked (with `pdf` read as absent; see
// `tests/document.rs`).
#[tokio::test]
async fn undecodable_pdf_does_not_fail_the_push() {
    let body = OUTGOING_INVOICE.replace("<pdf></pdf>", "<pdf>not base64!</pdf>");

    let (status, response) = call(Some("not-the-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.contains("KEY_ERR"));

    let (status, response) = call(Some("secret-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.contains("<szamlavalasz"), "{response}");
    assert!(!response.contains("hibakod"), "{response}");
}

// A date whose text is not a date is content too: the push is Acked with the
// date read as absent. The text's bytes must not matter: the timezone strip
// once split the text six bytes from its end and panicked inside a multi-byte
// character, after the key had been verified, and nothing in the router
// catches a panic, so szamlazz.hu got no answer at all.
#[tokio::test]
async fn date_that_is_not_a_date_is_acked_whatever_its_bytes() {
    for kelt in ["é12345", "éé€", "12345é", "2015-12-01junk"] {
        let body =
            OUTGOING_INVOICE.replace("<kelt>2015-12-01</kelt>", &format!("<kelt>{kelt}</kelt>"));

        let (status, response) = call(Some("secret-key"), body.as_bytes(), false).await;
        assert_eq!(status, StatusCode::OK, "{kelt:?}: {response}");
        assert!(response.contains("<szamlavalasz"), "{kelt:?}: {response}");
        assert!(response.contains("<id>123456</id>"), "{kelt:?}: {response}");
        assert!(!response.contains("hibakod"), "{kelt:?}: {response}");
    }
}

// The authenticated 400 is for a body that is not the pushed document at all:
// szamlazz.hu would retry it identically for 72 hours and then drop it, so
// it must never be the answer to a document that merely omits what the XSD
// requires: that one is Acked.
#[tokio::test]
async fn authenticated_400_is_reserved_for_a_body_that_is_not_a_document() {
    let truncated = &OUTGOING_INVOICE.as_bytes()[..OUTGOING_INVOICE.len() - 20];
    let (status, response) = call(Some("secret-key"), truncated, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!response.contains("valasz"), "{response}");

    let without_id = OUTGOING_INVOICE.replacen("<id>123456</id>", "", 1);
    let (status, _) = call(Some("secret-key"), without_id.as_bytes(), false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let identity_only = br#"<szamla xmlns="http://www.szamlazz.hu/szamla"><alap><id>42</id><szamlaszam>E-1</szamlaszam></alap></szamla>"#;
    let (status, response) = call(Some("secret-key"), identity_only, false).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert!(response.contains("<id>42</id>"), "{response}");
    assert!(!response.contains("hibakod"), "{response}");

    let empty_batch = br#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv"/>"#;
    let (status, response) = call(Some("secret-key"), empty_batch, false).await;
    assert_eq!(status, StatusCode::OK, "{response}");
    assert!(response.contains("<nyugtavalasz"), "{response}");
}

#[tokio::test]
async fn full_xml_shape_is_checked_after_authentication_before_dispatch() {
    let bank = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>7</id></banktranz>"#;
    for body in [
        format!("{bank}{}", bank.replace(">7<", ">8<")),
        format!("{bank}not XML"),
        bank.replace("<id>", "<future>\0</future><id>"),
        bank.replace("<id>", "<future attr='&#0;'/><id>"),
    ] {
        let (status, ack) = call(Some("not-the-key"), &body, true).await;
        assert_eq!(status, StatusCode::OK, "{body:?}");
        assert!(ack.contains("<banktranzvalasz"));
        assert!(ack.contains("KEY_ERR"));

        // Shape must stop dispatch before an Ack can be returned.
        let (status, response) = call(Some("secret-key"), &body, true).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}: {response}");
        assert!(!response.contains("valasz"));
    }
}

#[cfg(feature = "opendal")]
#[tokio::test]
async fn concatenated_records_are_refused_before_json_only_archiving() {
    let operator =
        opendal::Operator::new(opendal::services::Memory::default()).expect("memory storage");
    let archiver = szamlazz_adatkapcsolat::archive::Archiver::builder(operator.clone())
        .save_xml(false)
        .build();
    let app = szamlazz_adatkapcsolat::axum::router("secret-key", archiver);
    let bank = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>7</id></banktranz>"#;
    let body = format!("{bank}{}", bank.replace(">7<", ">8<"));
    let (status, _) = send(app, request(Some("secret-key"), &body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    for id in [7, 8] {
        assert!(
            !operator
                .exists(&format!("bank-transactions/undated/{id}.json"))
                .await
                .expect("check archived record")
        );
    }
}

// The docs guarantee the header accompanies every push, so a missing header
// is transport damage (e.g. a stripping proxy), not an unknown key. KEY_ERR
// would halt resends (bank transactions and receipts permanently), while a
// non-200 keeps the 72-hour retry window alive.
#[tokio::test]
async fn missing_key_answers_retryable_non_200() {
    let (status, body) = call(None, BANK_TRANSACTION.as_bytes(), false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains("KEY_ERR"));
}

// An unauthenticated client must not be able to make the receiver inspect a
// body: the header is checked first, so an invalid body without the header is
// 401, not 400.
#[tokio::test]
async fn missing_key_is_401_before_the_body_is_inspected() {
    for body in [
        b"<whatever/>".as_slice(),
        b"<szamla>\xff</szamla>".as_slice(),
        b"".as_slice(),
    ] {
        let (status, response) = call(None, body, false).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body:?}");
        assert!(!response.contains("KEY_ERR"));
    }
}

// Only the root element is inspected before the key check (enough to shape a
// KEY_ERR Ack), while the per-element namespace pass runs for authenticated
// pushes only. A child in the wrong namespace is therefore KEY_ERR under a
// wrong key and 400 under the right one.
#[tokio::test]
async fn namespace_validation_runs_only_after_the_key_is_accepted() {
    let body = OUTGOING_INVOICE.replacen("<szallito>", "<szallito xmlns=\"\">", 1);

    let (status, response) = call(Some("not-the-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        response.contains("<hibakod>KEY_ERR</hibakod>"),
        "{response}"
    );

    let (status, response) = call(Some("secret-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(response.contains("wrong namespace"), "{response}");
}

#[tokio::test]
async fn handler_error_becomes_500() {
    let (status, body) = call(Some("secret-key"), OUTGOING_INVOICE, true).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    // The handler's error must not be echoed back to szamlazz.hu.
    assert!(!body.contains("database down"));
}

#[tokio::test]
async fn invalid_ack_xml_becomes_500() {
    let app = szamlazz_adatkapcsolat::axum::router(
        "secret-key",
        TestHandler {
            fail: false,
            invalid_ack: true,
        },
    );
    let response = app
        .oneshot(request(Some("secret-key"), OUTGOING_INVOICE))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn receipt_batch_round_trip() {
    let (status, body) = call(Some("secret-key"), RECEIPT_BATCH.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<nyugtavalasz"));
    assert!(!body.contains("hibakod"));
}

#[tokio::test]
async fn invalid_or_unknown_documents_are_never_acknowledged() {
    for body in [
        b"<whatever/>".as_slice(),
        b"<szamla xmlns=\"https://wrong.example\"/>".as_slice(),
        b"<szamla>\xff</szamla>".as_slice(),
    ] {
        let (status, response) = call(Some("not-the-key"), body, false).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(!response.contains("KEY_ERR"));
    }
}

#[tokio::test]
async fn configurable_body_limit_is_applied_at_construction() {
    let low = szamlazz_adatkapcsolat::axum::router_with_body_limit(
        "secret-key",
        TestHandler::default(),
        BodyLimit::Max(OUTGOING_INVOICE.len() - 1),
    );
    let response = low
        .oneshot(request(Some("secret-key"), OUTGOING_INVOICE))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let high = szamlazz_adatkapcsolat::axum::router_with_body_limit(
        "secret-key",
        TestHandler::default(),
        BodyLimit::Max(OUTGOING_INVOICE.len()),
    );
    let response = high
        .oneshot(request(Some("secret-key"), OUTGOING_INVOICE))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

/// A well-formed-looking body of `size` bytes that no parse accepts.
fn oversized_body(size: usize) -> Vec<u8> {
    let mut body = Vec::with_capacity(size + 64);
    body.extend_from_slice(b"<banktranz xmlns=\"http://www.szamlazz.hu/banktranz\"><!--");
    body.resize(size, b'x');
    body.extend_from_slice(b"--></banktranz>");
    body
}

// Számlázz.hu publishes no maximum size, but the receiver sits on the public
// internet: the default is a generous, documented cap, and lifting it is an
// explicit choice, never what a caller gets by not thinking about it.
#[tokio::test]
async fn default_body_limit_is_64_mib_and_unlimited_is_explicit() {
    assert_eq!(BodyLimit::default(), BodyLimit::Max(64 * 1024 * 1024));
    let over = oversized_body(64 * 1024 * 1024 + 1);

    let fixed = szamlazz_adatkapcsolat::axum::router("secret-key", TestHandler::default());
    let response = fixed
        .oneshot(request(Some("secret-key"), &over))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    let resolved = szamlazz_adatkapcsolat::axum::router_with_resolver(UnavailableResolver);
    let response = resolved
        .oneshot(request(Some("secret-key"), &over))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);

    // Explicitly unlimited: the body is buffered and read, then refused by the
    // typed parse (a transaction without an id), not by the limit.
    let unlimited = szamlazz_adatkapcsolat::axum::router_with_body_limit(
        "secret-key",
        TestHandler::default(),
        BodyLimit::Unlimited,
    );
    let response = unlimited
        .oneshot(request(Some("secret-key"), &over))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let unlimited = szamlazz_adatkapcsolat::axum::router_with_resolver_and_body_limit(
        UnavailableResolver,
        BodyLimit::Unlimited,
    );
    let response = unlimited
        .oneshot(request(Some("secret-key"), &over))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// The cap is enforced before the key is checked, and only for requests that
// carry the header: an unauthenticated client never gets the body buffered.
#[tokio::test]
async fn body_limit_applies_after_the_header_check_and_before_the_key_check() {
    let over = oversized_body(64 * 1024 * 1024 + 1);
    let (status, _) = call(Some("not-the-key"), &over, false).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);

    let (status, _) = call(None, &over, false).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn nested_push_accepts_paths_with_and_without_trailing_slash() {
    for path in ["/push", "/push/"] {
        let app = szamlazz_adatkapcsolat::axum::nest_at(
            Router::new(),
            "/push",
            szamlazz_adatkapcsolat::axum::router("secret-key", TestHandler::default()),
        );
        let response = app
            .oneshot(request_at(path, Some("secret-key"), OUTGOING_INVOICE))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK, "path {path}");
        assert!(response.headers().get("location").is_none(), "path {path}");
    }
}

#[tokio::test]
async fn nested_push_accepts_key_appended_to_url() {
    let app = szamlazz_adatkapcsolat::axum::nest_at(
        Router::new(),
        "/push",
        szamlazz_adatkapcsolat::axum::router("secret-key", TestHandler::default()),
    );
    let response = app
        .oneshot(request_at(
            "/push/secret-key",
            Some("secret-key"),
            OUTGOING_INVOICE,
        ))
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn key_appended_url_still_authenticates_header() {
    let (status, body) = call_at("/secret-key", Some("wrong-key"), OUTGOING_INVOICE, true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<hibakod>KEY_ERR</hibakod>"));

    let (status, body) = call_at("/secret-key", None, OUTGOING_INVOICE, true).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains("KEY_ERR"));
}

/// The handler of one connection, recording which connection was called.
#[derive(Clone)]
struct ConnectionHandler {
    connection: &'static str,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl Handler for ConnectionHandler {
    type Error = Infallible;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Infallible>> + MaybeSend {
        self.calls.lock().expect("calls").push(self.connection);
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, Infallible>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, Infallible>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, Infallible>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

/// Two connections, `first-key` and `second-key`, each with a handler held
/// by the resolver and handed out by `Arc::clone`; the keys are compared in
/// constant time, as a resolver holding secrets should.
struct Connections {
    first: Arc<ConnectionHandler>,
    second: Arc<ConnectionHandler>,
}

fn connections(calls: &Arc<Mutex<Vec<&'static str>>>) -> Connections {
    Connections {
        first: Arc::new(ConnectionHandler {
            connection: "first",
            calls: calls.clone(),
        }),
        second: Arc::new(ConnectionHandler {
            connection: "second",
            calls: calls.clone(),
        }),
    }
}

impl szamlazz_adatkapcsolat::axum::KeyResolver for Connections {
    type Handler = ConnectionHandler;
    type Error = Infallible;

    fn resolve(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<Arc<Self::Handler>>, Infallible>> + MaybeSend {
        ready(Ok([
            ("first-key", &self.first),
            ("second-key", &self.second),
        ]
        .into_iter()
        .find(|(known, _)| szamlazz_adatkapcsolat::keys_match(key, known))
        .map(|(_, handler)| Arc::clone(handler))))
    }
}

/// A resolver that builds the connection's handler per request from what the
/// lookup found, the shape a database-backed resolver takes: nothing is held
/// across requests.
struct PerRequest {
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl szamlazz_adatkapcsolat::axum::KeyResolver for PerRequest {
    type Handler = ConnectionHandler;
    type Error = Infallible;

    async fn resolve(&self, key: &str) -> Result<Option<Arc<Self::Handler>>, Infallible> {
        tokio::task::yield_now().await;
        Ok(match key {
            "first-key" => Some(Arc::new(ConnectionHandler {
                connection: "first",
                calls: self.calls.clone(),
            })),
            _ => None,
        })
    }
}

// KEY_ERR is "your key is wrong": szamlazz.hu never resends a bank
// transaction or receipt answered with it, and an invoice only when it next
// changes. A resolver that could not check must therefore answer a non-200
// with no Ack, so the record stays in the 72-hour retry window.
#[tokio::test]
async fn unavailable_resolver_answers_503_without_an_ack_for_every_root() {
    let incoming = incoming_invoice();
    for (name, body) in [
        ("szamla", OUTGOING_INVOICE.as_bytes()),
        ("szamlabe", incoming.as_bytes()),
        ("banktranz", BANK_TRANSACTION.as_bytes()),
        ("xmlnyugtaarchiv", RECEIPT_BATCH.as_bytes()),
    ] {
        let app = szamlazz_adatkapcsolat::axum::router_with_resolver(UnavailableResolver);
        let (status, text) = send(app, request(Some("any-key"), body)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "root {name}");
        assert!(!text.contains("KEY_ERR"), "root {name}: {text}");
        assert!(!text.contains("valasz"), "root {name}: {text}");
        // The resolver's error is internal detail; it is not echoed.
        assert!(!text.contains("timed out"), "root {name}: {text}");
    }
}

#[tokio::test]
async fn unknown_key_answers_key_err_of_the_pushed_kind_for_every_root() {
    let incoming = incoming_invoice();
    for (body, ack_root) in [
        (OUTGOING_INVOICE.as_bytes(), "<szamlavalasz"),
        (incoming.as_bytes(), "<szamlabevalasz"),
        (BANK_TRANSACTION.as_bytes(), "<banktranzvalasz"),
        (RECEIPT_BATCH.as_bytes(), "<nyugtavalasz"),
    ] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let app = szamlazz_adatkapcsolat::axum::router_with_resolver(connections(&calls));
        let (status, text) = send(app, request(Some("third-key"), body)).await;
        assert_eq!(status, StatusCode::OK, "{ack_root}");
        assert!(text.contains(ack_root), "{text}");
        assert!(text.contains("<hibakod>KEY_ERR</hibakod>"), "{text}");
        assert!(calls.lock().expect("calls").is_empty());
    }
}

#[tokio::test]
async fn escaped_namespaces_reach_authentication_for_every_root() {
    let incoming = incoming_invoice();
    for (original, ack_root) in [
        (OUTGOING_INVOICE, "<szamlavalasz"),
        (incoming.as_str(), "<szamlabevalasz"),
        (BANK_TRANSACTION, "<banktranzvalasz"),
        (RECEIPT_BATCH, "<nyugtavalasz"),
    ] {
        let body = original.replace("http://", "http:&#47;&#47;");
        for key in ["secret-key", "not-the-key"] {
            let (status, ack) = call(Some(key), &body, false).await;
            assert_eq!(status, StatusCode::OK, "{ack_root}, {key}: {ack}");
            assert!(ack.contains(ack_root), "{ack}");
            assert_eq!(ack.contains("KEY_ERR"), key == "not-the-key");
        }
        // Normalization cannot move the full parse before authentication.
        let malformed = format!("{body}not XML");
        let (status, ack) = call(Some("not-the-key"), &malformed, false).await;
        assert_eq!(status, StatusCode::OK);
        assert!(ack.contains(ack_root));
        assert!(ack.contains("KEY_ERR"));
        let (status, _) = call(Some("secret-key"), &malformed, false).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn dtd_shape_is_checked_after_authentication_for_every_root() {
    for (root, content, ack_root) in [
        ("banktranz", "<id>7</id>", "<banktranzvalasz"),
        (
            "szamla",
            "<alap><id>7</id><szamlaszam>E-1</szamlaszam></alap>",
            "<szamlavalasz",
        ),
        (
            "szamlabe",
            "<alap><id>7</id><szamlaszam>E-1</szamlaszam></alap>",
            "<szamlabevalasz",
        ),
        (
            "xmlnyugtaarchiv",
            "<nyugta><alap><id>7</id></alap></nyugta>",
            "<nyugtavalasz",
        ),
    ] {
        for (declaration, valid) in [
            ("<!ELEMENT !!!>", false),
            ("<!ELEMENT a:b:c EMPTY>", false),
            ("<!ENTITY a:b 'x'>", false),
            ("<!ELEMENT p:unused EMPTY>", true),
            ("<!ENTITY unused '&#0;'>", false),
            ("<?XML invalid?>", false),
            ("<!ATTLIST extension future CDATA 'a>b'>", true),
        ] {
            let body = format!(
                r#"<!DOCTYPE {root} [{declaration}]><{root} xmlns="http:&#47;&#47;www.szamlazz.hu/{root}">{content}</{root}>"#
            );
            let (status, ack) = call(Some("not-the-key"), &body, false).await;
            assert_eq!(status, StatusCode::OK, "{body}: {ack}");
            assert!(ack.contains(ack_root));
            assert!(ack.contains("KEY_ERR"));
            let (status, ack) = call(Some("secret-key"), &body, false).await;
            assert_eq!(
                status,
                if valid {
                    StatusCode::OK
                } else {
                    StatusCode::BAD_REQUEST
                },
                "{body}: {ack}"
            );
            assert_eq!(ack.contains(ack_root), valid);
        }
    }
}

#[tokio::test]
async fn resolver_selects_the_connection_for_each_key() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let app = szamlazz_adatkapcsolat::axum::router_with_resolver(connections(&calls));

    for key in ["first-key", "second-key"] {
        let response = app
            .clone()
            .oneshot(request(Some(key), OUTGOING_INVOICE))
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(*calls.lock().expect("calls"), ["first", "second"]);
}

// The matched handler is owned, so a resolver may build it per request.
#[tokio::test]
async fn resolver_may_build_the_connection_per_request() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let app = szamlazz_adatkapcsolat::axum::router_with_resolver(PerRequest {
        calls: calls.clone(),
    });

    let (status, text) = send(app.clone(), request(Some("first-key"), OUTGOING_INVOICE)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("<id>123456</id>"), "{text}");

    let (status, text) = send(app, request(Some("other-key"), OUTGOING_INVOICE)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("<hibakod>KEY_ERR</hibakod>"), "{text}");
    assert_eq!(*calls.lock().expect("calls"), ["first"]);
}
