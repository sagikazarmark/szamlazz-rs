//! Diagnostics cross the same privacy boundary as document projections.

use super::common::{
    Doc, api_error, create, created_without_totals, credit, delete, external_id_query,
    http_builder, not_found, number_query, storno, taxpayer_query,
};
use super::harness::*;
use restate_szamlazz::gateway::{
    DeleteOutcome, Gateway, SetCreditEntriesOutcome, Unanswered, Unconfirmed,
};
use wiremock::ResponseTemplate;

const KEY: &str = "DIAGNOSTIC-AGENT-KEY-215";
const PRIVATE: &str = "PRIVATE-BUYER-TEXT-215";

#[tokio::test]
async fn unmanaged_storno_alerts_before_fallback_can_discard_credentials() {
    let logs = Logs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();
    for verification_fails in [true, false] {
        let h = Harness::start().await;
        let id = storno_id();
        external_id_query(id.as_str())
            .respond_with(not_found())
            .up_to_n_times(1)
            .expect(1)
            .mount(&h.server)
            .await;
        external_id_query(id.as_str())
            .respond_with(if verification_fails {
                Doc {
                    referenced_invoice: Some("SZ-1"),
                    ..Doc::new("SS-1", "SS")
                }
                .response()
            } else {
                api_error("135", &format!("{KEY} {PRIVATE}"))
            })
            .expect(1)
            .mount(&h.server)
            .await;
        storno()
            .respond_with(created_without_totals("SS-1"))
            .expect(1)
            .mount(&h.server)
            .await;
        number_query("SS-1")
            .respond_with(if verification_fails {
                api_error("135", &format!("{KEY} {PRIVATE}"))
            } else {
                not_found()
            })
            .expect(1)
            .mount(&h.server)
            .await;
        let before = logs.0.lock().expect("logs").len();
        let outcome = h.gateway.storno(storno_request(&id)).await;
        if verification_fails {
            assert!(
                matches!(outcome, Ok(restate_szamlazz::gateway::StornoOutcome::AlreadyReversed { storno_number }) if storno_number == "SS-1")
            );
        } else {
            assert!(matches!(outcome, Err(Unconfirmed::ReQueryFailed { .. })));
        }
        let text =
            String::from_utf8(logs.0.lock().expect("logs")[before..].to_vec()).expect("utf8");
        assert_eq!(
            text.matches("fix the account's agent key").count(),
            1,
            "{text}"
        );
        assert!(text.contains("namespace=acct"), "{text}");
        assert!(text.contains("code=135"), "{text}");
        assert_private(&text);
    }
}

fn assert_private(text: &str) {
    for sentinel in [KEY, PRIVATE] {
        assert!(
            !text.contains(sentinel),
            "diagnostic leaked {sentinel}: {text}"
        );
    }
}

#[tokio::test]
async fn http_failure_keeps_status_and_operation_without_upstream_body() {
    let h = Harness::start().await;
    let gateway = open(&h.server, "acct", KEY);
    let response = ResponseTemplate::new(502).set_body_string(format!("{KEY} {PRIVATE}"));
    number_query("SZ-PRIVATE")
        .respond_with(response.clone())
        .mount(&h.server)
        .await;
    credit()
        .respond_with(response)
        .expect(1)
        .mount(&h.server)
        .await;

    let read = gateway
        .verify("SZ-PRIVATE")
        .await
        .expect_err("unanswered read");
    assert_private(&serde_json::to_string(&read).expect("serialize"));
    assert_private(&read.to_string());
    assert!(read.to_string().contains("HTTP 502"));
    assert!(read.to_string().contains("query"));
    let lost = gateway.set_credit_entries("SZ-PRIVATE", &[], true).await;
    assert!(matches!(
        lost,
        SetCreditEntriesOutcome::Lost(Unanswered::Transport(_))
    ));
    let serialized = serde_json::to_string(&lost).expect("serialize");
    assert_private(&serialized);
    assert!(serialized.contains("HTTP 502"));
    assert!(serialized.contains("set-credit-entries"));
}

