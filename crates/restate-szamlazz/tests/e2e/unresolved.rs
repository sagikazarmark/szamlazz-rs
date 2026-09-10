//! #205 investigation: delayed visibility is a scripted assumption, not vendor evidence.
//! A separate server keeps the test-only protection probe out of the Order step table.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use restate_e2e_harness::{Call, Restate, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::*;
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::contract::{Fault, TerminalCode};
use rust_decimal::dec;
use serde_json::json;
use tokio::sync::Notify;
use wiremock::{MockServer, ResponseTemplate};

use crate::common::{
    Doc, create_for, created, external_id_query, not_found, number_query, order_query,
};
use crate::harness::accounts::services_with_config;
use crate::harness::{MAIN_SERVER, create_body};

const SERVER: ServerSpec = ServerSpec {
    name: "unresolved",
    ..MAIN_SERVER
};

/// The first send is accepted into the script's pending work, but its reply is
/// lost (HTTP 500) and its document stays invisible until `visible` is released.
/// Later sends succeed independently: deliberately no simulated vendor dedupe.
struct PendingSend {
    visible: Arc<AtomicBool>,
    sends: Arc<AtomicUsize>,
    received: Arc<Notify>,
}

impl PendingSend {
    async fn mount(mock: &MockServer, order: &'static str, kind: &'static str) -> Self {
        let visible = Arc::new(AtomicBool::new(false));
        let sends = Arc::new(AtomicUsize::new(0));
        let received = Arc::new(Notify::new());
        let query_visible = visible.clone();
        external_id_query(&format!("acct:{order}:{kind}"))
            .respond_with(move |_: &wiremock::Request| {
                if query_visible.load(Ordering::SeqCst) {
                    Doc::of(
                        "FIRST",
                        if kind.starts_with("corrective") {
                            "HS"
                        } else {
                            "SZ"
                        },
                        order,
                    )
                    .response()
                } else {
                    not_found()
                }
            })
            .mount(mock)
            .await;
        let count = sends.clone();
        let signal = received.clone();
        create_for(order)
            .respond_with(move |_: &wiremock::Request| {
                let previous = count.fetch_add(1, Ordering::SeqCst);
                signal.notify_one();
                if previous == 0 {
                    // The queue is verified during this window, before exhaustion.
                    ResponseTemplate::new(500).set_delay(Duration::from_secs(4))
                } else {
                    created("SECOND", "1000", "1270")
                }
            })
            .mount(mock)
            .await;
        Self {
            visible,
            sends,
            received,
        }
    }

    async fn received(&self) {
        tokio::time::timeout(Duration::from_secs(30), self.received.notified())
            .await
            .expect("the first send reached the mock");
    }
}

/// Current production policy, with the valid minimum execution count. Its
/// default 2m delay remains validated, but is never applied on exhaustion.
#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; models vendor delayed visibility"]
async fn e2e_unresolved_exhaustion_admits_a_second_send() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher.launch(&SERVER).await;
    let mock = MockServer::start().await;
    let config: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct", "issue": { "max_attempts": 1 }
    }))
    .expect("config");
    let (order, agent) =
        services_with_config(&mock.uri(), config.validate().expect("valid policy"));
    restate
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;

    // Same target, corrective (no duplicate-order-number guard), and competing
    // invoice/prepayment chains: the same Order lock must protect all three.
    for (key, first_handler, next_handler, kind) in [
        (
            "UNRESOLVED-SAME",
            "create_invoice",
            "create_invoice",
            "invoice",
        ),
        (
            "UNRESOLVED-CORRECTION",
            "correct_invoice",
            "correct_invoice",
            "corrective:c1",
        ),
        (
            "UNRESOLVED-CROSS",
            "create_invoice",
            "create_prepayment",
            "invoice",
        ),
    ] {
        let pending = PendingSend::mount(&mock, key, kind).await;
        for absent in ["invoice", "prepayment", "final", "proforma"] {
            if absent != kind {
                external_id_query(&format!("acct:{key}:{absent}"))
                    .respond_with(not_found())
                    .mount(&mock)
                    .await;
            }
        }
        order_query(key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let mut body = create_body(dec!(1000), false);
        if first_handler == "correct_invoice" {
            number_query("BASE")
                .respond_with(Doc::of("BASE", "SZ", key).response())
                .mount(&mock)
                .await;
            body = json!({"invoice_number": "BASE", "correction_id": "c1", "document": body["document"]});
        }
        let first = Call::object("Szamlazz.Order", key, first_handler);
        let second = Call::object("Szamlazz.Order", key, next_handler);
        let submitted = restate.invoke(&first.send(), Some(&body), Some(key)).await;
        assert_eq!(submitted.status, 202, "{}", submitted.body);
        pending.received().await;
        let next_key = format!("{key}-next");
        let queued = restate
            .invoke(&second.send(), Some(&body), Some(&next_key))
            .await;
        assert_eq!(queued.status, 202, "{}", queued.body);
        assert_queued(&restate, queued.invocation_id()).await;
        assert_eq!(pending.sends.load(Ordering::SeqCst), 1);

        let exhausted = restate.invoke(&first, Some(&body), Some(key)).await;
        assert_eq!(exhausted.status, 500, "{}", exhausted.body);
        assert_eq!(
            exhausted.fault::<Fault>().code,
            TerminalCode::OutcomeUnknown
        );
        let next = restate.invoke(&second, Some(&body), Some(&next_key)).await;
        assert_eq!(next.status, 200, "{}", next.body);
        assert_eq!(next.body["outcome"], "issued");
        assert_eq!(
            pending.sends.load(Ordering::SeqCst),
            2,
            "current behavior: the queued invocation sends while the first is invisible"
        );
        assert!(!pending.visible.load(Ordering::SeqCst));
        pending.visible.store(true, Ordering::SeqCst);
        eprintln!(
            "{key}: first outcome_unknown, queued {next_handler} issued; two sends before visibility (scripted)"
        );
    }
}

