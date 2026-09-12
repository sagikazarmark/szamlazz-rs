use super::*;
use futures_util::FutureExt;
use std::panic::AssertUnwindSafe;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn document(number: &str, kind: &str, extra: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(format!(
        "<szamla xmlns=\"http://www.szamlazz.hu/szamla\">\
         <szallito><nev>Seller</nev><cim><irsz>1</irsz><telepules>B</telepules><cim>C</cim></cim></szallito>\
         <alap><id>1</id><szamlaszam>{number}</szamlaszam><tipus>{kind}</tipus><telj>2026-08-31</telj><eszamla>1</eszamla>{extra}</alap>\
         <vevo><nev>Buyer</nev></vevo><tetelek></tetelek>\
         <osszegek><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek></szamla>"
    ), "application/xml")
}

fn candidate(number: &str) -> ResponseTemplate {
    document(number, "SS", "<hivszamlaszam>FINAL</hivszamlaszam>")
}

fn acknowledged() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(
        "<xmlszamlavalasz xmlns=\"http://www.szamlazz.hu/xmlszamlavalasz\"><sikeres>true</sikeres><szamlaszam>STORNO</szamlaszam></xmlszamlavalasz>",
        "application/xml",
    )
}

async fn scripted(replies: Vec<ResponseTemplate>) -> (MockServer, Run) {
    let server = MockServer::start().await;
    let index = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .respond_with(move |_: &wiremock::Request| {
            replies
                .get(index.fetch_add(1, Ordering::SeqCst))
                .cloned()
                .unwrap_or_else(|| ResponseTemplate::new(500))
        })
        .mount(&server)
        .await;
    let http = szamlazz_agent::reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .build()
        .expect("local HTTP client");
    let run = Run {
        client: Client::builder()
            .credentials(Credentials::agent_key("synthetic-key"))
            .endpoint(server.uri())
            .http_client(http)
            .build()
            .expect("client"),
        order: "local-cleanup".into(),
        // The earlier proforma is deliberately dependent on final cleanup.
        known: vec![InvoiceNumber::new("PROFORMA"), InvoiceNumber::new("FINAL")],
        unresolved: None,
    };
    (server, run)
}

async fn assert_requests(server: &MockServer, expected: &[(&str, &str)]) {
    let requests = server.received_requests().await.expect("requests");
    assert_eq!(
        requests.len(),
        expected.len(),
        "no resend or dependent cleanup on uncertainty"
    );
    for (request, (operation, number)) in requests.iter().zip(expected) {
        let body = std::str::from_utf8(&request.body).expect("multipart UTF-8");
        assert!(body.contains(operation), "{body}");
        assert!(
            body.contains(&format!("<szamlaszam>{number}</szamlaszam>")),
            "{body}"
        );
    }
}

const QUERY: &str = "name=\"action-szamla_agent_xml\"";
const STORNO: &str = "name=\"action-szamla_agent_st\"";

#[tokio::test]
async fn paired_cleanup_blocks_dependents_without_exact_fresh_reversal() {
    for original in [
        document("FINAL", "VS", "<sztornozott>false</sztornozott>"),
        document("FINAL", "VS", ""),
        document("OTHER", "VS", "<sztornozott>true</sztornozott>"),
        ResponseTemplate::new(200).insert_header("szlahu_error_code", "7"),
        ResponseTemplate::new(503),
    ] {
        let (server, run) = scripted(vec![
            document("FINAL", "VS", ""),
            acknowledged(),
            candidate("STORNO"),
            original,
        ])
        .await;
        assert!(
            AssertUnwindSafe(run.finish(Ok(())))
                .catch_unwind()
                .await
                .is_err()
        );
        assert_requests(
            &server,
            &[
                (QUERY, "FINAL"),
                (STORNO, "FINAL"),
                (QUERY, "STORNO"),
                (QUERY, "FINAL"),
            ],
        )
        .await;
    }
    // A different storno referencing the right original is not this candidate.
    let (server, run) = scripted(vec![
        document("FINAL", "VS", ""),
        acknowledged(),
        candidate("OTHER-STORNO"),
    ])
    .await;
    assert!(
        AssertUnwindSafe(run.finish(Ok(())))
            .catch_unwind()
            .await
            .is_err()
    );
    assert_requests(
        &server,
        &[(QUERY, "FINAL"), (STORNO, "FINAL"), (QUERY, "STORNO")],
    )
    .await;
}

#[tokio::test]
async fn paired_cleanup_unlocks_dependents_only_after_fresh_original() {
    let (server, run) = scripted(vec![
        document("FINAL", "VS", ""), acknowledged(), candidate("STORNO"),
        document("FINAL", "VS", "<sztornozott>true</sztornozott>"),
        document("PROFORMA", "D", ""),
        ResponseTemplate::new(200).set_body_raw(
            "<xmlszamladbkdelvalasz xmlns=\"http://www.szamlazz.hu/xmlszamladbkdelvalasz\"><sikeres>true</sikeres></xmlszamladbkdelvalasz>", "application/xml"),
    ]).await;
    run.finish(Ok(())).await;
    assert_requests(
        &server,
        &[
            (QUERY, "FINAL"),
            (STORNO, "FINAL"),
            (QUERY, "STORNO"),
            (QUERY, "FINAL"),
            (QUERY, "PROFORMA"),
            ("name=\"action-szamla_agent_dijbekero_torlese\"", "PROFORMA"),
        ],
    )
    .await;
}

#[tokio::test]
async fn unverified_direct_reversal_retains_intent_and_never_resends_in_cleanup() {
    let (server, mut run) = scripted(vec![
        document("FINAL", "VS", ""),
        acknowledged(),
        candidate("STORNO"),
        document("FINAL", "VS", "<sztornozott>false</sztornozott>"),
    ])
    .await;
    let result =
        AssertUnwindSafe(run.reverse(&InvoiceNumber::new("FINAL"), false, "exact-storno".into()))
            .catch_unwind()
            .await;
    assert!(result.is_err());
    assert!(
        run.unresolved
            .as_ref()
            .is_some_and(|intent| intent.contains("exact-storno"))
    );
    assert!(
        AssertUnwindSafe(run.finish(Ok(())))
            .catch_unwind()
            .await
            .is_err()
    );
    assert_requests(
        &server,
        &[
            (QUERY, "FINAL"),
            (STORNO, "FINAL"),
            (QUERY, "STORNO"),
            (QUERY, "FINAL"),
        ],
    )
    .await;
}

#[test]
fn budapest_timezone_handles_winter_and_summer() {
    let zone = jiff::tz::TimeZone::get("Europe/Budapest").expect("dev timezone backend");
    for (timestamp, date) in [
        ("2026-01-01T22:30:00Z", "2026-01-01"),
        ("2026-07-01T22:30:00Z", "2026-07-02"),
    ] {
        assert_eq!(
            timestamp
                .parse::<jiff::Timestamp>()
                .expect("timestamp")
                .to_zoned(zone.clone())
                .date(),
            date.parse::<Date>().expect("date")
        );
    }
}
