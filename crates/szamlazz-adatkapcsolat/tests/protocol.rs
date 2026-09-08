//! End-to-end protocol tests: fixture documents through parsing, the Handler
//! trait, and the axum router.

#![cfg(feature = "axum")]

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt as _;
use rust_decimal::dec;
use std::convert::Infallible;
use std::future::ready;
use std::sync::{Arc, Mutex};
use szamlazz_adatkapcsolat::axum::BodyLimit;
use szamlazz_adatkapcsolat::{
    Ack, BankTransaction, Document, Handler, InvoiceAck, InvoiceAppearance, InvoiceDocument,
    KEY_HEADER, MaybeSend, ReceiptBatch, TransactionDirection, VatRate,
};
use tower::util::ServiceExt as _;

const OUTGOING_INVOICE: &[u8] = include_bytes!("synthetic/szamla.xml");

const BANK_TRANSACTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<banktranz xmlns="http://www.szamlazz.hu/banktranz">
  <id>987</id>
  <bankszamla>11111111-22222222-33333333</bankszamla>
  <erteknap>2026-07-03</erteknap>
  <irany>BE</irany>
  <technikai>false</technikai>
  <osszeg>12700.0</osszeg>
  <devizanem>HUF</devizanem>
  <partner><nev>Kovács Bt.</nev><bankszamla>44444444-55555555</bankszamla></partner>
  <kozlemeny>E-2026-123</kozlemeny>
</banktranz>"#;

const RECEIPT_BATCH: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv">
  <nyugta>
    <alap>
      <id>1</id>
      <nyugtaszam>NYGTA-2026-1</nyugtaszam>
      <tipus>NY</tipus>
      <stornozott>false</stornozott>
      <kelt>2026-07-03</kelt>
      <fizmod>készpénz</fizmod>
      <penznem>HUF</penznem>
      <devizaarf>1</devizaarf>
      <fokonyvVevo>311</fokonyvVevo>
      <teszt>false</teszt>
      <adoszam>12345678-1-42</adoszam>
    </alap>
    <tetelek>
      <tetel><megnevezes>Kitten doormat</megnevezes><nettoEgysegar>10000</nettoEgysegar><mennyiseg>2.0</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>20000.0</netto><afatipus>AAM</afatipus><afakulcs>27</afakulcs><afa>5400.0</afa><brutto>25400.0</brutto><fokonyv><arbevetel>911</arbevetel><afa>467</afa></fokonyv></tetel>
    </tetelek>
    <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>20000</netto><afa>5400</afa><brutto>25400</brutto></afakulcsossz><totalossz><netto>20000</netto><afa>5400</afa><brutto>25400</brutto></totalossz></osszegek>
  </nyugta>
  <nyugta>
    <alap><id>2</id><nyugtaszam>NYGTA-2026-2</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-03</kelt><fizmod>bankkártya</fizmod><penznem>HUF</penznem><teszt>false</teszt><adoszam>12345678-1-42</adoszam></alap>
    <tetelek><tetel><megnevezes>Service</megnevezes><nettoEgysegar>100</nettoEgysegar><mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>100</netto><afakulcs>27</afakulcs><afa>27</afa><brutto>127</brutto></tetel></tetelek>
    <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>100</netto><afa>27</afa><brutto>127</brutto></afakulcsossz><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek>
  </nyugta>
</xmlnyugtaarchiv>"#;

