//! `Szamlazz.Order.delete_proforma` under Restate: its one path, the order's
//! live proforma found under its external id (`lookup-proforma`) and
//! deleted (`delete-proforma-{number}`), then a second call finding nothing.
//! The guard (a paid proforma, `force`, a collision) and the answers (335, a
//! refusal, a lost reply) are unit tests of `service::storno` and
//! `tests/gateway/`.

use serde_json::json;

use crate::harness::Harness;
use crate::harness::szamlazz::{Doc, delete_of, external_id_query, not_found, proforma_deleted};

/// The order's live proforma is found under its external id and deleted after
/// one send; once gone, a second call finds nothing and answers
/// `deleted{reason: absent}` from the lookup alone.
pub(crate) async fn proforma_is_deleted_by_the_orders_handler(h: &Harness) {
    // Live for the first call's lookup, gone after the delete.
    external_id_query("acct:E2E-D1:proforma")
        .respond_with(Doc::of("D-D1", "D", "E2E-D1").response())
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
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-proforma",
            "delete-proforma-D-D1"
        ]
    );

    let again = h
        .call("E2E-D1", "delete_proforma", &json!({}), "e2e-d1-k2")
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.body["deleted"], true, "{}", again.body);
    assert_eq!(again.body["reason"], "absent", "{}", again.body);
    assert_eq!(
        h.admin().runs(again.invocation_id()).await,
        ["namespace", "account", "lookup-proforma"],
        "the lookup answered; no delete step"
    );
    assert_eq!(h.delete_bodies_of("D-D1").await.len(), 1, "one send in all");
}
