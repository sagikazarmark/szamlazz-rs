//! Public exact queries: provider spelling, independent retries and replay.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use restate_e2e_harness::{Call, Restate, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::Endpoint;
use restate_sdk::service::IntoServiceDefinition as _;
use restate_szamlazz::config::WorkerConfig;
use restate_szamlazz::contract::{Fault, TerminalCode};
use rust_decimal::dec;
use serde_json::json;
use wiremock::{MockServer, ResponseTemplate};

use crate::common::{
    Doc, create_for, external_id_query, not_found, number_query, order_query, szlahu_down,
};
use crate::harness::accounts::services_with_config;
use crate::harness::{MAIN_SERVER, create_body};

async fn deploy(restate: &Restate, mock: &MockServer, query_attempts: Option<u32>) {
    let mut body = json!({
        "namespace": "acct",
        "read": {"max_attempts": 3, "initial_delay": "1s", "max_delay": "1s", "factor": 1.0}
    });
    if let Some(attempts) = query_attempts {
        body["query"] = json!({"max_attempts": attempts, "initial_delay": "1s", "max_delay": "1s", "factor": 1.0});
    }
    let config: WorkerConfig = serde_json::from_value(body).expect("config");
    let (order, agent) = services_with_config(&mock.uri(), config.validate().expect("valid"));
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_initial_interval(Duration::from_secs(1))
            .retry_policy_max_interval(Duration::from_secs(1))
            .retry_policy_max_attempts(5)
            .retry_policy_pause_on_max_attempts(),
    );
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .bind(agent)
                .build(),
        )
        .await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; provider-number codec and exact wire identity"]
