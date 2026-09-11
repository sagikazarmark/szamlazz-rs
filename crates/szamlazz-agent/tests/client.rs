//! Integration tests for the reqwest convenience client against a mock
//! server. The wire logic itself is covered by the `wire` unit tests; these
//! verify the HTTP shell: multipart shape, header capture, error mapping.

#![cfg(feature = "client-reqwest")]

use jiff::civil::date;
use rust_decimal::dec;
use szamlazz_agent::client::REQUEST_TIMEOUT;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::{
    Client, ClientError, Credentials, Currency, Language, LineItem, OutcomeClass, PaymentMethod,
    Rounding, VatRate, reqwest,
};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A client for `endpoint` with the test key, over the default client's
/// settings (a cookie jar, `REQUEST_TIMEOUT`, no redirects) with **no root
/// certificates**: building the default client parses the system CA store,
/// which a test whose every endpoint is plain `http://` needs nothing of
/// (#136). `Client::send` is what these tests are about; the default client's
/// construction is the live tests' to exercise.
fn client_for(endpoint: impl Into<String>) -> Client {
    let http = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .cookie_store(true)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("http client");
    Client::builder()
        .credentials(Credentials::agent_key("key"))
        .endpoint(endpoint)
        .http_client(http)
        .build()
        .expect("client")
}

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
        vec![
            LineItem::try_calculated(
                "Fejlesztés",
                dec!(1),
                "db",
                dec!(10000),
                VatRate::percent(27),
                Rounding::minor_unit(&Currency::HUF),
            )
            .expect("fits"),
        ],
    )
}

/// Intentional empty replacement uses the normal checked transport; leaving a
/// registration unfinished still fails before sending anything.
#[tokio::test]
async fn clears_credit_entries_only_through_explicit_request() {
    use szamlazz_agent::ops::credit_entry::{ClearCreditEntries, RegisterCreditEntry};
    use szamlazz_agent::wire::AgentRequest;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I-1</szamlaszam><szamlabrutto>1270</szamlabrutto><kintlevoseg>1270</kintlevoseg></xmlszamlavalasz>"#,
            "application/xml",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let client = client_for(server.uri());
    assert!(matches!(
        client.send(&RegisterCreditEntry::new("I-1")).await,
        Err(ClientError::Request(
            szamlazz_agent::RequestError::EmptyCreditEntryReplace
        ))
    ));
    let request = ClearCreditEntries {
        issuer_tax_number: Some("12345678-1-13".into()),
        aggregator: Some("shop".into()),
        ..ClearCreditEntries::new("I-1")
    };
    let wire = request
        .to_wire(&Credentials::agent_key("key"))
        .expect("checked clear");
    let body = String::from_utf8(wire.body).expect("UTF-8");
    assert!(body.contains("name=\"action-szamla_agent_kifiz\""));
    assert!(body.contains("<szamlaszam>I-1</szamlaszam><adoszam>12345678-1-13</adoszam><additiv>false</additiv><aggregator>shop</aggregator><valaszVerzio>2</valaszVerzio>"));
    assert!(!body.contains("<kifizetes>"));
    let balance = client.send(&request).await.expect("acknowledged");
    assert_eq!(balance.invoice_number.as_str(), "I-1");
    assert_eq!(balance.outstanding, Some(dec!(1270)));
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].body, body.as_bytes());
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

    let client = client_for(server.uri());

    let created = client
        .send(&sample_invoice())
        .await
        .expect("success")
        .into_issued()
        .expect("an issued document");
    assert_eq!(created.invoice_number.as_str(), "E-TST-2026-3");
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

    let client = client_for(server.uri());

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

    let client = client_for(server.uri());

    assert!(matches!(
        client.send(&sample_invoice()).await,
        Err(ClientError::ServiceUnavailable(message)) if message == "maintenance"
    ));
}

