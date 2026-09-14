//! Actual-service #247 experiment. Fake provider only; no live tests in this target.
#![allow(missing_docs, clippy::too_many_lines)]

mod chain;
#[path = "../common/mod.rs"]
mod common;
mod extensions;
mod proformas;

use axum::{Router, body::Body, extract::State};
use http_body_util::{BodyExt as _, Full};
use restate_e2e_harness::{
    Call, Restate, ServerSpec,
    gate::{PROTOCOL_V7, ReusePolicy, VQUEUES, launcher_or_skip},
};
use restate_sdk::{
    endpoint::{HandleOptions, HandlerOptions, ProtocolMode, ServiceOptions},
    prelude::*,
};
use restate_szamlazz::service::{WriteCheckpoint, WriteObserver};
use restate_szamlazz::{
    Agent, Order,
    account::{Accounts, StaticConfig, StaticResolver},
    config::WorkerConfig,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::{sync::Notify, task::JoinHandle};

#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent fake-provider controls, not domain state"
)]
struct Case {
    sends: usize,
    documents: Vec<String>,
    visible: bool,
    deduplicate: bool,
    lost_answer: bool,
    no_effect: bool,
    later_code: Option<&'static str>,
    guard: Option<&'static str>,
    hold: Option<WriteCheckpoint>,
    workers: Vec<usize>,
    duplicate_evidence: bool,
    evidence_read: bool,
}

#[derive(Default)]
struct Lab {
    cases: Mutex<BTreeMap<String, Case>>,
    reached: Notify,
    release: Notify,
    cut: Notify,
    keep_old: AtomicBool,
    old_tasks: Mutex<Vec<JoinHandle<http::Response<Body>>>>,
    fetches: AtomicUsize,
    credentials_missing: AtomicBool,
}

struct Store {
    resolver: Arc<StaticResolver>,
    lab: Arc<Lab>,
}
impl restate_szamlazz::account::CredentialStore for Store {
    fn fetch<'a>(
        &'a self,
        credential_ref: &'a restate_szamlazz::account::CredentialRef,
    ) -> restate_szamlazz::account::BoxFuture<
        'a,
        Result<szamlazz_agent::Credentials, restate_szamlazz::account::FetchError>,
    > {
        self.lab.fetches.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if self.lab.credentials_missing.load(Ordering::SeqCst) {
                Err(restate_szamlazz::account::FetchError::Gone {
                    credential_ref: credential_ref.clone(),
                })
            } else {
                self.resolver.fetch(credential_ref).await
            }
        })
    }
}

impl Lab {
    fn change<T>(&self, key: &str, f: impl FnOnce(&mut Case) -> T) -> T {
        f(self
            .cases
            .lock()
            .expect("cases")
            .entry(key.into())
            .or_default())
    }
    fn counts(&self, key: &str) -> (usize, usize) {
        self.change(key, |s| (s.sends, s.documents.len()))
    }
}

struct Observer {
    lab: Arc<Lab>,
    worker: usize,
}
impl WriteObserver for Observer {
    fn reached(
        &self,
        order: &str,
        point: WriteCheckpoint,
    ) -> restate_szamlazz::account::BoxFuture<'_, ()> {
        let hold = self.lab.change(order, |s| {
            if point == WriteCheckpoint::OrdinaryGuardsPassed {
                s.workers.push(self.worker);
            }
            if s.hold == Some(point) {
                s.hold = None;
                true
            } else {
                false
            }
        });
        Box::pin(async move {
            if hold {
                self.lab.reached.notify_one();
                self.lab.release.notified().await;
            }
        })
    }
}

#[derive(Clone)]
struct Host {
    endpoints: Arc<Vec<Endpoint>>,
    exchanges: Arc<AtomicUsize>,
    lab: Arc<Lab>,
}

