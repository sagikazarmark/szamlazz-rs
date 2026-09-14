//! Real Restate + workerd acceptance; requires the built examples/workers host.
#![allow(missing_docs, clippy::too_many_lines)]

#[path = "../common/mod.rs"]
mod common;
use restate_e2e_harness::{
    Call, ServerSpec,
    gate::{PROTOCOL_V7, ReusePolicy, SCOPED_VIRTUAL_OBJECTS, VQUEUES, launcher_or_skip},
};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

struct Host(Child);
impl Host {
    fn finish(&mut self) {
        self.0.stdin.take();
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            if let Some(status) = self.0.try_wait().expect("host status") {
                assert!(status.success(), "host shutdown: {status}");
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("Workers host did not shut down within 15s");
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn request() -> Value {
    json!({"document":{
        "buyer":{"name":"WORKER PRIVATE BUYER","zip":"1000","city":"City","address":"Address"},
        "items":[{"name":"Item","quantity":"1","unit":"db","unit_price":"1000","vat_rate":"27"}],
        "fulfillment_date":"2026-09-14","due_date":"2026-09-21","payment_method":"transfer"
    },"options":{"proforma":"none"}})
}

#[tokio::test]
#[ignore = "requires real Restate, node and built Workers bundle"]
async fn e2e_workers_signed_scoped_ordinary() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let mock = wiremock::MockServer::start().await;
    let mut host = Host(
        Command::new("node")
            .arg("test/host.mjs")
            .current_dir(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../examples/workers"
            ))
            .env("MOCK_URL", mock.uri())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("node host"),
    );
    let stdout = host.0.stdout.take().expect("stdout");
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = ready_tx.send(result);
    });
    let line = tokio::time::timeout(Duration::from_secs(30), ready_rx)
        .await
        .expect("host startup deadline")
        .expect("host startup task")
        .expect("host startup");
    let ready: Value = serde_json::from_str(&line).expect("host readiness");
    let uri = ready["uri"].as_str().expect("URI");
    let signing: &'static str = Box::leak(
        format!(
            "RESTATE_WORKER__INVOKER__REQUEST_IDENTITY_PRIVATE_KEY_PEM_FILE={}",
            ready["keyPath"].as_str().expect("key")
        )
        .into_boxed_str(),
    );
    let server = launcher
        .launch(&ServerSpec {
            name: "workers",
            features: &[
                (VQUEUES, true),
                (PROTOCOL_V7, true),
                (SCOPED_VIRTUAL_OBJECTS, true),
            ],
            env: Box::leak(vec![signing].into_boxed_slice()),
        })
        .await;
    let registration = common::http_client()
        .post(format!("{}/deployments", server.admin().base()))
        .json(&json!({"uri":uri,"force":true,"use_http_11":true}))
        .send()
        .await
        .expect("register HTTP/1 Workers host");
    let status = registration.status();
    let body = registration.text().await.expect("registration body");
    assert!(status.is_success(), "{status}: {body}");
    let unsigned = common::http_client()
        .post(format!("{uri}/invoke/Szamlazz.Agent/check_account"))
        .send()
        .await
        .expect("unsigned request");
    assert_eq!(unsigned.status(), 401);
    wiremock::Mock::given(wiremock::matchers::body_string_contains(
        "action-szamla_agent_xml",
    ))
    .respond_with(common::not_found())
    .with_priority(100)
    .mount(&mock)
    .await;
    for scope in ["alpha", "beta"] {
        let probe = server
            .invoke(
                &Call::service("Szamlazz.Agent", "check_account").scoped(scope),
                None,
                None,
            )
            .await;
        assert_eq!(probe.body["scope"], scope, "{probe:?}");
        assert_eq!(probe.body["credentials"]["state"], "ok", "{probe:?}");
    }
    for mode in ["retry", "resolver-hang", "store-hang"] {
        common::http_client()
            .post(format!("{uri}/__credentials/{mode}"))
            .send()
            .await
            .expect("credential control request")
            .error_for_status()
            .expect("credential control status");
        let started = std::time::Instant::now();
        let response = server
            .invoke(
                &Call::service("Szamlazz.Agent", "check_account").scoped("alpha"),
                None,
                None,
            )
            .await;
        if mode == "retry" {
            assert_eq!(response.body["credentials"]["state"], "ok", "{response:?}");
            assert!(started.elapsed() >= Duration::from_millis(400));
        } else {
            assert_eq!(response.status, 503, "{response:?}");
            assert!(started.elapsed() >= Duration::from_secs(9));
            assert!(started.elapsed() < Duration::from_secs(45));
        }
        assert!(
            !response
                .body
                .to_string()
                .contains("ACCEPTANCE-PRIVATE-SOURCE")
        );
    }
    common::http_client()
        .post(format!("{uri}/__credentials/normal"))
        .send()
        .await
        .expect("restore credentials request")
        .error_for_status()
        .expect("restore credentials status");
    for scope in ["alpha", "beta"] {
        common::create_for("SAME")
            .and(wiremock::matchers::body_string_contains(
                if scope == "alpha" {
                    "WORKER-ALPHA-TEST-KEY"
                } else {
                    "WORKER-BETA-TEST-KEY"
                },
            ))
            .respond_with(common::created(
                if scope == "alpha" {
                    "ALPHA-1"
                } else {
                    "BETA-1"
                },
                "1000",
                "1270",
            ))
            .expect(1)
            .mount(&mock)
            .await;
        let call = Call::object("Szamlazz.Order", "SAME", "create_invoice").scoped(scope);
        let response = server
            .invoke(&call, Some(&request()), Some("same-key"))
            .await;
        assert_eq!(response.body["outcome"], "issued", "{response:?}");
        let before = mock.received_requests().await.expect("requests").len();
        common::http_client()
            .post(format!("{uri}/__credentials/missing"))
            .send()
            .await
            .expect("remove credentials")
            .error_for_status()
            .expect("removed");
        let replay = server
            .invoke(&call, Some(&request()), Some("same-key"))
            .await;
        assert_eq!(replay.body, response.body);
        common::http_client()
            .post(format!("{uri}/__credentials/normal"))
            .send()
            .await
            .expect("restore credentials")
            .error_for_status()
            .expect("restored");
        assert_eq!(
            mock.received_requests().await.expect("requests").len(),
            before
        );
        let journal = format!(
            "{:?}",
            server.admin().journal(response.invocation_id()).await
        );
        assert!(
            !journal.contains("WORKER-ALPHA-TEST-KEY") && !journal.contains("WORKER PRIVATE BUYER")
        );
    }
    let sent = Arc::new(AtomicUsize::new(0));
    let visible = Arc::new(AtomicBool::new(false));
    // Provider has accepted a write but its answer is still in flight when the
    // JS runtime is replaced. A freshly visible holder prevents another send.
    let crash_sent = Arc::new(AtomicBool::new(false));
    let sending = crash_sent.clone();
    common::create_for("CRASH")
        .respond_with(move |_: &wiremock::Request| {
            sending.store(true, Ordering::SeqCst);
            common::created("CRASH-1", "1000", "1270").set_delay(Duration::from_secs(20))
        })
        .expect(1)
        .mount(&mock)
        .await;
    let sending = crash_sent.clone();
    common::external_id_query("workers:CRASH:invoice")
        .respond_with(move |_: &wiremock::Request| {
            if sending.load(Ordering::SeqCst) {
                common::Doc::of("CRASH-1", "SZ", "CRASH").response()
            } else {
                common::not_found()
            }
        })
        .with_priority(1)
        .mount(&mock)
        .await;
    let crash = Call::object("Szamlazz.Order", "CRASH", "create_invoice").scoped("alpha");
    let crashed = server
        .invoke(&crash.send(), Some(&request()), Some("crash"))
        .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !crash_sent.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("first send reached provider");
    common::http_client()
        .post(format!("{uri}/__interrupt"))
        .send()
        .await
        .expect("interrupt request")
        .error_for_status()
        .expect("interrupt status");
    server
        .admin()
        .await_status(crashed.invocation_id(), &["completed"])
        .await;
    let response = server.invoke(&crash, Some(&request()), Some("crash")).await;
    assert_eq!(response.body["outcome"], "issued", "{response:?}");
    assert_eq!(response.body["invoice_number"], "CRASH-1");
    let count = sent.clone();
    common::create_for("UNCERTAIN")
        .respond_with(move |_: &wiremock::Request| {
            count.fetch_add(1, Ordering::SeqCst);
            wiremock::ResponseTemplate::new(500)
        })
        .mount(&mock)
        .await;
    let shown = visible.clone();
    common::external_id_query("workers:UNCERTAIN:invoice")
        .respond_with(move |_: &wiremock::Request| {
            if shown.load(Ordering::SeqCst) {
                common::Doc::of("RECOVERED-1", "SZ", "UNCERTAIN").response()
            } else {
                common::not_found()
            }
        })
        .with_priority(1)
        .mount(&mock)
        .await;
    let call = Call::object("Szamlazz.Order", "UNCERTAIN", "create_invoice").scoped("alpha");
    let started = server
        .invoke(&call.send(), Some(&request()), Some("uncertain"))
        .await;
    let id = started.invocation_id();
    server
        .admin()
        .await_status_with_timeout(id, &["paused"], Duration::from_secs(30))
        .await;
    assert_eq!(sent.load(Ordering::SeqCst), 1);
    let marker = server
        .invoke(
            &Call::object("Szamlazz.Order", "UNCERTAIN", "observe_unresolved").scoped("alpha"),
            None,
            None,
        )
        .await;
    assert_eq!(marker.body["state"], "unresolved");
    let timestamp: jiff::Timestamp = marker.body["marker"]["created_at"]
        .as_str()
        .expect("timestamp text")
        .parse()
        .expect("JS-backed marker timestamp");
    assert!((jiff::Timestamp::now().as_second() - timestamp.as_second()).abs() < 60);
    common::http_client()
        .post(format!("{uri}/__replace"))
        .send()
        .await
        .expect("replace request")
        .error_for_status()
        .expect("replace status");
    server.admin().resume(id).await;
    server.admin().await_status(id, &["paused"]).await;
    assert_eq!(sent.load(Ordering::SeqCst), 1);
    visible.store(true, Ordering::SeqCst);
    server.admin().resume(id).await;
    server.admin().await_status(id, &["completed"]).await;
    let result = server
        .invoke(&call, Some(&request()), Some("uncertain"))
        .await;
    assert_eq!(result.body["outcome"], "reconciled", "{result:?}");
    assert_eq!(sent.load(Ordering::SeqCst), 1);
    // Cancellation while the provider's body is pending retains uncertainty.
    let cancelling = Arc::new(AtomicBool::new(false));
    let received = cancelling.clone();
    common::create_for("CANCEL-ACTIVE")
        .respond_with(move |_: &wiremock::Request| {
            received.store(true, Ordering::SeqCst);
            wiremock::ResponseTemplate::new(500).set_delay(Duration::from_secs(3))
        })
        .expect(1)
        .mount(&mock)
        .await;
    let cancel_call =
        Call::object("Szamlazz.Order", "CANCEL-ACTIVE", "create_invoice").scoped("alpha");
    let active = server
        .invoke(&cancel_call.send(), Some(&request()), Some("cancel-active"))
        .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !cancelling.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("active send");
    server.admin().cancel(active.invocation_id()).await;
    server
        .admin()
        .await_status(active.invocation_id(), &["completed"])
        .await;
    let cancelled = server
        .invoke(&cancel_call, Some(&request()), Some("cancel-active"))
        .await;
    let fault = cancelled.fault::<restate_szamlazz::contract::Fault>();
    assert_eq!(
        fault.code,
        restate_szamlazz::contract::TerminalCode::OutcomeUnknown
    );
    assert_eq!(
        server
            .invoke(
                &Call::object("Szamlazz.Order", "CANCEL-ACTIVE", "observe_unresolved")
                    .scoped("alpha"),
                None,
                None
            )
            .await
            .body["state"],
        "unresolved"
    );
    assert_eq!(
        server
            .invoke(&cancel_call, Some(&request()), Some("cancel-successor"))
            .await
            .status,
        500
    );
    // A never-effective write is still uncertain. Kill does not clear it.
    common::create_for("NO-EFFECT")
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(&mock)
        .await;
    let call = Call::object("Szamlazz.Order", "NO-EFFECT", "create_invoice").scoped("alpha");
    let original = server
        .invoke(&call.send(), Some(&request()), Some("no-effect"))
        .await;
    server
        .admin()
        .await_status(original.invocation_id(), &["paused"])
        .await;
    server.admin().kill(original.invocation_id()).await;
    server
        .admin()
        .await_status(original.invocation_id(), &["completed"])
        .await;
    let successor = server
        .invoke(&call, Some(&request()), Some("new-key"))
        .await;
    assert_eq!(successor.status, 500, "{successor:?}");
    assert_eq!(
        server
            .invoke(
                &Call::object("Szamlazz.Order", "NO-EFFECT", "observe_unresolved").scoped("alpha"),
                None,
                None
            )
            .await
            .body["state"],
        "unresolved"
    );
    let before = mock.received_requests().await.expect("requests").len();
    for handler in ["correct_invoice"] {
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", "BLOCKED", handler).scoped("alpha"),
                Some(&json!({})),
                None,
            )
            .await;
        assert_eq!(response.status, 400);
        assert!(
            response.body["message"]
                .as_str()
                .expect("fault")
                .contains("mutation unsupported"),
            "{handler}: {response:?}"
        );
    }
    for handler in ["set_credit_entries"] {
        let response = server
            .invoke(
                &Call::service("Szamlazz.Agent", handler).scoped("alpha"),
                Some(&json!({})),
                None,
            )
            .await;
        assert_eq!(response.status, 400);
        assert!(
            response.body["message"]
                .as_str()
                .expect("fault")
                .contains("mutation unsupported"),
            "{handler}: {response:?}"
        );
    }
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before
    );
    for (key, number, options) in [
        (
            "REISSUE",
            "OLD-1",
            json!({"reissue":{"expected_number":"OLD-1"},"proforma":"none"}),
        ),
        ("CONVERT", "D-1", json!({"proforma":"auto"})),
        ("NAMED", "D-2", json!({"proforma":{"number":"D-2"}})),
    ] {
        let mut doc = common::Doc::of(number, if key == "REISSUE" { "SZ" } else { "D" }, key);
        doc.reversed = key == "REISSUE";
        common::external_id_query(&format!(
            "workers:{key}:{}",
            if key == "REISSUE" {
                "invoice"
            } else {
                "proforma"
            }
        ))
        .respond_with(doc.response())
        .with_priority(1)
        .mount(&mock)
        .await;
        common::number_query(number)
            .respond_with(doc.response())
            .with_priority(1)
            .mount(&mock)
            .await;
        common::create_for(key)
            .respond_with(move |r: &wiremock::Request| {
                if key != "REISSUE" {
                    assert!(String::from_utf8_lossy(&r.body).contains(&format!(
                        "<dijbekeroSzamlaszam>{number}</dijbekeroSzamlaszam>"
                    )));
                }
                common::created("NEW-EXTENSION", "1000", "1270")
            })
            .expect(1)
            .mount(&mock)
            .await;
        let mut body = request();
        body["options"] = options;
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", key, "create_invoice").scoped("alpha"),
                Some(&body),
                None,
            )
            .await;
        assert_eq!(response.body["outcome"], "issued", "{key}: {response:?}");
    }
    // The same unfinished-run rules hold after a real workerd replacement:
    // replacement evidence wins over the old reissue/proforma prerequisites.
    for (key, reissue) in [("REISSUE-REPLAY", true), ("CONVERSION-REPLAY", false)] {
        let accepted = Arc::new(AtomicBool::new(false));
        let reached = accepted.clone();
        common::create_for(key)
            .respond_with(move |r: &wiremock::Request| {
                if !reissue {
                    assert!(
                        String::from_utf8_lossy(&r.body)
                            .contains("<dijbekeroSzamlaszam>D-REPLAY</dijbekeroSzamlaszam>")
                    );
                }
                reached.store(true, Ordering::SeqCst);
                common::created("REPLACEMENT-1", "1000", "1270").set_delay(Duration::from_secs(20))
            })
            .expect(1)
            .mount(&mock)
            .await;
        let reached = accepted.clone();
        common::external_id_query(&format!("workers:{key}:invoice"))
            .respond_with(move |_: &wiremock::Request| {
                if reached.load(Ordering::SeqCst) {
                    return common::Doc::of("REPLACEMENT-1", "SZ", key).response();
                }
                if reissue {
                    let mut old = common::Doc::of("OLD-REPLAY", "SZ", key);
                    old.reversed = true;
                    return old.response();
                }
                common::not_found()
            })
            .with_priority(1)
            .mount(&mock)
            .await;
        if !reissue {
            let reached = accepted.clone();
            common::external_id_query(&format!("workers:{key}:proforma"))
                .respond_with(move |_: &wiremock::Request| {
                    if reached.load(Ordering::SeqCst) {
                        common::not_found()
                    } else {
                        common::Doc::of("D-REPLAY", "D", key).response()
                    }
                })
                .with_priority(1)
                .mount(&mock)
                .await;
            common::number_query("D-REPLAY")
                .respond_with(common::Doc::of("D-REPLAY", "D", key).response())
                .with_priority(1)
                .mount(&mock)
                .await;
        }
        let mut body = request();
        body["options"] = if reissue {
            json!({"reissue":{"expected_number":"OLD-REPLAY"},"proforma":"none"})
        } else {
            json!({"proforma":"auto"})
        };
        let call = Call::object("Szamlazz.Order", key, "create_invoice").scoped("alpha");
        let started = server.invoke(&call.send(), Some(&body), Some(key)).await;
        tokio::time::timeout(Duration::from_secs(10), async {
            while !accepted.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("extension send reached provider");
        common::http_client()
            .post(format!("{uri}/__interrupt"))
            .send()
            .await
            .expect("replace worker")
            .error_for_status()
            .expect("replacement status");
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let response = server.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(response.body["outcome"], "issued", "{key}: {response:?}");
        assert_eq!(response.body["invoice_number"], "REPLACEMENT-1");
    }
    let observation = server
        .invoke(
            &Call::object("Szamlazz.Order", "NO-EFFECT", "observe_unresolved").scoped("alpha"),
            None,
            None,
        )
        .await;
    let body = json!({"operator":"test-operator","marker":observation.body["marker"],"evidence":{"type":"not_executed","audit_reference":"INC-WORKER","did_not_execute_and_cannot_execute_later":true}});
    let recover = Call::object("Szamlazz.Order", "NO-EFFECT", "recover").scoped("alpha");
    let recovered = server
        .invoke(&recover, Some(&body), Some("recover-worker"))
        .await;
    assert_eq!(recovered.status, 200, "{recovered:?}");
    assert_eq!(
        server
            .invoke(
                &Call::object("Szamlazz.Order", "NO-EFFECT", "observe_unresolved").scoped("alpha"),
                None,
                None
            )
            .await
            .body["state"],
        "absent"
    );
    assert_eq!(
        server
            .invoke(&recover, Some(&body), Some("recover-worker"))
            .await
            .body,
        recovered.body
    );
    let mut document = common::Doc::of("FACTS-1", "SZ", "FACTS");
    document.net = "9007199254740993.01";
    document.vat = "0";
    document.gross = "9007199254740993.01";
    let xml=document.xml().replace("<vevo><nev>Buyer</nev></vevo>","<vevo><nev>Queried Buyer</nev><adoszam>12345678-2-42</adoszam></vevo>")
        .replace("<osszegek>","<osszegek><afakulcsossz><afatipus>AAM</afatipus><afakulcs>0</afakulcs><netto>9007199254740993.01</netto><afa>0</afa><brutto>9007199254740993.01</brutto></afakulcsossz>");
    common::number_query("FACTS-1")
        .respond_with(wiremock::ResponseTemplate::new(200).set_body_raw(xml, "application/xml"))
        .with_priority(1)
        .mount(&mock)
        .await;
    let query = server
        .invoke(
            &Call::service("Szamlazz.Agent", "query").scoped("alpha"),
            Some(&json!({"selector":{"invoice_number":"FACTS-1"}})),
            None,
        )
        .await;
    assert_eq!(query.status, 200, "{query:?}");
    assert_eq!(query.body["gross_total"], "9007199254740993.01");
    assert_eq!(query.body["buyer"]["tax_number"], "12345678-2-42");
    assert_eq!(query.body["by_vat_rate"][0]["gross"], "9007199254740993.01");
    assert_eq!(query.body["by_vat_rate"][0]["vat_type"], "AAM");
    mock.verify().await;
    proforma_lifecycle(&server, &mock, uri).await;
    chain_lifecycle(&server, &mock, uri).await;
    chain_resubmission(&server, &mock, uri).await;
    storno_lifecycle(&server, &mock, uri).await;
    server.finish().await;
    host.finish();
}

async fn storno_lifecycle(
    server: &restate_e2e_harness::Restate,
    mock: &wiremock::MockServer,
    uri: &str,
) {
    common::number_query("UNMANAGED-STORNO")
        .respond_with(common::Doc::unmanaged("UNMANAGED-STORNO", "SZ").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::storno_of_number_repeating_telj("UNMANAGED-STORNO")
        .respond_with(common::created("SS-UNMANAGED", "-1000", "-1270"))
        .expect(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::service("Szamlazz.Agent", "storno").scoped("alpha"),
            Some(&json!({"invoice_number":"UNMANAGED-STORNO"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "reversed", "{response:?}");
    for (key, recorded) in [("STORNO-REPLAY", false), ("STORNO-RECORDED", true)] {
        let original = format!("ORIGINAL-{key}");
        let reversal = format!("SS-{key}");
        let sends = Arc::new(AtomicUsize::new(0));
        let visible = Arc::new(AtomicBool::new(false));
        let shown = visible.clone();
        let number = original.clone();
        common::number_query(&original)
            .respond_with(move |_: &wiremock::Request| {
                let mut doc = common::Doc::of(&number, "SZ", key);
                doc.reversed = shown.load(Ordering::SeqCst);
                doc.response()
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let shown = visible.clone();
        let number = original.clone();
        let ss = reversal.clone();
        common::external_id_query(&format!("workers:{key}:storno:{original}"))
            .respond_with(move |_: &wiremock::Request| {
                if shown.load(Ordering::SeqCst) {
                    let mut doc = common::Doc::of(&ss, "SS", key);
                    doc.referenced_invoice = Some(&number);
                    doc.response()
                } else {
                    common::not_found()
                }
            })
            .with_priority(1)
            .mount(mock)
            .await;
        let count = sends.clone();
        let ss = reversal.clone();
        common::storno_of_number_repeating_telj(&original)
            .respond_with(move |req: &wiremock::Request| {
                assert!(String::from_utf8_lossy(&req.body).contains("notify@example.test"));
                let n = count.fetch_add(1, Ordering::SeqCst);
                if recorded {
                    wiremock::ResponseTemplate::new(500)
                } else if n == 0 {
                    common::created(&ss, "-1000", "-1270").set_delay(Duration::from_secs(20))
                } else {
                    common::created(&ss, "-1000", "-1270")
                }
            })
            .mount(mock)
            .await;
        let body = json!({"invoice_number":original,"buyer_email":"notify@example.test"});
        let call = Call::object("Szamlazz.Order", key, "storno_invoice").scoped("alpha");
        let started = server.invoke(&call.send(), Some(&body), Some(key)).await;
        if recorded {
            server
                .admin()
                .await_status(started.invocation_id(), &["paused"])
                .await;
            let observed = server
                .invoke(
                    &Call::object("Szamlazz.Order", key, "observe_unresolved").scoped("alpha"),
                    None,
                    None,
                )
                .await;
            assert_eq!(observed.body["state"], "unresolved");
            assert!(
                observed.body["marker"]["execution_contract"]["request_response_storno_v1"]
                    .is_object()
            );
            server.admin().resume(started.invocation_id()).await;
            server
                .admin()
                .await_status(started.invocation_id(), &["paused"])
                .await;
            assert_eq!(sends.load(Ordering::SeqCst), 1);
            visible.store(true, Ordering::SeqCst);
            server.admin().resume(started.invocation_id()).await;
        } else {
            tokio::time::timeout(Duration::from_secs(10), async {
                while sends.load(Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("storno send");
            common::http_client()
                .post(format!("{uri}/__interrupt"))
                .send()
                .await
                .expect("interrupt")
                .error_for_status()
                .expect("interrupted");
        }
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let response = server.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(response.body["outcome"], "reversed", "{response:?}");
        assert_eq!(response.body["storno_number"], reversal);
        assert_eq!(sends.load(Ordering::SeqCst), if recorded { 1 } else { 2 });
    }
    mock.verify().await;
}

async fn chain_resubmission(
    server: &restate_e2e_harness::Restate,
    mock: &wiremock::MockServer,
    uri: &str,
) {
    for (key, handler) in [
        ("ES-RESEND", "create_prepayment"),
        ("VS-RESEND", "create_final"),
    ] {
        if handler == "create_final" {
            common::external_id_query("workers:VS-RESEND:prepayment")
                .respond_with(common::Doc::of("ES-PINNED", "ES", key).response())
                .with_priority(1)
                .mount(mock)
                .await;
            common::number_query("ES-PINNED")
                .respond_with(common::Doc::of("ES-PINNED", "ES", key).response())
                .with_priority(1)
                .mount(mock)
                .await;
        }
        let sends = Arc::new(AtomicUsize::new(0));
        let count = sends.clone();
        common::create_for(key)
            .respond_with(move |req: &wiremock::Request| {
                if handler == "create_final" {
                    assert!(
                        String::from_utf8_lossy(&req.body)
                            .contains("<elolegSzamlaszam>ES-PINNED</elolegSzamlaszam>")
                    );
                }
                let reply = common::created("CHAIN-RESUBMITTED", "1000", "1270");
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    reply.set_delay(Duration::from_secs(20))
                } else {
                    reply
                }
            })
            .expect(2)
            .mount(mock)
            .await;
        let mut body = request();
        body["options"] = json!({});
        let call = Call::object("Szamlazz.Order", key, handler).scoped("alpha");
        let started = server.invoke(&call.send(), Some(&body), Some(key)).await;
        tokio::time::timeout(Duration::from_secs(10), async {
            while sends.load(Ordering::SeqCst) == 0 {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("first chain send");
        common::http_client()
            .post(format!("{uri}/__interrupt"))
            .send()
            .await
            .expect("interrupt")
            .error_for_status()
            .expect("interrupted");
        server
            .admin()
            .await_status(started.invocation_id(), &["completed"])
            .await;
        let response = server.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(response.body["outcome"], "issued", "{response:?}");
        assert_eq!(sends.load(Ordering::SeqCst), 2);
    }
    mock.verify().await;
}

async fn chain_lifecycle(
    server: &restate_e2e_harness::Restate,
    mock: &wiremock::MockServer,
    uri: &str,
) {
    let prepayment_created = Arc::new(AtomicBool::new(false));
    let final_sent = Arc::new(AtomicBool::new(false));
    let issued = prepayment_created.clone();
    common::external_id_query("workers:CHAIN:proforma")
        .respond_with(move |_: &wiremock::Request| {
            if issued.load(Ordering::SeqCst) {
                common::not_found()
            } else {
                common::Doc::of("D-CHAIN", "D", "CHAIN").response()
            }
        })
        .with_priority(1)
        .mount(mock)
        .await;
    common::number_query("D-CHAIN")
        .respond_with(common::Doc::of("D-CHAIN", "D", "CHAIN").response())
        .with_priority(1)
        .mount(mock)
        .await;
    let issued = prepayment_created.clone();
    let sent = final_sent.clone();
    common::external_id_query("workers:CHAIN:prepayment")
        .respond_with(move |_: &wiremock::Request| {
            if !issued.load(Ordering::SeqCst) {
                return common::not_found();
            }
            let mut doc = common::Doc::of("ES-WORKER", "ES", "CHAIN");
            doc.reversed = sent.load(Ordering::SeqCst);
            doc.response()
        })
        .with_priority(1)
        .mount(mock)
        .await;
    let sent = final_sent.clone();
    common::number_query("ES-WORKER")
        .respond_with(move |_: &wiremock::Request| {
            let mut doc = common::Doc::of("ES-WORKER", "ES", "CHAIN");
            doc.reversed = sent.load(Ordering::SeqCst);
            doc.response()
        })
        .with_priority(1)
        .mount(mock)
        .await;
    let issued = prepayment_created.clone();
    common::create_for("CHAIN")
        .and(wiremock::matchers::body_string_contains(
            "<elolegszamla>true</elolegszamla>",
        ))
        .respond_with(move |req: &wiremock::Request| {
            assert!(
                String::from_utf8_lossy(&req.body)
                    .contains("<dijbekeroSzamlaszam>D-CHAIN</dijbekeroSzamlaszam>")
            );
            issued.store(true, Ordering::SeqCst);
            common::created("ES-WORKER", "1000", "1270")
        })
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"] = json!({});
    let prepayment = server
        .invoke(
            &Call::object("Szamlazz.Order", "CHAIN", "create_prepayment").scoped("alpha"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(prepayment.body["outcome"], "issued", "{prepayment:?}");
    let sent = final_sent.clone();
    common::create_for("CHAIN")
        .and(wiremock::matchers::body_string_contains(
            "<vegszamla>true</vegszamla>",
        ))
        .respond_with(move |req: &wiremock::Request| {
            assert!(
                String::from_utf8_lossy(&req.body)
                    .contains("<elolegSzamlaszam>ES-WORKER</elolegSzamlaszam>")
            );
            sent.store(true, Ordering::SeqCst);
            common::created("VS-WORKER", "500", "635").set_delay(Duration::from_secs(20))
        })
        .expect(1)
        .mount(mock)
        .await;
    let visible = Arc::new(AtomicBool::new(false));
    let shown = visible.clone();
    common::external_id_query("workers:CHAIN:final")
        .respond_with(move |_: &wiremock::Request| {
            if shown.load(Ordering::SeqCst) {
                common::Doc::of("VS-WORKER", "VS", "CHAIN").response()
            } else {
                common::not_found()
            }
        })
        .with_priority(1)
        .mount(mock)
        .await;
    body["document"]["items"][0]["unit_price"] = json!("1500");
    body["document"]["items"].as_array_mut().expect("items").push(json!({"name":"Deduct prepayment","quantity":"1","unit":"db","unit_price":"-1000","vat_rate":"27"}));
    let call = Call::object("Szamlazz.Order", "CHAIN", "create_final").scoped("alpha");
    let started = server
        .invoke(&call.send(), Some(&body), Some("chain-final"))
        .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !final_sent.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("final send");
    common::http_client()
        .post(format!("{uri}/__interrupt"))
        .send()
        .await
        .expect("interrupt")
        .error_for_status()
        .expect("interrupt status");
    server
        .admin()
        .await_status(started.invocation_id(), &["paused"])
        .await;
    let observation = server
        .invoke(
            &Call::object("Szamlazz.Order", "CHAIN", "observe_unresolved").scoped("alpha"),
            None,
            None,
        )
        .await;
    assert_eq!(observation.body["marker"]["prepayment_number"], "ES-WORKER");
    assert_eq!(
        observation.body["marker"]["execution_contract"],
        "request_response_final_v1"
    );
    server.admin().resume(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["paused"])
        .await;
    visible.store(true, Ordering::SeqCst);
    server.admin().resume(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["completed"])
        .await;
    assert_eq!(
        server
            .invoke(&call, Some(&body), Some("chain-final"))
            .await
            .body["outcome"],
        "reconciled"
    );
    mock.verify().await;
}

async fn proforma_lifecycle(
    server: &restate_e2e_harness::Restate,
    mock: &wiremock::MockServer,
    uri: &str,
) {
    common::number_query("D-NAMED-WORKER")
        .respond_with(common::Doc::of("D-NAMED-WORKER", "D", "NAMED-WORKER-DELETE").response())
        .with_priority(1)
        .mount(mock)
        .await;
    common::delete_of("D-NAMED-WORKER")
        .respond_with(common::proforma_deleted())
        .expect(1)
        .mount(mock)
        .await;
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "NAMED-WORKER-DELETE", "delete_proforma")
                .scoped("alpha"),
            Some(&json!({"expected_number":"D-NAMED-WORKER","mode":"named_target"})),
            None,
        )
        .await;
    assert_eq!(response.body["outcome"], "deleted", "{response:?}");
    let created = Arc::new(AtomicBool::new(false));
    let deleted = Arc::new(AtomicBool::new(false));
    let c = created.clone();
    let d = deleted.clone();
    common::external_id_query("workers:PROFORMA:proforma")
        .respond_with(move |_: &wiremock::Request| {
            if c.load(Ordering::SeqCst) && !d.load(Ordering::SeqCst) {
                common::Doc::of("D-WORKER", "D", "PROFORMA").response()
            } else {
                common::not_found()
            }
        })
        .with_priority(1)
        .mount(mock)
        .await;
    let c = created.clone();
    common::create_for("PROFORMA")
        .respond_with(move |_: &wiremock::Request| {
            c.store(true, Ordering::SeqCst);
            common::created("D-WORKER", "1000", "1270")
        })
        .expect(1)
        .mount(mock)
        .await;
    let mut body = request();
    body["options"] = json!({});
    let create = server
        .invoke(
            &Call::object("Szamlazz.Order", "PROFORMA", "create_proforma").scoped("alpha"),
            Some(&body),
            None,
        )
        .await;
    assert_eq!(create.body["outcome"], "issued", "{create:?}");
    common::number_query("D-WORKER")
        .respond_with(common::Doc::of("D-WORKER", "D", "PROFORMA").response())
        .with_priority(1)
        .mount(mock)
        .await;
    let d = deleted.clone();
    common::delete_of("D-WORKER")
        .respond_with(move |_: &wiremock::Request| {
            d.store(true, Ordering::SeqCst);
            common::proforma_deleted().set_delay(Duration::from_secs(20))
        })
        .expect(1)
        .mount(mock)
        .await;
    let call = Call::object("Szamlazz.Order", "PROFORMA", "delete_proforma").scoped("alpha");
    let body = json!({"expected_number":"D-WORKER"});
    let started = server
        .invoke(&call.send(), Some(&body), Some("delete-worker"))
        .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !deleted.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("delete sent");
    common::http_client()
        .post(format!("{uri}/__interrupt"))
        .send()
        .await
        .expect("interrupt")
        .error_for_status()
        .expect("interrupt status");
    server
        .admin()
        .await_status(started.invocation_id(), &["paused"])
        .await;
    let observed = server
        .invoke(
            &Call::object("Szamlazz.Order", "PROFORMA", "observe_unresolved").scoped("alpha"),
            None,
            None,
        )
        .await;
    assert_eq!(observed.body["state"], "unresolved");
    server.admin().resume(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["paused"])
        .await;
    server.admin().kill(started.invocation_id()).await;
    server
        .admin()
        .await_status(started.invocation_id(), &["completed"])
        .await;
    assert_eq!(
        server
            .invoke(&call, Some(&body), Some("new-delete"))
            .await
            .status,
        500
    );
    let evidence = json!({"operator":"operator","marker":observed.body["marker"],"evidence":{"type":"completed","audit_reference":"INC-WORKER-DELETE","completion":{"type":"deleted","number":"D-WORKER"},"completed_and_cannot_execute_later":true}});
    let response = server
        .invoke(
            &Call::object("Szamlazz.Order", "PROFORMA", "recover").scoped("alpha"),
            Some(&evidence),
            None,
        )
        .await;
    assert_eq!(response.status, 200, "{response:?}");
    mock.verify().await;
}
