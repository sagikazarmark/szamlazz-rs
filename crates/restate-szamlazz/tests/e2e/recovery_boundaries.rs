//! Persisted-state and pinned-account recovery through actual Restate.
use super::*;
use restate_szamlazz::contract::{Fault, TerminalCode};

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; malformed persisted markers"]
#[allow(
    clippy::too_many_lines,
    reason = "one persisted-state corruption matrix across all mutation adapters"
)]
async fn e2e_recovery_unreadable_state_blocks_every_mutation() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "unreadable-markers",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let (order, _) = services_with_config(
        &mock.uri(),
        WorkerConfig::new("acct".parse().expect("namespace"))
            .validate()
            .expect("config"),
    );
    restate
        .deploy(
            Endpoint::builder()
                .bind(order.with_recovery_authorizer(Arc::new(Operator)))
                .build(),
        )
        .await;
    for (index, corruption) in [
        "encoding",
        "json",
        "version",
        "identity",
        "padded-order",
        "operation",
    ]
    .into_iter()
    .enumerate()
    {
        let key = format!("UNREADABLE-{index}");
        let marker = json!({"version":1,"token":"owner","owner_invocation":"owner","created_at":"2026-09-11T12:00:00Z","scope":null,"order":key,"namespace":"acct","external_id":format!("acct:{key}:invoice"),"account_id":"acct","endpoint":mock.uri(),"credential_ref":"acct","operation":{"type":"create","kind":"invoice","expected_number":null,"corrected_number":null}});
        let mut changed = marker.clone();
        let raw = match corruption {
            "encoding" => vec![0xff],
            "json" => b"{\"version\":".to_vec(),
            "version" => {
                changed["version"] = json!(2);
                serde_json::to_vec(&changed).expect("bytes")
            }
            "identity" => {
                changed["external_id"] = json!("acct:ANOTHER:invoice");
                serde_json::to_vec(&changed).expect("bytes")
            }
            "padded-order" => {
                changed["order"] = json!(format!(" {key} "));
                serde_json::to_vec(&changed).expect("bytes")
            }
            _ => {
                changed["operation"]["corrected_number"] = json!("BASE");
                serde_json::to_vec(&changed).expect("bytes")
            }
        };
        let response = crate::common::http_client()
            .post(format!(
                "{}/services/Szamlazz.Order/state",
                restate.admin().base()
            ))
            .json(&json!({"object_key":key,"new_state":{"unresolved-write":raw}}))
            .send()
            .await
            .expect("state injection");
        assert_eq!(response.status(), 202);
        let observe = Call::object("Szamlazz.Order", &key, "observe_unresolved");
        tokio::time::timeout(Duration::from_secs(30), async {
            loop {
                if restate.invoke(&observe, None, None).await.body["state"] == "unreadable" {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .expect("state patch visible through observation");
        for handler in [
            "create_proforma",
            "create_invoice",
            "create_prepayment",
            "create_final",
            "correct_invoice",
            "storno_invoice",
            "delete_proforma",
        ] {
            let body = match handler {
                "correct_invoice" => {
                    json!({"invoice_number":"BASE","correction_id":"c1","document":create_body(dec!(1))["document"]})
                }
                "storno_invoice" => json!({"invoice_number":"BASE"}),
                "delete_proforma" => json!({"expected_number":"D-1"}),
                _ => create_body(dec!(1)),
            };
            let reply = restate
                .invoke(
                    &Call::object("Szamlazz.Order", &key, handler),
                    Some(&body),
                    None,
                )
                .await;
            assert_eq!(
                reply.fault::<Fault>().code,
                TerminalCode::OutcomeUnknown,
                "{corruption}/{handler}"
            );
            assert!(restate.admin().runs(reply.invocation_id()).await.is_empty());
            assert!(
                !restate
                    .admin()
                    .journal(reply.invocation_id())
                    .await
                    .iter()
                    .any(|entry| entry.entry_type.contains("ClearState"))
            );
        }
        // Supply a well-formed request so refusal comes from the stored bytes,
        // not the request decoder or an exact-marker comparison.
        let body = json!({"marker":marker,"evidence":{"type":"document","number":"ISSUED"}});
        let reply = restate
            .invoke(
                &Call::object("Szamlazz.Order", &key, "recover"),
                Some(&body),
                None,
            )
            .await;
        let fault: Fault = reply.fault();
        assert_eq!(fault.code, TerminalCode::OutcomeUnknown);
        assert!(fault.message.contains("unreadable"));
        assert_eq!(
            restate.admin().runs(reply.invocation_id()).await,
            ["authorize-recovery"]
        );
        assert!(
            !restate
                .admin()
                .journal(reply.invocation_id())
                .await
                .iter()
                .any(|entry| entry.entry_type.contains("ClearState"))
        );
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "unreadable"
        );
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; pinned recovery after resolver change"]
#[allow(
    clippy::too_many_lines,
    reason = "one pinned-account recovery lifecycle including interrupted verification replay"
)]
async fn e2e_recovery_pins_account_and_replays_verification_without_credentials() {
    use crate::harness::accounts::{AGENT_KEY, multi_account_services};
    use restate_szamlazz::service::WriteCheckpoint;
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "pinned-recovery",
            ..SERVER
        })
        .await;
    let original = MockServer::start().await;
    let replacement = MockServer::start().await;
    let (accounts, order, _) = multi_account_services(&original.uri()).await;
    let hold = Arc::new(InterruptAt {
        point: WriteCheckpoint::RecoveryVerified,
        once: AtomicBool::new(false),
        reached: Notify::new(),
    });
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
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
    let key = "PINNED-RECOVERY";
    for kind in ["invoice", "proforma", "prepayment", "final"] {
        external_id_query(&format!("acct:{key}:{kind}"))
            .respond_with(not_found())
            .mount(&original)
            .await;
    }
    order_query(key)
        .respond_with(not_found())
        .mount(&original)
        .await;
    create_for(key)
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&original)
        .await;
    let owner = restate
        .invoke(
            &Call::object("Szamlazz.Order", key, "create_invoice")
                .scoped("acme")
                .send(),
            Some(&create_body(dec!(1))),
            None,
        )
        .await;
    restate
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    restate.admin().kill(owner.invocation_id()).await;
    let observe = Call::object("Szamlazz.Order", key, "observe_unresolved").scoped("acme");
    let marker = restate.invoke(&observe, None, None).await.body["marker"].clone();
    assert_eq!(marker["endpoint"], original.uri());
    assert_eq!(marker["credential_ref"], "acme");
    original.verify().await;
    original.reset().await;
    accounts.update("acme", |account| {
        account.id = "changed".into();
        account.endpoint = replacement.uri().parse().expect("endpoint");
        account.credential_ref = "beta".into();
    });
    external_id_query(&format!("acct:{key}:invoice"))
        .and(wiremock::matchers::body_string_contains(
            crate::common::agent_key_tag(AGENT_KEY),
        ))
        .respond_with(Doc::of("ISSUED", "SZ", key).response())
        .expect(1)
        .mount(&original)
        .await;
    let before_fetch = accounts.fetches("acme");
    let before_resolve = accounts.resolutions("acme");
    let call = Call::object("Szamlazz.Order", key, "recover").scoped("acme");
    let body = json!({"marker":marker,"evidence":{"type":"document","number":"ISSUED"}});
    let recovery = restate
        .invoke(&call.send(), Some(&body), Some("pinned-recovery"))
        .await;
    tokio::time::timeout(Duration::from_secs(30), hold.reached.notified())
        .await
        .expect("verification recorded");
    restate.admin().pause(recovery.invocation_id()).await;
    assert!(
        restate_e2e_harness::run_result(
            &restate.admin().journal(recovery.invocation_id()).await,
            "verify-recovery"
        )
        .is_some()
    );
    assert_eq!(accounts.fetches("acme"), before_fetch + 1);
    accounts.set_unavailable("acme", true);
    accounts.set_unavailable("beta", true);
    restate.admin().resume(recovery.invocation_id()).await;
    let reply = restate
        .invoke(&call, Some(&body), Some("pinned-recovery"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["evidence"], body["evidence"]);
    assert_eq!(accounts.fetches("acme"), before_fetch + 1);
    assert_eq!(accounts.fetches("beta"), 0);
    assert_eq!(accounts.resolutions("acme"), before_resolve);
    assert!(
        replacement
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
    assert_eq!(
        restate.invoke(&observe, None, None).await.body["state"],
        "absent"
    );
    crate::write_commands::check_settled(
        &restate.admin().journal(reply.invocation_id()).await,
        "record-recovery",
    );
    original.verify().await;
    restate.finish().await;
}