#[tokio::test]
async fn credential_answers_discard_sensitive_messages_but_other_vendor_answers_pass_through() {
    for code in ["3", "135", "136", "164", "259"] {
        let h = Harness::start().await;
        number_query("SZ-PRIVATE")
            .respond_with(api_error(code, &format!("{KEY} {PRIVATE}")))
            .mount(&h.server)
            .await;
        let outcome = h.gateway.verify("SZ-PRIVATE").await.expect("answered");
        let text = serde_json::to_string(&outcome).expect("serialize");
        assert!(text.contains(code));
        if code == "259" {
            // Deliberate compatibility boundary: vendor business messages are data.
            assert!(text.contains(PRIVATE));
        } else {
            assert_private(&text);
            assert!(text.contains("credentials rejected"));
        }
    }
}

#[tokio::test]
async fn malformed_response_values_never_become_durable_diagnostics() {
    let private = format!("{KEY} {PRIVATE}");
    let doc = Doc::new("SZ-1", "SZ").xml();
    let bodies = [
        format!("<{KEY}>{PRIVATE}</{KEY}>"),
        format!("<szamla><{KEY}>{PRIVATE}</wrong>"),
        doc.replace("<netto>1000</netto>", &format!("<netto>{private}</netto>")),
        doc.replace(
            "<kelt>2026-09-03</kelt>",
            &format!("<kelt>{private}</kelt>"),
        ),
    ];
    for body in bodies {
        let h = Harness::start().await;
        number_query("SZ-1")
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&h.server)
            .await;
        let error = h.gateway.verify("SZ-1").await.expect_err("unanswered");
        assert_private(&serde_json::to_string(&error).expect("serialize"));
        assert_private(&format!("{error:?} {error}"));
        assert!(error.to_string().contains("query: parse:"));
    }
}

#[tokio::test]
async fn down_header_is_a_category_on_reads_leading_queries_and_one_shot_sends() {
    let h = Harness::start().await;
    let down = ResponseTemplate::new(503).insert_header("szlahu_down", format!("{KEY} {PRIVATE}"));
    external_id_query(external_id().as_str())
        .respond_with(down.clone())
        .mount(&h.server)
        .await;
    external_id_query(storno_id().as_str())
        .respond_with(down.clone())
        .mount(&h.server)
        .await;
    taxpayer_query("12345678")
        .respond_with(down.clone())
        .mount(&h.server)
        .await;
    delete().respond_with(down.clone()).mount(&h.server).await;
    credit().respond_with(down).mount(&h.server).await;
    let read = h.try_lookup(&[]).await.expect_err("unanswered");
    let taxpayer = h
        .gateway
        .query_taxpayer(&prefix())
        .await
        .expect_err("unanswered");
    let create = h.create(None).await.expect("leading answer");
    let storno = h
        .gateway
        .storno(storno_request(&storno_id()))
        .await
        .expect("leading answer");
    let delete = h.delete("D-1").await;
    let credit = h.gateway.set_credit_entries("SZ-1", &[], true).await;
    assert!(matches!(
        delete,
        DeleteOutcome::Lost(Unanswered::Unavailable(_))
    ));
    assert!(matches!(
        credit,
        SetCreditEntriesOutcome::Lost(Unanswered::Unavailable(_))
    ));
    for text in [
        serde_json::to_string(&read).expect("serialize"),
        serde_json::to_string(&taxpayer).expect("serialize"),
        serde_json::to_string(&create).expect("serialize"),
        serde_json::to_string(&storno).expect("serialize"),
        serde_json::to_string(&delete).expect("serialize"),
        serde_json::to_string(&credit).expect("serialize"),
    ] {
        assert_private(&text);
        assert!(text.contains("szlahu_down"), "{text}");
    }
}

