//! Both protected pre-send boundaries require fresh, matching reversal evidence.
use super::*;
use crate::common::storno_of_number;

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; protected storno lookup evidence"]
async fn e2e_protected_storno_lookup_requires_complete_evidence() {
    exercise(false).await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; protected storno leading-query evidence"]
async fn e2e_protected_storno_leading_query_requires_complete_evidence() {
    exercise(true).await;
}

#[allow(
    clippy::too_many_lines,
    reason = "each evidence defect crosses refusal, journal inspection and later settlement"
)]
async fn exercise(leading: bool) {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: if leading {
                "storno-leading-evidence"
            } else {
                "storno-lookup-evidence"
            },
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let mut config = WorkerConfig::new("acct".parse().expect("namespace"));
    config.read.max_attempts = Some(1);
    let (order, _) = services_with_config(&mock.uri(), config.validate().expect("config"));
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

    for defect in [
        "candidate-order",
        "live-original",
        "original-type",
        "original-order",
    ] {
        let key = format!("EVIDENCE-{defect}");
        let valid = Arc::new(AtomicBool::new(false));
        let original_reads = Arc::new(AtomicUsize::new(0));
        let candidate_reads = Arc::new(AtomicUsize::new(0));
        let fixed = valid.clone();
        let reads = original_reads.clone();
        let expected_order = key.clone();
        number_query("SZ-ORIGINAL")
            .respond_with(move |_: &wiremock::Request| {
                let initial = reads.fetch_add(1, Ordering::SeqCst) == 0;
                let valid = fixed.load(Ordering::SeqCst);
                // The initial verify deliberately reports a live invoice. Only
                // the fresh check inside lookup can establish its reversal.
                Doc {
                    reversed: !initial && (valid || defect != "live-original"),
                    ..Doc::of(
                        "SZ-ORIGINAL",
                        if !initial && !valid && defect == "original-type" {
                            "D"
                        } else {
                            "SZ"
                        },
                        if !initial && !valid && defect == "original-order" {
                            "ANOTHER"
                        } else {
                            &expected_order
                        },
                    )
                }
                .response()
            })
            .mount(&mock)
            .await;
        let fixed = valid.clone();
        let reads = candidate_reads.clone();
        let expected_order = key.clone();
        external_id_query(&format!("acct:{key}:storno:SZ-ORIGINAL"))
            .respond_with(move |_: &wiremock::Request| {
                // Force the second journey past lookup, so the candidate first
                // appears inside the armed write's leading query.
                if reads.fetch_add(1, Ordering::SeqCst) == 0 && leading {
                    return not_found();
                }
                Doc {
                    referenced_invoice: Some("SZ-ORIGINAL"),
                    ..Doc::of(
                        "SS-CANDIDATE",
                        "SS",
                        if !fixed.load(Ordering::SeqCst) && defect == "candidate-order" {
                            "ANOTHER"
                        } else {
                            &expected_order
                        },
                    )
                }
                .response()
            })
            .mount(&mock)
            .await;
        storno_of_number("SZ-ORIGINAL")
            .respond_with(created("SS-UNEXPECTED", "-1000", "-1270"))
            .expect(0)
            .mount(&mock)
            .await;
        let call = Call::object("Szamlazz.Order", &key, "storno_invoice");
        let observe = Call::object("Szamlazz.Order", &key, "observe_unresolved");
        let body = json!({"invoice_number":"SZ-ORIGINAL"});
        let owner = if leading {
            let owner = restate.invoke(&call.send(), Some(&body), Some(&key)).await;
            restate
                .admin()
                .await_status(owner.invocation_id(), &["paused"])
                .await;
            assert_eq!(
                restate.invoke(&observe, None, None).await.body["state"],
                "unresolved"
            );
            assert_eq!(
                restate.admin().runs(owner.invocation_id()).await,
                [
                    "namespace",
                    "account",
                    "verify-original-SZ-ORIGINAL",
                    "lookup-storno-SZ-ORIGINAL",
                    "prepare-write",
                    "arm-write",
                    "storno-SZ-ORIGINAL",
                    "reconcile-write"
                ],
                "{defect}: evidence failure after arming must reconcile"
            );
            Some(owner)
        } else {
            let refused = restate.invoke(&call, Some(&body), None).await;
            assert_eq!(refused.status, 503, "{defect}: {}", refused.body);
            assert_eq!(
                restate.admin().runs(refused.invocation_id()).await,
                [
                    "namespace",
                    "account",
                    "verify-original-SZ-ORIGINAL",
                    "lookup-storno-SZ-ORIGINAL"
                ],
                "{defect}: evidence failure before arming stays a read fault"
            );
            assert_eq!(
                restate.invoke(&observe, None, None).await.body["state"],
                "absent"
            );
            None
        };
        // Inspect zero sends before allowing positive evidence, not just after
        // the invocation's final answer.
        mock.verify().await;
        valid.store(true, Ordering::SeqCst);
        if let Some(owner) = owner {
            restate.admin().resume(owner.invocation_id()).await;
        } else {
            original_reads.store(0, Ordering::SeqCst);
        }
        let completed = restate
            .invoke(&call, Some(&body), leading.then_some(key.as_str()))
            .await;
        assert_eq!(completed.status, 200, "{defect}: {}", completed.body);
        assert_eq!(completed.body["outcome"], "reversed", "{defect}");
        assert_eq!(completed.body["storno_number"], "SS-CANDIDATE", "{defect}");
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
        assert!(
            original_reads.load(Ordering::SeqCst) >= 2,
            "fresh original was queried"
        );
        mock.verify().await;
        mock.reset().await;
    }
    restate.finish().await;
}
