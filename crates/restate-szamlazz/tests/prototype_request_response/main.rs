//! THROWAWAY #247: real Restate `RequestResponse`, current Order versus a narrow
//! replay-risk issuance shell. Not a proposed production contract or WASM port.
//! Run instructions and limitations: the adjacent README.

#![allow(clippy::too_many_lines, missing_docs)]

#[path = "../common/mod.rs"]
mod common;
mod vendor;

use std::collections::BTreeMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

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
use restate_szamlazz::{
    Agent, Order,
    account::{Accounts, StaticConfig, StaticResolver},
    config::WorkerConfig,
};
use serde_json::{Value, json};
use szamlazz_agent::{
    Client, Credentials, InvoiceSelector,
    ops::{invoice::CreationOutcome, query_xml::QueryInvoiceXml},
};
use tokio::{sync::Notify, task::JoinHandle};

const NARROW: &str = "NarrowPrototype";
const MARKER: &str = "prototype-unresolved";

#[derive(Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent fake-provider and interruption controls in a throwaway lab"
)]
struct Case {
    sends: usize,
    documents: Vec<String>,
    visible: bool,
    deduplicate: bool,
    lost_answer: bool,
    no_effect: bool,
    reject_second: bool,
    hold_before_send: bool,
    hold_after_send: bool,
    events: Vec<String>,
}

#[derive(Default)]
struct Lab {
    cases: Mutex<BTreeMap<String, Case>>,
    reached: Notify,
    release: Notify,
    cut: Notify,
    keep_old: AtomicBool,
    old_tasks: Mutex<Vec<JoinHandle<http::Response<Body>>>>,
    exchanges: AtomicUsize,
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

    fn state(&self, key: &str) -> Value {
        self.change(key, |s| {
            json!({"sends": s.sends, "documents": s.documents,
            "visible": s.visible, "events": s.events})
        })
    }

    async fn hold(&self, key: &str, before: bool) {
        let hold = self.change(key, |s| {
            let flag = if before {
                &mut s.hold_before_send
            } else {
                &mut s.hold_after_send
            };
            std::mem::take(flag)
        });
        if hold {
            self.reached.notify_one();
            self.release.notified().await;
        }
    }
}

// This experimental shell deliberately uses the low-level client for writes:
// Gateway::create_once's public permission contract forbids the speculative
// replay send being investigated. Real serialization/parsing and Gateway's
// read-only intent verification are reused. It is NOT a second send gate.
struct NarrowPrototype {
    lab: Arc<Lab>,
    provider: String,
    worker: usize,
}

