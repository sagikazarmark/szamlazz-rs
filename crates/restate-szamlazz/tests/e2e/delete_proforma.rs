//! `Szamlazz.Order.delete_proforma`: the order's live proforma deleted and
//! then `absent`, the paid-proforma guard and `force`, the collision, 335 and
//! a lost reply.

use serde_json::json;
use wiremock::ResponseTemplate;

use crate::harness::Harness;
use crate::harness::szamlazz::{
    Doc, delete_of, external_id_query, not_found, proforma_deleted, proforma_gone,
};

/// (vii-d) `delete_proforma`: the order's live proforma is found under its
/// external id (`proforma-for-delete`) and deleted (`delete-proforma-{number}`,
/// one send); once gone, a second call finds nothing and answers
/// `deleted{reason: absent}` without sending.
pub(crate) async fn proforma_is_deleted_by_the_orders_handler(h: &Harness) {
    h.reset().await;
    // Live for the first call's lookup, gone after the delete.
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(Doc::new("D-D1", "D", "E2E-D1").response())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    delete_of("D-D1")
        .respond_with(proforma_deleted())
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call("E2E-D1", "delete_proforma", &json!({}), "e2e-d1-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], true, "{}", reply.body);
    assert!(reply.body["reason"].is_null(), "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-D1"
        ]
    );

    let again = h
        .ok("E2E-D1", "delete_proforma", &json!({}), "e2e-d1-k2")
        .await;
    assert_eq!(again["deleted"], true, "{again}");
    assert_eq!(again["reason"], "absent", "{again}");
    eprintln!(
        "(vii-d) delete_proforma → deleted after one send; again → absent, nothing sent: pass"
    );
}

/// (vii-d') `delete_proforma`'s other answers. szamlazz.hu has no guard
/// against deleting a proforma with registered credit entries, so the handler
/// has one: a paid proforma without `force` is `{deleted: false, reason:
/// proforma_paid}` after the lookup alone, and with `force` it is deleted (one
/// send). A document under `…:proforma` that is not this order's proforma is
/// `{deleted: false, reason: external_id_collision}`, never touched, since the
/// newest holder may hide a proforma of ours behind it. szamlazz.hu's 335 (no
/// such proforma: deleted since the lookup) is `{deleted: true}` like a fresh
/// deletion. A lost reply is the `outcome_unknown` fault about the proforma:
/// the delete has no retry of its own (one send, one step journaled), and the
/// next call's lookup tells.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the paid guard with and without force, the collision, 335 and the lost reply"
)]
pub(crate) async fn delete_proforma_guards_paid_proformas_and_settles_every_answer(h: &Harness) {
    let paid = Doc {
        external_id: Some("acct:E2E-46:proforma"),
        payments: &["1270"],
        ..Doc::new("D-46", "D", "E2E-46")
    };

    // Paid, without `force`: refused after the lookup, nothing sent.
    h.reset().await;
    h.holds(&paid).await;
    delete_of("D-46")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-46", "delete_proforma", &json!({}), "e2e-46-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], false, "{}", reply.body);
    assert_eq!(reply.body["reason"], "proforma_paid", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "proforma-for-delete"],
        "the lookup is what refused"
    );
    assert_eq!(h.requests_seen().await, 1, "the lookup, nothing else");

    // Paid, with `force`: deleted.
    h.reset().await;
    h.holds(&paid).await;
    delete_of("D-46")
        .respond_with(proforma_deleted())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-46",
            "delete_proforma",
            &json!({ "force": true }),
            "e2e-46-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], true, "{}", reply.body);
    assert!(reply.body["reason"].is_null(), "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-46"
        ]
    );
    assert_eq!(h.delete_bodies().await.len(), 1, "exactly one delete");

    // Another order's proforma under our id: a collision, never touched.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-46:proforma"),
        ..Doc::new("D-OTHER", "D", "OTHER-46")
    })
    .await;
    delete_of("D-OTHER")
        .respond_with(proforma_deleted())
        .expect(0)
        .mount(&h.mock)
        .await;
    let collision = h
        .ok(
            "E2E-46",
            "delete_proforma",
            &json!({ "force": true }),
            "e2e-46-k3",
        )
        .await;
    assert_eq!(collision["deleted"], false, "{collision}");
    assert_eq!(collision["reason"], "external_id_collision", "{collision}");
    assert_eq!(h.requests_seen().await, 1, "nothing sent");

    // 335 (gone since the lookup): deleted all the same.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-47:proforma"),
        ..Doc::new("D-47", "D", "E2E-47")
    })
    .await;
    delete_of("D-47")
        .respond_with(proforma_gone())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-47", "delete_proforma", &json!({}), "e2e-47-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["deleted"], true, "{}", reply.body);
    assert!(reply.body["reason"].is_null(), "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-47"
        ]
    );
    assert_eq!(h.delete_bodies().await.len(), 1, "one send");

    // A lost reply: `outcome_unknown` about the proforma, one send.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-48:proforma"),
        ..Doc::new("D-48", "D", "E2E-48")
    })
    .await;
    delete_of("D-48")
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-48", "delete_proforma", &json!({}), "e2e-48-k1")
        .await;
    assert_eq!(reply.status, 500, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "outcome_unknown", "{fault:?}");
    assert!(
        fault.message.contains("proforma deletion outcome unknown"),
        "{fault:?}"
    );
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );
    assert_eq!(fault.order.as_deref(), Some("E2E-48"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("proforma"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-48:proforma"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-D-48"
        ]
    );
    assert_eq!(
        h.delete_bodies().await.len(),
        1,
        "the delete has no retry of its own"
    );
    eprintln!(
        "(vii-d') delete_proforma: paid → not deleted{{proforma_paid}}, force → deleted; collision → not deleted; 335 → deleted; lost reply → outcome_unknown after one send: pass"
    );
}