#[test]
fn parses_outgoing_invoice_fixture() {
    let Document::OutgoingInvoice(invoice) =
        Document::parse(OUTGOING_INVOICE).expect("parse fixture")
    else {
        panic!("expected outgoing invoice");
    };
    assert!(invoice.info.id > 0);
    assert_eq!(invoice.info.invoice_number, "2015-123");
    assert!(invoice.supplier.name.is_some());
    assert!(!invoice.items.is_empty());
    assert_eq!(invoice.info.source, Some(34));
    assert_eq!(invoice.info.registration_number, None);
    assert_eq!(invoice.info.e_invoice, Some(InvoiceAppearance::Paper));
    assert_eq!(invoice.info.kata_ledger, Some(false));
    assert!(invoice.info.email.is_some());
    assert_eq!(invoice.buyer.location, Some(1));
    assert_eq!(invoice.buyer.private_person, Some(false));
    assert_eq!(
        invoice
            .buyer
            .buyer_ledger
            .as_ref()
            .and_then(|ledger| ledger.customer.as_deref()),
        Some("12345A")
    );
    assert_eq!(invoice.items[0].vat_type.as_deref(), Some("ÁKK"));
    assert_eq!(invoice.items[0].vat_rate, Some(dec!(0)));
    assert_eq!(invoice.items[0].ordering, Some(1));
    assert_eq!(
        invoice.items[0]
            .ledger
            .as_ref()
            .and_then(|ledger| ledger.revenue.as_deref()),
        Some("12345A")
    );
    assert_eq!(invoice.payments[0].exchange_rate, Some(dec!(275)));
    assert_eq!(invoice.totals.per_vat_rate[0].vat_rate, Some(dec!(0)));
    assert_eq!(
        invoice.items[0].effective_vat(),
        Some(VatRate::Special("ÁKK"))
    );
    assert_eq!(invoice.raw_xml().map(str::as_bytes), Some(OUTGOING_INVOICE));
}

#[test]
fn preserves_all_invoice_appearance_codes() {
    let fixture = std::str::from_utf8(OUTGOING_INVOICE).expect("fixture UTF-8");
    for (code, expected) in [
        (0, InvoiceAppearance::NotInvoice),
        (1, InvoiceAppearance::Paper),
        (2, InvoiceAppearance::Electronic(2)),
        (3, InvoiceAppearance::Electronic(3)),
        (91, InvoiceAppearance::Unknown(91)),
    ] {
        let body = fixture.replace(
            "<eszamla>1</eszamla>",
            &format!("<eszamla>{code}</eszamla>"),
        );
        let Document::OutgoingInvoice(invoice) = Document::parse(body.as_bytes()).expect("parse")
        else {
            panic!("expected outgoing invoice");
        };
        assert_eq!(invoice.info.e_invoice, Some(expected));
        assert_eq!(
            invoice.info.e_invoice.map(InvoiceAppearance::code),
            Some(code)
        );
    }
}

#[test]
fn preserves_extended_invoice_fields_in_both_directions() {
    let fixture = std::str::from_utf8(OUTGOING_INVOICE).expect("fixture UTF-8");
    let enriched = fixture
        .replacen(
            "<megjegyzes></megjegyzes>",
            "<megjegyzes></megjegyzes><afatipus>EU-OSS</afatipus>",
            1,
        )
        .replacen(
            "<osszegek>",
            "<qutetek><qutet><nev>Fee</nev><afatipus>AAM</afatipus><afakulcs>27</afakulcs><netto>10</netto><afa>0</afa><brutto>10</brutto><elszdattol>2026-01-01</elszdattol><elszdatig>2026-01-31</elszdatig><afalevon>0</afalevon><cimkek><cimke>finance</cimke></cimkek></qutet></qutetek><cimkek><cimke>priority</cimke></cimkek><osszegek>",
            1,
        );
    let Document::OutgoingInvoice(outgoing) =
        Document::parse(enriched.as_bytes()).expect("outgoing")
    else {
        panic!("expected outgoing invoice");
    };
    assert_eq!(outgoing.info.vat_type.as_deref(), Some("EU-OSS"));
    assert_eq!(outgoing.tags, ["priority"]);
    assert_eq!(outgoing.financial_items.len(), 1);
    assert_eq!(outgoing.financial_items[0].vat_rate, Some(dec!(27)));
    assert_eq!(outgoing.financial_items[0].tags, ["finance"]);

    let incoming = enriched
        .replace("http://www.szamlazz.hu/szamla", "http://www.szamlazz.hu/szamlabe")
        .replace("<szamla xmlns=", "<szamlabe xmlns=")
        .replace("</szamla>", "</szamlabe>")
        .replacen(
            "<telj>2015-12-02</telj>",
            "<telj>2015-12-02</telj><folyamatostelj>true</folyamatostelj><elszDatTol>2015-12-01</elszDatTol><elszDatIg>2015-12-31</elszDatIg>",
            1,
        )
        .replacen("<teszt>false</teszt>", "<teszt>false</teszt><dobdel>true</dobdel>", 1);
    let Document::IncomingInvoice(incoming) =
        Document::parse(incoming.as_bytes()).expect("incoming")
    else {
        panic!("expected incoming invoice");
    };
    assert_eq!(incoming.info.continuous_fulfillment, Some(true));
    assert!(incoming.info.settlement_start.is_some());
    assert!(incoming.info.settlement_end.is_some());
    assert_eq!(incoming.info.deleted, Some(true));
    assert_eq!(incoming.buyer.location, Some(1));
}

