//! `Szamlazz.Order.create_proforma`: the auto-link into the invoice and the
//! proforma reported `consumed` by `get`; a proforma after the order's own
//! invoice (`order_invoiced`) against a foreign one.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{Doc, create, created, not_found, order_query};
use crate::harness::{Harness, document};

/// (vii) a proforma, then an invoice with the default `proforma: auto` ⇒ the
/// create carries `dijbekeroSzamlaszam`; `get` then reports the proforma
/// `consumed` by the invoice.
pub(crate) async fn proforma_auto_link_and_consumed(h: &Harness) {
    h.reset().await;
    // The proforma create checks that the order is not invoiced yet.
    h.absent("E2E-7", &["invoice", "prepayment", "final", "proforma"])
        .await;
    order_query("E2E-7")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .and(body_string_contains("<dijbekero>true</dijbekero>"))
        .respond_with(created("D-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let proforma = h
        .ok(
            "E2E-7",
            "create_proforma",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-7-p1",
        )
        .await;
    assert_eq!(proforma["outcome"], "issued", "{proforma}");
    assert_eq!(proforma["kind"], "proforma");
    assert_eq!(proforma["invoice_number"], "D-7");
    assert_eq!(proforma["external_id"], "acct:E2E-7:proforma");

    h.reset().await;
    h.absent("E2E-7", &["prepayment", "final", "invoice"]).await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-7:proforma"),
        ..Doc::new("D-7", "D", "E2E-7")
    })
    .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-7</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-7", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let invoice = h
        .ok(
            "E2E-7",
            "create_invoice",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-7-k1",
        )
        .await;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-7");
    assert_eq!(invoice["warnings"], json!([]));

    // After the conversion the proforma is gone from the query surface and
    // the invoice carries `hivdijbekszam`.
    h.reset().await;
    h.absent("E2E-7", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-7:invoice"),
        referenced_proforma: Some("D-7"),
        ..Doc::new("SZ-7", "SZ", "E2E-7")
    })
    .await;
    let status = h.get("E2E-7").await;
    assert_eq!(status["proforma"]["state"], "consumed", "{status}");
    assert_eq!(status["proforma"]["by"], "SZ-7");
    assert_eq!(status["proforma"]["number"], "D-7");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["invoice"]["number"], "SZ-7");
    assert_eq!(status["invoice"]["referenced_proforma"], "D-7");
    eprintln!("(vii) proforma → invoice (auto link) → get shows consumed: pass");
}

/// (x-a) `create_proforma` on an order whose invoice or prepayment invoice is
/// live: our own document (under `…:invoice` / `…:prepayment`) is
/// `conflict{order_invoiced, existing_number}` (a proforma after the invoice
/// makes no sense, but the invoice is ours, not another channel's), while a
/// live invoice under the order number that is under none of our ids stays
/// `conflict{foreign}`. Nothing is created either way.
pub(crate) async fn proforma_after_the_orders_invoice_is_order_invoiced_not_foreign(h: &Harness) {
    let body = json!({ "document": document(dec!(1000)) });

    // Our own live invoice, then our own live prepayment invoice.
    for (ours, other, number, tipus) in [
        ("invoice", "prepayment", "SZ-33", "SZ"),
        ("prepayment", "invoice", "ES-33", "ES"),
    ] {
        h.reset().await;
        h.absent("E2E-33", &[other, "final", "proforma"]).await;
        h.holds(&Doc {
            external_id: Some(&format!("acct:E2E-33:{ours}")),
            ..Doc::new(number, tipus, "E2E-33")
        })
        .await;
        create()
            .respond_with(created("D-33", "1000", "1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let conflict = h
            .ok(
                "E2E-33",
                "create_proforma",
                &body,
                &format!("e2e-33-p-{ours}"),
            )
            .await;
        assert_eq!(conflict["outcome"], "conflict", "{ours}: {conflict}");
        assert_eq!(
            conflict["conflict_reason"], "order_invoiced",
            "{ours}: {conflict}"
        );
        assert_eq!(conflict["existing_number"], number, "{ours}");
        assert_eq!(conflict["kind"], "proforma");
        assert_eq!(conflict["external_id"], "acct:E2E-33:proforma");
    }

    // A live invoice under the order number that is under none of our ids.
    h.reset().await;
    h.absent("E2E-33", &["invoice", "prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc::new("SZ-FOREIGN", "SZ", "E2E-33")).await;
    create()
        .respond_with(created("D-33", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok("E2E-33", "create_proforma", &body, "e2e-33-p-foreign")
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "foreign", "{conflict}");
    assert_eq!(conflict["existing_number"], "SZ-FOREIGN");
    eprintln!(
        "(x-a) create_proforma after our invoice → conflict{{order_invoiced}}; after a foreign one → conflict{{foreign}}: pass"
    );
}
