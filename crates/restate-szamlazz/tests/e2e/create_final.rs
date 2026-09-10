//! `Szamlazz.Order.create_final` under Restate: the target lookup, the
//! prepayment invoice settled by `lookup-prepayment`, then the full lookup
//! and the create naming it on the wire. A fresh retry still finds the final
//! invoice after that prepayment is reversed. The refusals (nothing under
//! `…:prepayment`, a reversed one, a collision) and the exclusivity rows a live final invoice
//! closes the order with (#62) are unit tests of `service::create`.

use rust_decimal::dec;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, create_for, created, external_id_query, holds_after_misses, order_query,
};
use crate::harness::{Harness, create_body};

/// A live prepayment invoice under `…:prepayment` is recorded by
/// `lookup-prepayment` and named on the wire (`elolegSzamlaszam` beside
/// the `vegszamla` flag, under the `…:final` external id), and is never
/// foreign to the lookup step's hint although it is the newest live
/// invoice-kind document under the order.
pub(crate) async fn create_final_names_its_live_prepayment_invoice(h: &Harness) {
    external_id_query("acct:E2E-43:prepayment")
        .respond_with(Doc::of("ES-43", "ES", "E2E-43").response())
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-43:prepayment")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("ES-43", "ES", "E2E-43")
            }
            .response(),
        )
        .expect(0)
        .mount(&h.mock)
        .await;
    order_query("E2E-43")
        .respond_with(Doc::of("ES-43", "ES", "E2E-43").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    holds_after_misses(
        &h.mock,
        3,
        &Doc {
            external_id: Some("acct:E2E-43:final"),
            ..Doc::of("VS-43", "VS", "E2E-43")
        },
    )
    .await;
    create_for("E2E-43")
        .and(body_string_contains("<vegszamla>true</vegszamla>"))
        .and(body_string_contains(
            "<elolegSzamlaszam>ES-43</elolegSzamlaszam>",
        ))
        .and(body_string_contains(
            "<szamlaKulsoAzon>acct:E2E-43:final</szamlaKulsoAzon>",
        ))
        .respond_with(created("VS-43", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-43",
            "create_final",
            &create_body(dec!(1000)),
            "e2e-43-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["kind"], "final");
    assert_eq!(reply.body["invoice_number"], "VS-43");
    assert_eq!(reply.body["external_id"], "acct:E2E-43:final");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-final",
            "lookup-prepayment",
            "lookup-final",
            "create-final",
        ]
    );
    assert_eq!(
        h.create_bodies_of("E2E-43").await.len(),
        1,
        "exactly one create"
    );

    // Reversing the prepayment since issuance cannot hide the live final
    // invoice from a fresh retry, nor trigger another prerequisite read.
    let before = h.requests_of_order("E2E-43").await.len();
    let again = h
        .call(
            "E2E-43",
            "create_final",
            &create_body(dec!(1000)),
            "e2e-43-k2",
        )
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_ne!(again.invocation_id(), reply.invocation_id());
    assert_eq!(again.body["outcome"], "already_issued", "{}", again.body);
    assert_eq!(again.body["invoice_number"], "VS-43");
    assert_eq!(
        h.admin().runs(again.invocation_id()).await,
        ["namespace", "account", "lookup-final"]
    );
    assert_eq!(h.requests_of_order("E2E-43").await.len(), before + 1);
    assert_eq!(h.create_bodies_of("E2E-43").await.len(), 1);
}