#[restate_sdk::object]
impl NarrowPrototype {
    #[handler(journal_retention = "1d")]
    async fn issue(&self, ctx: ObjectContext<'_>) -> Result<Json<Value>, HandlerError> {
        let key = ctx.key().to_owned();
        self.lab.change(&key, |s| {
            s.events
                .push(format!("execution on worker {}", self.worker));
        });
        if ctx.get::<String>(MARKER).await?.is_some() {
            return Ok(Json(json!({"outcome": "blocked"})));
        }
        ctx.set(MARKER, ctx.invocation_id().to_owned());
        // Portable durability barrier: completed replay does not need a local permit.
        ctx.run(|| async { Ok(()) })
            .name("marker-committed")
            .await?;

        let Json(result) = ctx.run(|| async {
            self.lab.change(&key, |s| s.events.push(format!("write closure on worker {}", self.worker)));
            let gateway = gateway(&self.provider);
            let order = key.parse().expect("order");
            let external = external_of(&key);
            let (kind, base) = kind_of(&key);
            let operation = restate_szamlazz::gateway::recovery::WriteOperation::Create {
                kind,
                expected_number: None,
                corrected_number: base.map(str::to_owned),
            };
            let evidence = gateway.reconcile(restate_szamlazz::gateway::ReconciliationRequest {
                external_id: &external, order: &order, operation: &operation, candidate: None,
            }).await;
            if let Ok(restate_szamlazz::gateway::ReconciliationOutcome::Created(found)) = evidence {
                return Ok(Json(json!({"outcome":"found", "number": found.number})));
            }
            // A separate query distinguishes actual absence from blocked/failed
            // verification. Both reads execute anew on an unfinished-run replay.
            let client = agent_client(&self.provider);
            match client.send(&QueryInvoiceXml::new(InvoiceSelector::ExternalId(external.clone()))).await {
                Err(szamlazz_agent::ClientError::Api(api)) if api.code.to_string() == "7" => {},
                _ => return Ok(Json(json!({"outcome":"uncertain", "reason":"query did not establish absence"}))),
            }
            self.lab.hold(&key, true).await;
            let create = gateway.build_create(
                kind, &document(), &order,
                &restate_szamlazz::identity::ExternalId::new(external),
                restate_szamlazz::gateway::DocumentRefs {corrected:base, ..Default::default()},
            ).expect("create");
            let answer = client.send(&create).await;
            self.lab.hold(&key, false).await;
            // Later refusal is NOT negative settlement of a possible earlier send.
            let result = match answer {
                Ok(CreationOutcome::Issued(found)) => json!({"outcome":"issued", "number":found.invoice_number.as_str()}),
                _ => json!({"outcome":"uncertain", "reason":"write did not establish issuance"}),
            };
            Ok(Json(result))
        }).name("write").retry_policy(RunRetryPolicy::new().max_attempts(1)).await?;

        let result = if result["outcome"] == "uncertain" {
            let Json(found) = ctx
                .run(|| async {
                    self.lab.change(&key, |s| {
                        s.events.push(format!(
                            "read-only reconciliation on worker {}",
                            self.worker
                        ));
                    });
                    let gateway = gateway(&self.provider);
                    let order = key.parse().expect("order");
                    let (kind, base) = kind_of(&key);
                    let operation = restate_szamlazz::gateway::recovery::WriteOperation::Create {
                        kind,
                        expected_number: None,
                        corrected_number: base.map(str::to_owned),
                    };
                    match gateway
                        .reconcile(restate_szamlazz::gateway::ReconciliationRequest {
                            external_id: &external_of(&key),
                            order: &order,
                            operation: &operation,
                            candidate: None,
                        })
                        .await
                    {
                        Ok(restate_szamlazz::gateway::ReconciliationOutcome::Created(found)) => {
                            Ok(Json(json!({"outcome":"reconciled", "number":found.number})))
                        }
                        _ => Err(HandlerError::from(std::io::Error::other(
                            "prototype: still uncertain",
                        ))),
                    }
                })
                .name("reconcile")
                .await?;
            found
        } else {
            result
        };
        // This is the risk acceptance under investigation: matching issuance is
        // enough to finish, but proves neither uniqueness nor no delayed send.
        ctx.clear(MARKER);
        Ok(Json(result))
    }

    #[handler]
    async fn observe(&self, ctx: SharedObjectContext<'_>) -> Result<Json<Value>, HandlerError> {
        Ok(Json(json!({"marker":ctx.get::<String>(MARKER).await?})))
    }
}

fn kind_of(key: &str) -> (restate_szamlazz::identity::IssuedKind, Option<&'static str>) {
    if key == "corrective-crash" {
        (
            restate_szamlazz::identity::IssuedKind::Corrective,
            Some("BASE-1"),
        )
    } else {
        (restate_szamlazz::identity::IssuedKind::Invoice, None)
    }
}

fn external_of(key: &str) -> String {
    if key == "corrective-crash" {
        format!("proto:{key}:corrective:c1")
    } else {
        format!("proto:{key}:invoice")
    }
}

fn document() -> restate_szamlazz::contract::DocumentInput {
    use restate_szamlazz::contract::{BuyerInput, DocumentInput, LineItemInput, PaymentMethod};
    DocumentInput::new(
        BuyerInput::new("Prototype Buyer", "1000", "City", "Address"),
        vec![LineItemInput::new(
            "Prototype",
            rust_decimal::Decimal::ONE,
            "db",
            rust_decimal::Decimal::from(1000),
            "27",
        )],
        jiff::civil::date(2026, 9, 14),
        jiff::civil::date(2026, 9, 21),
        PaymentMethod::Transfer,
    )
}

fn agent_client(provider: &str) -> Client {
    Client::builder()
        .credentials(Credentials::agent_key("PROTOTYPE-NOT-A-REAL-KEY"))
        .endpoint(provider)
        .http_client(
            common::http_builder()
                .retry(szamlazz_agent::reqwest::retry::never())
                .build()
                .expect("HTTP"),
        )
        .build()
        .expect("client")
}