async fn handle(State(host): State<Host>, request: http::Request<Body>) -> http::Response<Body> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 16 * 1024 * 1024)
        .await
        .expect("request");
    let index = host.exchanges.fetch_add(1, Ordering::SeqCst) % 2;
    let response = host.endpoints[index].handle_with_options(
        http::Request::from_parts(parts, Full::new(bytes)),
        HandleOptions {
            protocol_mode: ProtocolMode::RequestResponse,
        },
    );
    let mut task = tokio::spawn(async move {
        let (parts, body) = response.into_parts();
        let bytes = body
            .collect()
            .await
            .expect("fully buffered SDK output")
            .to_bytes();
        http::Response::from_parts(parts, Body::from(bytes))
    });
    tokio::select! {
        result = &mut task => result.expect("SDK task"),
        () = host.lab.cut.notified() => {
            if host.lab.keep_old.swap(false, Ordering::SeqCst) {
                host.lab.old_tasks.lock().expect("old tasks").push(task);
            } else {
                task.abort();
                assert!(task.await.expect_err("aborted").is_cancelled());
            }
            http::Response::builder().status(503).body(Body::from("test: interrupted execution")).expect("response")
        }
    }
}

fn endpoint(provider: &str, lab: Arc<Lab>, worker: usize) -> Endpoint {
    let config: StaticConfig = serde_json::from_value(json!({"account":{
        "id":"experiment", "agent_key":"EXPERIMENT-NOT-A-REAL-KEY", "endpoint":provider
    }}))
    .expect("config");
    let resolver = Arc::new(StaticResolver::try_from(config).expect("resolver"));
    let accounts = Accounts::new(
        resolver.clone(),
        Arc::new(Store {
            resolver,
            lab: lab.clone(),
        }),
    );
    let config = WorkerConfig::new("rr".parse().expect("namespace"))
        .validate()
        .expect("config");
    let policy = HandlerOptions::default()
        .retry_policy_initial_interval(Duration::from_millis(100))
        .retry_policy_max_interval(Duration::from_millis(100))
        .retry_policy_max_attempts(2)
        .retry_policy_pause_on_max_attempts();
    Endpoint::builder()
        .bind(
            Order::from_parts(accounts.clone(), config.clone())
                .experimental_request_response()
                .with_write_observer(Arc::new(Observer { lab, worker }))
                .into_service_definition()
                .options(
                    ServiceOptions::default()
                        .handler("create_invoice", policy.clone())
                        .handler("create_proforma", policy.clone())
                        .handler("create_prepayment", policy.clone())
                        .handler("create_final", policy.clone())
                        .handler("delete_proforma", policy),
                ),
        )
        .bind(Agent::from_parts(accounts, config).experimental_request_response())
        .build()
}

fn request() -> Value {
    json!({"document": {
        "buyer": {"name":"Experimental Buyer", "zip":"1000", "city":"City", "address":"Address"},
        "items":[{"name":"Item", "quantity":"1", "unit":"db", "unit_price":"1000", "vat_rate":"27"}],
        "fulfillment_date":"2026-09-14", "due_date":"2026-09-21", "payment_method":"transfer"
    }, "options":{"proforma":"none"}})
}

async fn marker(server: &Restate, key: &str) -> Value {
    server
        .invoke(
            &Call::object("Szamlazz.Order", key, "observe_unresolved"),
            None,
            None,
        )
        .await
        .body
}

