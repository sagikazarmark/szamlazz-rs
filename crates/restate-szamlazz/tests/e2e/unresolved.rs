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

#[path = "recovery_evidence.rs"]
mod recovery_evidence;

#[path = "recovery_boundaries.rs"]
mod recovery_boundaries;

#[path = "document_identity.rs"]
mod document_identity;

#[path = "protected_storno_evidence.rs"]
mod protected_storno_evidence;

#[path = "release_hardening.rs"]
mod release_hardening;

#[path = "contradictory_replies.rs"]
mod contradictory_replies;

struct Operator;
impl restate_szamlazz::service::RecoveryAuthorizer for Operator {
    fn authorize(
        &self,
        _scope: Option<&str>,
        _order: &str,
        _headers: &restate_sdk::context::HeaderMap,
    ) -> Option<String> {
        // This deployment is reachable only by this operator test process.
        Some("test-operator".to_owned())
    }
}

struct AuthenticatedOperators(std::sync::Mutex<std::collections::HashSet<String>>);
impl restate_szamlazz::service::RecoveryAuthorizer for AuthenticatedOperators {
    fn authorize(
        &self,
        _scope: Option<&str>,
        _order: &str,
        headers: &restate_sdk::context::HeaderMap,
    ) -> Option<String> {
        let assertion = headers.get("x-operator-assertion")?;
        self.0
            .lock()
            .expect("operator admission registry")
            .contains(assertion)
            .then(|| "authenticated-operator".to_owned())
    }
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; host operator admission boundary"]
async fn e2e_unresolved_operator_boundary_refuses_spoofed_recovery() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "recovery-auth",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
    let auth = Arc::new(AuthenticatedOperators(std::sync::Mutex::new(
        std::collections::HashSet::new(),
    )));
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.with_recovery_authorizer(auth.clone()))
                .build(),
        )
        .await;
    let http = crate::common::http_client();
    let observe = Call::object("Szamlazz.Order", "AUTH", "observe_unresolved");
    let url = format!("{}{}", restate.ingress_url(), observe.path());
    // A caller-supplied identity string is insufficient: only host-admitted
    // assertions are accepted. Nothing in the request body admits an operator.
    for assertion in [None, Some("spoofed-operator")] {
        let mut request = http.post(&url);
        if let Some(value) = assertion {
            request = request.header("x-operator-assertion", value);
        }
        assert_eq!(request.send().await.expect("response").status(), 403);
    }
    auth.0
        .lock()
        .expect("registry")
        .insert("host-admitted-session".into());
    let response = http
        .post(&url)
        .header("x-operator-assertion", "host-admitted-session")
        .send()
        .await
        .expect("response");
    assert_eq!(response.status(), 200);
    let marker = json!({"version":1,"token":"owner","owner_invocation":"owner","created_at":"2026-09-10T12:00:00Z","scope":null,"order":"AUTH","namespace":"acct","external_id":"acct:AUTH:invoice","account_id":"acct","endpoint":mock.uri(),"credential_ref":"acct","operation":{"type":"create","kind":"invoice","expected_number":null,"corrected_number":null}});
    let recover = Call::object("Szamlazz.Order", "AUTH", "recover");
    let body = json!({"marker":marker,"evidence":{"type":"not_executed","audit_reference":"INC-216","did_not_execute_and_cannot_execute_later":true}});
    let response = http
        .post(format!("{}{}", restate.ingress_url(), recover.path()))
        .header("x-operator-assertion", "spoofed-operator")
        .json(&body)
        .send()
        .await
        .expect("response");
    assert_eq!(response.status(), 403);
    assert!(mock.received_requests().await.expect("requests").is_empty());
    restate.finish().await;
}

struct InterruptAt {
    point: restate_szamlazz::service::WriteCheckpoint,
    once: AtomicBool,
    reached: Notify,
}