/// Test-only retention + marker design. The marker guards later invocations;
/// the execution-local permit guards replay of an uncompleted send run.
struct RetainedWrite {
    url: String,
    http: szamlazz_agent::reqwest::Client,
}

#[allow(missing_docs, reason = "test-only generated client")]
#[restate_sdk::object(name = "RetainedWrite")]
impl RetainedWrite {
    #[handler(
        journal_retention = "1d",
        invocation_retry_policy(
            initial_interval = "1s",
            max_attempts = 1,
            on_max_attempts = "pause"
        )
    )]
    async fn write(&self, ctx: ObjectContext<'_>) -> HandlerResult<bool> {
        if ctx.get::<bool>("unresolved").await?.unwrap_or(false) {
            return Err(TerminalError::new_with_code(409, "unresolved marker").into());
        }
        let visible = ctx.run(|| self.query()).name("lookup-target").await?;
        if visible {
            return Ok(true);
        }
        ctx.set("unresolved", true);
        // Only execution of the arm closure grants a volatile send permit.
        // Replaying its completed result grants none. Awaiting its completion
        // orders the marker and arm before the next run can perform HTTP I/O.
        let permit = AtomicBool::new(false);
        ctx.run(|| async {
            permit.store(true, Ordering::SeqCst);
            Ok(())
        })
        .name("arm-write")
        .await?;
        ctx.run(|| async {
            if permit.swap(false, Ordering::SeqCst) {
                // A lost exchange is uncertainty too, not permission to retry.
                let _ = self.http.post(format!("{}/send", self.url)).send().await;
            }
            // Deliberately journal uncertainty as data, not a retryable send.
            Ok(())
        })
        .name("send-once")
        .retry_policy(RunRetryPolicy::new().max_attempts(1))
        .await?;
        // The run has no explicit policy: ordinary errors spend the handler's
        // invocation-policy budget and pause. Resume retries this read alone.
        let reconciled = ctx
            .run(|| async {
                if self.query().await? {
                    Ok(true)
                } else {
                    Err(
                        std::io::Error::other("unresolved send; absence is not resend evidence")
                            .into(),
                    )
                }
            })
            .name("reconcile-target")
            .await?;
        ctx.clear("unresolved");
        Ok(reconciled)
    }

    #[handler]
    async fn observe(&self, ctx: SharedObjectContext<'_>) -> HandlerResult<bool> {
        Ok(ctx.run(|| self.query()).name("observe-target").await?)
    }
}

impl RetainedWrite {
    async fn query(&self) -> HandlerResult<bool> {
        Ok(self
            .http
            .get(format!("{}/query", self.url))
            .send()
            .await?
            .text()
            .await?
            == "visible")
    }
}

