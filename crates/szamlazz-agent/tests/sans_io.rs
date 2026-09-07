//! The sans-IO boundary driven by an HTTP client this crate knows nothing
//! about: `to_wire` yields what any client needs to send, and `RawResponse`
//! takes what any client returns. The blocking `ureq` stands in for "any
//! client"; the same round trip is the README's sans-IO example.

use szamlazz_agent::Credentials;
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use wiremock::matchers::{body_string_contains, header_regex, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn a_blocking_client_drives_the_sans_io_core_end_to_end() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/szamla/"))
        .and(header_regex(
            "content-type",
            r"^multipart/form-data; boundary=.+",
        ))
        .and(body_string_contains(
            "name=\"action-szamla_agent_taxpayer\"",
        ))
        .and(body_string_contains(
            "<szamlaagentkulcs>key</szamlaagentkulcs>",
        ))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("set-cookie", "JSESSIONID=ABC123; Path=/; HttpOnly")
                .set_body_raw(
                    include_bytes!("synthetic/taxpayer.xml").to_vec(),
                    "application/xml",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;

    let request = QueryTaxpayer::new("12345678").expect("prefix");
    let wire = request
        .to_wire(&Credentials::agent_key("key"))
        .expect("wire");
    let endpoint = format!("{}/szamla/", server.uri());

    let raw = tokio::task::spawn_blocking(move || {
        let mut response = ureq::post(&endpoint)
            .content_type(&wire.content_type)
            .send(&wire.body[..])
            .expect("http");
        let status = response.status().as_u16();
        let body = response.body_mut().read_to_vec().expect("body");
        let headers = response.headers().iter().map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        });
        RawResponse::new(headers, body).with_status(status)
    })
    .await
    .expect("blocking task");

    assert_eq!(raw.status(), Some(200));
    let taxpayer = request.parse(&raw).expect("parse");
    assert!(taxpayer.valid);
    assert_eq!(taxpayer.name.as_deref(), Some("SYNTHETIC SOFTWARE KFT."));
    assert_eq!(taxpayer.tax_number.as_deref(), Some("12345678"));
    assert_eq!(
        raw.session_cookie().as_deref(),
        Some("JSESSIONID=ABC123"),
        "the cookie a sans-IO caller replays as `Cookie` on its next request"
    );
}