async fn e2e_query_provider_numbers_preserve_spelling_and_refuse_mismatches() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "query-numbers",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    deploy(&restate, &mock, Some(1)).await;
    let call = Call::service("Szamlazz.Agent", "query");
    for number in ["", " \t\n", "\u{a0}", "bad\0number", "bad\u{ffff}number"] {
        let reply = restate
            .invoke(
                &call,
                Some(&json!({"selector": {"invoice_number": number}})),
                None,
            )
            .await;
        assert_eq!(reply.status, 400, "{}", reply.body);
        assert_eq!(reply.fault::<Fault>().code, TerminalCode::InvalidInput);
        assert!(restate.admin().runs(reply.invocation_id()).await.is_empty());
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
    for number in [
        "SZ-1".to_owned(),
        "  Külső: invoice 1  ".to_owned(),
        "é".repeat(80),
        "e\u{301}:\t1\n".to_owned(),
    ] {
        number_query(&number)
            .respond_with(Doc::new(&number, "SZ").response())
            .expect(1)
            .mount(&mock)
            .await;
        let reply = restate
            .invoke(
                &call,
                Some(&json!({"selector": {"invoice_number": number}})),
                None,
            )
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["invoice_number"], number);
        mock.verify().await;
        mock.reset().await;
    }
    // XML metacharacters and CR must round-trip as the same text, not markup
    // or an XML line-ending-normalized spelling.
    let number = " A&B<\"'>\r1 ";
    let escaped = " A&amp;B&lt;&quot;&apos;&gt;&#13;1 ";
    number_query(escaped)
        .respond_with(Doc::new(escaped, "SZ").response())
        .expect(1)
        .mount(&mock)
        .await;
    let reply = restate
        .invoke(
            &call,
            Some(&json!({"selector": {"invoice_number": number}})),
            None,
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], number);
    mock.verify().await;
    mock.reset().await;
    // No trimming, case folding or NFC equivalence is allowed to manufacture a match.
    for (requested, returned) in [
        (" vendor:1 ", "vendor:1"),
        ("vendor:1", "VENDOR:1"),
        ("e\u{301}:1", "é:1"),
    ] {
        number_query(requested)
            .respond_with(Doc::new(returned, "SZ").response())
            .expect(1)
            .mount(&mock)
            .await;
        let reply = restate
            .invoke(
                &call,
                Some(&json!({"selector": {"invoice_number": requested}})),
                None,
            )
            .await;
        assert_eq!(reply.status, 503, "{}", reply.body);
        assert_eq!(reply.fault::<Fault>().code, TerminalCode::Unavailable);
        assert_eq!(
            mock.received_requests().await.expect("requests").len(),
            1,
            "no fallback selector"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; query run policies and retained completions"]
async fn e2e_query_policy_is_independent_and_retains_completed_results() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "query-policy",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let call = Call::service("Szamlazz.Agent", "query");
    for (query_attempts, expected) in [(None, 3), (Some(1), 1), (Some(2), 2)] {
        deploy(&restate, &mock, query_attempts).await;
        number_query("SZ-QUERY-POLICY")
            .respond_with(szlahu_down())
            .expect(expected)
            .mount(&mock)
            .await;
        let body = json!({"selector": {"invoice_number": "SZ-QUERY-POLICY"}});
        let key = format!("query-policy-{expected}");
        let reply = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(reply.status, 503, "{}", reply.body);
        assert_eq!(reply.fault::<Fault>().code, TerminalCode::Unavailable);
        let retained = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(retained.invocation_id(), reply.invocation_id());
        assert_eq!(retained.body, reply.body);
        mock.verify().await;
        mock.reset().await;
    }
    // A fresh key starts a new observation; a retained success reuses its facts.
    deploy(&restate, &mock, Some(1)).await;
    number_query("SZ-QUERY-POLICY")
        .respond_with(Doc::new("SZ-QUERY-POLICY", "SZ").response())
        .expect(1)
        .mount(&mock)
        .await;
    let body = json!({"selector": {"invoice_number": "SZ-QUERY-POLICY"}});
    let reply = restate
        .invoke(&call, Some(&body), Some("query-fresh"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let retained = restate
        .invoke(&call, Some(&body), Some("query-fresh"))
        .await;
    assert_eq!(retained.invocation_id(), reply.invocation_id());
    assert_eq!(retained.body, reply.body);
    crate::harness::query::replay_completed(restate.admin(), reply.invocation_id(), &reply.body)
        .await;
    mock.verify().await;
    mock.reset().await;

    // A mutation's ownership read still spends [read], not the single-query override.
    external_id_query("acct:QUERY-LOOKUP:invoice")
        .respond_with(szlahu_down())
        .up_to_n_times(2)
        .expect(2)
        .mount(&mock)
        .await;
    external_id_query("acct:QUERY-LOOKUP:invoice")
        .respond_with(Doc::of("SZ-LOOKUP", "SZ", "QUERY-LOOKUP").response())
        .expect(1)
        .mount(&mock)
        .await;
    let reply = restate
        .invoke(
            &Call::object("Szamlazz.Order", "QUERY-LOOKUP", "create_invoice"),
            Some(&create_body(dec!(1000))),
            None,
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "already_issued");
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; query override cannot shorten protected reconciliation"]
async fn e2e_query_policy_leaves_protected_reconciliation_retrying_read_only() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "query-reconcile",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    deploy(&restate, &mock, Some(1)).await;
    let sent = Arc::new(AtomicBool::new(false));
    let reads = Arc::new(AtomicUsize::new(0));
    let after_send = sent.clone();
    let count = reads.clone();
    external_id_query("acct:QUERY-RECONCILE:invoice")
        .respond_with(move |_: &wiremock::Request| {
            if !after_send.load(Ordering::SeqCst) {
                return not_found();
            }
            if count.fetch_add(1, Ordering::SeqCst) < 2 {
                return szlahu_down();
            }
            Doc::of("SZ-RECONCILED", "SZ", "QUERY-RECONCILE").response()
        })
        .mount(&mock)
        .await;
    for kind in ["prepayment", "final", "proforma"] {
        external_id_query(&format!("acct:QUERY-RECONCILE:{kind}"))
            .respond_with(not_found())
            .mount(&mock)
            .await;
    }
    order_query("QUERY-RECONCILE")
        .respond_with(not_found())
        .mount(&mock)
        .await;
    create_for("QUERY-RECONCILE")
        .respond_with(move |_: &wiremock::Request| {
            sent.store(true, Ordering::SeqCst);
            ResponseTemplate::new(500)
        })
        .expect(1)
        .mount(&mock)
        .await;
    let reply = restate
        .invoke(
            &Call::object("Szamlazz.Order", "QUERY-RECONCILE", "create_invoice"),
            Some(&create_body(dec!(1000))),
            None,
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reconciled");
    assert_eq!(reads.load(Ordering::SeqCst), 3);
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; interrupted single-attempt reads may execute again"]
async fn e2e_query_single_attempt_is_not_a_durable_wire_cap() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "query-interrupt",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    deploy(&restate, &mock, Some(1)).await;
    let reached = Arc::new(tokio::sync::Notify::new());
    let signal = reached.clone();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    number_query("SZ-INTERRUPTED")
        .respond_with(move |_: &wiremock::Request| {
            let response = Doc::new("SZ-INTERRUPTED", "SZ").response();
            if count.fetch_add(1, Ordering::SeqCst) == 0 {
                signal.notify_one();
                response.set_delay(Duration::from_secs(60))
            } else {
                response
            }
        })
        .expect(2)
        .mount(&mock)
        .await;
    let call = Call::service("Szamlazz.Agent", "query");
    let body = json!({"selector": {"invoice_number": "SZ-INTERRUPTED"}});
    let submitted = restate
        .invoke(&call.send(), Some(&body), Some("query-interrupted"))
        .await;
    tokio::time::timeout(Duration::from_secs(30), reached.notified())
        .await
        .expect("first provider request");
    // The provider has received the request, but no result has reached the journal.
    restate.admin().pause(submitted.invocation_id()).await;
    let journal = restate.admin().journal(submitted.invocation_id()).await;
    assert!(restate_e2e_harness::run_result(&journal, "query").is_none());
    restate.admin().resume(submitted.invocation_id()).await;
    let reply = restate
        .invoke(&call, Some(&body), Some("query-interrupted"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.invocation_id(), submitted.invocation_id());
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    mock.verify().await;
    restate.finish().await;
}
