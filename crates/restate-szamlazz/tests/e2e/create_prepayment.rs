//! `Szamlazz.Order.create_prepayment` under Restate: its two paths, the
//! proforma linked under the default `auto` (`proforma-link`) and the one
//! named by number (`verify-proforma-{number}`), each ending in an issued
//! prepayment invoice with the reference beside the `elolegszamla` flag on
//! the wire (#69). The refusals (`none` beside a live proforma, the other
//! chain's live document, `options.proforma` on a kind that takes none) are
//! unit tests of `service::create`.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{Doc, create_for, created};
use crate::harness::{Harness, document};

/// Under `auto` the order's live proforma is found by the `proforma-link` read
/// and linked explicitly; under `{number}` the named proforma is verified by
/// number (this order's) in place of the link read and linked. Both creates
/// carry `dijbekeroSzamlaszam` and `elolegszamla`; one order each.
pub(crate) async fn prepayment_converts_the_proforma_under_auto_and_by_number(h: &Harness) {
    // `auto`: the proforma under its external id.
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-10:proforma"),
        ..Doc::of("D-10", "D", "E2E-10")
    })
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
    h.absent("E2E-10P", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&Doc::of("D-10P", "D", "E2E-10P")).await;
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
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
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
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "verify-proforma-D-10P",
            "lookup-prepayment",
            "create-prepayment",
        ]
    );
    assert_eq!(h.create_bodies_of("E2E-10P").await.len(), 1);
}
