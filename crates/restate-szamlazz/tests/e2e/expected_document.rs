//! Expected-document intent survives expiry of finite request deduplication.
//! The scripted vendor observations are not an atomic compare-and-set.

use crate::harness::szamlazz::{
    Doc, create_for, created, delete_of, external_id_query, not_found, number_query, order_query,
    proforma_deleted,
};
use crate::harness::{Harness, reissue_body};
use rust_decimal::dec;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

pub(crate) async fn purged_reissue_intent_cannot_replace_its_replacement(h: &Harness) {
    let order = "E2E-INTENT-R";
    let state = Arc::new(AtomicUsize::new(0));
    let observation = Arc::clone(&state);
    external_id_query("acct:E2E-INTENT-R:invoice")
        .respond_with(move |_: &wiremock::Request| {
            let state = observation.load(Ordering::SeqCst);
            Doc {
                reversed: state != 1,
                ..Doc::of(
                    if state == 0 {
                        "SZ-INTENT-A"
                    } else {
                        "SZ-INTENT-B"
                    },
                    "SZ",
                    order,
                )
            }
            .response()
        })
        .mount(&h.mock)
        .await;
    order_query(order)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.absent(order, &["prepayment", "final", "proforma"]).await;
    let landed = Arc::clone(&state);
    create_for(order)
        .respond_with(move |_: &wiremock::Request| {
            landed.store(1, Ordering::SeqCst);
            created("SZ-INTENT-B", "1000", "1270")
        })
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = reissue_body(dec!(1000), "SZ-INTENT-A");
    let first = h.call(order, "create_invoice", &body, "intent-r").await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(first.body["outcome"], "issued");
    h.assert_state_absent(None, order).await;
    let stored = h.call(order, "create_invoice", &body, "intent-r").await;
    assert_eq!(stored.invocation_id(), first.invocation_id());
    assert_eq!(stored.body, first.body);
    h.admin().purge(first.invocation_id()).await;
    assert!(h.admin().journal(first.invocation_id()).await.is_empty());
    let again = h.call(order, "create_invoice", &body, "intent-r").await;
    assert_eq!(
        again.body["conflict_reason"], "target_changed",
        "{}",
        again.body
    );
    assert_eq!(again.body["existing_number"], "SZ-INTENT-B");
    h.admin().purge(again.invocation_id()).await;
    state.store(2, Ordering::SeqCst); // B was reversed outside this invocation.
    let delayed = h.call(order, "create_invoice", &body, "intent-r").await;
    assert_eq!(
        delayed.body["conflict_reason"], "target_changed",
        "{}",
        delayed.body
    );
    assert_eq!(delayed.body["existing_number"], "SZ-INTENT-B");
    assert_eq!(h.create_bodies_of(order).await.len(), 1);
}

pub(crate) async fn purged_deletion_intent_cannot_delete_a_replacement(h: &Harness) {
    let order = "E2E-INTENT-D";
    let state = Arc::new(AtomicUsize::new(0));
    let observation = Arc::clone(&state);
    external_id_query("acct:E2E-INTENT-D:proforma")
        .respond_with(
            move |_: &wiremock::Request| match observation.load(Ordering::SeqCst) {
                0 => Doc::of("D-INTENT-A", "D", order).response(),
                1 => not_found(),
                _ => Doc::of("D-INTENT-B", "D", order).response(),
            },
        )
        .mount(&h.mock)
        .await;
    number_query("D-INTENT-A")
        .respond_with(Doc::of("D-INTENT-A", "D", order).response())
        .expect(1)
        .mount(&h.mock)
        .await;
    let landed = Arc::clone(&state);
    delete_of("D-INTENT-A")
        .respond_with(move |_: &wiremock::Request| {
            landed.store(1, Ordering::SeqCst);
            proforma_deleted()
        })
        .expect(1)
        .mount(&h.mock)
        .await;
    delete_of("D-INTENT-B")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    let body = json!({"expected_number": "D-INTENT-A", "force": true});
    let first = h.call(order, "delete_proforma", &body, "intent-d").await;
    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(first.body, json!({"deleted": true, "reason": null}));
    h.assert_state_absent(None, order).await;
    let stored = h.call(order, "delete_proforma", &body, "intent-d").await;
    assert_eq!(stored.invocation_id(), first.invocation_id());
    assert_eq!(stored.body, first.body);
    h.admin().purge(first.invocation_id()).await;
    let absent = h.call(order, "delete_proforma", &body, "intent-d").await;
    assert_eq!(absent.body, json!({"deleted": true, "reason": "absent"}));
    h.admin().purge(absent.invocation_id()).await;
    state.store(2, Ordering::SeqCst);
    let delayed = h.call(order, "delete_proforma", &body, "intent-d").await;
    assert_eq!(
        delayed.body,
        json!({"deleted": false, "reason": "target_changed"})
    );
    assert_eq!(h.delete_bodies_of("D-INTENT-A").await.len(), 1);
    assert!(h.delete_bodies_of("D-INTENT-B").await.is_empty());
    h.assert_state_absent(None, order).await;
}