async fn mount(mock: &wiremock::MockServer, lab: &Arc<Lab>, key: &str) {
    let state = lab.clone();
    let name = key.to_owned();
    let match_name = name.clone();
    wiremock::Mock::given(wiremock::matchers::body_string_contains(
        "action-szamla_agent_xml",
    ))
    .and(move |r: &wiremock::Request| {
        let body = String::from_utf8_lossy(&r.body);
        body.contains(&format!(":{match_name}:")) || body.contains(&format!(">{match_name}<"))
    })
    .respond_with(move |r: &wiremock::Request| {
        state.change(&name, |s| {
            let body = String::from_utf8_lossy(&r.body);
            if s.duplicate_evidence && s.sends > 0 && body.contains(":invoice<") {
                if std::mem::replace(&mut s.evidence_read, true) {
                    return common::api_error("3", "query blocked after positive evidence");
                }
                return common::Doc::of("DUPLICATE-1", "SZ", &name).response();
            }
            if let Some(guard) = s.guard {
                if guard == "query" {
                    return common::api_error("3", "credentials rejected");
                }
                if body.contains(&format!(":{guard}<")) {
                    let kind = match guard {
                        "prepayment" => "ES",
                        "final" => "VS",
                        "proforma" => "D",
                        _ => "SZ",
                    };
                    return common::Doc::of(
                        "GUARD-1",
                        kind,
                        if guard == "invoice" {
                            "other-order"
                        } else {
                            &name
                        },
                    )
                    .response();
                }
                if matches!(guard, "foreign" | "foreign-proforma")
                    && body.contains("<rendelesSzam>")
                {
                    return common::Doc::of(
                        "FOREIGN-1",
                        if guard == "foreign-proforma" {
                            "D"
                        } else {
                            "SZ"
                        },
                        &name,
                    )
                    .response();
                }
            }
            // Only the invoice external id or the order hint can return this
            // document. A broad matcher would fabricate cross-kind collisions.
            if s.visible
                && (body.contains(":invoice<") || body.contains("<rendelesSzam>"))
                && let Some(number) = s.documents.last()
            {
                return common::Doc::of(number, "SZ", &name).response();
            }
            common::not_found()
        })
    })
    .with_priority(1)
    .mount(mock)
    .await;
    let state = lab.clone();
    let name = key.to_owned();
    common::create_for(key)
        .respond_with(move |_: &wiremock::Request| {
            state.change(&name, |s| {
                s.sends += 1;
                if s.duplicate_evidence {
                    s.documents.push("DUPLICATE-1".into());
                    return common::api_error("71", "scripted duplicate with visible evidence");
                }
                if let Some(code) = s.later_code
                    && s.sends > 1
                {
                    return common::api_error(code, "scripted later answer");
                }
                if s.no_effect {
                    return wiremock::ResponseTemplate::new(500);
                }
                if !s.deduplicate || s.documents.is_empty() {
                    s.documents.push(format!("{name}-{}", s.sends));
                }
                if s.lost_answer {
                    wiremock::ResponseTemplate::new(500)
                } else {
                    common::created(s.documents.last().expect("document"), "1000", "1270")
                }
            })
        })
        .mount(mock)
        .await;
}

async fn pause(server: &Restate, id: &str) {
    server
        .admin()
        .await_status_with_timeout(id, &["paused"], Duration::from_secs(30))
        .await;
}

