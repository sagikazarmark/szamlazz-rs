//! `Szamlazz.Order.correct_invoice` under Restate: its one path, the base
//! verified by number, the lookup of the correction id (no order-number hint:
//! correctives are exempt) and the create naming the base on the wire; the
//! same `correction_id` again finding the corrective. The verify's refusals
//! (7, a reversed base, another order's) are unit tests of `service::create`;
//! the 71/152 on a corrective is `tests/gateway/`'s.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, create_for, created, holds_after_misses, number_query, order_query,
};
use crate::harness::{Harness, document};

/// The base is verified by number (it must carry this order's number); then
/// the corrective is issued under `{namespace}:{order}:corrective:{correction_id}`
/// with the base named on the wire (`helyesbitettSzamlaszam` beside the
/// `helyesbitoszamla` flag), through `verify-base-{number}`,
/// `lookup-corrective` and `create-corrective`, the order-number hint never
/// taken (a live foreign invoice under the order is never met); the same
/// `correction_id` again (a new key) finds it: `already_issued`.
pub(crate) async fn corrective_is_issued_under_its_correction_id(h: &Harness) {
    // The base, by number only: `holds` would mount the order selector too,
    // ahead of the `expect(0)` below, and wiremock answers with the first
    // mounted match, so the zero expectation could never fire.
    number_query("SZ-C1")
        .respond_with(Doc::of("SZ-C1", "SZ", "E2E-C1").response())
        .mount(&h.mock)
        .await;
    // A live invoice of another channel under the order: never queried.
    order_query("E2E-C1")
        .respond_with(Doc::of("SZ-FOREIGN-C1", "SZ", "E2E-C1").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    // The corrective's id: absent for the lookup step and the create step's
    // leading query, then the issued corrective.
    holds_after_misses(
        &h.mock,
        2,
        &Doc {
            external_id: Some("acct:E2E-C1:corrective:fix-1"),
            referenced_invoice: Some("SZ-C1"),
            ..Doc::of("HS-C1", "HS", "E2E-C1")
        },
    )
    .await;
    create_for("E2E-C1")
        .and(body_string_contains(
            "<szamlaKulsoAzon>acct:E2E-C1:corrective:fix-1</szamlaKulsoAzon>",
        ))
        .and(body_string_contains(
            "<helyesbitettSzamlaszam>SZ-C1</helyesbitettSzamlaszam>",
        ))
        .and(body_string_contains(
            "<helyesbitoszamla>true</helyesbitoszamla>",
        ))
        .respond_with(created("HS-C1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let body = json!({
        "invoice_number": "SZ-C1",
        "correction_id": "fix-1",
        "document": document(dec!(-1000)),
    });

    let reply = h
        .call("E2E-C1", "correct_invoice", &body, "e2e-c1-k1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "HS-C1");
    assert_eq!(reply.body["kind"], "corrective");
    assert_eq!(reply.body["external_id"], "acct:E2E-C1:corrective:fix-1");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-base-SZ-C1",
            "lookup-corrective",
            "create-corrective"
        ]
    );

    let again = h.ok("E2E-C1", "correct_invoice", &body, "e2e-c1-k2").await;
    assert_eq!(again["outcome"], "already_issued", "{again}");
    assert_eq!(again["invoice_number"], "HS-C1");
    assert_eq!(
        h.create_bodies_of("E2E-C1").await.len(),
        1,
        "one create for the correction id"
    );
}
