//! Contradictory numbered acknowledgements must not grant a later send.
use super::*;
use crate::common::storno_of_number;
use restate_szamlazz::contract::{Fault, TerminalCode};

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; contradictory write settlement"]
#[allow(
    clippy::too_many_lines,
    reason = "three reply shapes cross the same durable protection boundary"
)]
async fn e2e_contradictory_numbered_replies_retain_write_protection() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "contradictory-replies",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let config = WorkerConfig::new("acct".parse().expect("namespace"))
        .validate()
        .expect("config");
    let (order, _) = services_with_config(&mock.uri(), config);
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
    for (key, storno, notification_failure) in [
        ("OLD-CREATE", false, false),
        ("OLD-NOTIFY", false, true),
        ("OLD-STORNO", true, false),
    ] {
        let mut reply = if storno {
            created("SZ-OLD", "-1000", "-1270")
        } else {
            created("SZ-OLD", "1000", "1270")
        };
        if notification_failure {
            reply = reply.insert_header("szlahu_error_code", "56");
        }
        if storno {
            number_query("SZ-OLD")
                .respond_with(Doc::of("SZ-OLD", "SZ", key).response())
                .mount(&mock)
                .await;
            external_id_query(&format!("acct:{key}:storno:SZ-OLD"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
            storno_of_number("SZ-OLD")
                .respond_with(reply)
                .expect(1)
                .mount(&mock)
                .await;
        } else {
            external_id_query(&format!("acct:{key}:invoice"))
                .respond_with(
                    Doc {
                        reversed: true,
                        ..Doc::of("SZ-OLD", "SZ", key)
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
            create_for(key)
                .respond_with(reply)
                .expect(1)
                .mount(&mock)
                .await;
        }
        order_query(key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let call = Call::object(
            "Szamlazz.Order",
            key,
            if storno {
                "storno_invoice"
            } else {
                "create_invoice"
            },
        );
        let body = if storno {
            json!({"invoice_number":"SZ-OLD"})
        } else {
            let mut body = create_body(dec!(1000));
            body["options"] = json!({"reissue":{"expected_number":"SZ-OLD"}});
            body
        };
        let owner = restate.invoke(&call.send(), Some(&body), Some(key)).await;
        // A bad implementation completes; a protected uncertain write pauses.
        restate
            .admin()
            .await_status(owner.invocation_id(), &["completed", "paused"])
            .await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let state = restate.invoke(&observe, None, None).await;
        assert_eq!(state.body["state"], "unresolved", "{key}: {}", state.body);
        assert!(
            restate
                .admin()
                .runs(owner.invocation_id())
                .await
                .iter()
                .any(|run| run == "reconcile-write")
        );
        // Release the lock, preserving uncertainty, then use a fresh invocation.
        restate.admin().kill(owner.invocation_id()).await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["completed"])
            .await;
        let blocked = restate.invoke(&call, Some(&body), None).await;
        assert_eq!(blocked.status, 500, "{key}: {}", blocked.body);
        assert_eq!(blocked.fault::<Fault>().code, TerminalCode::OutcomeUnknown);
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "unresolved"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}