/// Native loopback evidence through the configured transport hook: capture,
/// clone/reuse, fresh jars and same-origin isolation. This exercises neither
/// vendor account selection, browser CORS nor production root-store loading.
#[tokio::test]
async fn injected_cookie_jars_are_reused_by_clones_and_isolated_when_fresh() {
    use std::sync::Arc;
    use szamlazz_agent::ops::taxpayer::QueryTaxpayer;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(|request: &wiremock::Request| {
            let body = String::from_utf8_lossy(&request.body);
            let session = if body.contains("<szamlaagentkulcs>account-a</szamlaagentkulcs>") {
                "JSESSIONID=session-a; Path=/; HttpOnly"
            } else {
                "JSESSIONID=session-b; Path=/; HttpOnly"
            };
            ResponseTemplate::new(200)
                .insert_header("set-cookie", session)
                .set_body_raw(
                    include_bytes!("synthetic/taxpayer.xml").to_vec(),
                    "application/xml",
                )
        })
        .expect(6)
        .mount(&server)
        .await;

    let open = |key| {
        let http = reqwest::Client::builder()
            .tls_certs_only(std::iter::empty())
            .cookie_provider(Arc::new(reqwest::cookie::Jar::default()))
            .timeout(REQUEST_TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("fresh HTTP client and jar");
        Client::builder()
            .credentials(Credentials::agent_key(key))
            .endpoint(server.uri())
            .http_client(http)
            .build()
            .expect("client")
    };
    let a = open("account-a");
    let b = open("account-b");
    let query = QueryTaxpayer::new("12345678").expect("prefix");
    a.send(&query).await.expect("capture a");
    a.clone().send(&query).await.expect("clone reuses a");
    b.send(&query).await.expect("b starts without a");
    b.send(&query).await.expect("reuse b");
    open("account-a")
        .send(&query)
        .await
        .expect("refresh a with a fresh jar");
    a.send(&query)
        .await
        .expect("original still has its session");

    let requests = server.received_requests().await.expect("requests");
    let cookies: Vec<_> = requests
        .iter()
        .map(|request| {
            request
                .headers
                .get("cookie")
                .map(|value| value.to_str().expect("cookie"))
        })
        .collect();
    assert_eq!(
        cookies,
        [
            None,
            Some("JSESSIONID=session-a"),
            None,
            Some("JSESSIONID=session-b"),
            None,
            Some("JSESSIONID=session-a")
        ]
    );
}

/// A URL nothing listens on is a transport error on send; a string that is
/// not a URL never gets that far (`BuildError::InvalidEndpoint` at build).
#[tokio::test]
async fn maps_transport_errors() {
    let client = client_for("http://127.0.0.1:1/");

    let error = client.send(&sample_invoice()).await.expect_err("error");
    assert!(matches!(error, ClientError::Transport(_)), "{error:?}");
    assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
}

#[tokio::test]
async fn interrupted_response_retains_headers_without_promoting_them_to_a_verdict() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    for vendor_headers in [
        "szlahu_error_code: 3\r\nszlahu_error: login\r\n",
        "szlahu_down: maintenance\r\n",
        "szlahu_error_code: 56\r\nszlahu_szamlaszam: I-2\r\n",
        "szlahu_szamlaszam: I-2\r\n",
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let endpoint = format!("http://{}/", listener.local_addr().expect("address"));
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("timeout");
            let mut request = Vec::new();
            let mut buffer = [0; 4096];
            loop {
                let read = stream.read(&mut buffer).expect("request");
                assert_ne!(read, 0, "incomplete request");
                request.extend_from_slice(&buffer[..read]);
                if let Some(end) = request.windows(4).position(|s| s == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().expect("length"))
                        })
                        .expect("content length");
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\nConnection: close\r\n{vendor_headers}Set-Cookie: JSESSIONID=COOKIE_SECRET\r\nSet-Cookie: other=SECOND_SECRET\r\n\r\nx").expect("partial reply");
        });
        let error = client_for(endpoint)
            .send(&sample_invoice())
            .await
            .expect_err("incomplete body");
        server.join().expect("server");
        assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
        let diagnostic = format!("{error:?} {error}");
        assert!(!diagnostic.contains("COOKIE_SECRET"));
        assert!(!diagnostic.contains("SECOND_SECRET"));
        let ClientError::IncompleteResponse(received) = &error else {
            panic!("missing received evidence: {error:?}");
        };
        assert_eq!(received.status, 200);
        assert_eq!(received.headers.get_all("set-cookie").iter().count(), 2);
        for line in vendor_headers.lines() {
            let (name, value) = line.split_once(':').expect("header");
            assert_eq!(received.headers[name], value.trim());
        }
        assert!(std::error::Error::source(&error).is_some());
    }
}

#[tokio::test]
async fn rejects_invalid_request_before_http() {
    let server = MockServer::start().await;
    let client = client_for(server.uri());
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

/// Every failure the client can produce, classified by what it says about the
/// document: a failure that never reached szamlazz.hu or that it refused is
/// `Rejected`; one after which it may have issued the document (a lost
/// exchange, `szlahu_down`, a body the crate cannot read, an open code) is
/// `Unknown`.
#[tokio::test]
async fn classifies_every_failure_by_outcome() {
    async fn send_to(response: ResponseTemplate) -> ClientError {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(response)
            .mount(&server)
            .await;
        client_for(server.uri())
            .send(&sample_invoice())
            .await
            .expect_err("error")
    }

    let transport = client_for("http://127.0.0.1:1/")
        .send(&sample_invoice())
        .await
        .expect_err("error");
    assert!(matches!(transport, ClientError::Transport(_)));
    assert_eq!(transport.outcome_class(), OutcomeClass::Unknown);

    let down =
        send_to(ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance")).await;
    assert!(matches!(down, ClientError::ServiceUnavailable(_)));
    assert_eq!(down.outcome_class(), OutcomeClass::Unknown);

    let parse =
        send_to(ResponseTemplate::new(200).set_body_raw(b"<not xml".to_vec(), "text/plain")).await;
    assert!(matches!(parse, ClientError::Parse(_)), "{parse:?}");
    assert_eq!(parse.outcome_class(), OutcomeClass::Unknown);

    // The client hands the status to the response: a proxy's 502 page with no
    // szamlazz.hu header is refused by status, not read as an odd body.
    let proxy = send_to(
        ResponseTemplate::new(502).set_body_raw(b"<html>Bad Gateway</html>".to_vec(), "text/html"),
    )
    .await;
    assert!(
        matches!(proxy, ClientError::HttpStatus { status: 502, .. }),
        "{proxy:?}"
    );
    assert_eq!(proxy.outcome_class(), OutcomeClass::Unknown);

    let open = send_to(
        ResponseTemplate::new(200)
            .insert_header("szlahu_error_code", "55")
            .insert_header("szlahu_error", "signing"),
    )
    .await;
    assert!(matches!(open, ClientError::Api(_)));
    assert_eq!(open.outcome_class(), OutcomeClass::Unknown);

    let refused = send_to(
        ResponseTemplate::new(200)
            .insert_header("szlahu_error_code", "259")
            .insert_header("szlahu_error", "net"),
    )
    .await;
    assert!(matches!(refused, ClientError::Api(_)));
    assert_eq!(refused.outcome_class(), OutcomeClass::Rejected);

    let server = MockServer::start().await;
    let mut request = sample_invoice();
    request.items.clear();
    let never_sent = client_for(server.uri())
        .send(&request)
        .await
        .expect_err("invalid request");
    assert!(matches!(never_sent, ClientError::Request(_)));
    assert_eq!(never_sent.outcome_class(), OutcomeClass::Rejected);
}
