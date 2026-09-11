//! Observable replay filtering through a real endpoint and external operation.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use restate_e2e_harness::{
    Call, ServerSpec,
    gate::{PROTOCOL_V7, ReusePolicy, VQUEUES, launcher_or_skip},
};
use restate_sdk::prelude::*;
use serde_json::json;
use tracing_subscriber::{EnvFilter, Layer as _, layer::SubscriberExt as _};

use super::*;
use crate::account::{StaticConfig, StaticResolver};
use crate::contract::{QueryRequest, QueryResponse, TerminalCode};
use crate::test_support::{LogCapture, api_error};

struct LoggingProbe {
    parts: Parts,
    executions: Arc<AtomicU32>,
    gates: Arc<AtomicU32>,
}

#[allow(missing_docs, reason = "test-only SDK-generated clients")]
#[restate_sdk::service]
impl LoggingProbe {
    #[handler(journal_retention = "1d")]
    async fn probe(&self, ctx: Context<'_>) -> Result<Json<QueryResponse>, HandlerError> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        let ctx = &ctx;
        execute(ctx, &self.parts, |exec| async move {
            tracing::info!("before completed probe");
            exec.check_account_request(ctx).await?;
            let gates = Arc::clone(&self.gates);
            // A run retry forces the handler to replay namespace/account/probe.
            // Ingress deduplication of a completed invocation would not do so.
            super::super::support::run_reading(ctx, "gate", &exec, move |gateway| async move {
                if gates.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(crate::gateway::Unanswered::Unavailable("replay now".into()));
                }
                tracing::info!("fresh external operation");
                gateway
                    .probe(&crate::identity::ExternalId::for_probe(
                        &"fresh".parse().expect("namespace"),
                    ))
                    .await
            })
            .await?;
            // A real worker operation, including execution-local credentials,
            // Gateway traffic and the paging warning after a credential code.
            let request: QueryRequest = serde_json::from_value(json!({
                "selector": {"invoice_number": "LOG-1"}
            }))
            .expect("query request");
            exec.query_request(ctx, request).await.map(Json)
        })
        .await
    }
}

#[tokio::test]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN"]
#[allow(
    clippy::too_many_lines,
    reason = "one real replay and its observable log assertions"
)]
async fn e2e_replay_filter_keeps_fresh_operation_logs_and_correlation() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    // Like an endpoint host, subscribe process-wide: a parallel ignored test
    // can otherwise register shared SDK span callsites with no subscriber and
    // cache them as disabled. Unique markers distinguish this invocation.
    // Same layer/filter order as the recommended subscriber.
    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(capture.clone())
            .with_filter(EnvFilter::new("info,restate_szamlazz=debug"))
            .with_filter(restate_sdk::filter::ReplayAwareFilter),
    );
    tracing::subscriber::set_global_default(subscriber).expect("one logging e2e subscriber");
    let server = launcher
        .launch(&ServerSpec {
            name: "replay-logging",
            features: &[(PROTOCOL_V7, true), (VQUEUES, true)],
            env: &[],
        })
        .await;
    let mock = wiremock::MockServer::start().await;
    for (selector, code) in [
        ("logs:check-account", "7"),
        ("fresh:check-account", "42"),
        ("LOG-1", "135"),
    ] {
        wiremock::Mock::given(wiremock::matchers::body_string_contains(selector))
            .respond_with(api_error(code, "scripted answer"))
            .expect(1)
            .mount(&mock)
            .await;
    }
    let config: StaticConfig = serde_json::from_value(json!({"accounts": {"acme": {
        "id": "logging-account", "agent_key": "logging-test-key", "endpoint": mock.uri()
    }}}))
    .expect("config");
    let resolver = Arc::new(StaticResolver::try_from(config).expect("resolver"));
    let executions = Arc::new(AtomicU32::new(0));
    let gates = Arc::new(AtomicU32::new(0));
    server
        .deploy(
            restate_sdk::prelude::Endpoint::builder()
                .bind(LoggingProbe {
                    parts: Parts {
                        accounts: Accounts::new(resolver.clone(), resolver),
                        config: WorkerConfig::new("logs".parse().expect("namespace"))
                            .validate()
                            .expect("config"),
                    },
                    executions: executions.clone(),
                    gates: gates.clone(),
                })
                .build(),
        )
        .await;
    let reply = server
        .invoke(
            &Call::service("LoggingProbe", "probe").scoped("acme"),
            None,
            None,
        )
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert_eq!(
        reply.fault::<crate::contract::Fault>().code,
        TerminalCode::CredentialsRejected
    );
    assert_eq!(
        executions.load(Ordering::SeqCst),
        2,
        "actual handler replay"
    );
    assert_eq!(gates.load(Ordering::SeqCst), 2);
    mock.verify().await;
    let logs = capture.logs();
    assert_eq!(
        logs.lines()
            .filter(|line| line.contains("before completed probe"))
            .count(),
        1,
        "the replay's duplicate event must be suppressed: {logs}"
    );
    for message in [
        "fresh external operation",
        "the probe was answered with a non-credential code",
        "szamlazz.hu rejected the agent credentials",
    ] {
        let events: Vec<_> = logs.lines().filter(|line| line.contains(message)).collect();
        assert_eq!(
            events.len(),
            1,
            "fresh event must survive: {message}: {logs}"
        );
        for field in [
            "scope=acme",
            "account.id=logging-account",
            reply.invocation_id(),
        ] {
            assert!(events[0].contains(field), "missing {field}: {}", events[0]);
        }
    }
    reconciliation_alert_survives_resume(&server, &capture).await;
    server.finish().await;
}

