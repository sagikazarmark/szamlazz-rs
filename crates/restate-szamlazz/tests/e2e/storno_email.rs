//! Actual Restate, mocked provider: recipient intent and notification outcomes.
use super::*;
use crate::common::storno_of_number;

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; uncertain storno never retries notification"]
#[allow(
    clippy::too_many_lines,
    reason = "one retained-write journey through pause, resume and later invocation"
)]
async fn e2e_storno_email_uncertainty_resumes_read_only() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "storno-email-uncertain",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "storno_invoice",
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
    let visible = Arc::new(AtomicBool::new(false));
    let seen = visible.clone();
    number_query("SZ-1")
        .respond_with(move |_: &wiremock::Request| {
            Doc {
                reversed: seen.load(Ordering::SeqCst),
                ..Doc::of("SZ-1", "SZ", "EMAIL")
            }
            .response()
        })
        .mount(&mock)
        .await;
    let seen = visible.clone();
    external_id_query("acct:EMAIL:storno:SZ-1")
        .respond_with(move |_: &wiremock::Request| {
            if seen.load(Ordering::SeqCst) {
                Doc {
                    referenced_invoice: Some("SZ-1"),
                    ..Doc::of("SS-1", "SS", "EMAIL")
                }
                .response()
            } else {
                not_found()
            }
        })
        .mount(&mock)
        .await;
    storno_of_number("SZ-1")
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&mock)
        .await;
    let call = Call::object("Szamlazz.Order", "EMAIL", "storno_invoice");
    let original = json!({"invoice_number":"SZ-1", "buyer_email":"original@example.com"});
    let owner = restate
        .invoke(&call.send(), Some(&original), Some("uncertain-email"))
        .await;
    restate
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    let observation = restate
        .invoke(
            &Call::object("Szamlazz.Order", "EMAIL", "observe_unresolved"),
            None,
            None,
        )
        .await;
    assert_eq!(observation.body["state"], "unresolved");
    // Recovery needs document identity, not a mutable recipient lookup. The
    // marker exposes no buyer address and authorizes no replacement send.
    assert!(
        !observation
            .body
            .to_string()
            .contains("original@example.com")
    );
    visible.store(true, Ordering::SeqCst);
    restate.admin().resume(owner.invocation_id()).await;
    let changed = json!({"invoice_number":"SZ-1", "buyer_email":"changed@example.com"});
    let completed = restate
        .invoke(&call, Some(&changed), Some("uncertain-email"))
        .await;
    assert_eq!(completed.body["outcome"], "reversed");
    assert_eq!(completed.body["storno_number"], "SS-1");
    assert_eq!(
        completed.body["warnings"],
        json!([]),
        "queries cannot recover notification history"
    );
    let later = restate
        .invoke(&call, Some(&changed), Some("later-email"))
        .await;
    assert_eq!(later.body["outcome"], "reversed");
    let requests = mock.received_requests().await.expect("requests");
    let bodies: Vec<_> = requests
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body))
        .filter(|body| body.contains("action-szamla_agent_st"))
        .collect();
    assert_eq!(
        bodies.len(),
        1,
        "resume and new request cannot retry notification"
    );
    assert!(bodies[0].contains("<email>original@example.com</email>"));
    assert!(!bodies[0].contains("changed@example.com"));
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; storno recipient and warning contract"]
#[allow(
    clippy::too_many_lines,
    reason = "both public handlers, wire observations and completed replay"
)]
async fn e2e_storno_email_contract_and_completed_replay() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "storno-email",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, agent) = services_with_config(&mock.uri(), config);
    restate
        .deploy(Endpoint::builder().bind(order).bind(agent).build())
        .await;
    for managed in [true, false] {
        let call = if managed {
            Call::object("Szamlazz.Order", "EMAIL", "storno_invoice")
        } else {
            Call::service("Szamlazz.Agent", "storno")
        };
        let bad = restate
            .invoke(
                &call,
                Some(&json!({"invoice_number":"SZ-1", "buyer_email":"bad\n@example.com"})),
                None,
            )
            .await;
        assert_eq!(bad.status, 400);
        assert!(restate.admin().runs(bad.invocation_id()).await.is_empty());
        assert!(mock.received_requests().await.expect("requests").is_empty());
        for explicit in [false, true] {
            let doc = Doc {
                order: managed.then_some("EMAIL"),
                ..Doc::new("SZ-1", "SZ")
            };
            number_query("SZ-1")
                .respond_with(doc.response())
                .expect(1)
                .mount(&mock)
                .await;
            let id = if managed {
                "acct:EMAIL:storno:SZ-1"
            } else {
                "acct:by-number:SZ-1:storno"
            };
            external_id_query(id)
                .respond_with(not_found())
                .expect(2)
                .mount(&mock)
                .await;
            storno_of_number("SZ-1")
                .respond_with(
                    created("SS-1", "-1000", "-1270")
                        .insert_header("szlahu_error_code", "56")
                        .insert_header("szlahu_error", "notification failed"),
                )
                .expect(1)
                .mount(&mock)
                .await;
            let body = json!({"invoice_number":"SZ-1", "buyer_email":explicit.then_some("Buyer+storno@example.com")});
            let key = format!("email-{managed}-{explicit}");
            let first = restate.invoke(&call, Some(&body), Some(&key)).await;
            assert_eq!(first.status, 200, "{}", first.body);
            assert_eq!(first.body["outcome"], "reversed");
            assert_eq!(first.body["storno_number"], "SS-1");
            assert_eq!(
                first.body["warnings"],
                json!(["notification_delivery_failed"])
            );
            // Retained completion wins even if an ingress retry supplies new input.
            let changed = json!({"invoice_number":"SZ-1", "buyer_email":"changed@example.com"});
            let replay = restate.invoke(&call, Some(&changed), Some(&key)).await;
            assert_eq!(replay.body, first.body);
            let requests = mock.received_requests().await.expect("requests");
            let bodies: Vec<_> = requests
                .iter()
                .map(|r| String::from_utf8_lossy(&r.body))
                .filter(|body| body.contains("action-szamla_agent_st"))
                .collect();
            assert_eq!(bodies.len(), 1);
            assert_eq!(
                bodies[0].contains("<email>Buyer+storno@example.com</email>"),
                explicit
            );
            assert_eq!(bodies[0].contains("<email>"), explicit);
            assert!(!bodies[0].contains("changed@example.com"));
            assert!(bodies[0].contains(&crate::common::original_telj_tag()));
            mock.verify().await;
            mock.reset().await;
        }
    }
    // The recipient never bypasses Agent's managed-order routing.
    number_query("SZ-MANAGED")
        .respond_with(Doc::of("SZ-MANAGED", "SZ", "EMAIL").response())
        .expect(1)
        .mount(&mock)
        .await;
    let routed = restate
        .invoke(
            &Call::service("Szamlazz.Agent", "storno"),
            Some(&json!({"invoice_number":"SZ-MANAGED", "buyer_email":"a@example.com"})),
            None,
        )
        .await;
    assert_eq!(routed.body["outcome"], "managed_by_order");
    assert_eq!(routed.body["order_key"], "EMAIL");
    assert_eq!(mock.received_requests().await.expect("requests").len(), 1);
    mock.verify().await;
    restate.finish().await;
}