async fn admission_cases(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>) {
    let before = mock.received_requests().await.expect("requests").len();
    for handler in ["correct_invoice", "storno_invoice"] {
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", "unsupported", handler),
                Some(&json!({})),
                None,
            )
            .await;
        assert_eq!(response.status, 400, "{handler}: {response:?}");
        assert!(
            response.body["message"]
                .as_str()
                .expect("fault")
                .contains("mutation unsupported"),
            "must refuse at capability boundary: {handler}"
        );
    }
    for handler in ["storno", "set_credit_entries"] {
        let response = server
            .invoke(
                &Call::service("Szamlazz.Agent", handler),
                Some(&json!({})),
                None,
            )
            .await;
        assert_eq!(response.status, 400, "{handler}: {response:?}");
        assert!(
            response.body["message"]
                .as_str()
                .expect("fault")
                .contains("mutation unsupported")
        );
    }
    let mut invalid = request();
    invalid["document"]["items"][0]["unit_price"] = json!("79228162514264337593543950335");
    invalid["document"]["items"][0]["quantity"] = json!("10");
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "invalid-money", "create_invoice"),
            Some(&invalid),
            None,
        )
        .await;
    assert_eq!(response.status, 400, "{response:?}");
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before,
        "unsupported/invalid work must not contact provider"
    );
    for (key, guard, reason) in [
        ("initial-collision", "invoice", "external_id_collision"),
        ("initial-prepaid", "prepayment", "prepaid_chain"),
        ("initial-final", "final", "prepaid_chain"),
        ("initial-proforma", "proforma", "proforma_live"),
        (
            "initial-foreign-proforma",
            "foreign-proforma",
            "proforma_live",
        ),
        ("initial-foreign", "foreign", "foreign"),
    ] {
        mount(mock, lab, key).await;
        lab.change(key, |s| s.guard = Some(guard));
        let mut body = request();
        body["options"] = json!({"proforma":"none"});
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", key, "create_invoice"),
                Some(&body),
                None,
            )
            .await;
        assert_eq!(response.body["outcome"], "conflict", "{key}: {response:?}");
        assert_eq!(
            response.body["conflict_reason"], reason,
            "{key}: {response:?}"
        );
        assert_eq!(marker(server, key).await["state"], "absent");
        assert_eq!(lab.counts(key), (0, 0));
    }
    let probe = server
        .invoke(
            &Call::service("Szamlazz.Agent", "check_account"),
            None,
            None,
        )
        .await;
    assert_eq!(probe.body["credentials"]["state"], "ok", "{probe:?}");
    let get = server
        .invoke(
            &Call::object("Szamlazz.Order", "empty-read", "get"),
            None,
            None,
        )
        .await;
    assert_eq!(get.status, 200, "{get:?}");

    // State survives a deployment switch. Neither a legacy marker nor an
    // unfamiliar future contract can be adopted as open-run resend intent.
    for (key, version) in [("legacy-marker", 1), ("unknown-marker", 99)] {
        let retained = json!({"version":version,"token":"owner","owner_invocation":"owner","created_at":"2026-09-14T12:00:00Z","scope":null,"order":key,"namespace":"rr","external_id":format!("rr:{key}:invoice"),"account_id":"experiment","endpoint":mock.uri(),"credential_ref":"experiment","operation":{"type":"create","kind":"invoice","expected_number":null,"corrected_number":null}});
        let raw = serde_json::to_vec(&retained).expect("state");
        let response = common::http_client()
            .post(format!(
                "{}/services/Szamlazz.Order/state",
                server.admin().base()
            ))
            .json(&json!({"object_key":key,"new_state":{"unresolved-write":raw}}))
            .send()
            .await
            .expect("state patch");
        assert_eq!(response.status(), 202);
        tokio::time::timeout(Duration::from_secs(30), async {
            while marker(server, key).await["state"] == "absent" {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("state visible");
        let before = mock.received_requests().await.expect("requests").len();
        let blocked = server
            .invoke(
                &Call::object("Szamlazz.Order", key, "create_invoice"),
                Some(&request()),
                None,
            )
            .await;
        assert_eq!(blocked.status, 500, "{blocked:?}");
        assert_eq!(
            mock.received_requests().await.expect("requests").len(),
            before
        );
        assert_eq!(marker(server, key).await["marker"], retained);
    }
}

async fn failure_case(server: &Restate, mock: &wiremock::MockServer, lab: &Arc<Lab>, key: &str) {
    mount(mock, lab, key).await;
    lab.change(key, |s| {
        s.visible = key == "visible-crash";
        s.deduplicate = key == "dedup-crash";
        s.lost_answer = key == "recorded-unknown";
        s.no_effect = matches!(key, "no-effect" | "cancelled");
        s.later_code = match key {
            "later-refusal" => Some("259"),
            "later-credentials" => Some("3"),
            "later-duplicate" => Some("71"),
            _ => None,
        };
        s.hold = if key == "old-worker" {
            Some(WriteCheckpoint::OrdinaryGuardsPassed)
        } else if key.ends_with("crash") || key.starts_with("later-") || key.starts_with("guard-") {
            Some(WriteCheckpoint::Sent)
        } else {
            None
        };
    });
    let call = Call::object("Szamlazz.Order", key, "create_invoice");
    let started = server
        .invoke(&call.send(), Some(&request()), Some(key))
        .await;
    let id = started.invocation_id();
    if lab.change(key, |s| s.hold.is_some())
        || key == "old-worker"
        || key.ends_with("crash")
        || key.starts_with("later-")
        || key.starts_with("guard-")
    {
        tokio::time::timeout(Duration::from_secs(30), lab.reached.notified())
            .await
            .expect("held checkpoint");
        if let Some(guard) = key.strip_prefix("guard-") {
            if guard == "initialization" {
                lab.credentials_missing.store(true, Ordering::SeqCst);
            } else {
                lab.change(key, |s| {
                    s.guard = Some(match guard {
                        "prepayment" => "prepayment",
                        "final" => "final",
                        "proforma" => "proforma",
                        "collision" => "invoice",
                        "query" => "query",
                        "foreign" => "foreign",
                        _ => panic!("guard"),
                    });
                });
            }
        }
        lab.keep_old.store(key == "old-worker", Ordering::SeqCst);
        lab.cut.notify_one();
    }
    let retained = matches!(key, "recorded-unknown" | "no-effect" | "cancelled")
        || key.starts_with("later-")
        || key.starts_with("guard-");
    if retained {
        pause(server, id).await;
        let observed = marker(server, key).await;
        assert_eq!(observed["state"], "unresolved", "{key}: {observed}");
        assert_eq!(
            observed["marker"]["execution_contract"],
            "request_response_ordinary_v1"
        );
        let expected = if key.starts_with("later-") { 2 } else { 1 };
        assert_eq!(lab.counts(key).0, expected, "{key}");
        let journal = server.admin().journal(id).await;
        assert!(
            restate_e2e_harness::run_result(&journal, "create-ordinary-request-response").is_some(),
            "uncertainty is recorded data: {key}"
        );
        server.admin().resume(id).await;
        pause(server, id).await;
        assert_eq!(
            lab.counts(key).0,
            expected,
            "resume must be read-only: {key}"
        );
        if matches!(key, "no-effect" | "cancelled") {
            if key == "cancelled" {
                server.admin().cancel(id).await;
            } else {
                server.admin().kill(id).await;
            }
            server.admin().await_status(id, &["completed"]).await;
            let successor = server
                .invoke(&call, Some(&request()), Some(&format!("new-{key}")))
                .await;
            assert_eq!(successor.status, 500, "{successor:?}");
            assert!(
                successor.body["message"]
                    .as_str()
                    .expect("fault")
                    .contains("outcome_unknown")
            );
            assert_eq!(marker(server, key).await["state"], "unresolved");
            assert_eq!(lab.counts(key), (1, 0));
            return;
        }
        lab.change(key, |s| {
            s.visible = true;
            s.guard = None;
        });
        lab.credentials_missing.store(false, Ordering::SeqCst);
        server.admin().resume(id).await;
    }
    server.admin().await_status(id, &["completed"]).await;
    assert_eq!(marker(server, key).await["state"], "absent", "{key}");
    if key == "old-worker" {
        assert_eq!(lab.counts(key), (1, 1));
        lab.release.notify_one();
        let tasks = std::mem::take(&mut *lab.old_tasks.lock().expect("old tasks"));
        assert_eq!(tasks.len(), 1);
        for task in tasks {
            tokio::time::timeout(Duration::from_secs(30), task)
                .await
                .expect("old execution finishes")
                .expect("old task");
        }
        assert_eq!(
            lab.counts(key),
            (2, 2),
            "positive completion does not fence surviving old work"
        );
    }
    let expected_sends = if matches!(key, "invisible-crash" | "dedup-crash" | "old-worker")
        || key.starts_with("later-")
    {
        2
    } else {
        1
    };
    let expected_documents = if matches!(key, "invisible-crash" | "old-worker") {
        2
    } else {
        1
    };
    assert_eq!(
        lab.counts(key),
        (expected_sends, expected_documents),
        "{key}"
    );
    let before = mock.received_requests().await.expect("requests").len();
    let fetches = lab.fetches.load(Ordering::SeqCst);
    lab.credentials_missing.store(true, Ordering::SeqCst);
    let replay = server.invoke(&call, Some(&request()), Some(key)).await;
    lab.credentials_missing.store(false, Ordering::SeqCst);
    assert_eq!(replay.status, 200, "{key}: {replay:?}");
    assert_eq!(
        replay.body["outcome"],
        if retained { "reconciled" } else { "issued" },
        "{key}"
    );
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before,
        "completed replay has no provider I/O"
    );
    assert_eq!(
        lab.fetches.load(Ordering::SeqCst),
        fetches,
        "completed replay needs no credentials"
    );
    let journal = format!("{:?}", server.admin().journal(id).await);
    assert!(!journal.contains("EXPERIMENT-NOT-A-REAL-KEY"));
    assert!(!journal.contains("Experimental Buyer"));
    println!(
        "actual RequestResponse {key}: sends={expected_sends}, documents={expected_documents}, outcome={}",
        replay.body["outcome"]
    );
}

#[tokio::test]
#[ignore = "requires real Restate; fake provider only"]
async fn e2e_request_response_actual_order() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let server = launcher
        .launch(&ServerSpec {
            name: "actual-request-response",
            features: &[(VQUEUES, true), (PROTOCOL_V7, true)],
            env: &[],
        })
        .await;
    let mock = wiremock::MockServer::start().await;
    wiremock::Mock::given(wiremock::matchers::body_string_contains(
        "action-szamla_agent_xml",
    ))
    .respond_with(common::not_found())
    .with_priority(100)
    .mount(&mock)
    .await;
    common::create_for("normal")
        .respond_with(common::created("SZ-1", "1000", "1270"))
        .mount(&mock)
        .await;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let uri = format!("http://{}", listener.local_addr().expect("address"));
    let lab = Arc::new(Lab::default());
    let host = Host {
        endpoints: Arc::new(vec![
            endpoint(&mock.uri(), lab.clone(), 0),
            endpoint(&mock.uri(), lab.clone(), 1),
        ]),
        exchanges: Arc::default(),
        lab: lab.clone(),
    };
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(async move {
        axum::serve(listener, Router::new().fallback(handle).with_state(host))
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .expect("host");
    });
    server.admin().register(&uri).await;
    admission_cases(&server, &mock, &lab).await;
    let call = Call::object("Szamlazz.Order", "normal", "create_invoice");
    let started = server
        .invoke(&call.send(), Some(&request()), Some("normal"))
        .await;
    let id = started.invocation_id();
    server
        .admin()
        .await_status(id, &["completed", "paused"])
        .await;
    assert_eq!(
        marker(&server, "normal").await["state"],
        "absent",
        "ordinary issuance must progress through buffered RequestResponse"
    );
    let response = server.invoke(&call, Some(&request()), Some("normal")).await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["invoice_number"], "SZ-1");
    let requests = mock.received_requests().await.expect("requests");
    assert_eq!(requests.iter().filter(|r| String::from_utf8_lossy(&r.body).contains("name=\"action-xmlagentxmlfile\"")).count(),1);
    mount(&mock, &lab, "duplicate-evidence").await;
    lab.change("duplicate-evidence", |s| s.duplicate_evidence = true);
    let duplicate = server
        .invoke(
            &Call::object("Szamlazz.Order", "duplicate-evidence", "create_invoice").send(),
            Some(&request()),
            Some("duplicate-evidence"),
        )
        .await;
    server
        .admin()
        .await_status(duplicate.invocation_id(), &["completed", "paused"])
        .await;
    assert_eq!(
        marker(&server, "duplicate-evidence").await["state"],
        "absent",
        "already-found duplicate evidence must settle without another query"
    );
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "duplicate-evidence", "create_invoice"),
            Some(&request()),
            Some("duplicate-evidence"),
        )
        .await;
    assert_eq!(response.body["outcome"], "reconciled", "{response:?}");
    extensions::reissue(&server, &mock).await;
    proformas::create(&server, &mock).await;
    chain::issue(&server, &mock).await;
    chain::final_invoice(&server, &mock).await;
    chain::interrupted(&server, &mock, &lab).await;
    chain::reissue_and_recover(&server, &mock).await;
    proformas::delete(&server, &mock).await;
    proformas::guards(&server, &mock).await;
    proformas::interruptions(&server, &mock, &lab).await;
    extensions::conversion(&server, &mock).await;
    extensions::recovery(&server, &mock).await;
    extensions::marker_compatibility(&server, &mock).await;
    extensions::interrupted(&server, &mock, &lab).await;
    for key in [
        "visible-crash",
        "invisible-crash",
        "dedup-crash",
        "recorded-unknown",
        "no-effect",
        "later-refusal",
        "later-credentials",
        "later-duplicate",
        "old-worker",
        "guard-prepayment",
        "guard-final",
        "guard-proforma",
        "guard-collision",
        "guard-query",
        "guard-foreign",
        "guard-initialization",
        "cancelled",
    ] {
        failure_case(&server, &mock, &lab, key).await;
    }
    stop.send(()).expect("shutdown");
    serving.await.expect("host joined");
    server.finish().await;
}