/// Proves the pinned Rust SDK/server can retain the invocation on pause,
/// attach retries, and resume read-only reconciliation without repeating a
/// completed send. Also probes the cross-invocation marker after manual kill.
/// The arming permit's crash windows require the follow-up's interruption tests.
#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; test-only recovery design probe"]
#[allow(
    clippy::too_many_lines,
    reason = "one protocol probe: retain, resume, then kill with a marker"
)]
async fn e2e_unresolved_retention_probe() {
    use wiremock::{
        Mock,
        matchers::{method, path},
    };

    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "retained",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let visible = Arc::new(AtomicBool::new(false));
    let query_visible = visible.clone();
    let (queries, mut observed_queries) = tokio::sync::watch::channel(0_u32);
    Mock::given(method("GET"))
        .and(path("/query"))
        .respond_with(move |_: &wiremock::Request| {
            queries.send_modify(|count| *count += 1);
            ResponseTemplate::new(200).set_body_string(if query_visible.load(Ordering::SeqCst) {
                "visible"
            } else {
                "absent"
            })
        })
        .mount(&mock)
        .await;
    Mock::given(method("POST"))
        .and(path("/send"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1..=2)
        .mount(&mock)
        .await;
    restate
        .deploy(
            Endpoint::builder()
                .bind(RetainedWrite {
                    url: mock.uri(),
                    http: crate::common::http_client(),
                })
                .build(),
        )
        .await;

    let call = Call::object("RetainedWrite", "order", "write");
    let first = restate.invoke(&call.send(), None, Some("first")).await;
    assert_eq!(first.status, 202);
    restate
        .admin()
        .await_status(first.invocation_id(), &["paused"])
        .await;
    let second = restate.invoke(&call.send(), None, Some("second")).await;
    assert_eq!(second.status, 202);
    assert_queued(&restate, second.invocation_id()).await;
    let attached = restate.invoke(&call.send(), None, Some("first")).await;
    assert_eq!(attached.invocation_id(), first.invocation_id());
    let observation = restate
        .invoke(
            &Call::object("RetainedWrite", "order", "observe"),
            None,
            None,
        )
        .await;
    assert_eq!(observation.status, 200);
    assert_eq!(
        observation.body, false,
        "shared observation works during pause"
    );

    // Resume with absence still reported: pause again, second stays queued.
    let before = *observed_queries.borrow_and_update();
    restate.admin().resume(first.invocation_id()).await;
    tokio::time::timeout(
        Duration::from_secs(30),
        observed_queries.wait_for(|count| *count > before),
    )
    .await
    .expect("resume executed a new absent query")
    .expect("query observer");
    restate
        .admin()
        .await_status(first.invocation_id(), &["paused"])
        .await;
    assert_queued(&restate, second.invocation_id()).await;
    assert_eq!(send_count(&mock).await, 1, "absent resume did not send");
    mock.verify().await;
    visible.store(true, Ordering::SeqCst);
    restate.admin().resume(first.invocation_id()).await;
    let completed = restate.invoke(&call, None, Some("first")).await;
    assert_eq!(completed.status, 200, "{}", completed.body);
    assert_eq!(completed.body, true);
    assert_eq!(completed.invocation_id(), first.invocation_id());
    let next = restate.invoke(&call, None, Some("second")).await;
    assert_eq!(next.status, 200, "{}", next.body);
    assert_eq!(next.body, true);
    assert_eq!(
        restate.admin().runs(first.invocation_id()).await,
        [
            "lookup-target",
            "arm-write",
            "send-once",
            "reconcile-target"
        ]
    );
    assert_eq!(
        restate.admin().runs(second.invocation_id()).await,
        ["lookup-target"]
    );
    mock.verify().await;
    assert_eq!(
        send_count(&mock).await,
        1,
        "one send through positive reconciliation"
    );
    eprintln!(
        "retained: paused + queued; same key attached; absent resume paused; visible resume completed; one send"
    );

    // Manual kill bypasses compensation and releases the lock. The marker
    // must outlive that invocation and refuse the already-queued mutation.
    visible.store(false, Ordering::SeqCst);
    let killed_call = Call::object("RetainedWrite", "killed-order", "write");
    let killed = restate
        .invoke(&killed_call.send(), None, Some("kill-first"))
        .await;
    restate
        .admin()
        .await_status(killed.invocation_id(), &["paused"])
        .await;
    let queued = restate
        .invoke(&killed_call.send(), None, Some("kill-next"))
        .await;
    assert_queued(&restate, queued.invocation_id()).await;
    assert_eq!(
        send_count(&mock).await,
        2,
        "the killed order sent once before kill"
    );
    restate.admin().kill(killed.invocation_id()).await;
    let blocked = restate.invoke(&killed_call, None, Some("kill-next")).await;
    assert_eq!(blocked.status, 409, "{}", blocked.body);
    assert_eq!(blocked.body["message"], "unresolved marker");
    assert!(
        restate
            .admin()
            .runs(blocked.invocation_id())
            .await
            .is_empty()
    );
    assert_eq!(
        send_count(&mock).await,
        2,
        "one send on each order; none after kill"
    );
    eprintln!("marker: manual kill released the lock; queued mutation refused; no additional send");
}

async fn send_count(mock: &MockServer) -> usize {
    mock.received_requests()
        .await
        .expect("requests")
        .iter()
        .filter(|request| request.method == "POST")
        .count()
}

async fn assert_queued(restate: &Restate, id: &str) {
    let status = restate
        .admin()
        .await_status(id, &["pending", "inboxed"])
        .await;
    assert!(matches!(status.as_str(), "pending" | "inboxed"));
    assert!(
        restate.admin().runs(id).await.is_empty(),
        "the queued invocation has executed no run"
    );
}