fn gateway(provider: &str) -> restate_szamlazz::gateway::Gateway {
    let mut account = restate_szamlazz::account::Account::new("prototype", "prototype");
    account.endpoint = provider.parse().expect("endpoint");
    restate_szamlazz::gateway::Gateway::open_with_http(
        account,
        Credentials::agent_key("PROTOTYPE-NOT-A-REAL-KEY"),
        common::http_builder()
            .retry(szamlazz_agent::reqwest::retry::never())
            .build()
            .expect("HTTP"),
    )
    .expect("gateway")
}

fn endpoint(provider: &str, lab: Arc<Lab>, worker: usize) -> Endpoint {
    let config: StaticConfig = serde_json::from_value(json!({"account":{
        "id":"prototype", "agent_key":"PROTOTYPE-NOT-A-REAL-KEY", "endpoint":provider
    }}))
    .expect("config");
    let resolver = Arc::new(StaticResolver::try_from(config).expect("resolver"));
    let accounts = Accounts::new(resolver.clone(), resolver);
    let config = WorkerConfig::new("proto".parse().expect("namespace"))
        .validate()
        .expect("worker config");
    let policy = HandlerOptions::default()
        .retry_policy_initial_interval(Duration::from_millis(100))
        .retry_policy_max_interval(Duration::from_millis(100))
        .retry_policy_max_attempts(2)
        .retry_policy_pause_on_max_attempts();
    Endpoint::builder()
        .bind(
            Order::from_parts(accounts.clone(), config.clone())
                .into_service_definition()
                .options(ServiceOptions::default().handler("create_invoice", policy.clone())),
        )
        .bind(Agent::from_parts(accounts, config))
        .bind(
            NarrowPrototype {
                lab,
                provider: provider.into(),
                worker,
            }
            .into_service_definition()
            .options(ServiceOptions::default().handler("issue", policy)),
        )
        .build()
}

#[derive(Clone)]
struct Host {
    endpoints: Arc<Vec<Endpoint>>,
    lab: Arc<Lab>,
}

async fn handle(State(host): State<Host>, request: http::Request<Body>) -> http::Response<Body> {
    let (parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, 16 * 1024 * 1024)
        .await
        .expect("buffer request");
    let index = host.lab.exchanges.fetch_add(1, Ordering::SeqCst) % 2;
    let endpoint = host.endpoints[index].clone();
    let response = endpoint.handle_with_options(
        http::Request::from_parts(parts, Full::new(bytes)),
        HandleOptions {
            protocol_mode: ProtocolMode::RequestResponse,
        },
    );
    // Buffer all SDK output: no hidden bidirectional acknowledgement path.
    let mut task = tokio::spawn(async move {
        let (parts, body) = response.into_parts();
        let bytes = body.collect().await.expect("SDK output").to_bytes();
        http::Response::from_parts(parts, Body::from(bytes))
    });
    tokio::select! {
        result = &mut task => result.expect("SDK task"),
        () = host.lab.cut.notified() => {
            if host.lab.keep_old.swap(false, Ordering::SeqCst) {
                host.lab.old_tasks.lock().expect("old tasks").push(task);
            } else {
                task.abort();
                assert!(task.await.expect_err("aborted task").is_cancelled());
            }
            http::Response::builder().status(503).body(Body::from("PROTOTYPE: interrupted execution")).expect("response")
        }
    }
}