impl restate_szamlazz::service::WriteObserver for InterruptAt {
    fn reached(
        &self,
        _order: &str,
        point: restate_szamlazz::service::WriteCheckpoint,
    ) -> restate_szamlazz::account::BoxFuture<'_, ()> {
        Box::pin(async move {
            if point == self.point && !self.once.swap(true, Ordering::SeqCst) {
                self.reached.notify_one();
                std::future::pending::<()>().await;
            }
        })
    }
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; production arming interruption matrix"]
#[allow(
    clippy::too_many_lines,
    reason = "one complete durable-boundary interruption matrix"
)]
async fn e2e_unresolved_interrupted_arm_and_open_send_never_regrant_permission() {
    use restate_szamlazz::service::WriteCheckpoint;
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "arm-interruption",
            ..SERVER
        })
        .await;
    let mut mocks = Vec::new();
    for (index, point) in [
        WriteCheckpoint::BeforeMarker,
        WriteCheckpoint::AfterMarker,
        WriteCheckpoint::Armed,
        WriteCheckpoint::Sent,
        WriteCheckpoint::Reconciled,
        WriteCheckpoint::Settled,
    ]
    .into_iter()
    .enumerate()
    {
        let mock = MockServer::start().await;
        let key = format!("INTERRUPT-{index}");
        let visible = Arc::new(AtomicBool::new(false));
        let query_visible = visible.clone();
        let query_key = key.clone();
        external_id_query(&format!("acct:{key}:invoice"))
            .respond_with(move |_: &wiremock::Request| {
                if query_visible.load(Ordering::SeqCst) {
                    Doc::of("ISSUED", "SZ", &query_key).response()
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        for kind in ["prepayment", "final", "proforma"] {
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        order_query(&key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let sends = Arc::new(AtomicUsize::new(0));
        let count = sends.clone();
        let publish = visible.clone();
        create_for(&key)
            .respond_with(move |_: &wiremock::Request| {
                count.fetch_add(1, Ordering::SeqCst);
                publish.store(true, Ordering::SeqCst);
                if point == WriteCheckpoint::Reconciled {
                    ResponseTemplate::new(500)
                } else {
                    created("ISSUED", "1000", "1270")
                }
            })
            .mount(&mock)
            .await;
        let hold = Arc::new(InterruptAt {
            point,
            once: AtomicBool::new(false),
            reached: Notify::new(),
        });
        let config = WorkerConfig::new("acct".parse().expect("namespace"))
            .validate()
            .expect("config");
        let (order, _) = services_with_config(&mock.uri(), config);
        let options = restate_sdk::endpoint::ServiceOptions::default().handler(
            "create_invoice",
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_initial_interval(Duration::from_secs(1))
                .retry_policy_max_attempts(1)
                .retry_policy_pause_on_max_attempts(),
        );
        restate
            .deploy(
                Endpoint::builder()
                    .bind(
                        order
                            .with_write_observer(hold.clone())
                            .with_recovery_authorizer(Arc::new(Operator))
                            .into_service_definition()
                            .options(options),
                    )
                    .build(),
            )
            .await;
        let call = Call::object("Szamlazz.Order", &key, "create_invoice");
        let body = create_body(dec!(1000));
        let submitted = restate.invoke(&call.send(), Some(&body), Some(&key)).await;
        tokio::time::timeout(Duration::from_secs(30), hold.reached.notified())
            .await
            .expect("checkpoint reached");
        // Administrative pause forcibly aborts the executing endpoint task; replay
        // resumes the same pinned code with a fresh execution-local permit.
        restate.admin().pause(submitted.invocation_id()).await;
        let journal = restate.admin().journal(submitted.invocation_id()).await;
        crate::write_commands::check(&journal);
        if matches!(
            point,
            WriteCheckpoint::Armed
                | WriteCheckpoint::Sent
                | WriteCheckpoint::Reconciled
                | WriteCheckpoint::Settled
        ) {
            assert!(restate_e2e_harness::run_result(&journal, "arm-write").is_some());
            let marker_command = journal
                .iter()
                .find(|entry| entry.entry_type.contains("SetState"))
                .expect("marker SetState command");
            let arm_command = journal
                .iter()
                .find(|entry| entry.is_run() && entry.name.as_deref() == Some("arm-write"))
                .expect("arm command");
            assert!(marker_command.index < arm_command.index);
            assert!(marker_command.raw_contains("unresolved-write"));
            assert!(marker_command.raw_contains(&key));
            assert!(!marker_command.raw_contains(crate::harness::accounts::AGENT_KEY));
        }
        let prior_sends = sends.load(Ordering::SeqCst);
        if matches!(
            point,
            WriteCheckpoint::Sent | WriteCheckpoint::Reconciled | WriteCheckpoint::Settled
        ) {
            assert_eq!(prior_sends, 1);
        }
        restate.admin().resume(submitted.invocation_id()).await;
        if point == WriteCheckpoint::Armed {
            restate
                .admin()
                .await_status(submitted.invocation_id(), &["paused"])
                .await;
            assert_eq!(
                sends.load(Ordering::SeqCst),
                0,
                "completed arm replay grants no permission"
            );
            let observed = restate
                .invoke(
                    &Call::object("Szamlazz.Order", &key, "observe_unresolved"),
                    None,
                    None,
                )
                .await;
            assert_eq!(observed.body["state"], "unresolved");
            restate.admin().kill(submitted.invocation_id()).await;
            let recovery = json!({"marker":observed.body["marker"],"evidence":{"type":"not_executed","audit_reference":"interrupted-before-send","did_not_execute_and_cannot_execute_later":true}});
            assert_eq!(
                restate
                    .invoke(
                        &Call::object("Szamlazz.Order", &key, "recover"),
                        Some(&recovery),
                        None
                    )
                    .await
                    .status,
                200
            );
        } else {
            restate
                .admin()
                .await_status(submitted.invocation_id(), &["completed"])
                .await;
            let completed = restate.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(completed.status, 200, "{point:?}: {}", completed.body);
            assert_eq!(
                sends.load(Ordering::SeqCst),
                1,
                "{point:?}: never a second send"
            );
        }
        mocks.push(mock);
    }
    restate.finish().await;
    drop(mocks);
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; final refusal after unresolved send"]
async fn e2e_unresolved_final_refusal_cannot_erase_the_first_send() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "final-uncertainty",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let key = "FINAL-UNCERTAIN";
    let evidence = Arc::new(AtomicUsize::new(0));
    let query = evidence.clone();
    external_id_query("acct:FINAL-UNCERTAIN:final")
        .respond_with(
            move |_: &wiremock::Request| match query.load(Ordering::SeqCst) {
                1 => crate::common::api_error("73", "final already issued"),
                2 => crate::common::api_error("135", "credentials rejected"),
                3 => crate::common::szlahu_down(),
                4 => Doc::of("FINAL", "VS", key).response(),
                _ => not_found(),
            },
        )
        .mount(&mock)
        .await;
    external_id_query("acct:FINAL-UNCERTAIN:prepayment")
        .respond_with(Doc::of("ADVANCE", "ES", key).response())
        .mount(&mock)
        .await;
    order_query(key)
        .respond_with(Doc::of("ADVANCE", "ES", key).response())
        .mount(&mock)
        .await;
    let sends = Arc::new(AtomicUsize::new(0));
    let count = sends.clone();
    create_for(key)
        .respond_with(move |_: &wiremock::Request| {
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                ResponseTemplate::new(500)
            } else {
                crate::common::api_error("73", "later refusal cannot settle earlier send")
            }
        })
        .mount(&mock)
        .await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_final",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_initial_interval(Duration::from_secs(1))
            .retry_policy_max_attempts(1)
            .retry_policy_pause_on_max_attempts(),
    );
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .build(),
        )
        .await;
    let call = Call::object("Szamlazz.Order", key, "create_final");
    let body = create_body(dec!(1000));
    let owner = restate.invoke(&call.send(), Some(&body), Some(key)).await;
    restate
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    for answer in 1..=3 {
        evidence.store(answer, Ordering::SeqCst);
        let before = mock.received_requests().await.expect("requests").len();
        restate.admin().resume(owner.invocation_id()).await;
        tokio::time::timeout(Duration::from_secs(30), async {
            while mock.received_requests().await.expect("requests").len() == before {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("read-only reconciliation executed");
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        assert_eq!(sends.load(Ordering::SeqCst), 1);
    }
    evidence.store(4, Ordering::SeqCst);
    restate.admin().resume(owner.invocation_id()).await;
    let completed = restate.invoke(&call, Some(&body), Some(key)).await;
    assert_eq!(completed.status, 200, "{}", completed.body);
    assert_eq!(completed.body["outcome"], "reconciled");
    assert_eq!(sends.load(Ordering::SeqCst), 1);
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; production marker after kill"]
#[allow(
    clippy::too_many_lines,
    reason = "one production kill and evidence-recovery scenario"
)]
async fn e2e_unresolved_kill_preserves_marker_and_recovery_requires_evidence() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "marker-recovery",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("validated defaults");
    let (order, agent) = services_with_config(&mock.uri(), config);
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.with_recovery_authorizer(Arc::new(Operator)))
                .bind(agent)
                .build(),
        )
        .await;
    let key = "MARKER-KILL";
    let pending = PendingSend::mount(&mock, key, "invoice").await;
    for kind in ["prepayment", "final", "proforma"] {
        external_id_query(&format!("acct:{key}:{kind}"))
            .respond_with(not_found())
            .mount(&mock)
            .await;
    }
    order_query(key)
        .respond_with(not_found())
        .mount(&mock)
        .await;
    let body = create_body(dec!(1000));
    let create = Call::object("Szamlazz.Order", key, "create_invoice");
    let first = restate
        .invoke(&create.send(), Some(&body), Some("owner"))
        .await;
    pending.received().await;
    let observation = Call::object("Szamlazz.Order", key, "observe_unresolved");
    let observed = restate.invoke(&observation, None, None).await;
    assert_eq!(observed.status, 200, "{}", observed.body);
    assert_eq!(observed.body["state"], "unresolved");
    let marker = observed.body["marker"].clone();
    assert_eq!(marker["owner_invocation"], first.invocation_id());
    assert!(
        !marker
            .to_string()
            .contains(crate::harness::accounts::AGENT_KEY)
    );
    let mut queued = Vec::new();
    for handler in ["create_invoice", "create_prepayment", "correct_invoice"] {
        let call = Call::object("Szamlazz.Order", key, handler);
        let request = if handler == "correct_invoice" {
            json!({"invoice_number":"BASE", "correction_id":"c1", "document":body["document"]})
        } else {
            body.clone()
        };
        let submitted = restate
            .invoke(&call.send(), Some(&request), Some(handler))
            .await;
        assert_queued(&restate, submitted.invocation_id()).await;
        queued.push((call, request, handler));
    }
    restate.admin().kill(first.invocation_id()).await;
    for (call, request, id) in queued {
        let refused = restate.invoke(&call, Some(&request), Some(id)).await;
        assert_eq!(refused.status, 500, "{}", refused.body);
        assert_eq!(
            refused.fault::<restate_szamlazz::contract::Fault>().code,
            restate_szamlazz::contract::TerminalCode::OutcomeUnknown
        );
        assert!(
            restate
                .admin()
                .runs(refused.invocation_id())
                .await
                .is_empty()
        );
    }
    assert_eq!(pending.sends.load(Ordering::SeqCst), 1);
    let recover = Call::object("Szamlazz.Order", key, "recover");
    // A completed/killed owner satisfies invocation drain but must still block
    // a scope switch. Exercise the operator's executable inventory, not a copy.
    restate
        .admin()
        .await_status(first.invocation_id(), &["completed"])
        .await;
    let blocked = migration_inventory(&restate).await;
    assert_eq!(blocked.status.code(), Some(1), "{blocked:?}");
    let inventory: serde_json::Value =
        serde_json::from_slice(&blocked.stdout).expect("inventory JSON");
    assert_eq!(inventory["unfinished_invocations"], json!([]));
    assert_eq!(inventory["order_state"][0]["service_key"], key);
    assert!(inventory["order_state"][0]["scope"].is_null());
    let mut request = json!({"marker":marker, "evidence":{"type":"not_executed","audit_reference":"INC-216","did_not_execute_and_cannot_execute_later":true}});
    request["marker"]["token"] = json!("stale");
    assert_eq!(
        restate.invoke(&recover, Some(&request), None).await.status,
        400
    );
    request["marker"] = marker.clone();
    for (field, value) in [
        ("scope", json!("wrong-scope")),
        ("account_id", json!("wrong-account")),
        ("endpoint", json!("https://wrong.example/szamla")),
        ("namespace", json!("other")),
        ("operation", json!({"type":"storno","number":"OTHER"})),
    ] {
        let mut wrong = request.clone();
        wrong["marker"][field] = value;
        assert_eq!(
            restate.invoke(&recover, Some(&wrong), None).await.status,
            400
        );
    }
    request["evidence"] = json!({"type":"document", "number":"FIRST"});
    assert_eq!(
        restate.invoke(&recover, Some(&request), None).await.status,
        500
    );
    pending.visible.store(true, Ordering::SeqCst);
    let mut wrong_number = request.clone();
    wrong_number["evidence"]["number"] = json!("DIFFERENT");
    assert_eq!(
        restate
            .invoke(&recover, Some(&wrong_number), None)
            .await
            .status,
        500
    );
    let settled = restate.invoke(&recover, Some(&request), None).await;
    assert_eq!(settled.status, 200, "{}", settled.body);
    assert_eq!(settled.body["operator"], "test-operator");
    assert_eq!(
        restate.invoke(&observation, None, None).await.body["state"],
        "absent"
    );
    assert_eq!(pending.sends.load(Ordering::SeqCst), 1);
    let clean = migration_inventory(&restate).await;
    assert!(clean.status.success(), "{clean:?}");
    // Only after operator recovery do we close all ingress for the final gate.
    restate.set_public("Szamlazz.Order", false).await;
    restate.set_public("Szamlazz.Agent", false).await;
    restate.drain().await;
    assert_ne!(restate.invoke(&observation, None, None).await.status, 200);
    assert!(migration_inventory(&restate).await.status.success());
    restate.finish().await;
}