#[test]
fn parses_bank_transaction() {
    let Document::BankTransaction(tx) =
        Document::parse(BANK_TRANSACTION.as_bytes()).expect("parse")
    else {
        panic!("expected bank transaction");
    };
    assert_eq!(tx.id, 987);
    assert_eq!(tx.direction, Some(TransactionDirection::Incoming));
    assert_eq!(tx.amount, Some(dec!(12700.0)));
    assert_eq!(tx.technical, Some(false));
    assert_eq!(
        tx.partner.as_ref().and_then(|p| p.name.as_deref()),
        Some("Kovács Bt.")
    );
    assert_eq!(tx.memo.as_deref(), Some("E-2026-123"));
    assert_eq!(tx.raw_xml(), Some(BANK_TRANSACTION));
}

#[test]
fn parses_receipt_batch() {
    let Document::Receipts(batch) = Document::parse(RECEIPT_BATCH.as_bytes()).expect("parse")
    else {
        panic!("expected receipts");
    };
    assert_eq!(batch.receipts.len(), 2);
    let first = &batch.receipts[0];
    assert_eq!(first.info.receipt_number.as_deref(), Some("NYGTA-2026-1"));
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.items[0].gross_value, Some(dec!(25400.0)));
    assert_eq!(first.info.customer_ledger.as_deref(), Some("311"));
    assert_eq!(first.info.exchange_rate, Some(dec!(1)));
    assert_eq!(first.items[0].vat_rate, Some(dec!(27)));
    assert_eq!(first.items[0].vat_type.as_deref(), Some("AAM"));
    assert_eq!(
        first.items[0].effective_vat(),
        Some(VatRate::Special("AAM"))
    );
    assert_eq!(
        first.items[0]
            .ledger
            .as_ref()
            .and_then(|ledger| ledger.revenue.as_deref()),
        Some("911")
    );
    assert_eq!(batch.raw_xml(), Some(RECEIPT_BATCH));
}

#[test]
fn accepts_receipts_without_issuer_tax_number_seen_in_official_batches() {
    let body = RECEIPT_BATCH.replacen("<adoszam>12345678-1-42</adoszam>", "", 1);
    let Document::Receipts(batch) = Document::parse(body.as_bytes()).expect("parse") else {
        panic!("expected receipts");
    };
    assert_eq!(batch.receipts[0].info.tax_number, None);
}

#[test]
fn unknown_root_is_an_error() {
    let error = Document::parse(b"<?xml version=\"1.0\"?><whatever/>").expect_err("error");
    assert!(error.to_string().contains("whatever"));
}

