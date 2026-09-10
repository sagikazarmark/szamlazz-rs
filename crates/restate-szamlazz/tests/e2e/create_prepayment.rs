//! `Szamlazz.Order.create_prepayment` under Restate: its two full paths begin
//! with the target lookup, then exclusivity and the proforma link under
//! `auto` (`lookup-proforma`) or by number (`verify-proforma-{number}`). The
//! full lookup and create issue a prepayment invoice with the reference
//! beside `elolegszamla` on the wire (#69). A fresh retry finds the target
//! before verifying the consumed named proforma. The refusals (`none` beside a live proforma, the other
//! chain's live document, `options.proforma` on a kind that takes none) are
//! unit tests of `service::create`.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, create_for, created, holds, holds_after_misses, not_found, number_query, order_query,
};
use crate::harness::{Harness, document};

/// Under `auto` the order's live proforma is found by the `lookup-proforma` read
/// and linked explicitly; under `{number}` the named proforma is verified by
/// number (this order's) in place of the link read and linked. Both creates
/// carry `dijbekeroSzamlaszam` and `elolegszamla`; one order each.
#[allow(
    clippy::too_many_lines,
    reason = "two link paths and a fresh retry after consumption"
)]
pub(crate) async fn prepayment_converts_the_proforma_under_auto_and_by_number(h: &Harness) {
    // `auto`: the proforma under its external id.
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-10:proforma"),
            ..Doc::of("D-10", "D", "E2E-10")
        },
    )
    .await;
    create_for("E2E-10")
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    // `{number}`: the proforma by number and order, under no external id of
    // ours (issued by another channel, linked into this order's chain).
    h.absent("E2E-10P", &["invoice", "final"]).await;
    holds_after_misses(
        &h.mock,
        3,
        &Doc {
            external_id: Some("acct:E2E-10P:prepayment"),
            referenced_proforma: Some("D-10P"),
            ..Doc::of("ES-10P", "ES", "E2E-10P")
        },
    )
    .await;
    number_query("D-10P")
        .respond_with(Doc::of("D-10P", "D", "E2E-10P").response())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    number_query("D-10P")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.mock)
        .await;
    order_query("E2E-10P")
        .respond_with(Doc::of("D-10P", "D", "E2E-10P").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    create_for("E2E-10P")
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10P</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10P", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-10-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let issued = &reply.body;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    assert_eq!(issued["kind"], "prepayment");
    assert_eq!(issued["invoice_number"], "ES-10");
    assert_eq!(issued["external_id"], "acct:E2E-10:prepayment");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-invoice",
            "lookup-final",
            "lookup-proforma",
            "lookup-prepayment",
            "create-prepayment",
        ]
    );
    assert_eq!(h.create_bodies_of("E2E-10").await.len(), 1);

    let reply = h
        .call(
            "E2E-10P",
            "create_prepayment",
            &json!({
                "document": document(dec!(1000)),
                "options": { "proforma": { "number": "D-10P" } },
            }),
            "e2e-10p-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "ES-10P");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-invoice",
            "lookup-final",
            "verify-proforma-D-10P",
            "lookup-prepayment",
            "create-prepayment",
        ]
    );
    assert_eq!(h.create_bodies_of("E2E-10P").await.len(), 1);

    // Fresh retry after conversion: the named proforma answers code 7 now,
    // but the existing prepayment invoice settles the request first.
    let before = h.requests_of_order("E2E-10P").await.len();
    let again = h
        .call(
            "E2E-10P",
            "create_prepayment",
            &json!({
                "document": document(dec!(1000)),
                "options": { "proforma": { "number": "D-10P" } },
            }),
            "e2e-10p-k2",
        )
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_ne!(again.invocation_id(), reply.invocation_id());
    assert_eq!(again.body["outcome"], "already_issued", "{}", again.body);
    assert_eq!(again.body["invoice_number"], "ES-10P");
    assert_eq!(
        h.admin().runs(again.invocation_id()).await,
        ["namespace", "account", "lookup-prepayment"]
    );
    assert_eq!(h.requests_of_order("E2E-10P").await.len(), before + 1);
    assert_eq!(h.create_bodies_of("E2E-10P").await.len(), 1);
}