async fn migration_inventory(restate: &Restate) -> std::process::Output {
    let admin = restate.admin_url().to_owned();
    tokio::task::spawn_blocking(move || {
        let mut command = if let Some(binary) = std::env::var_os("XTASK_BIN") {
            std::process::Command::new(binary)
        } else {
            let mut command = std::process::Command::new("cargo");
            command.args(["run", "--quiet", "--locked", "-p", "xtask", "--"]);
            command.current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."));
            command
        };
        command
            .arg("check-order-migration")
            .arg("--admin-url")
            .arg(admin)
            .env_remove("RESTATE_ADMIN_TOKEN")
            .output()
            .expect("start migration inventory: set XTASK_BIN or make Cargo available on PATH")
    })
    .await
    .expect("inventory task")
}

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
                    let mut document = Doc::of(
                        "FIRST",
                        if kind.starts_with("corrective") {
                            "HS"
                        } else {
                            "SZ"
                        },
                        order,
                    );
                    if kind.starts_with("corrective") {
                        document.referenced_invoice = Some("BASE");
                    }
                    document.response()
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
#[allow(
    clippy::too_many_lines,
    reason = "three production interleavings through pause and resume"
)]
async fn e2e_unresolved_exhaustion_retains_one_send() {
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
    let mut options = restate_sdk::endpoint::ServiceOptions::default();
    for handler in ["create_invoice", "create_prepayment", "correct_invoice"] {
        options = options.handler(
            handler,
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_initial_interval(Duration::from_secs(1))
                .retry_policy_max_attempts(1)
                .retry_policy_pause_on_max_attempts(),
        );
    }
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .bind(agent)
                .build(),
        )
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
        let mut body = create_body(dec!(1000));
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

        tokio::time::sleep(Duration::from_secs(6)).await;
        assert_eq!(
            pending.sends.load(Ordering::SeqCst),
            1,
            "uncertainty never authorizes a second send"
        );
        assert_queued(&restate, queued.invocation_id()).await;
        restate
            .admin()
            .await_status(submitted.invocation_id(), &["paused"])
            .await;
        let attached = restate.invoke(&first.send(), Some(&body), Some(key)).await;
        assert_eq!(attached.invocation_id(), submitted.invocation_id());
        let before = mock.received_requests().await.expect("requests").len();
        restate.admin().resume(submitted.invocation_id()).await;
        tokio::time::timeout(Duration::from_secs(30), async {
            while mock.received_requests().await.expect("requests").len() == before {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("absent resume queried");
        restate
            .admin()
            .await_status(submitted.invocation_id(), &["paused"])
            .await;
        assert_eq!(pending.sends.load(Ordering::SeqCst), 1);
        assert_queued(&restate, queued.invocation_id()).await;
        pending.visible.store(true, Ordering::SeqCst);
        restate.admin().resume(submitted.invocation_id()).await;
        restate
            .admin()
            .await_status_with_timeout(
                submitted.invocation_id(),
                &["completed"],
                Duration::from_secs(300),
            )
            .await;
        let completed = restate.invoke(&first, Some(&body), Some(key)).await;
        assert_eq!(completed.status, 200, "{}", completed.body);
        let next = restate.invoke(&second, Some(&body), Some(&next_key)).await;
        assert_eq!(next.status, 200, "{}", next.body);
        assert_eq!(
            next.body["outcome"],
            if next_handler == "create_prepayment" {
                "conflict"
            } else {
                "already_issued"
            }
        );
        assert_eq!(
            pending.sends.load(Ordering::SeqCst),
            1,
            "queued calls cannot send while the first is invisible"
        );
        eprintln!(
            "{key}: retained owner reconciled; queued {next_handler} performed normal checks; one send"
        );
    }
    restate.finish().await;
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
    restate.finish().await;
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
