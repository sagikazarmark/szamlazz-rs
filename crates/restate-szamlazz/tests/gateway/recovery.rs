//! External orchestrator evidence boundary: no Restate marker or private API.

use super::common::{
    Doc, api_error, create, external_id_query, not_found, number_query, order_query,
};
use super::harness::*;
use restate_szamlazz::contract::IssuedKind;
use restate_szamlazz::gateway::{
    CreateOutcome, CreatePermission, CreateStepRequest, DocumentRefs, ReconciliationOutcome,
    ReconciliationRequest, recovery::WriteOperation,
};
use wiremock::ResponseTemplate;

#[tokio::test]
async fn retained_corrective_intent_reconciles_without_another_send() {
    let h = Harness::start().await;
    let id = restate_szamlazz::ExternalId::new("acct:ORD-1:corrective:c1");
    let order = order();
    let create_request = h
        .gateway
        .build_create(
            IssuedKind::Corrective,
            &document(),
            &order,
            &id,
            DocumentRefs {
                corrected: Some("SZ-BASE"),
                ..DocumentRefs::default()
            },
        )
        .expect("build corrective");
    let request = CreateStepRequest {
        external_id: &id,
        order: &order,
        kind: IssuedKind::Corrective,
        create: &create_request,
        reversed: None,
    };
    // The expert caller retains intent before granting its sole send permission.
    let retained = serde_json::to_string(&request.operation()).expect("retain intent");
    external_id_query(id.as_str())
        .respond_with(not_found())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.server)
        .await;
    external_id_query(id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-WRONG"),
                ..Doc::new("HS-WRONG", "HS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.server)
        .await;
    assert!(
        h.gateway
            .create_once(request, CreatePermission::grant())
            .await
            .is_err()
    );
    h.server.verify().await;

    // Simulated next execution: restore retained intent, open a fresh Gateway,
    // and run reads only, through mismatching, absent and finally matching evidence.
    let operation: WriteOperation = serde_json::from_str(&retained).expect("restore intent");
    let gateway = gateway(&h.server);
    for (response, positive) in [
        (not_found(), false),
        (
            Doc {
                referenced_invoice: Some("SZ-WRONG"),
                ..Doc::new("HS-WRONG", "HS")
            }
            .response(),
            false,
        ),
        (
            Doc {
                referenced_invoice: Some("SZ-BASE"),
                ..Doc::new("HS-NEW", "HS")
            }
            .response(),
            true,
        ),
    ] {
        h.server.reset().await;
        external_id_query(id.as_str())
            .respond_with(response)
            .expect(1)
            .mount(&h.server)
            .await;
        let evidence = gateway
            .reconcile(ReconciliationRequest {
                external_id: id.as_str(),
                order: &order,
                operation: &operation,
                candidate: None,
            })
            .await
            .expect("answered recovery query");
        assert_eq!(
            matches!(evidence, ReconciliationOutcome::Created(_)),
            positive,
            "{evidence:?}"
        );
        assert_eq!(h.bodies().await.len(), 1, "reconciliation is a query only");
        h.server.verify().await;
    }
}

#[tokio::test]
async fn create_reconciliation_checks_old_target_ownership_base_and_candidate() {
    let operation = WriteOperation::Create {
        kind: IssuedKind::Corrective,
        expected_number: Some("HS-OLD".into()),
        corrected_number: Some("SZ-BASE".into()),
    };
    for (number, kind, doc_order, reversed, base, candidate, positive) in [
        ("HS-OLD", "HS", "ORD-1", false, Some("SZ-BASE"), None, false),
        ("HS-OLD", "HS", "ORD-1", true, Some("SZ-BASE"), None, false),
        ("HS-NEW", "HS", "OTHER", false, Some("SZ-BASE"), None, false),
        ("HS-NEW", "SZ", "ORD-1", false, Some("SZ-BASE"), None, false),
        (
            "HS-NEW",
            "HS",
            "ORD-1",
            false,
            Some("SZ-OTHER"),
            None,
            false,
        ),
        ("HS-NEW", "HS", "ORD-1", false, None, None, false),
        (
            "HS-NEW",
            "HS",
            "ORD-1",
            false,
            Some("SZ-BASE"),
            Some("HS-OTHER"),
            false,
        ),
        (
            "HS-NEW",
            "HS",
            "ORD-1",
            false,
            Some("SZ-BASE"),
            Some("HS-NEW"),
            true,
        ),
        ("HS-NEW", "HS", "ORD-1", true, Some("SZ-BASE"), None, true),
    ] {
        let h = Harness::start().await;
        external_id_query("acct:ORD-1:corrective:c1")
            .respond_with(
                Doc {
                    order: Some(doc_order),
                    reversed,
                    referenced_invoice: base,
                    ..Doc::new(number, kind)
                }
                .response(),
            )
            .expect(1)
            .mount(&h.server)
            .await;
        let result = h
            .gateway
            .reconcile(ReconciliationRequest {
                external_id: "acct:ORD-1:corrective:c1",
                order: &order(),
                operation: &operation,
                candidate,
            })
            .await
            .expect("answered recovery query");
        assert_eq!(
            matches!(result, ReconciliationOutcome::Created(_)),
            positive,
            "{number}: {result:?}"
        );
        assert_eq!(h.bodies().await.len(), 1);
    }
}

#[tokio::test]
async fn missing_corrective_intent_cannot_establish_completion() {
    let h = Harness::start().await;
    external_id_query("acct:ORD-1:corrective:c1")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-BASE"),
                ..Doc::new("HS-NEW", "HS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;
    let result = h
        .gateway
        .reconcile(ReconciliationRequest {
            external_id: "acct:ORD-1:corrective:c1",
            order: &order(),
            candidate: None,
            operation: &WriteOperation::Create {
                kind: IssuedKind::Corrective,
                expected_number: None,
                corrected_number: None,
            },
        })
        .await
        .expect("answered recovery query");
    assert!(
        matches!(result, ReconciliationOutcome::Inconclusive { .. }),
        "{result:?}"
    );
}

#[tokio::test]
async fn storno_reconciliation_requires_paired_evidence() {
    for (original, positive) in [
        (Doc::reversed("SZ-1", "SZ").response(), true),
        (Doc::new("SZ-1", "SZ").response(), false),
        (
            Doc {
                order: Some("OTHER"),
                ..Doc::reversed("SZ-1", "SZ")
            }
            .response(),
            false,
        ),
        (Doc::reversed("SZ-1", "D").response(), false),
        (not_found(), false),
        (api_error("135", "private credentials"), false),
    ] {
        let h = Harness::start().await;
        // A vendor-reported number wider than a mutation target remains usable.
        let candidate = " SS:vendor-reported-number-with-more-than-forty-bytes ";
        number_query(candidate)
            .respond_with(
                Doc {
                    referenced_invoice: Some("SZ-1"),
                    ..Doc::new(candidate, "SS")
                }
                .response(),
            )
            .expect(1)
            .mount(&h.server)
            .await;
        number_query("SZ-1")
            .respond_with(original)
            .expect(1)
            .mount(&h.server)
            .await;
        let result = h
            .gateway
            .reconcile(ReconciliationRequest {
                external_id: storno_id().as_str(),
                order: &order(),
                operation: &WriteOperation::Storno {
                    number: "SZ-1".into(),
                },
                candidate: Some(candidate),
            })
            .await
            .expect("answered paired queries");
        assert_eq!(
            matches!(result, ReconciliationOutcome::Reversed { .. }),
            positive,
            "{result:?}"
        );
        assert_eq!(h.bodies().await.len(), 2);
        h.server.verify().await;
    }
}

#[tokio::test]
async fn storno_reconciliation_requires_exact_candidate_identity() {
    for wrong in [
        Doc {
            referenced_invoice: Some("SZ-OTHER"),
            ..Doc::new("SS-1", "SS")
        },
        Doc {
            referenced_invoice: Some("SZ-1"),
            order: Some("OTHER"),
            ..Doc::new("SS-1", "SS")
        },
        Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-OTHER", "SS")
        },
        Doc {
            referenced_invoice: Some("SZ-1"),
            ..Doc::new("SS-1", "SZ")
        },
    ] {
        let h = Harness::start().await;
        number_query("SS-1")
            .respond_with(wrong.response())
            .expect(1)
            .mount(&h.server)
            .await;
        let result = h
            .gateway
            .reconcile(ReconciliationRequest {
                external_id: storno_id().as_str(),
                order: &order(),
                candidate: Some("SS-1"),
                operation: &WriteOperation::Storno {
                    number: "SZ-1".into(),
                },
            })
            .await;
        assert!(
            matches!(
                result,
                Err(_) | Ok(ReconciliationOutcome::Inconclusive { .. })
            ),
            "{result:?}"
        );
        assert_eq!(
            h.bodies().await.len(),
            1,
            "no fallback substitutes an operator candidate"
        );
    }
}

#[tokio::test]
async fn storno_discovery_uses_the_order_hint_only_after_external_id_absence() {
    let h = Harness::start().await;
    external_id_query(storno_id().as_str())
        .respond_with(not_found())
        .expect(1)
        .mount(&h.server)
        .await;
    order_query("ORD-1")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;
    number_query("SZ-1")
        .respond_with(Doc::reversed("SZ-1", "SZ").response())
        .expect(1)
        .mount(&h.server)
        .await;
    let result = h
        .gateway
        .reconcile(ReconciliationRequest {
            external_id: storno_id().as_str(),
            order: &order(),
            candidate: None,
            operation: &WriteOperation::Storno {
                number: "SZ-1".into(),
            },
        })
        .await
        .expect("answered discovery queries");
    assert!(
        matches!(result, ReconciliationOutcome::Reversed { storno_number } if storno_number == "SS-1")
    );
    assert_eq!(h.bodies().await.len(), 3);
    h.server.verify().await;
}

#[tokio::test]
async fn reconciliation_preserves_read_failures_and_never_proves_deletion() {
    for response in [
        not_found(),
        api_error("135", "private credentials"),
        api_error("57", "XML"),
        ResponseTemplate::new(500),
    ] {
        let h = Harness::start().await;
        external_id_query(external_id().as_str())
            .respond_with(response)
            .expect(1)
            .mount(&h.server)
            .await;
        let result = h
            .gateway
            .reconcile(ReconciliationRequest {
                external_id: external_id().as_str(),
                order: &order(),
                candidate: None,
                operation: &WriteOperation::Create {
                    kind: IssuedKind::Invoice,
                    expected_number: None,
                    corrected_number: None,
                },
            })
            .await;
        assert!(
            matches!(
                result,
                Err(_)
                    | Ok(ReconciliationOutcome::Inconclusive { .. }
                        | ReconciliationOutcome::CredentialsRejected(_)
                        | ReconciliationOutcome::Api(_))
            ),
            "{result:?}"
        );
        assert_eq!(h.bodies().await.len(), 1);
    }
    let h = Harness::start().await;
    let result = h
        .gateway
        .reconcile(ReconciliationRequest {
            external_id: "acct:ORD-1:proforma",
            order: &order(),
            candidate: None,
            operation: &WriteOperation::Delete {
                number: "D-1".into(),
            },
        })
        .await
        .expect("deletion reconciliation needs no query");
    assert!(matches!(result, ReconciliationOutcome::Inconclusive { .. }));
    assert!(h.bodies().await.is_empty());
}

#[tokio::test]
async fn wrong_base_before_send_is_collision_not_uncertainty() {
    let h = Harness::start().await;
    let id = restate_szamlazz::ExternalId::new("acct:ORD-1:corrective:c1");
    external_id_query(id.as_str())
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-WRONG"),
                ..Doc::new("HS-WRONG", "HS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.server)
        .await;
    let result = h.create_kind(IssuedKind::Corrective, &id, None).await;
    assert!(
        matches!(result, Ok(CreateOutcome::Collision(_))),
        "{result:?}"
    );
    assert_eq!(h.bodies().await.len(), 1);
}