async fn mount(mock: &wiremock::MockServer, lab: &Arc<Lab>, key: &str) {
    let state = lab.clone();
    let name = key.to_owned();
    let match_name = key.to_owned();
    wiremock::Mock::given(wiremock::matchers::body_string_contains(
        "action-szamla_agent_xml",
    ))
    .and(move |request: &wiremock::Request| {
        let body = String::from_utf8_lossy(&request.body);
        body.contains(&format!(":{match_name}:")) || body.contains(&format!(">{match_name}<"))
    })
    .respond_with(move |_: &wiremock::Request| {
        state.change(&name, |s| {
            if s.visible
                && let Some(number) = s.documents.last()
            {
                let (_, base) = kind_of(&name);
                let mut doc =
                    common::Doc::of(number, if base.is_some() { "HS" } else { "SZ" }, &name);
                doc.referenced_invoice = base;
                doc.response()
            } else {
                common::not_found()
            }
        })
    })
    .mount(mock)
    .await;
    let state = lab.clone();
    let name = key.to_owned();
    common::create_for(key)
        .respond_with(move |_: &wiremock::Request| {
            state.change(&name, |s| {
                s.sends += 1;
                s.events.push(format!("provider received send {}", s.sends));
                if s.no_effect {
                    return wiremock::ResponseTemplate::new(500);
                }
                if s.reject_second && s.sends > 1 {
                    return common::api_error("259", "scripted rejection");
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

async fn wait_signal(signal: &Notify) {
    tokio::time::timeout(Duration::from_secs(30), signal.notified())
        .await
        .expect("checkpoint reached");
}

async fn begin(server: &Restate, service: &str, key: &str, body: Option<&Value>) -> String {
    let handler = if service == NARROW {
        "issue"
    } else {
        "create_invoice"
    };
    server
        .invoke(&Call::object(service, key, handler).send(), body, Some(key))
        .await
        .invocation_id()
        .into()
}

async fn completed(server: &Restate, id: &str) {
    server
        .admin()
        .await_status_with_timeout(id, &["completed"], Duration::from_secs(45))
        .await;
}

async fn report(server: &Restate, lab: &Lab, service: &str, key: &str, id: &str) -> Value {
    let observe = if service == NARROW {
        "observe"
    } else {
        "observe_unresolved"
    };
    let marker = server
        .invoke(&Call::object(service, key, observe), None, None)
        .await
        .body;
    let journal = server.admin().journal(id).await;
    let value = json!({"case":key, "state":lab.state(key), "marker":marker,
        "runs":server.admin().runs(id).await,
        "write_result_recorded":restate_e2e_harness::run_result(&journal, "write").is_some(),
        "invocation":format!("{:?}", server.admin().invocation(id).await)});
    println!(
        "PROTOTYPE {}",
        serde_json::to_string(&value).expect("report")
    );
    value
}

#[tokio::test]
#[ignore = "throwaway #247; requires RESTATE_SERVER_BIN, uses no live provider"]
async fn request_response_prototype() {
    let launcher =
        launcher_or_skip(ReusePolicy::Never).expect("prototype requires a real Restate binary");
    let server = launcher
        .launch(&ServerSpec {
            name: "request-response-prototype",
            features: &[(VQUEUES, true), (PROTOCOL_V7, true)],
            env: &[],
        })
        .await;
    let mock = wiremock::MockServer::start().await;
    let lab = Arc::new(Lab::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("listener");
    let uri = format!("http://{}", listener.local_addr().expect("address"));
    let host = Host {
        endpoints: Arc::new(vec![
            endpoint(&mock.uri(), lab.clone(), 0),
            endpoint(&mock.uri(), lab.clone(), 1),
        ]),
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
    let mut rows = Vec::new();

    // The actual Agent proves ordinary durable reads progress on this host.
    wiremock::Mock::given(wiremock::matchers::body_string_contains(
        "proto:check-account",
    ))
    .respond_with(common::not_found())
    .mount(&mock)
    .await;
    let probe = server
        .invoke(
            &Call::service("Szamlazz.Agent", "check_account"),
            None,
            None,
        )
        .await;
    assert_eq!(probe.body["credentials"]["state"], "ok", "{probe:?}");

    // Current production Order: no injected fault, yet arming cannot send.
    let key = "strict";
    mount(&mock, &lab, key).await;
    let id = begin(
        &server,
        "Szamlazz.Order",
        key,
        Some(&json!({"document":document(),"options":{}})),
    )
    .await;
    server.admin().await_status(&id, &["paused"]).await;
    let row = report(&server, &lab, "Szamlazz.Order", key, &id).await;
    assert_eq!(row["state"]["sends"], 0);
    assert_eq!(row["marker"]["state"], "unresolved");
    rows.push(row);
    server.admin().kill(&id).await;

    for key in [
        "normal",
        "visible-crash",
        "invisible-crash",
        "corrective-crash",
        "dedup-crash",
        "recorded-unknown",
        "no-effect",
        "later-rejection",
        "old-worker",
    ] {
        mount(&mock, &lab, key).await;
        lab.change(key, |s| {
            s.visible = matches!(key, "normal" | "visible-crash");
            s.deduplicate = key == "dedup-crash";
            s.lost_answer = key == "recorded-unknown";
            s.no_effect = key == "no-effect";
            s.reject_second = key == "later-rejection";
            s.hold_before_send = key == "old-worker";
            s.hold_after_send = matches!(
                key,
                "visible-crash"
                    | "invisible-crash"
                    | "corrective-crash"
                    | "dedup-crash"
                    | "later-rejection"
            );
        });
        let id = begin(&server, NARROW, key, None).await;
        if matches!(
            key,
            "visible-crash"
                | "invisible-crash"
                | "corrective-crash"
                | "dedup-crash"
                | "later-rejection"
                | "old-worker"
        ) {
            wait_signal(&lab.reached).await;
            lab.keep_old.store(key == "old-worker", Ordering::SeqCst);
            lab.cut.notify_one();
        }
        if matches!(key, "recorded-unknown" | "no-effect" | "later-rejection") {
            server.admin().await_status(&id, &["paused"]).await;
            let row = report(&server, &lab, NARROW, key, &id).await;
            assert!(row["marker"]["marker"].is_string());
            assert_eq!(
                row["state"]["sends"],
                if key == "later-rejection" { 2 } else { 1 }
            );
            // Resume while absent remains read-only, even after later rejection.
            server.admin().resume(&id).await;
            server.admin().await_status(&id, &["paused"]).await;
            assert_eq!(lab.state(key)["sends"], row["state"]["sends"]);
            rows.push(row);
            if key == "no-effect" {
                server.admin().kill(&id).await;
                server.admin().await_status(&id, &["completed"]).await;
                let successor = server
                    .invoke(
                        &Call::object(NARROW, key, "issue"),
                        None,
                        Some("new-decision-no-effect"),
                    )
                    .await;
                assert_eq!(successor.body["outcome"], "blocked");
                let mut row = report(&server, &lab, NARROW, key, &id).await;
                row["successor"] = successor.body;
                assert_eq!(lab.state(key)["sends"], 1);
                rows.push(row);
                continue;
            }
            lab.change(key, |s| s.visible = true);
            server.admin().resume(&id).await;
        }
        completed(&server, &id).await;
        if key == "old-worker" {
            assert_eq!(lab.state(key)["sends"], 1);
            // Replacement finished while old execution still owns a live closure.
            lab.release.notify_one();
            let tasks = std::mem::take(&mut *lab.old_tasks.lock().expect("tasks"));
            for task in tasks {
                tokio::time::timeout(Duration::from_secs(30), task)
                    .await
                    .expect("old task finishes")
                    .expect("old SDK task");
            }
            assert_eq!(lab.state(key)["sends"], 2);
        }
        let mut row = report(&server, &lab, NARROW, key, &id).await;
        assert!(row["marker"]["marker"].is_null());
        let expected = if matches!(
            key,
            "invisible-crash"
                | "corrective-crash"
                | "dedup-crash"
                | "later-rejection"
                | "old-worker"
        ) {
            2
        } else {
            1
        };
        assert_eq!(row["state"]["sends"], expected);
        // Retained ingress identity replays output, not another provider send.
        let replay = server
            .invoke(&Call::object(NARROW, key, "issue"), None, Some(key))
            .await;
        assert_eq!(replay.status, 200);
        assert_eq!(lab.state(key)["sends"], expected);
        row["response"] = replay.body;
        let document_count = if matches!(key, "invisible-crash" | "corrective-crash" | "old-worker")
        {
            2
        } else {
            1
        };
        assert_eq!(
            lab.state(key)["documents"]
                .as_array()
                .expect("documents")
                .len(),
            document_count
        );
        rows.push(row);
    }
    stop.send(()).expect("stop host");
    serving.await.expect("host joined");
    server.finish().await;
    if let Ok(path) = std::env::var("PROTOTYPE_REPORT_PATH") {
        std::fs::write(path,serde_json::to_string_pretty(&json!({
            "prototype":"#247 RequestResponse narrow replay", "server":"1.7.8",
            "sdk":"0.12.0", "shared_core":"7.0.3", "protocol":"RequestResponse v7",
            "limitations":"Fake provider, native buffered host, two in-process endpoint instances. Not a workerd test or a measured provider risk rate.",
            "agent_check_account":probe.body, "observations":rows
        })).expect("JSON")).expect("write requested prototype report");
    }
}