/// A production Order loses its reply, pauses on a credential rejection, then
/// resumes read-only. Fresh reconciliation warnings must survive replay filtering.
#[allow(
    clippy::too_many_lines,
    reason = "one production pause/resume lifecycle and its correlated log assertions"
)]
async fn reconciliation_alert_survives_resume(
    server: &restate_e2e_harness::Restate,
    capture: &LogCapture,
) {
    use crate::test_support::Doc;
    use restate_sdk::service::IntoServiceDefinition as _;
    use std::sync::atomic::AtomicBool;
    use wiremock::{Mock, ResponseTemplate, matchers::body_string_contains};

    let mock = wiremock::MockServer::start().await;
    let sent = Arc::new(AtomicBool::new(false));
    let visible = Arc::new(AtomicBool::new(false));
    let sent_query = sent.clone();
    let visible_query = visible.clone();
    Mock::given(body_string_contains("action-szamla_agent_xml"))
        .respond_with(move |request: &wiremock::Request| {
            if String::from_utf8_lossy(&request.body).contains("logs:RECONCILE-LOG:invoice")
                && sent_query.load(Ordering::SeqCst)
            {
                if visible_query.load(Ordering::SeqCst) {
                    Doc::of("SZ-LOG", "SZ", "RECONCILE-LOG").response()
                } else {
                    api_error("135", "PRIVATE-CREDENTIAL-ANSWER")
                }
            } else {
                api_error("7", "not found")
            }
        })
        .mount(&mock)
        .await;
    Mock::given(body_string_contains("action-xmlagentxmlfile"))
        .respond_with(move |_: &wiremock::Request| {
            sent.store(true, Ordering::SeqCst);
            ResponseTemplate::new(500)
        })
        .expect(1)
        .mount(&mock)
        .await;
    let config: StaticConfig = serde_json::from_value(json!({"account": {
        "id":"recovery-log-account", "agent_key":"PRIVATE-KEY", "endpoint":mock.uri()
    }}))
    .expect("config");
    let order = crate::Order::from_parts(
        Accounts::from(StaticResolver::try_from(config).expect("resolver")),
        WorkerConfig::new("logs".parse().expect("namespace"))
            .validate()
            .expect("config"),
    );
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_max_attempts(1)
            .retry_policy_pause_on_max_attempts(),
    );
    server
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .build(),
        )
        .await;
    let body = json!({"document": {
        "buyer":{"name":"Buyer", "zip":"1000", "city":"City", "address":"Address"},
        "items":[{"name":"Item", "quantity":"1", "unit":"db", "unit_price":"1000", "vat_rate":"27"}],
        "fulfillment_date":"2026-09-11", "due_date":"2026-09-11", "payment_method":"transfer"
    }});
    let call = Call::object("Szamlazz.Order", "RECONCILE-LOG", "create_invoice");
    let owner = server
        .invoke(&call.send(), Some(&body), Some("reconcile-log"))
        .await;
    server
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    for expected in [1, 2] {
        if expected == 2 {
            server.admin().resume(owner.invocation_id()).await;
            server
                .admin()
                .await_status(owner.invocation_id(), &["paused"])
                .await;
        }
        let logs = capture.logs();
        let alerts: Vec<_> = logs
            .lines()
            .filter(|line| {
                line.contains(owner.invocation_id()) && line.contains("fix the account's agent key")
            })
            .collect();
        assert_eq!(alerts.len(), expected, "{logs}");
        for alert in alerts {
            for field in [
                "code=135",
                "namespace=logs",
                "order=RECONCILE-LOG",
                "account.id=recovery-log-account",
            ] {
                assert!(alert.contains(field), "missing {field}: {alert}");
            }
            assert!(!alert.contains("PRIVATE-"));
        }
    }
    visible.store(true, Ordering::SeqCst);
    server.admin().resume(owner.invocation_id()).await;
    let completed = server
        .invoke(&call, Some(&body), Some("reconcile-log"))
        .await;
    assert_eq!(completed.status, 200, "{}", completed.body);
    assert_eq!(completed.body["outcome"], "reconciled");
    mock.verify().await;
}
