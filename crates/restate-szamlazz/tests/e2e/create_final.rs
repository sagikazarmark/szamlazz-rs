//! `Szamlazz.Order.create_final` under Restate: its one path, the prepayment
//! invoice settled first (`lookup-prepayment`), then the lookup and the
//! create naming it on the wire. The refusals (nothing under `…:prepayment`,
//! a reversed one, a collision) and the exclusivity rows a live final invoice
//! closes the order with (#62) are unit tests of `service::create`.

use rust_decimal::dec;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{Doc, create_for, created, holds};
use crate::harness::{Harness, create_body};

/// A live prepayment invoice under `…:prepayment` is recorded by
/// `lookup-prepayment` and named on the wire (`elolegSzamlaszam` beside
/// the `vegszamla` flag, under the `…:final` external id), and is never
/// foreign to the lookup step's hint although it is the newest live
/// invoice-kind document under the order.
pub(crate) async fn create_final_names_its_live_prepayment_invoice(h: &Harness) {
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-43:prepayment"),
            ..Doc::of("ES-43", "ES", "E2E-43")
        },
    )
    .await;
    h.absent("E2E-43", &["final"]).await;
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
            &create_body(dec!(1000), false),
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
}
