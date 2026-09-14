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
    for handler in [
        "create_proforma",
        "create_prepayment",
        "create_final",
        "correct_invoice",
        "storno_invoice",
        "delete_proforma",
        "recover",
    ] {
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
    for handler in ["storno", "set_credit_entries"] {
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
    for options in [
        json!({"reissue":{"expected_number":"SZ-OLD"}}),
        json!({"proforma":{"number":"D-1"}}),
    ] {
        let mut body = request();
        body["options"] = options;
        let response = server
            .invoke(
                &Call::object("Szamlazz.Order", "UNSUPPORTED-OPTION", "create_invoice")
                    .scoped("alpha"),
                Some(&body),
                None,
            )
            .await;
        assert_eq!(response.status, 400);
        assert!(
            response.body["message"]
                .as_str()
                .expect("fault")
                .contains("experimental RequestResponse")
        );
    }
    assert_eq!(
        mock.received_requests().await.expect("requests").len(),
        before
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
    server.finish().await;
    host.finish();
}
