//! `Szamlazz.Order.create_proforma` under Restate, and what follows it: the
//! proforma issued through its path (the three exclusivity reads, the
//! lookup, the create step), the invoice that names it by number (the
//! `verify-proforma-{number}` path of `create_invoice`), and `get` reporting
//! the proforma `consumed` by the invoice (the `get` path). The link's
//! decisions (`auto`, `none`, `{number}` on a proforma of another order or
//! none at all, on a non-proforma) are unit tests of `service::create`; the
//! reference on the create body is `tests/gateway/`'s.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, create_for, created, holds_after_misses, number_query, order_query,
};
use crate::harness::{Harness, document};

/// A proforma, then the invoice naming it (`options.proforma: {number}`),
/// then `get`: the proforma is `issued` with `dijbekero` on the wire; the
/// invoice verifies the named proforma by number (this order's) and carries
/// `dijbekeroSzamlaszam`; after the conversion the proforma is gone from the
/// query surface and the invoice carries `hivdijbekszam`, which `get` derives
/// into `{state: consumed, by}`. Three handler paths walked in full, on one
/// order.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the proforma, the invoice naming it, the status"
)]
pub(crate) async fn proforma_then_the_invoice_naming_it_then_get_consumed(h: &Harness) {
    // Nothing under the proforma, prepayment and final ids throughout: the
    // proforma create's exclusivity reads and lookup miss, the invoice's
    // exclusivity reads miss, and `get` finds the proforma consumed.
    h.absent("E2E-7", &["proforma", "prepayment", "final"])
        .await;
    // The newest document under the order is the proforma once it exists;
    // a `D` is never foreign, so the hint reads the same before and after.
    order_query("E2E-7")
        .respond_with(Doc::of("D-7", "D", "E2E-7").response())
        .mount(&h.mock)
        .await;
    // The invoice id: absent for the proforma create's exclusivity read, the
    // invoice's lookup and its leading query; then the issued invoice,
    // carrying the proforma it consumed, for `get`.
    holds_after_misses(
        &h.mock,
        3,
        &Doc {
            external_id: Some("acct:E2E-7:invoice"),
            referenced_proforma: Some("D-7"),
            ..Doc::of("SZ-7", "SZ", "E2E-7")
        },
    )
    .await;
    // The invoice's verify of the named proforma.
    number_query("D-7")
        .respond_with(Doc::of("D-7", "D", "E2E-7").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    create_for("E2E-7")
        .and(body_string_contains("<dijbekero>true</dijbekero>"))
        .respond_with(created("D-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_for("E2E-7")
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-7</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-7",
            "create_proforma",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-7-p1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let proforma = &reply.body;
    assert_eq!(proforma["outcome"], "issued", "{proforma}");
    assert_eq!(proforma["kind"], "proforma");
    assert_eq!(proforma["invoice_number"], "D-7");
    assert_eq!(proforma["external_id"], "acct:E2E-7:proforma");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "create-proforma",
        ]
    );

    let reply = h
        .call(
            "E2E-7",
            "create_invoice",
            &json!({
                "document": document(dec!(1000)),
                "options": { "proforma": { "number": "D-7" } },
            }),
            "e2e-7-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let invoice = &reply.body;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-7");
    assert_eq!(invoice["warnings"], json!([]));
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-final",
            "verify-proforma-D-7",
            "lookup-invoice",
            "create-invoice",
        ],
        "the named proforma is verified in place of the link read"
    );
    assert_eq!(
        h.create_bodies_of("E2E-7").await.len(),
        2,
        "the proforma and the invoice"
    );

    let reply = h.get_reply("E2E-7").await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let status = &reply.body;
    assert_eq!(status["proforma"]["state"], "consumed", "{status}");
    assert_eq!(status["proforma"]["by"], "SZ-7");
    assert_eq!(status["proforma"]["number"], "D-7");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["invoice"]["number"], "SZ-7");
    assert_eq!(status["invoice"]["referenced_proforma"], "D-7");
    assert_eq!(status["invoice"]["gross"], "1270");
    assert_eq!(
        status["invoice"]["e_invoice"], false,
        "the fixture's default `eszamla` is `1`, paper"
    );
    assert_eq!(status["prepayment"], serde_json::Value::Null);
    assert_eq!(status["final"], serde_json::Value::Null);
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-proforma",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
        ]
    );
}