// `Document::parse` refuses shape only: what is not the pushed document at
// all. Content (a missing element, an unknown token, an undecodable PDF) is
// read leniently and is `parse_strict`'s concern (`tests/document.rs`).
#[test]
fn rejects_shapes_that_are_not_the_document() {
    let truncated = &OUTGOING_INVOICE[..OUTGOING_INVOICE.len() - 20];
    assert!(Document::parse(truncated).is_err());

    for root in ["szamla", "szamlabe", "banktranz", "xmlnyugtaarchiv"] {
        let body = format!(r#"<{root} xmlns="https://wrong.example"/>"#);
        assert!(Document::parse(body.as_bytes()).is_err(), "accepted {root}");
    }

    let wrong_child_namespace = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("UTF-8")
        .replacen("<szallito>", "<szallito xmlns=\"\">", 1);
    assert!(Document::parse(wrong_child_namespace.as_bytes()).is_err());

    // The identity the receiver Acks with: the id and, for an invoice, its
    // number.
    let without_id = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("UTF-8")
        .replacen("<id>123456</id>", "", 1);
    assert!(Document::parse(without_id.as_bytes()).is_err());
    let without_number = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("UTF-8")
        .replacen("<szamlaszam>2015-123</szamlaszam>", "", 1);
    assert!(Document::parse(without_number.as_bytes()).is_err());
    let receipt_without_id = RECEIPT_BATCH.replacen("<id>1</id>", "", 1);
    assert!(Document::parse(receipt_without_id.as_bytes()).is_err());
    let transaction_without_id = BANK_TRANSACTION.replacen("<id>987</id>", "", 1);
    assert!(Document::parse(transaction_without_id.as_bytes()).is_err());

    assert!(Document::parse(b"<szamla>\xff</szamla>").is_err());
    assert!(Document::parse(b"not xml at all").is_err());
}

/// Accepts everything by default; `fail` answers every invoice with an error,
/// `invalid_ack` with a registration number that cannot be rendered.
#[derive(Clone, Default)]
struct TestHandler {
    fail: bool,
    invalid_ack: bool,
}

impl Handler for TestHandler {
    type Error = String;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        if self.fail {
            return ready(Err("database down".to_owned()));
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
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

fn request(key: Option<&str>, body: &[u8]) -> Request<Body> {
    request_at("/", key, body)
}

fn request_at(path: &str, key: Option<&str>, body: &[u8]) -> Request<Body> {
    let mut builder = Request::post(path).header("content-type", "application/xml");
    if let Some(key) = key {
        builder = builder.header(KEY_HEADER, key);
    }
    builder.body(Body::from(body.to_vec())).expect("request")
}

async fn call(key: Option<&str>, body: &[u8], fail: bool) -> (StatusCode, String) {
    call_at("/", key, body, fail).await
}

async fn call_at(path: &str, key: Option<&str>, body: &[u8], fail: bool) -> (StatusCode, String) {
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
async fn send(app: Router, request: Request<Body>) -> (StatusCode, String) {
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

#[tokio::test]
async fn acks_document_with_valid_key() {
    let (status, body) = call(Some("secret-key"), OUTGOING_INVOICE, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<szamlavalasz"));
    assert!(body.contains("<iktatoszam>IKT-1</iktatoszam>"));
}

struct MismatchedAck;

impl Handler for MismatchedAck {
    type Error = String;

    fn outgoing_invoice(
        &self,
        _invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(-1)))
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

#[tokio::test]
async fn normalizes_handler_ack_to_the_pushed_invoice_id() {
    let app = szamlazz_adatkapcsolat::axum::router("secret-key", MismatchedAck);
    let response = app
        .oneshot(request(Some("secret-key"), OUTGOING_INVOICE))
        .await
        .expect("response");
    let body = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = String::from_utf8_lossy(&body);
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
    let body = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("fixture UTF-8")
        .replace("<pdf></pdf>", "<pdf>not base64!</pdf>");

    let (status, response) = call(Some("not-the-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.contains("KEY_ERR"));

    let (status, response) = call(Some("secret-key"), body.as_bytes(), false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(response.contains("<szamlavalasz"), "{response}");
    assert!(!response.contains("hibakod"), "{response}");
}

// The authenticated 400 is for a body that is not the pushed document at all:
// szamlazz.hu would retry it identically for 72 hours and then drop it, so
// it must never be the answer to a document that merely omits what the XSD
// requires: that one is Acked.
#[tokio::test]
async fn authenticated_400_is_reserved_for_a_body_that_is_not_a_document() {
    let truncated = &OUTGOING_INVOICE[..OUTGOING_INVOICE.len() - 20];
    let (status, response) = call(Some("secret-key"), truncated, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(!response.contains("valasz"), "{response}");

    let without_id = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("fixture UTF-8")
        .replacen("<id>123456</id>", "", 1);
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
    let body = std::str::from_utf8(OUTGOING_INVOICE)
        .expect("fixture UTF-8")
        .replacen("<szallito>", "<szallito xmlns=\"\">", 1);

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

#[derive(Clone)]
struct TenantHandler {
    tenant: &'static str,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl Handler for TenantHandler {
    type Error = String;

    fn outgoing_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        self.calls.lock().expect("calls").push(self.tenant);
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn incoming_invoice(
        &self,
        invoice: InvoiceDocument,
    ) -> impl Future<Output = Result<InvoiceAck, String>> + MaybeSend {
        ready(Ok(InvoiceAck::accept(invoice.info.id)))
    }

    fn bank_transaction(
        &self,
        _tx: BankTransaction,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }

    fn receipts(
        &self,
        _batch: ReceiptBatch,
    ) -> impl Future<Output = Result<Ack, String>> + MaybeSend {
        ready(Ok(Ack::accept()))
    }
}

struct Tenants {
    first: TenantHandler,
    second: TenantHandler,
}

/// Two tenants, `first-key` and `second-key`, recording which one was called.
fn tenants(calls: &Arc<Mutex<Vec<&'static str>>>) -> Tenants {
    Tenants {
        first: TenantHandler {
            tenant: "first",
            calls: calls.clone(),
        },
        second: TenantHandler {
            tenant: "second",
            calls: calls.clone(),
        },
    }
}

impl szamlazz_adatkapcsolat::axum::KeyResolver for Tenants {
    type Handler = TenantHandler;
    type Error = Infallible;

    fn resolve(
        &self,
        key: &str,
    ) -> impl Future<Output = Result<Option<&Self::Handler>, Infallible>> + MaybeSend {
        ready(Ok(match key {
            "first-key" => Some(&self.first),
            "second-key" => Some(&self.second),
            _ => None,
        }))
    }
}

/// A database-shaped resolver whose lookup is down: it cannot tell a wrong key
/// from a right one, so it must not answer either way. Written as an
/// `async fn` that awaits, the way a real lookup would.
struct UnavailableResolver;

impl szamlazz_adatkapcsolat::axum::KeyResolver for UnavailableResolver {
    type Handler = TestHandler;
    type Error = String;

    async fn resolve(&self, _key: &str) -> Result<Option<&Self::Handler>, String> {
        tokio::task::yield_now().await;
        Err("key store timed out".to_owned())
    }
}

fn incoming_invoice() -> Vec<u8> {
    std::str::from_utf8(OUTGOING_INVOICE)
        .expect("fixture UTF-8")
        .replace(
            "http://www.szamlazz.hu/szamla",
            "http://www.szamlazz.hu/szamlabe",
        )
        .replace("<szamla xmlns=", "<szamlabe xmlns=")
        .replace("</szamla>", "</szamlabe>")
        .into_bytes()
}

// KEY_ERR is "your key is wrong": szamlazz.hu never resends a bank
// transaction or receipt answered with it, and an invoice only when it next
// changes. A resolver that could not check must therefore answer a non-200
// with no Ack, so the record stays in the 72-hour retry window.
#[tokio::test]
async fn unavailable_resolver_answers_503_without_an_ack_for_every_root() {
    let incoming = incoming_invoice();
    for (name, body) in [
        ("szamla", OUTGOING_INVOICE),
        ("szamlabe", incoming.as_slice()),
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
        (OUTGOING_INVOICE, "<szamlavalasz"),
        (incoming.as_slice(), "<szamlabevalasz"),
        (BANK_TRANSACTION.as_bytes(), "<banktranzvalasz"),
        (RECEIPT_BATCH.as_bytes(), "<nyugtavalasz"),
    ] {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let app = szamlazz_adatkapcsolat::axum::router_with_resolver(tenants(&calls));
        let (status, text) = send(app, request(Some("third-key"), body)).await;
        assert_eq!(status, StatusCode::OK, "{ack_root}");
        assert!(text.contains(ack_root), "{text}");
        assert!(text.contains("<hibakod>KEY_ERR</hibakod>"), "{text}");
        assert!(calls.lock().expect("calls").is_empty());
    }
}

#[tokio::test]
async fn resolver_selects_business_context_for_each_key() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let app = szamlazz_adatkapcsolat::axum::router_with_resolver(tenants(&calls));

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