#[tokio::test]
async fn nested_create_and_storno_failures_keep_both_categories_and_safe_logs() {
    let logs = Logs::default();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(logs.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    tracing::callsite::rebuild_interest_cache();
    for is_storno in [false, true] {
        for re_query in [
            ResponseTemplate::new(200).set_body_string(format!("<{KEY}>{PRIVATE}</{KEY}>")),
            ResponseTemplate::new(503).insert_header("szlahu_down", format!("{KEY} {PRIVATE}")),
            api_error("135", &format!("{KEY} {PRIVATE}")),
        ] {
            let h = Harness::start().await;
            let id = if is_storno {
                storno_id()
            } else {
                external_id()
            };
            external_id_query(id.as_str())
                .respond_with(not_found())
                .up_to_n_times(1)
                .mount(&h.server)
                .await;
            external_id_query(id.as_str())
                .respond_with(re_query)
                .mount(&h.server)
                .await;
            let send = if is_storno { storno() } else { create() };
            send.respond_with(
                ResponseTemplate::new(502).set_body_string(format!("{KEY} {PRIVATE}")),
            )
            .expect(1)
            .mount(&h.server)
            .await;
            let error = if is_storno {
                h.gateway
                    .storno(storno_request(&id))
                    .await
                    .expect_err("unconfirmed")
            } else {
                match h.create(None).await {
                    Err(error) => error,
                    Ok(restate_szamlazz::gateway::CreateOutcome::CredentialsRejected(answer)) => {
                        assert_eq!(answer.code, "135");
                        assert!(answer.message.contains("create: HTTP 502"), "{answer:?}");
                        assert!(
                            answer
                                .message
                                .contains("credentials rejected: browser session active")
                        );
                        assert_private(&serde_json::to_string(&answer).expect("serialize"));
                        continue;
                    }
                    other => panic!("expected uncertainty or credential fault: {other:?}"),
                }
            };
            if let Unconfirmed::ReQueryFailed { sent, re_query } = &error {
                assert!(sent.contains("HTTP 502"), "{sent}");
                assert!(sent.contains(if is_storno { "storno" } else { "create" }));
                assert!(
                    re_query.contains("parse:")
                        || re_query.contains("szlahu_down")
                        || re_query.contains("credentials rejected"),
                    "{re_query}"
                );
            } else {
                panic!("expected both causes: {error:?}");
            }
            assert_private(&serde_json::to_string(&error.to_string()).expect("serialize failure"));
            assert_private(&format!("{error:?}"));
        }
    }
    let text = String::from_utf8(logs.0.lock().expect("logs").clone()).expect("utf8");
    assert!(
        text.contains("re-querying"),
        "positive logging control: {text}"
    );
    assert_private(&text);
}

#[tokio::test]
async fn transport_failure_drops_sensitive_url_and_source_but_keeps_category() {
    use restate_szamlazz::account::{Account, Endpoint};
    use szamlazz_agent::Credentials;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("port");
    let address = listener.local_addr().expect("address");
    drop(listener);
    let mut account = Account::new("acct", "acct");
    account.endpoint =
        Endpoint::parse(&format!("http://{address}/{KEY}?buyer={PRIVATE}")).expect("endpoint");
    let gateway = Gateway::open_with_http(
        account,
        Credentials::agent_key(KEY),
        http_builder().no_proxy().build().expect("http"),
    )
    .expect("gateway");
    let error = gateway
        .verify("SZ-1")
        .await
        .expect_err("connection refused");
    assert_private(&serde_json::to_string(&error).expect("serialize"));
    assert_private(&format!("{error:?} {error}"));
    assert!(error.to_string().contains("query: transport: connection"));
}

#[derive(Clone, Default)]
struct Logs(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Logs {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("logs").extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Logs {
    type Writer = Self;
    fn make_writer(&'a self) -> Self {
        self.clone()
    }
}