/// Every ordinary handler checks intent before its distinct prerequisites.
pub(crate) async fn expected_target_outcomes_precede_prerequisites(h: &Harness) {
    for (kind, token) in [
        ("invoice", "SZ"),
        ("prepayment", "ES"),
        ("final", "VS"),
        ("proforma", "D"),
    ] {
        for (case, reason, existing) in [
            ("absent", "target_changed", None),
            ("live", "live", Some("EXPECTED")),
            ("changed", "target_changed", Some("REPLACEMENT")),
            ("collision", "external_id_collision", Some("OTHER")),
        ] {
            let order = format!("I-{kind}-{case}");
            let response = match case {
                "absent" => not_found(),
                "live" => Doc::of("EXPECTED", token, &order).response(),
                "changed" => Doc {
                    reversed: true,
                    ..Doc::of("REPLACEMENT", token, &order)
                }
                .response(),
                _ => Doc::of("OTHER", "SS", &order).response(),
            };
            external_id_query(&format!("acct:{order}:{kind}"))
                .respond_with(response)
                .expect(1)
                .mount(&h.mock)
                .await;
            create_for(&order)
                .respond_with(created("UNWANTED", "1000", "1270"))
                .expect(0)
                .mount(&h.mock)
                .await;
            let reply = h
                .call(
                    &order,
                    &format!("create_{kind}"),
                    &reissue_body(dec!(1000), "EXPECTED"),
                    &order,
                )
                .await;
            assert_eq!(reply.status, 200, "{}", reply.body);
            assert_eq!(reply.body["conflict_reason"], reason, "{}", reply.body);
            assert_eq!(reply.body["existing_number"], json!(existing));
            assert_eq!(
                h.admin().runs(reply.invocation_id()).await,
                [
                    "namespace".to_owned(),
                    "account".to_owned(),
                    format!("lookup-{kind}")
                ]
            );
            h.assert_state_absent(None, &order).await;
        }
    }
}

pub(crate) async fn reissue_rechecks_the_expected_holder_after_prerequisites(h: &Harness) {
    let order = "E2E-INTENT-LOOKUP";
    external_id_query("acct:E2E-INTENT-LOOKUP:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-LOOKUP-A", "SZ", order)
            }
            .response(),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-INTENT-LOOKUP:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-LOOKUP-B", "SZ", order)
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    h.absent(order, &["prepayment", "final", "proforma"]).await;
    order_query(order)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for(order)
        .respond_with(created("UNWANTED", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            order,
            "create_invoice",
            &reissue_body(dec!(1000), "SZ-LOOKUP-A"),
            "intent-lookup",
        )
        .await;
    assert_eq!(
        reply.body["conflict_reason"], "target_changed",
        "{}",
        reply.body
    );
    assert_eq!(reply.body["existing_number"], "SZ-LOOKUP-B");
    assert!(h.create_bodies_of(order).await.is_empty());
    h.assert_state_absent(None, order).await;
}

