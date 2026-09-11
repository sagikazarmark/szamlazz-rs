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
    server.finish().await;
}
