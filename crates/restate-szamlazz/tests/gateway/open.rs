//! `Gateway::open` and `Gateway::open_with_http`: the caller-built client is
//! the transport, the account's key is on the wire, and two gateways share no
//! key and no session.

use super::common::{external_id_query, http_builder, not_found};
use super::harness::*;
use restate_szamlazz::account::{Account, Endpoint};
use restate_szamlazz::contract::Selector;
use restate_szamlazz::gateway::{Gateway, QueryOutcome};
use szamlazz_agent::{Credentials, reqwest};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer};

fn agent_key_on_the_wire(body: &str) -> Option<&str> {
    let start = body.find("<szamlaagentkulcs>")? + "<szamlaagentkulcs>".len();
    let end = body[start..].find("</szamlaagentkulcs>")? + start;
    Some(&body[start..end])
}

/// The caller-built client is the transport: what it is configured with (a
/// header here; a proxy or a TLS setup in an embedder) is on every request
/// the gateway sends. The embedder's hook of `Gateway::open_with_http`.
#[tokio::test]
async fn a_gateway_opened_over_a_caller_built_client_sends_through_it() {
    let server = MockServer::start().await;
    external_id_query("acme:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&server)
        .await;
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("x-embedder", "proxy-of-acme".parse().expect("header"));
    let http = http_builder()
        .default_headers(headers)
        .build()
        .expect("http client");
    let mut account = Account::new("acme", "acme");
    account.endpoint = Endpoint::parse(&server.uri()).expect("endpoint");
    let gateway = Gateway::open_with_http(account, Credentials::agent_key("key-acme"), http)
        .expect("gateway");

    let outcome = gateway
        .query(&Selector::ExternalId("acme:ORD-1:invoice".to_owned()))
        .await;
    assert!(matches!(outcome, Ok(QueryOutcome::NotFound)), "{outcome:?}");

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0]
            .headers
            .get("x-embedder")
            .map(|value| value.to_str().expect("ascii")),
        Some("proxy-of-acme"),
        "the request went through the caller's client"
    );
    let body = String::from_utf8_lossy(&sent[0].body);
    assert_eq!(agent_key_on_the_wire(&body), Some("key-acme"));
}

#[tokio::test]
async fn a_gateway_opened_from_an_account_sends_that_accounts_key() {
    let server = MockServer::start().await;
    external_id_query("acme:ORD-1:invoice")
        .respond_with(not_found())
        .mount(&server)
        .await;
    let gateway = open(&server, "acme", "key-acme");

    let outcome = gateway
        .query(&Selector::ExternalId("acme:ORD-1:invoice".to_owned()))
        .await;
    assert!(matches!(outcome, Ok(QueryOutcome::NotFound)), "{outcome:?}");

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 1);
    let body = String::from_utf8_lossy(&sent[0].body);
    assert_eq!(agent_key_on_the_wire(&body), Some("key-acme"));
    assert_eq!(gateway.account().id.as_str(), "acme");
}

/// Two accounts on one szamlazz.hu: each gateway's requests carry its own
/// key, and a session cookie szamlazz.hu sets for the first never travels
/// with the second: a fresh client per gateway, here the one [`open`] builds
/// per call, as the prologue builds one per execution. The first gateway's
/// second request *does* carry the cookie, proving the cookie store is live
/// and the test would catch a client shared between the two.
#[tokio::test]
async fn two_gateways_opened_from_two_accounts_share_no_key_and_no_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(not_found().insert_header("set-cookie", "JSESSIONID=session-of-acme; Path=/"))
        .mount(&server)
        .await;
    let acme = open(&server, "acme", "key-acme");
    let beta = open(&server, "beta", "key-beta");

    let selector = Selector::OrderNumber("ORD-1".to_owned());
    assert!(matches!(
        acme.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));
    assert!(matches!(
        beta.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));
    assert!(matches!(
        acme.query(&selector).await,
        Ok(QueryOutcome::NotFound)
    ));

    let sent = server.received_requests().await.expect("requests");
    assert_eq!(sent.len(), 3);
    let cookie = |i: usize| {
        sent[i]
            .headers
            .get("cookie")
            .map(|value| value.to_str().expect("ascii").to_owned())
    };
    let key = |i: usize| {
        agent_key_on_the_wire(&String::from_utf8_lossy(&sent[i].body)).map(str::to_owned)
    };

    assert_eq!(key(0).as_deref(), Some("key-acme"));
    assert_eq!(cookie(0), None, "acme's first request: no session yet");
    assert_eq!(key(1).as_deref(), Some("key-beta"));
    assert_eq!(cookie(1), None, "beta never saw acme's Set-Cookie");
    assert_eq!(key(2).as_deref(), Some("key-acme"));
    assert_eq!(
        cookie(2).as_deref(),
        Some("JSESSIONID=session-of-acme"),
        "acme's own client keeps its own session"
    );
}