pub(crate) async fn purged_corrective_request_still_cannot_reissue(h: &Harness) {
    let order = "E2E-INTENT-C";
    external_id_query("acct:E2E-INTENT-C:corrective:fix")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("HS-INTENT", "HS", order)
            }
            .response(),
        )
        .expect(2)
        .mount(&h.mock)
        .await;
    create_for(order)
        .respond_with(created("UNWANTED", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let body = json!({"invoice_number": "SZ-BASE", "correction_id": "fix", "document": crate::harness::document(dec!(1000))});
    let first = h.call(order, "correct_invoice", &body, "intent-c").await;
    assert_eq!(first.body["outcome"], "reversed", "{}", first.body);
    h.admin().purge(first.invocation_id()).await;
    let stale = h.call(order, "correct_invoice", &body, "intent-c").await;
    assert_eq!(stale.body["outcome"], "reversed", "{}", stale.body);
    assert_eq!(stale.body["invoice_number"], "HS-INTENT");
    let mut unsupported = body;
    unsupported["reissue"] = json!({"expected_number": "HS-INTENT"});
    let refused = h
        .call(order, "correct_invoice", &unsupported, "intent-c-refused")
        .await;
    assert_eq!(refused.status, 400, "{}", refused.body);
    assert!(h.admin().runs(refused.invocation_id()).await.is_empty());
    assert!(h.create_bodies_of(order).await.is_empty());
    h.assert_state_absent(None, order).await;
}

pub(crate) async fn deletion_preserves_ownership_and_consumed_target_outcomes(h: &Harness) {
    for (case, number, token, reason) in [
        ("collision", "D-OLD", "SZ", "external_id_collision"),
        ("changed", "D-NEW", "D", "target_changed"),
    ] {
        let order = format!("E2E-INTENT-D-{case}");
        external_id_query(&format!("acct:{order}:proforma"))
            .respond_with(Doc::of(number, token, &order).response())
            .expect(1)
            .mount(&h.mock)
            .await;
        delete_of(number)
            .respond_with(proforma_deleted())
            .expect(0)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                &order,
                "delete_proforma",
                &json!({"expected_number": "D-OLD", "force": true}),
                &order,
            )
            .await;
        assert_eq!(reply.body, json!({"deleted": false, "reason": reason}));
        h.assert_state_absent(None, &order).await;
    }
    let order = "E2E-INTENT-CONSUMED";
    h.absent(order, &["proforma", "prepayment", "final"]).await;
    external_id_query("acct:E2E-INTENT-CONSUMED:invoice")
        .respond_with(
            Doc {
                referenced_proforma: Some("D-CONSUMED"),
                ..Doc::of("SZ-CONSUMER", "SZ", order)
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    let status = h.get_reply(order).await;
    assert_eq!(
        status.body["proforma"]["state"], "consumed",
        "{}",
        status.body
    );
    delete_of("D-CONSUMED")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            order,
            "delete_proforma",
            &json!({"expected_number": "D-CONSUMED"}),
            "intent-consumed",
        )
        .await;
    assert_eq!(reply.body, json!({"deleted": true, "reason": "absent"}));
    h.assert_state_absent(None, order).await;
}

pub(crate) async fn missing_target_after_a_lost_reissue_preserves_uncertainty(h: &Harness) {
    let order = "E2E-INTENT-LOST";
    external_id_query("acct:E2E-INTENT-LOST:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-LOST-A", "SZ", order)
            }
            .response(),
        )
        .up_to_n_times(3)
        .expect(3)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-INTENT-LOST:invoice")
        .respond_with(not_found())
        .expect(2..)
        .mount(&h.mock)
        .await;
    h.absent(order, &["prepayment", "final", "proforma"]).await;
    order_query(order)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for(order)
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    let call = restate_e2e_harness::Call::object("Szamlazz.Order", order, "create_invoice");
    let body = reissue_body(dec!(1000), "SZ-LOST-A");
    let submitted = h
        .invoke(&call.send(), Some(&body), Some("intent-lost"))
        .await;
    h.admin()
        .await_status(submitted.invocation_id(), &["paused"])
        .await;
    assert_eq!(h.create_bodies_of(order).await.len(), 1);
    h.admin().cancel(submitted.invocation_id()).await;
    let reply = h.invoke(&call, Some(&body), Some("intent-lost")).await;
    assert_eq!(reply.status, 500, "{}", reply.body);
    assert_eq!(
        reply.fault().code,
        restate_szamlazz::contract::TerminalCode::OutcomeUnknown
    );
    assert_eq!(reply.fault().is_cancelled(), Some(true));
    assert_eq!(h.create_bodies_of(order).await.len(), 1);
    h.expect_unresolved(None, order).await;
}
