//! Recovery after a confirmed external effect becomes unqueryable by our id.
use super::*;
use crate::common::{delete_of, storno_of_number};

// Vendor evidence deliberately exceeds mutation-input bounds and includes XML metacharacters.
const VENDOR_NUMBER: &str = "VENDOR:document-number-outside-the-40-byte-bound & 1";
// Doc and number_query accept literal XML text in these fixtures.
const VENDOR_NUMBER_XML: &str = "VENDOR:document-number-outside-the-40-byte-bound &amp; 1";

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; interrupted storno and deletion adapters"]
#[allow(
    clippy::too_many_lines,
    reason = "two distinct protected write adapters after effect before result recording"
)]
async fn e2e_unresolved_storno_and_delete_do_not_repeat_an_interrupted_send() {
    use restate_szamlazz::service::WriteCheckpoint;
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "write-adapter-interruption",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    for deletion in [false, true] {
        let key = if deletion {
            "INTERRUPTED-DELETE"
        } else {
            "INTERRUPTED-STORNO"
        };
        let original = if deletion {
            "D-INTERRUPTED"
        } else {
            "SZ-INTERRUPTED"
        };
        let hold = Arc::new(InterruptAt {
            point: WriteCheckpoint::Sent,
            once: AtomicBool::new(false),
            reached: Notify::new(),
        });
        let (order, _) = services_with_config(
            &mock.uri(),
            WorkerConfig::new("acct".parse().expect("namespace"))
                .validate()
                .expect("config"),
        );
        let handler = if deletion {
            "delete_proforma"
        } else {
            "storno_invoice"
        };
        let options = restate_sdk::endpoint::ServiceOptions::default().handler(
            handler,
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
        let acted = Arc::new(AtomicBool::new(false));
        let state = acted.clone();
        number_query(original)
            .respond_with(move |_: &wiremock::Request| {
                Doc {
                    reversed: !deletion && state.load(Ordering::SeqCst),
                    ..Doc::of(original, if deletion { "D" } else { "SZ" }, key)
                }
                .response()
            })
            .mount(&mock)
            .await;
        let state = acted.clone();
        external_id_query(&if deletion {
            format!("acct:{key}:proforma")
        } else {
            format!("acct:{key}:storno:{original}")
        })
        .respond_with(move |_: &wiremock::Request| {
            if deletion {
                if state.load(Ordering::SeqCst) {
                    not_found()
                } else {
                    Doc::of(original, "D", key).response()
                }
            } else if state.load(Ordering::SeqCst) {
                Doc {
                    referenced_invoice: Some(original),
                    ..Doc::of("SS-INTERRUPTED", "SS", key)
                }
                .response()
            } else {
                not_found()
            }
        })
        .mount(&mock)
        .await;
        let send = if deletion {
            delete_of(original)
        } else {
            storno_of_number(original)
        };
        send.respond_with(move |_: &wiremock::Request| {
            acted.store(true, Ordering::SeqCst);
            if deletion {
                crate::common::proforma_deleted()
            } else {
                created("SS-INTERRUPTED", "-1000", "-1270")
            }
        })
        .expect(1)
        .mount(&mock)
        .await;
        let body = if deletion {
            json!({"expected_number":original})
        } else {
            json!({"invoice_number":original})
        };
        let call = Call::object("Szamlazz.Order", key, handler);
        let owner = restate.invoke(&call.send(), Some(&body), Some(key)).await;
        tokio::time::timeout(Duration::from_secs(30), hold.reached.notified())
            .await
            .expect("effect before result");
        restate.admin().pause(owner.invocation_id()).await;
        crate::write_commands::check(&restate.admin().journal(owner.invocation_id()).await);
        restate.admin().resume(owner.invocation_id()).await;
        if deletion {
            restate
                .admin()
                .await_status(owner.invocation_id(), &["paused"])
                .await;
            let observed = restate
                .invoke(
                    &Call::object("Szamlazz.Order", key, "observe_unresolved"),
                    None,
                    None,
                )
                .await;
            assert_eq!(observed.body["state"], "unresolved");
            restate.admin().kill(owner.invocation_id()).await;
            let evidence = json!({"marker":observed.body["marker"],"evidence":{"type":"completed","audit_reference":"confirmed-deletion","completion":{"type":"deleted","number":original},"completed_and_cannot_execute_later":true}});
            let recovered = restate
                .invoke(
                    &Call::object("Szamlazz.Order", key, "recover"),
                    Some(&evidence),
                    None,
                )
                .await;
            assert_eq!(recovered.status, 200);
            crate::write_commands::check(&restate.admin().journal(recovered.invocation_id()).await);
        } else {
            let completed = restate.invoke(&call, Some(&body), Some(key)).await;
            assert_eq!(completed.body["storno_number"], "SS-INTERRUPTED");
            crate::write_commands::check(&restate.admin().journal(completed.invocation_id()).await);
        }
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; recorded recovery survives interruption"]
#[allow(
    clippy::too_many_lines,
    reason = "recovery command boundaries and revocation in one runtime"
)]
async fn e2e_unresolved_recovery_replays_admission_and_recorded_evidence_before_clearing() {
    use restate_szamlazz::service::WriteCheckpoint;
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "recovery-interruption",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    for (index, point) in [
        WriteCheckpoint::RecoveryAuthorized,
        WriteCheckpoint::RecoveryVerified,
        WriteCheckpoint::RecoveryRecorded,
    ]
    .into_iter()
    .enumerate()
    {
        let key = format!("RECOVERY-REPLAY-{index}");
        let hold = Arc::new(InterruptAt {
            point,
            once: AtomicBool::new(false),
            reached: Notify::new(),
        });
        let auth = Arc::new(AuthenticatedOperators(std::sync::Mutex::new(
            std::collections::HashSet::from(["admitted".into()]),
        )));
        let (order, _) = services_with_config(
            &mock.uri(),
            WorkerConfig::new("acct".parse().expect("namespace"))
                .validate()
                .expect("config"),
        );
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
                            .with_recovery_authorizer(auth.clone())
                            .into_service_definition()
                            .options(options),
                    )
                    .build(),
            )
            .await;
        let visible = Arc::new(AtomicBool::new(false));
        let state = visible.clone();
        let order_key = key.clone();
        external_id_query(&format!("acct:{key}:invoice"))
            .respond_with(move |_: &wiremock::Request| {
                if state.load(Ordering::SeqCst) {
                    Doc::of(VENDOR_NUMBER_XML, "SZ", &order_key).response()
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        for kind in ["proforma", "prepayment", "final"] {
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        order_query(&key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        create_for(&key)
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock)
            .await;
        let owner = restate
            .invoke(
                &Call::object("Szamlazz.Order", &key, "create_invoice").send(),
                Some(&create_body(dec!(1000))),
                None,
            )
            .await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        restate.admin().kill(owner.invocation_id()).await;
        let http = crate::common::http_client();
        let observe = Call::object("Szamlazz.Order", &key, "observe_unresolved");
        let observed: serde_json::Value = http
            .post(format!("{}{}", restate.ingress_url(), observe.path()))
            .header("x-operator-assertion", "admitted")
            .send()
            .await
            .expect("observe")
            .json()
            .await
            .expect("marker");
        visible.store(true, Ordering::SeqCst);
        let body = json!({"marker":observed["marker"],"evidence":{"type":"document","number":VENDOR_NUMBER}});
        let call = Call::object("Szamlazz.Order", &key, "recover");
        let submitted: serde_json::Value = http
            .post(format!("{}{}", restate.ingress_url(), call.send().path()))
            .header("x-operator-assertion", "admitted")
            .header("Idempotency-Key", &key)
            .json(&body)
            .send()
            .await
            .expect("submit")
            .json()
            .await
            .expect("invocation");
        let id = submitted["invocationId"].as_str().expect("invocation id");
        tokio::time::timeout(Duration::from_secs(30), hold.reached.notified())
            .await
            .expect("recovery checkpoint");
        restate.admin().pause(id).await;
        let before = restate.admin().journal(id).await;
        assert!(
            !before
                .iter()
                .any(|entry| entry.entry_type.contains("ClearState"))
        );
        // Revocation applies to new invocations, not the command prefix already admitted.
        auth.0.lock().expect("registry").clear();
        if point != WriteCheckpoint::RecoveryAuthorized {
            visible.store(false, Ordering::SeqCst);
        }
        restate.admin().resume(id).await;
        let result = restate.invoke(&call, Some(&body), Some(&key)).await;
        assert_eq!(result.status, 200, "{}", result.body);
        assert_eq!(result.body["operator"], "authenticated-operator");
        assert_eq!(result.body["evidence"], body["evidence"]);
        let revoked = http
            .post(format!("{}{}", restate.ingress_url(), call.path()))
            .header("x-operator-assertion", "admitted")
            .header("Idempotency-Key", format!("{key}-revoked"))
            .json(&body)
            .send()
            .await
            .expect("revoked operator call");
        assert_eq!(revoked.status(), 403);
        let journal = restate.admin().journal(id).await;
        crate::write_commands::check_settled(&journal, "record-recovery");
        let recorded = journal
            .iter()
            .find(|entry| entry.is_run() && entry.name.as_deref() == Some("record-recovery"))
            .expect("recorded evidence");
        let cleared = journal
            .iter()
            .find(|entry| entry.entry_type.contains("ClearState"))
            .expect("clear marker");
        assert!(recorded.index < cleared.index);
        assert_eq!(
            journal
                .iter()
                .filter(
                    |entry| entry.is_run() && entry.name.as_deref() == Some("authorize-recovery")
                )
                .count(),
            1
        );
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; custom account marker round trip"]
#[allow(
    clippy::too_many_lines,
    reason = "custom resolver through send, kill, observation and recovery"
)]
async fn e2e_unresolved_custom_opaque_account_values_remain_recoverable() {
    use restate_szamlazz::account::{
        Account, AccountResolver, Accounts, BoxFuture, CredentialRef, CredentialStore, FetchError,
        ResolveError,
    };
    use restate_szamlazz::{Credentials, Order};
    struct CustomAccount {
        account: Account,
    }
    impl AccountResolver for CustomAccount {
        fn resolve<'a>(
            &'a self,
            _: Option<&'a str>,
        ) -> BoxFuture<'a, Result<Account, ResolveError>> {
            Box::pin(async { Ok(self.account.clone()) })
        }
    }
    impl CredentialStore for CustomAccount {
        fn fetch<'a>(
            &'a self,
            reference: &'a CredentialRef,
        ) -> BoxFuture<'a, Result<Credentials, FetchError>> {
            Box::pin(async move {
                assert_eq!(reference, &self.account.credential_ref);
                Ok(Credentials::agent_key(crate::harness::accounts::AGENT_KEY))
            })
        }
    }
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "opaque-recovery",
            ..SERVER
        })
        .await;
    let mock = MockServer::start().await;
    for (id, reference) in [("", "ref"), ("acct", "")] {
        let key = if id.is_empty() {
            "EMPTY-ACCOUNT"
        } else {
            "EMPTY-REF"
        };
        let mut account = Account::new(id, reference);
        account.endpoint = mock.uri().parse().expect("endpoint");
        let resolver = Arc::new(CustomAccount { account });
        let order = Order::from_parts(
            Accounts::new(resolver.clone(), resolver),
            WorkerConfig::new("acct".parse().expect("namespace"))
                .validate()
                .expect("config"),
        )
        .with_recovery_authorizer(Arc::new(Operator));
        let options = restate_sdk::endpoint::ServiceOptions::default().handler(
            "create_invoice",
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_max_attempts(1)
                .retry_policy_pause_on_max_attempts(),
        );
        restate
            .deploy(
                Endpoint::builder()
                    .bind(order.into_service_definition().options(options))
                    .build(),
            )
            .await;
        for kind in ["invoice", "prepayment", "final", "proforma"] {
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        order_query(key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        create_for(key)
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock)
            .await;
        let owner = restate
            .invoke(
                &Call::object("Szamlazz.Order", key, "create_invoice").send(),
                Some(&create_body(dec!(1000))),
                Some(key),
            )
            .await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        restate.admin().kill(owner.invocation_id()).await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let observed = restate.invoke(&observe, None, None).await;
        assert_eq!(observed.body["state"], "unresolved", "{}", observed.body);
        assert_eq!(observed.body["marker"]["account_id"], id);
        assert_eq!(observed.body["marker"]["credential_ref"], reference);
        let evidence = json!({"marker":observed.body["marker"],"evidence":{"type":"completed","audit_reference":"vendor-support-confirmation","completion":{"type":"issued","number":"SZ-RECOVERED"},"completed_and_cannot_execute_later":true}});
        assert_eq!(
            restate
                .invoke(
                    &Call::object("Szamlazz.Order", key, "recover"),
                    Some(&evidence),
                    None
                )
                .await
                .status,
            200
        );
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; storno evidence order identity"]
#[allow(
    clippy::too_many_lines,
    reason = "both wrong-order cases through immediate and operator verification"
)]
async fn e2e_unresolved_storno_requires_both_documents_to_carry_the_order() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "storno-order-evidence",
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
    for wrong_original in [false, true] {
        let key = if wrong_original {
            "WRONG-ORIGINAL"
        } else {
            "WRONG-STORNO"
        };
        let original = format!("SZ-{key}");
        let storno = format!("SS-{key}");
        number_query(&original)
            .respond_with(Doc::of(&original, "SZ", key).response())
            .up_to_n_times(1)
            .mount(&mock)
            .await;
        number_query(&original)
            .respond_with(
                Doc {
                    reversed: true,
                    ..Doc::of(
                        &original,
                        "SZ",
                        if wrong_original { "ANOTHER" } else { key },
                    )
                }
                .response(),
            )
            .mount(&mock)
            .await;
        number_query(&storno)
            .respond_with(
                Doc {
                    referenced_invoice: Some(&original),
                    ..Doc::of(&storno, "SS", if wrong_original { key } else { "ANOTHER" })
                }
                .response(),
            )
            .mount(&mock)
            .await;
        external_id_query(&format!("acct:{key}:storno:{original}"))
            .respond_with(not_found())
            .mount(&mock)
            .await;
        order_query(key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        storno_of_number(&original)
            .respond_with(crate::common::created_without_totals(&storno))
            .expect(1)
            .mount(&mock)
            .await;
        let call = Call::object("Szamlazz.Order", key, "storno_invoice");
        let owner = restate
            .invoke(
                &call.send(),
                Some(&json!({"invoice_number":original})),
                Some(key),
            )
            .await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused", "completed"])
            .await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let observed = restate.invoke(&observe, None, None).await;
        assert_eq!(
            observed.body["state"], "unresolved",
            "{wrong_original}: {}",
            observed.body
        );
        restate.admin().kill(owner.invocation_id()).await;
        let evidence = json!({"marker":observed.body["marker"],"evidence":{"type":"document","number":storno}});
        let refused = restate
            .invoke(
                &Call::object("Szamlazz.Order", key, "recover"),
                Some(&evidence),
                None,
            )
            .await;
        assert_eq!(refused.status, 500, "{}", refused.body);
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "unresolved"
        );
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; attestation excludes old documents"]
#[allow(
    clippy::too_many_lines,
    reason = "issued and reversed attestation boundaries through ingress"
)]
async fn e2e_unresolved_attestation_excludes_old_reissue_and_original_storno_numbers() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "attested-target",
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
    for reissue in [true, false] {
        let key = if reissue {
            "ATTEST-REISSUE"
        } else {
            "ATTEST-STORNO"
        };
        let original = if reissue {
            "OLD-ISSUED"
        } else {
            "OLD-ORIGINAL"
        };
        let external = if reissue {
            format!("acct:{key}:invoice")
        } else {
            format!("acct:{key}:storno:{original}")
        };
        external_id_query(&external)
            .respond_with(if reissue {
                Doc {
                    reversed: true,
                    ..Doc::of(original, "SZ", key)
                }
                .response()
            } else {
                not_found()
            })
            .mount(&mock)
            .await;
        number_query(original)
            .respond_with(Doc::of(original, "SZ", key).response())
            .mount(&mock)
            .await;
        for kind in ["proforma", "prepayment", "final"] {
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        order_query(key)
            .respond_with(not_found())
            .mount(&mock)
            .await;
        let send = if reissue {
            create_for(key)
        } else {
            storno_of_number(original)
        };
        send.respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock)
            .await;
        let body = if reissue {
            let mut body = create_body(dec!(1000));
            body["options"] = json!({"reissue":{"expected_number":original}});
            body
        } else {
            json!({"invoice_number":original})
        };
        let owner = restate
            .invoke(
                &Call::object(
                    "Szamlazz.Order",
                    key,
                    if reissue {
                        "create_invoice"
                    } else {
                        "storno_invoice"
                    },
                )
                .send(),
                Some(&body),
                Some(key),
            )
            .await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let marker = restate.invoke(&observe, None, None).await.body["marker"].clone();
        restate.admin().kill(owner.invocation_id()).await;
        let recover = Call::object("Szamlazz.Order", key, "recover");
        let mut request = json!({"marker":marker,"evidence":{"type":"completed","audit_reference":"AUDIT-302",
            "completion":{"type":if reissue { "issued" } else { "reversed" },"number":original},
            "completed_and_cannot_execute_later":true}});
        assert_eq!(
            restate.invoke(&recover, Some(&request), None).await.status,
            400
        );
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "unresolved"
        );
        request["evidence"]["completion"]["number"] = json!(VENDOR_NUMBER);
        let result = restate.invoke(&recover, Some(&request), None).await;
        assert_eq!(result.status, 200, "{}", result.body);
        assert_eq!(result.body["evidence"], request["evidence"]);
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; automatic storno evidence fallback"]
async fn e2e_unresolved_storno_automatically_uses_hint_or_external_id_evidence() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "storno-evidence",
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
        .deploy(Endpoint::builder().bind(order).build())
        .await;
    for candidate in [false, true] {
        let key = if candidate {
            "CANDIDATE-FALLBACK"
        } else {
            "HINT-FALLBACK"
        };
        let original = if candidate { "SZ-CANDIDATE" } else { "SZ-HINT" };
        let acted = Arc::new(AtomicBool::new(false));
        let state = acted.clone();
        number_query(original)
            .respond_with(move |_: &wiremock::Request| {
                Doc {
                    reversed: state.load(Ordering::SeqCst),
                    ..Doc::of(original, "SZ", key)
                }
                .response()
            })
            .mount(&mock)
            .await;
        let state = acted.clone();
        external_id_query(&format!("acct:{key}:storno:{original}"))
            .respond_with(move |_: &wiremock::Request| {
                if candidate && state.load(Ordering::SeqCst) {
                    Doc {
                        referenced_invoice: Some(original),
                        ..Doc::of("SS-REAL", "SS", key)
                    }
                    .response()
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        order_query(key)
            .respond_with(
                Doc {
                    referenced_invoice: Some(original),
                    ..Doc::of("SS-REAL", "SS", key)
                }
                .response(),
            )
            .mount(&mock)
            .await;
        number_query("SS-ABSENT")
            .respond_with(not_found())
            .mount(&mock)
            .await;
        storno_of_number(original)
            .respond_with(move |_: &wiremock::Request| {
                acted.store(true, Ordering::SeqCst);
                if candidate {
                    crate::common::created_without_totals("SS-ABSENT")
                } else {
                    ResponseTemplate::new(500)
                }
            })
            .expect(1)
            .mount(&mock)
            .await;
        let reply = restate
            .invoke(
                &Call::object("Szamlazz.Order", key, "storno_invoice"),
                Some(&json!({"invoice_number":original})),
                Some(key),
            )
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body["storno_number"], "SS-REAL");
        if candidate {
            let journal = restate.admin().journal(reply.invocation_id()).await;
            assert!(
                restate_e2e_harness::run_result(&journal, &format!("storno-{original}"))
                    .expect("send evidence")
                    .raw_contains("SS-ABSENT")
            );
        }
    }
    mock.verify().await;
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; original refusal survives diagnostic failure"]
#[allow(
    clippy::too_many_lines,
    reason = "one original-refusal diagnostic matrix"
)]
async fn e2e_unresolved_duplicate_refusal_survives_failed_diagnostic_query() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "duplicate-evidence",
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
                        .with_recovery_authorizer(Arc::new(Operator))
                        .into_service_definition()
                        .options(options),
                )
                .build(),
        )
        .await;
    for (key, failure, code) in [
        ("DUP-HTTP", 0, "71"),
        ("DUP-KEY", 1, "152"),
        ("DUP-HINT", 2, "71"),
        ("DUP-CORRECTIVE", 3, "152"),
    ] {
        let corrective = failure == 3;
        let kind = if corrective {
            "corrective:c1"
        } else {
            "invoice"
        };
        let queried = AtomicUsize::new(0);
        external_id_query(&format!("acct:{key}:{kind}"))
            .respond_with(move |_: &wiremock::Request| {
                if queried.fetch_add(1, Ordering::SeqCst) < 3 {
                    not_found()
                } else {
                    match failure {
                        0 => ResponseTemplate::new(502),
                        1 => crate::common::api_error("3", "credentials rejected"),
                        2 => not_found(),
                        _ => Doc {
                            referenced_invoice: Some("WRONG-BASE"),
                            ..Doc::of("HS-OTHER", "HS", key)
                        }
                        .response(),
                    }
                }
            })
            .mount(&mock)
            .await;
        for kind in ["prepayment", "final", "proforma"] {
            external_id_query(&format!("acct:{key}:{kind}"))
                .respond_with(not_found())
                .mount(&mock)
                .await;
        }
        let hints = AtomicUsize::new(0);
        order_query(key)
            .respond_with(move |_: &wiremock::Request| {
                if failure == 2 && hints.fetch_add(1, Ordering::SeqCst) > 0 {
                    crate::common::api_error("3", "credentials rejected")
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        create_for(key)
            .respond_with(crate::common::api_error(code, "duplicate order"))
            .expect(1)
            .mount(&mock)
            .await;
        let call = Call::object(
            "Szamlazz.Order",
            key,
            if corrective {
                "correct_invoice"
            } else {
                "create_invoice"
            },
        );
        let mut body = create_body(dec!(1000));
        if corrective {
            number_query("BASE")
                .respond_with(Doc::of("BASE", "SZ", key).response())
                .with_priority(1)
                .mount(&mock)
                .await;
            body =
                json!({"invoice_number":"BASE", "correction_id":"c1", "document":body["document"]});
        }
        let owner = restate.invoke(&call.send(), Some(&body), Some(key)).await;
        let status = restate
            .admin()
            .await_status(owner.invocation_id(), &["completed", "paused"])
            .await;
        assert_eq!(status, "completed", "a conclusive refusal must not pause");
        let reply = restate.invoke(&call, Some(&body), Some(key)).await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        if corrective {
            assert_eq!(reply.body["outcome"], "rejected");
            assert_eq!(reply.body["code"], code);
        } else {
            assert_eq!(reply.body["conflict_reason"], "duplicate_order_number");
        }
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
    }
    restate.finish().await;
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; operation-specific positive settlement"]
#[allow(clippy::too_many_lines, reason = "one runtime evidence matrix")]
async fn e2e_unresolved_positive_settlement_matches_the_operation() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "positive-settlement",
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
    let mut options = restate_sdk::endpoint::ServiceOptions::default();
    for handler in ["delete_proforma", "storno_invoice"] {
        options = options.handler(
            handler,
            restate_sdk::endpoint::HandlerOptions::default()
                .retry_policy_initial_interval(Duration::from_secs(1))
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

    for deleting in [true, false] {
        let (key, number, kind, handler) = if deleting {
            ("DELETE-EVIDENCE", "D-1", "D", "delete_proforma")
        } else {
            ("STORNO-EVIDENCE", "SZ-1", "SZ", "storno_invoice")
        };
        let acted = Arc::new(AtomicBool::new(false));
        let observed = acted.clone();
        number_query(number)
            .respond_with(move |_: &wiremock::Request| {
                if deleting && observed.load(Ordering::SeqCst) {
                    not_found()
                } else {
                    Doc {
                        reversed: observed.load(Ordering::SeqCst),
                        ..Doc::of(number, kind, key)
                    }
                    .response()
                }
            })
            .mount(&mock)
            .await;
        let observed = acted.clone();
        let external = if deleting {
            format!("acct:{key}:proforma")
        } else {
            format!("acct:{key}:storno:{number}")
        };
        external_id_query(&external)
            .respond_with(move |_: &wiremock::Request| {
                if deleting && !observed.load(Ordering::SeqCst) {
                    Doc::of(number, kind, key).response()
                } else {
                    not_found()
                }
            })
            .mount(&mock)
            .await;
        let send = if deleting {
            delete_of(number)
        } else {
            storno_of_number(number)
        };
        send.respond_with(move |_: &wiremock::Request| {
            // The delete completed, or another writer reversed the original just
            // before our idempotent storno repeat. Neither answer reaches the worker.
            acted.store(true, Ordering::SeqCst);
            ResponseTemplate::new(500)
        })
        .expect(1)
        .mount(&mock)
        .await;
        let request = if deleting {
            json!({"expected_number":number})
        } else {
            json!({"invoice_number":number})
        };
        let owner = restate
            .invoke(
                &Call::object("Szamlazz.Order", key, handler).send(),
                Some(&request),
                Some(key),
            )
            .await;
        restate
            .admin()
            .await_status(owner.invocation_id(), &["paused"])
            .await;
        let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
        let marker = restate.invoke(&observe, None, None).await.body["marker"].clone();
        restate.admin().kill(owner.invocation_id()).await;
        let recover = Call::object("Szamlazz.Order", key, "recover");
        let evidence = if deleting {
            let correct = json!({"type":"completed", "audit_reference":"SUPPORT-301",
                "completion":{"type":"deleted", "number":number}, "completed_and_cannot_execute_later":true});
            for wrong in [
                json!({"type":"completed", "audit_reference":"", "completion":{"type":"deleted", "number":number}, "completed_and_cannot_execute_later":true}),
                json!({"type":"completed", "audit_reference":"INC", "completion":{"type":"deleted", "number":"D-OTHER"}, "completed_and_cannot_execute_later":true}),
                json!({"type":"completed", "audit_reference":"INC", "completion":{"type":"issued", "number":number}, "completed_and_cannot_execute_later":true}),
            ] {
                assert_eq!(
                    restate
                        .invoke(
                            &recover,
                            Some(&json!({"marker":marker, "evidence":wrong})),
                            None
                        )
                        .await
                        .status,
                    400
                );
                assert_eq!(
                    restate.invoke(&observe, None, None).await.body["state"],
                    "unresolved"
                );
            }
            // Absence remains insufficient even after the actual deletion.
            assert_eq!(restate.invoke(&recover, Some(&json!({"marker":marker, "evidence":{"type":"document", "number":number}})), None).await.status, 500);
            correct
        } else {
            number_query(VENDOR_NUMBER_XML)
                .respond_with(
                    Doc {
                        referenced_invoice: Some(number),
                        ..Doc::of(VENDOR_NUMBER_XML, "SS", key)
                    }
                    .response(),
                )
                .mount(&mock)
                .await;
            number_query("SS-WRONG")
                .respond_with(
                    Doc {
                        referenced_invoice: Some("OTHER"),
                        ..Doc::of("SS-WRONG", "SS", key)
                    }
                    .response(),
                )
                .mount(&mock)
                .await;
            assert_eq!(restate.invoke(&recover, Some(&json!({"marker":marker, "evidence":{"type":"document", "number":"SS-WRONG"}})), None).await.status, 500);
            json!({"type":"document", "number":VENDOR_NUMBER})
        };
        let request = json!({"marker":marker, "evidence":evidence});
        let settled = restate.invoke(&recover, Some(&request), None).await;
        assert_eq!(settled.status, 200, "{}", settled.body);
        assert_eq!(settled.body["operator"], "test-operator");
        assert_eq!(settled.body["evidence"], evidence);
        assert_eq!(
            restate.invoke(&observe, None, None).await.body["state"],
            "absent"
        );
        assert_eq!(
            restate.invoke(&recover, Some(&request), None).await.status,
            400,
            "stale evidence cannot clear again"
        );
        let journal = restate.admin().journal(settled.invocation_id()).await;
        let receipt = restate_e2e_harness::run_result(&journal, "record-recovery")
            .expect("recorded evidence");
        assert!(receipt.raw_contains("test-operator"));
        assert!(receipt.raw_contains(if deleting {
            "SUPPORT-301"
        } else {
            VENDOR_NUMBER
        }));
    }
    mock.verify().await;
    restate.finish().await;
}
