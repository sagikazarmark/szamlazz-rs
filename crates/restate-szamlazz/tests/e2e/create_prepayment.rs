//! `Szamlazz.Order.create_prepayment`: the proforma consumed like the
//! invoice does (`none`, `auto`, `{number}`), and the two chains refusing
//! each other at the exclusivity step.

use rust_decimal::dec;
use serde_json::json;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{Doc, create, created, external_id_query, order_query};
use crate::harness::{Harness, create_body, document};

/// (x) `create_prepayment` consumes the order's proforma like `create_invoice`
/// does (#69): `options.proforma: none` while a live proforma of ours exists
/// is `conflict{proforma_live, existing_number}` after the `proforma-link`
/// read with nothing sent (szamlazz.hu would link it by shared order number
/// anyway); under the default `auto` the create carries
/// `dijbekeroSzamlaszam` beside the `elolegszamla` flag, and under
/// `{number}` the named proforma is verified like every found document
/// (`verify-proforma-{number}` in place of `proforma-link`) and linked.
/// `create_final` and `create_proforma` still take no `options.proforma`:
/// anything but `auto` is `invalid_input` before any szamlazz.hu call.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two refusals, the conflict, then the issued prepayment invoice under auto and under a number"
)]
pub(crate) async fn prepayment_converts_the_proforma_like_the_invoice(h: &Harness) {
    h.reset().await;
    let before = h.requests_seen().await;
    for (i, handler) in ["create_final", "create_proforma"].into_iter().enumerate() {
        let reply = h
            .call(
                "E2E-10",
                handler,
                &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
                &format!("e2e-10-refused-{i}"),
            )
            .await;
        assert_eq!(reply.status, 400, "{handler}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "invalid_input", "{handler}: {}", reply.body);
        assert!(
            fault.message.contains(&format!("not {handler}")),
            "{handler}: names the handler: {}",
            fault.message
        );
    }
    assert_eq!(h.requests_seen().await, before, "refused before any call");

    // The order's live proforma of ours, under `acct:E2E-10:proforma`.
    let proforma = Doc {
        external_id: Some("acct:E2E-10:proforma"),
        ..Doc::new("D-10", "D", "E2E-10")
    };
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&proforma).await;
    create()
        .respond_with(created("ES-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
            "e2e-10-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "proforma_live", "{conflict}");
    assert_eq!(conflict["existing_number"], "D-10");
    assert_eq!(conflict["kind"], "prepayment");
    assert_eq!(conflict["external_id"], "acct:E2E-10:prepayment");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
        ],
        "the proforma link is what refused, nothing after it: {runs:?}"
    );

    // Under `auto` the same proforma is linked explicitly.
    h.reset().await;
    h.absent("E2E-10", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&proforma).await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10",
            "create_prepayment",
            &json!({ "document": document(dec!(1000)) }),
            "e2e-10-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let issued = &reply.body;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    assert_eq!(issued["kind"], "prepayment");
    assert_eq!(issued["invoice_number"], "ES-10");
    assert_eq!(issued["external_id"], "acct:E2E-10:prepayment");
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
            "lookup-prepayment",
            "create-prepayment",
        ],
        "{runs:?}"
    );

    // Under `{number}` the named proforma is verified by number (this
    // order's) and linked, on another order.
    h.reset().await;
    h.absent("E2E-10p", &["invoice", "prepayment", "final"])
        .await;
    h.holds(&Doc::new("D-10p", "D", "E2E-10p")).await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-10p</dijbekeroSzamlaszam>",
        ))
        .and(body_string_contains("<elolegszamla>true</elolegszamla>"))
        .respond_with(created("ES-10p", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-10p",
            "create_prepayment",
            &json!({
                "document": document(dec!(1000)),
                "options": { "proforma": { "number": "D-10p" } },
            }),
            "e2e-10p-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "ES-10p");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-invoice",
            "exclusivity-final",
            "verify-proforma-D-10p",
            "lookup-prepayment",
            "create-prepayment",
        ],
        "{runs:?}"
    );
    eprintln!(
        "(x) create_prepayment: none → conflict{{proforma_live}}, auto → dijbekeroSzamlaszam on the wire, {{number}} → verified and linked; create_final/create_proforma refuse the option: pass"
    );
}

/// (x-f) the two chains refuse each other at the exclusivity step: a live
/// prepayment invoice of ours under `…:prepayment` refuses
/// `create_invoice`, a live invoice of ours under `…:invoice` refuses
/// `create_prepayment`, both as `conflict{prepaid_chain, existing_number}`
/// from the first exclusivity row with nothing read or sent after it. A
/// **reversed** prepayment invoice refuses nothing: `create_invoice` walks
/// every step and issues; the storno of the `ES`, the newest document under
/// the order, is not an invoice kind and so never foreign.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two refusals, then the create a reversed prepayment invoice does not refuse"
)]
pub(crate) async fn the_other_chains_live_document_refuses_the_create(h: &Harness) {
    // The handler and its kind, then the other chain's kind and the live
    // document of ours held under it.
    for (handler, kind, held_kind, held_number, held_tipus) in [
        ("create_invoice", "invoice", "prepayment", "ES-40", "ES"),
        ("create_prepayment", "prepayment", "invoice", "SZ-40", "SZ"),
    ] {
        h.reset().await;
        h.holds(&Doc {
            external_id: Some(&format!("acct:E2E-40:{held_kind}")),
            ..Doc::new(held_number, held_tipus, "E2E-40")
        })
        .await;
        create()
            .respond_with(created("X-40", "1000", "1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-40",
                handler,
                &create_body(dec!(1000), false),
                &format!("e2e-40-{handler}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{handler}: {}", reply.body);
        let conflict = &reply.body;
        assert_eq!(conflict["outcome"], "conflict", "{handler}: {conflict}");
        assert_eq!(
            conflict["conflict_reason"], "prepaid_chain",
            "{handler}: {conflict}"
        );
        assert_eq!(conflict["existing_number"], held_number, "{handler}");
        assert_eq!(conflict["kind"], kind, "{handler}");
        assert_eq!(conflict["external_id"], format!("acct:E2E-40:{kind}"));
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", &format!("exclusivity-{held_kind}")],
            "{handler}: the first exclusivity row is what refused"
        );
        assert_eq!(
            h.requests_seen().await,
            1,
            "{handler}: the one exclusivity lookup, nothing else"
        );
    }

    // A reversed prepayment invoice under `…:prepayment` refuses nothing.
    h.reset().await;
    external_id_query("acct:E2E-40:prepayment")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("ES-40", "ES", "E2E-40")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    h.absent("E2E-40", &["final", "proforma", "invoice"]).await;
    order_query("E2E-40")
        .respond_with(
            Doc {
                referenced_invoice: Some("ES-40"),
                ..Doc::new("SS-40", "SS", "E2E-40")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    create()
        .and(body_string_contains(
            "<szamlaKulsoAzon>acct:E2E-40:invoice</szamlaKulsoAzon>",
        ))
        .respond_with(created("SZ-40", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-40",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-40-k3",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-40");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
            "create-invoice",
        ]
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    eprintln!(
        "(x-f) live ES → create_invoice conflict{{prepaid_chain}}; live SZ → create_prepayment conflict{{prepaid_chain}}; reversed ES refuses nothing: pass"
    );
}
