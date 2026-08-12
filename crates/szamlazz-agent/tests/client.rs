//! Integration tests for the reqwest convenience client against a mock
//! server. The wire logic itself is covered by the sans-IO unit tests; these
//! verify the HTTP shell: multipart shape, header capture, error mapping.

#![cfg(feature = "client-reqwest")]

use jiff::civil::date;
use rust_decimal::dec;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::{
    Client, ClientError, Credentials, Currency, Language, LineItem, PaymentMethod, VatRate,
};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn sample_invoice() -> CreateInvoice {
    CreateInvoice::new(
        InvoiceKind::invoice(),
        InvoiceHeader::new(
            date(2026, 7, 4),
            date(2026, 7, 12),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23."),
        vec![LineItem::calculated_for_currency(
            "Fejlesztés",
            dec!(1),
            "db",
            dec!(10000),
            VatRate::percent(27),
            &Currency::HUF,
        )],
    )
}

#[tokio::test]
async fn sends_multipart_and_parses_success() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .and(header_regex(
            "content-type",
            r"multipart/form-data; boundary=.+",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            include_bytes!("synthetic/xmlszamlavalasz.xml").to_vec(),
            "application/xml",
        ))
        .expect(1)
        .mount(&server)
        .await;

    let client = Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint(server.uri())
        .build()
        .expect("client");

    let created = client.send(&sample_invoice()).await.expect("success");
    assert_eq!(
        created
            .invoice_number
            .as_ref()
            .map(szamlazz_agent::InvoiceNumber::as_str),
        Some("E-TST-2026-3")
    );
    assert_eq!(created.gross_total, Some(dec!(38100)));

    // The multipart body must carry the operation-selecting field name.
    let requests = server.received_requests().await.expect("requests");
    let body = String::from_utf8_lossy(&requests[0].body);
    assert!(body.contains("name=\"action-xmlagentxmlfile\""));
    assert!(body.contains("<xmlszamla xmlns=\"http://www.szamlazz.hu/xmlszamla\">"));
}

#[tokio::test]
async fn maps_header_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("szlahu_error_code", "3")
                .insert_header("szlahu_error", "Sikertelen+bejelentkez%C3%A9s"),
        )
        .mount(&server)
        .await;

    let client = Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint(server.uri())
        .build()
        .expect("client");

    let error = client.send(&sample_invoice()).await.expect_err("error");
    match error {
        ClientError::Api(api) => {
            assert_eq!(api.code, szamlazz_agent::ErrorCode::InvalidCredentials);
            assert_eq!(api.message, "Sikertelen bejelentkezés");
        }
        other => panic!("expected api error, got {other:?}"),
    }
}

#[tokio::test]
async fn maps_system_unavailability() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"))
        .mount(&server)
        .await;

    let client = Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint(server.uri())
        .build()
        .expect("client");

    assert!(matches!(
        client.send(&sample_invoice()).await,
        Err(ClientError::ServiceUnavailable(message)) if message == "maintenance"
    ));
}

#[tokio::test]
async fn maps_transport_errors() {
    let client = Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint("not a valid URL")
        .build()
        .expect("client");

    let error = client.send(&sample_invoice()).await.expect_err("error");
    assert!(matches!(error, ClientError::Transport(_)));
}

#[tokio::test]
async fn rejects_invalid_request_before_http() {
    let server = MockServer::start().await;
    let client = Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint(server.uri())
        .build()
        .expect("client");
    let mut request = sample_invoice();
    request.items.clear();

    let error = client.send(&request).await.expect_err("invalid request");
    assert!(matches!(
        error,
        ClientError::Request(szamlazz_agent::RequestError::MissingLineItems)
    ));
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}
