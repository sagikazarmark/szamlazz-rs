//! Release regressions at the real ingress and durable-operation boundaries.
use super::*;
use crate::common::api_error;
use restate_szamlazz::contract::{Fault, TerminalCode};

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; XML-invalid mutation identities"]
async fn e2e_release_invalid_identities_fail_before_the_prologue() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "release-identities",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let mut config = WorkerConfig::new("acct".parse().expect("namespace"));
    config.read.max_attempts = Some(1);
    let (order, agent) = services_with_config(&mock.uri(), config.validate().expect("config"));
    restate
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;
    for character in ['\u{fffe}', '\u{ffff}'] {
        let invalid = format!("BAD-{character}");
        let mut corrective = create_body(dec!(1000));
        corrective
            .as_object_mut()
            .expect("object")
            .remove("options");
        corrective["correction_id"] = json!("correction-1");
        corrective["invoice_number"] = json!(invalid);
        let mut linked = create_body(dec!(1000));
        linked["options"] = json!({"proforma":{"number":invalid}});
        for (call, body) in [
            (
                Call::object("Szamlazz.Order", &invalid, "create_invoice"),
                create_body(dec!(1000)),
            ),
            (
                Call::object("Szamlazz.Order", &invalid, "get"),
                serde_json::Value::Null,
            ),
            (
                Call::object("Szamlazz.Order", "VALID-ORDER", "storno_invoice"),
                json!({"invoice_number":invalid}),
            ),
            (
                Call::service("Szamlazz.Agent", "storno"),
                json!({"invoice_number":invalid}),
            ),
            (
                Call::object("Szamlazz.Order", "VALID-ORDER", "correct_invoice"),
                corrective,
            ),
            (
                Call::object("Szamlazz.Order", "VALID-ORDER", "create_invoice"),
                linked,
            ),
            (
                Call::object("Szamlazz.Order", "VALID-ORDER", "delete_proforma"),
                json!({"expected_number":invalid}),
            ),
        ] {
            let reply = restate
                .invoke(&call, (!body.is_null()).then_some(&body), None)
                .await;
            assert_eq!(reply.status, 400, "{}", reply.body);
            let fault = reply.fault::<Fault>();
            assert_eq!(fault.code, TerminalCode::InvalidInput);
            assert!(fault.message.contains("XML"), "{}", fault.message);
            assert!(restate.admin().runs(reply.invocation_id()).await.is_empty());
        }
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; get preserves the first answered fault"]
async fn e2e_release_get_stops_on_an_answered_fault() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "release-get",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let mut config = WorkerConfig::new("acct".parse().expect("namespace"));
    config.read.max_attempts = Some(1);
    let (order, _) = services_with_config(&mock.uri(), config.validate().expect("config"));
    restate
        .deploy(Endpoint::builder().bind(order).build())
        .await;
    for (code, expected) in [
        ("3", TerminalCode::CredentialsRejected),
        ("57", TerminalCode::Unavailable),
    ] {
        let key = format!("GET-{code}");
        external_id_query(&format!("acct:{key}:proforma"))
            .respond_with(api_error(code, "vendor answer"))
            .expect(1)
            .mount(&mock)
            .await;
        external_id_query(&format!("acct:{key}:invoice"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&mock)
            .await;
        let reply = restate
            .invoke(&Call::object("Szamlazz.Order", &key, "get"), None, None)
            .await;
        assert_eq!(reply.status, 503, "{}", reply.body);
        let fault = reply.fault::<Fault>();
        assert_eq!(fault.code, expected);
        assert_eq!(fault.szamlazz_code.as_deref(), Some(code));
        assert_eq!(
            restate.admin().runs(reply.invocation_id()).await,
            ["namespace", "account", "lookup-proforma"]
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; corrective leading-query identity"]
async fn e2e_release_corrective_leading_query_checks_the_base() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "release-corrective",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.with_recovery_authorizer(Arc::new(Operator)))
                .build(),
        )
        .await;
    for (base, reversed) in [
        (None, false),
        (Some("SZ-B"), false),
        (Some("SZ-B"), true),
        (Some("SZ-A"), false),
    ] {
        let key = "CORRECT-BASE";
        let reads = Arc::new(AtomicUsize::new(0));
        let counted = reads.clone();
        external_id_query("acct:CORRECT-BASE:corrective:correction-1")
            .respond_with(move |_: &wiremock::Request| {
                if counted.fetch_add(1, Ordering::SeqCst) < 2 {
                    not_found()
                } else {
                    Doc {
                        reversed,
                        referenced_invoice: base,
                        ..Doc::of("HS-FOUND", "HS", key)
                    }
                    .response()
                }
            })
            .expect(3)
            .mount(&mock)
            .await;
        number_query("SZ-A")
            .respond_with(Doc::of("SZ-A", "SZ", key).response())
            .mount(&mock)
            .await;
        create_for(key)
            .respond_with(created("HS-UNEXPECTED", "1000", "1270"))
            .expect(0)
            .mount(&mock)
            .await;
        let mut body = create_body(dec!(1000));
        body.as_object_mut().expect("object").remove("options");
        body["correction_id"] = json!("correction-1");
        body["invoice_number"] = json!("SZ-A");
        let reply = restate
            .invoke(
                &Call::object("Szamlazz.Order", key, "correct_invoice"),
                Some(&body),
                None,
            )
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        if base == Some("SZ-A") {
            assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
            assert_eq!(reply.body["invoice_number"], "HS-FOUND");
        } else {
            assert_eq!(reply.body["outcome"], "conflict", "{}", reply.body);
            assert_eq!(reply.body["conflict_reason"], "external_id_collision");
        }
        assert_eq!(reads.load(Ordering::SeqCst), 3);
        assert!(
            restate
                .admin()
                .runs(reply.invocation_id())
                .await
                .iter()
                .any(|run| run == "arm-write")
        );
        assert_eq!(
            restate
                .invoke(
                    &Call::object("Szamlazz.Order", key, "observe_unresolved"),
                    None,
                    None
                )
                .await
                .body["state"],
            "absent"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; corrective post-send evidence"]
async fn e2e_release_corrective_reconciliation_retains_wrong_base_uncertainty() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "release-corrective-reconcile",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "correct_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_max_attempts(1)
            .retry_policy_pause_on_max_attempts(),
    );
    restate
        .deploy(
            Endpoint::builder()
                .bind(
                    order
                        .with_recovery_authorizer(Arc::new(Operator))
                        .into_service_definition()
                        .options(options),
                )
                .build(),
        )
        .await;
    for (index, base) in [None, Some("SZ-B")].into_iter().enumerate() {
        let key = format!("CORRECT-RECONCILE-{index}");
        let sent = Arc::new(AtomicBool::new(false));
        let valid = Arc::new(AtomicBool::new(false));
        let acted = sent.clone();
        let fixed = valid.clone();
        let expected_order = key.clone();
        external_id_query(&format!("acct:{key}:corrective:correction-1"))
            .respond_with(move |_: &wiremock::Request| {
                if acted.load(Ordering::SeqCst) {
                    Doc {
                        referenced_invoice: if fixed.load(Ordering::SeqCst) {
                            Some("SZ-A")
                        } else {
                            base
                        },
                        ..Doc::of("HS-FOUND", "HS", &expected_order)
                    }
                    .response()
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        number_query("SZ-A")
            .respond_with(Doc::of("SZ-A", "SZ", &key).response())
            .mount(&mock)
            .await;
        create_for(&key)
            .respond_with(move |_: &wiremock::Request| {
                sent.store(true, Ordering::SeqCst);
                ResponseTemplate::new(500)
            })
            .expect(1)
            .mount(&mock)
            .await;
        let body = json!({"invoice_number":"SZ-A", "correction_id":"correction-1", "document":create_body(dec!(1000))["document"]});
        let call = Call::object("Szamlazz.Order", &key, "correct_invoice");
        let owner = restate.invoke(&call.send(), Some(&body), Some(&key)).await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        let observe = Call::object("Szamlazz.Order", &key, "observe_unresolved");
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "unresolved"
        );
        mock.verify().await;
        valid.store(true, Ordering::SeqCst);
        restate.admin().resume(owner.invocation_id()).await;
        let completed = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(completed.status, 200, "{}", completed.body);
        assert_eq!(completed.body["outcome"], "reconciled");
        assert_eq!(completed.body["invoice_number"], "HS-FOUND");
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}
