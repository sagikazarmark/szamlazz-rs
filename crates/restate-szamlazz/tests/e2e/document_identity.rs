//! Parsed responses must establish document identity before settling a write.
use super::*;
use crate::common::storno_of_number;

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; request validation and storno identity"]
async fn e2e_document_identity_refuses_invalid_selectors_and_wrong_originals() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "verify-identity",
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
    for selector in [
        json!({"external_id":"bad\u{0}id"}),
        json!({"order_number":"bad\u{ffff}order"}),
    ] {
        let refused = restate
            .invoke(
                &Call::service("Szamlazz.Agent", "query"),
                Some(&json!({"selector":selector})),
                None,
            )
            .await;
        assert_eq!(refused.status, 400, "{}", refused.body);
        assert_eq!(
            refused.fault::<restate_szamlazz::contract::Fault>().code,
            restate_szamlazz::contract::TerminalCode::InvalidInput
        );
        assert!(
            restate
                .admin()
                .runs(refused.invocation_id())
                .await
                .is_empty()
        );
    }
    assert!(mock.received_requests().await.expect("requests").is_empty());
    for managed in [false, true] {
        let number = if managed {
            "SZ-MANAGED"
        } else {
            "SZ-UNMANAGED"
        };
        number_query(number)
            .respond_with(
                Doc {
                    order: if managed { Some("VERIFY-ORDER") } else { None },
                    ..Doc::new("SZ-WRONG", "SZ")
                }
                .response(),
            )
            .mount(&mock)
            .await;
        storno_of_number(number)
            .respond_with(created("SS-WRONG", "-1000", "-1270"))
            .expect(0)
            .mount(&mock)
            .await;
        let call = if managed {
            Call::object("Szamlazz.Order", "VERIFY-ORDER", "storno_invoice")
        } else {
            Call::service("Szamlazz.Agent", "storno")
        };
        let refused = restate
            .invoke(&call, Some(&json!({"invoice_number":number})), None)
            .await;
        assert_eq!(refused.status, 503, "{}", refused.body);
        assert!(
            !restate
                .admin()
                .runs(refused.invocation_id())
                .await
                .iter()
                .any(|name| name == "arm-write")
        );
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; malformed settlement evidence"]
#[allow(
    clippy::too_many_lines,
    reason = "lost send, rejected evidence and subsequent settlement in one journey"
)]
async fn e2e_document_identity_retains_uncertainty_until_valid_evidence() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "document-identity",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let mut config = WorkerConfig::new("acct".parse().expect("namespace"));
    config.read.max_attempts = Some(1);
    let (order, _) = services_with_config(&mock.uri(), config.validate().expect("config"));
    let mut options = restate_sdk::endpoint::ServiceOptions::default();
    for handler in ["create_invoice", "storno_invoice"] {
        options = options.handler(
            handler,
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_max_attempts(1)
                .retry_policy_pause_on_max_attempts(),
        );
    }
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

    for storno in [false, true] {
        let key = if storno {
            "SELF-STORNO"
        } else {
            "NAMELESS-CREATE"
        };
        let original = "SZ-ORIGINAL";
        let sent = Arc::new(AtomicBool::new(false));
        let valid = Arc::new(AtomicBool::new(false));
        let external = if storno {
            format!("acct:{key}:storno:{original}")
        } else {
            format!("acct:{key}:invoice")
        };
        let state = sent.clone();
        let fixed = valid.clone();
        external_id_query(&external)
            .respond_with(move |_: &wiremock::Request| {
                if !state.load(Ordering::SeqCst) {
                    return not_found();
                }
                if storno {
                    Doc {
                        reversed: true,
                        referenced_invoice: Some(original),
                        ..Doc::of(
                            if fixed.load(Ordering::SeqCst) {
                                "SS-VALID"
                            } else {
                                original
                            },
                            "SS",
                            key,
                        )
                    }
                    .response()
                } else {
                    Doc::of(
                        if fixed.load(Ordering::SeqCst) {
                            "SZ-VALID"
                        } else {
                            ""
                        },
                        "SZ",
                        key,
                    )
                    .response()
                }
            })
            .mount(&mock)
            .await;
        let state = sent.clone();
        number_query(original)
            .respond_with(move |_: &wiremock::Request| {
                let acted = state.load(Ordering::SeqCst);
                Doc {
                    reversed: acted,
                    ..Doc::of(original, "SZ", key)
                }
                .response()
            })
            .mount(&mock)
            .await;
        number_query("SS-VALID")
            .respond_with(
                Doc {
                    referenced_invoice: Some(original),
                    ..Doc::of("SS-VALID", "SS", key)
                }
                .response(),
            )
            .mount(&mock)
            .await;
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
        let mutation = if storno {
            storno_of_number(original)
        } else {
            create_for(key)
        };
        mutation
            .respond_with(move |_: &wiremock::Request| {
                sent.store(true, Ordering::SeqCst);
                ResponseTemplate::new(500)
            })
            .expect(1)
            .mount(&mock)
            .await;
        let handler = if storno {
            "storno_invoice"
        } else {
            "create_invoice"
        };
        let body = if storno {
            json!({"invoice_number":original})
        } else {
            create_body(dec!(1000))
        };
        let call = Call::object("Szamlazz.Order", key, handler);
        let owner = restate.invoke(&call.send(), Some(&body), Some(key)).await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused", "completed"])
            .await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let observed = restate.invoke(&observe, None, None).await;
        assert_eq!(observed.body["state"], "unresolved", "{}", observed.body);
        if storno {
            restate.admin().kill(owner.invocation_id()).await;
            let recover = Call::object("Szamlazz.Order", key, "recover");
            let mut evidence = json!({"marker":observed.body["marker"],"evidence":{"type":"document","number":original}});
            // Only the candidate read is self-referential. The subsequent
            // original read remains a correctly numbered, reversed invoice,
            // so distinctness is tested independently of original-type checks.
            number_query(original)
                .respond_with(
                    Doc {
                        reversed: true,
                        referenced_invoice: Some(original),
                        ..Doc::of(original, "SS", key)
                    }
                    .response(),
                )
                .with_priority(1)
                .up_to_n_times(1)
                .mount(&mock)
                .await;
            let refused = restate.invoke(&recover, Some(&evidence), None).await;
            assert_eq!(refused.status, 500, "{}", refused.body);
            assert_eq!(
                restate.invoke(&observe, None, None).await.body["state"],
                "unresolved"
            );
            valid.store(true, Ordering::SeqCst);
            evidence["evidence"]["number"] = json!("SS-VALID");
            number_query(original)
                .respond_with(
                    Doc {
                        reversed: true,
                        ..Doc::of(original, "D", key)
                    }
                    .response(),
                )
                .with_priority(1)
                .up_to_n_times(1)
                .mount(&mock)
                .await;
            let refused = restate.invoke(&recover, Some(&evidence), None).await;
            assert_eq!(
                refused.status, 500,
                "a proforma cannot be the storno original: {}",
                refused.body
            );
            assert_eq!(
                restate.invoke(&observe, None, None).await.body["state"],
                "unresolved"
            );
            let recovered = restate.invoke(&recover, Some(&evidence), None).await;
            assert_eq!(recovered.status, 200, "{}", recovered.body);
        } else {
            valid.store(true, Ordering::SeqCst);
            restate.admin().resume(owner.invocation_id()).await;
            let completed = restate.invoke(&call, Some(&body), Some(key)).await;
            assert_eq!(
                completed.body["outcome"], "reconciled",
                "{}",
                completed.body
            );
            assert_eq!(
                completed.body["invoice_number"], "SZ-VALID",
                "{}",
                completed.body
            );
        }
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}
