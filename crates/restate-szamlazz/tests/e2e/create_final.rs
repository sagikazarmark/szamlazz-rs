//! `Szamlazz.Order.create_final`: a live final invoice closing the order to
//! the other creates (#62), and the prepayment invoice settled first
//! (`prepayment-for-final`).

use rust_decimal::dec;
use serde_json::Value;
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{Doc, api_error, create, created, external_id_query, order_query};
use crate::harness::{Harness, create_body};

/// (x-d) a live final invoice under `…:final` closes the order to every
/// other create (#62). After `ES` → `VS` → storno of the `ES`, `…:prepayment`
/// is reversed, `…:invoice` is absent and the newest document under the order
/// is the `ES`'s storno (nothing the lookup step's hint would call foreign),
/// so without a row for the final invoice a plain `SZ` (or a second `ES`
/// under `reissue`) landed beside the live `VS`. The exclusivity step finds
/// the `VS`: `create_invoice` and `create_prepayment`, with and without
/// `reissue`, are `conflict{prepaid_chain, existing_number}`, `create_proforma`
/// is `conflict{order_invoiced, existing_number}`, the refusal comes from
/// `exclusivity-final` with no lookup step after it, and nothing is sent. A
/// **reversed** `VS` refuses nothing: `create_invoice`, and `create_prepayment`
/// with `reissue`, proceed to `issued`; `create_final` reports the reversed
/// final and, with `reissue`, issues the next one under the same id; its own
/// check, `prepayment-for-final`, is unchanged.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the five refusals, then the three creates a reversed final does not refuse"
)]
pub(crate) async fn a_live_final_closes_the_order_to_the_other_creates(h: &Harness) {
    // ES-35 issued, VS-35 settled it, then ES-35 was reversed.
    h.reset().await;
    mount_prepaid_chain(h, "35", true, false).await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    for (i, (handler, kind, reissue, reason)) in [
        ("create_invoice", "invoice", false, "prepaid_chain"),
        ("create_invoice", "invoice", true, "prepaid_chain"),
        ("create_prepayment", "prepayment", false, "prepaid_chain"),
        ("create_prepayment", "prepayment", true, "prepaid_chain"),
        ("create_proforma", "proforma", false, "order_invoiced"),
    ]
    .into_iter()
    .enumerate()
    {
        let reply = h
            .call(
                "E2E-35",
                handler,
                &create_body(dec!(1000), reissue),
                &format!("e2e-35-k{i}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{handler}: {}", reply.body);
        let conflict = &reply.body;
        assert_eq!(
            conflict["outcome"], "conflict",
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(
            conflict["conflict_reason"], reason,
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(
            conflict["existing_number"], "VS-35",
            "{handler} reissue={reissue}: {conflict}"
        );
        assert_eq!(conflict["kind"], kind, "{handler}: {conflict}");
        assert_eq!(
            conflict["external_id"],
            format!("acct:E2E-35:{kind}"),
            "{handler}: {conflict}"
        );
        let runs = h.runs(reply.invocation_id()).await;
        assert_eq!(
            runs.last().map(String::as_str),
            Some("exclusivity-final"),
            "{handler}: the final invoice's row is what refused: {runs:?}"
        );
        assert!(
            !runs.iter().any(|name| name.starts_with("lookup-")),
            "{handler}: no lookup step after the refusal: {runs:?}"
        );
    }
    assert!(
        h.create_bodies().await.is_empty(),
        "nothing was sent beside the live VS-35"
    );

    // A reversed final refuses nothing: with both the `ES` and the `VS`
    // reversed, the plain invoice proceeds and a second prepayment invoice
    // proceeds under `reissue` (the hint's newest document is the final's
    // storno, not foreign), and each create lands under its own external id.
    for (handler, kind, reissue, number) in [
        ("create_invoice", "invoice", false, "SZ-36"),
        ("create_prepayment", "prepayment", true, "ES-36B"),
    ] {
        h.reset().await;
        mount_prepaid_chain(h, "36", true, true).await;
        create()
            .and(body_string_contains(format!(
                "<szamlaKulsoAzon>acct:E2E-36:{kind}</szamlaKulsoAzon>"
            )))
            .respond_with(created(number, "1000", "1270"))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-36",
                handler,
                &create_body(dec!(1000), reissue),
                &format!("e2e-36-{handler}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{handler}: {}", reply.body);
        assert_eq!(reply.body["outcome"], "issued", "{handler}: {}", reply.body);
        assert_eq!(reply.body["invoice_number"], number, "{handler}");
        assert_eq!(reply.body["external_id"], format!("acct:E2E-36:{kind}"));
        assert_eq!(
            h.create_bodies().await.len(),
            1,
            "{handler}: exactly one create"
        );
        let runs = h.runs(reply.invocation_id()).await;
        let mut expected = vec!["namespace", "account"];
        expected.extend(if kind == "invoice" {
            vec!["exclusivity-prepayment", "exclusivity-final"]
        } else {
            vec!["exclusivity-invoice", "exclusivity-final"]
        });
        // Both kinds convert a proforma (#69), so both run the link.
        expected.push("proforma-link");
        let (lookup, create_step) = (format!("lookup-{kind}"), format!("create-{kind}"));
        expected.extend([lookup.as_str(), create_step.as_str()]);
        assert_eq!(runs, expected, "{handler}: {runs:?}");
    }

    // A new final after a reversed one stays possible: `create_final` checks
    // its live prepayment (`prepayment-for-final`, unchanged), reports the
    // reversed final without `reissue`, and issues the next one with it,
    // settling the same prepayment.
    h.reset().await;
    mount_prepaid_chain(h, "37", false, true).await;
    create()
        .and(body_string_contains("<vegszamla>true</vegszamla>"))
        .and(body_string_contains(
            "<elolegSzamlaszam>ES-37</elolegSzamlaszam>",
        ))
        .respond_with(created("VS-38", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .ok(
            "E2E-37",
            "create_final",
            &create_body(dec!(1000), false),
            "e2e-37-k1",
        )
        .await;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["invoice_number"], "VS-37");
    assert_eq!(reversed["storno_number"], "SS-37");
    assert!(
        h.create_bodies().await.is_empty(),
        "reversed is not reissue"
    );
    let reply = h
        .call(
            "E2E-37",
            "create_final",
            &create_body(dec!(1000), true),
            "e2e-37-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["kind"], "final");
    assert_eq!(reply.body["invoice_number"], "VS-38");
    assert_eq!(reply.body["external_id"], "acct:E2E-37:final");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "prepayment-for-final",
            "lookup-final",
            "create-final",
        ],
        "no exclusivity row for the final invoice itself: {runs:?}"
    );
    eprintln!(
        "(x-d) live VS → create_invoice/create_prepayment conflict{{prepaid_chain}}, create_proforma conflict{{order_invoiced}}; reversed VS refuses nothing: pass"
    );
}

/// szamlazz.hu after `ES-{n}` → `VS-{n}` on order `E2E-{n}`, with the `ES`
/// and/or the `VS` reversed since: nothing under `…:invoice` or `…:proforma`,
/// the `ES` under `…:prepayment`, the `VS` (`hivszamlaszam` = the `ES`) under
/// `…:final`, and the newest document under the order the storno `SS-{n}` of
/// the last one reversed, or the `VS` itself while both are live.
async fn mount_prepaid_chain(h: &Harness, n: &str, es_reversed: bool, vs_reversed: bool) {
    let order = format!("E2E-{n}");
    let (es, vs, ss) = (format!("ES-{n}"), format!("VS-{n}"), format!("SS-{n}"));
    h.absent(&order, &["invoice", "proforma"]).await;
    external_id_query(&format!("acct:{order}:prepayment"))
        .respond_with(
            Doc {
                reversed: es_reversed,
                ..Doc::new(&es, "ES", &order)
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    let final_invoice = Doc {
        reversed: vs_reversed,
        referenced_invoice: Some(&es),
        ..Doc::new(&vs, "VS", &order)
    };
    external_id_query(&format!("acct:{order}:final"))
        .respond_with(final_invoice.response())
        .mount(&h.mock)
        .await;
    let newest = match (es_reversed, vs_reversed) {
        (_, true) => Doc {
            referenced_invoice: Some(&vs),
            ..Doc::new(&ss, "SS", &order)
        },
        (true, false) => Doc {
            referenced_invoice: Some(&es),
            ..Doc::new(&ss, "SS", &order)
        },
        (false, false) => final_invoice,
    };
    order_query(&order)
        .respond_with(newest.response())
        .mount(&h.mock)
        .await;
}

/// (x-h) `create_final` settles its prepayment invoice first
/// (`prepayment-for-final`): none under `…:prepayment` is
/// `conflict{prepayment_missing}` without an `existing_number`, a reversed one
/// is `conflict{prepayment_reversed, existing_number}`, both with nothing sent
/// and no step after the check; a live one is named on the wire
/// (`elolegSzamlaszam` beside the `vegszamla` flag), and is never foreign to
/// the lookup step's hint although it is the newest live invoice-kind document
/// under the order; and szamlazz.hu's 73 (the referenced prepayment invoice
/// cannot be identified: how the server enforces one final invoice per
/// prepayment invoice) is `rejected{73}` with its message, settled after one
/// send.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two refusals, the issued final on the wire and the 73"
)]
pub(crate) async fn create_final_settles_its_prepayment_invoice_first(h: &Harness) {
    // Absent.
    h.reset().await;
    h.absent("E2E-43", &["prepayment"]).await;
    create()
        .respond_with(created("VS-X", "1000", "1270"))
        .expect(0)
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
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(
        conflict["conflict_reason"], "prepayment_missing",
        "{conflict}"
    );
    assert_eq!(conflict["existing_number"], Value::Null, "{conflict}");
    assert_eq!(conflict["kind"], "final");
    assert_eq!(conflict["external_id"], "acct:E2E-43:final");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "prepayment-for-final"]
    );
    assert_eq!(h.requests_seen().await, 1);

    // Reversed.
    h.reset().await;
    external_id_query("acct:E2E-43:prepayment")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("ES-43", "ES", "E2E-43")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("VS-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-43",
            "create_final",
            &create_body(dec!(1000), false),
            "e2e-43-k2",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(
        conflict["conflict_reason"], "prepayment_reversed",
        "{conflict}"
    );
    assert_eq!(conflict["existing_number"], "ES-43");
    assert_eq!(h.requests_seen().await, 1);

    // Live: named on the wire, and not foreign although it is the newest
    // live invoice-kind document under the order.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-43:prepayment"),
        ..Doc::new("ES-43", "ES", "E2E-43")
    })
    .await;
    h.absent("E2E-43", &["final"]).await;
    create()
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
            "e2e-43-k3",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["kind"], "final");
    assert_eq!(reply.body["invoice_number"], "VS-43");
    assert_eq!(reply.body["external_id"], "acct:E2E-43:final");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "prepayment-for-final",
            "lookup-final",
            "create-final",
        ]
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");

    // szamlazz.hu's 73: the referenced prepayment invoice cannot be
    // identified (a rejection, settled after one send).
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-43:prepayment"),
        ..Doc::new("ES-43", "ES", "E2E-43")
    })
    .await;
    h.absent("E2E-43", &["final"]).await;
    create()
        .respond_with(api_error(
            "73",
            "A hivatkozott előlegszámla nem beazonosítható. Rendelésszám: E2E-43, előlegszámla száma: ES-43",
        ))
        .expect(1)
        .mount(&h.mock)
        .await;
    let rejected = h
        .ok(
            "E2E-43",
            "create_final",
            &create_body(dec!(1000), false),
            "e2e-43-k4",
        )
        .await;
    assert_eq!(rejected["outcome"], "rejected", "{rejected}");
    assert_eq!(rejected["code"], "73", "{rejected}");
    assert!(
        rejected["message"]
            .as_str()
            .is_some_and(|message| message.contains("nem beazonosítható")),
        "{rejected}"
    );
    assert_eq!(rejected["kind"], "final");
    assert_eq!(h.create_bodies().await.len(), 1, "one send, no re-send");
    eprintln!(
        "(x-h) create_final: absent ES → conflict{{prepayment_missing}}; reversed → conflict{{prepayment_reversed}}; live → issued with elolegSzamlaszam on the wire; 73 → rejected{{73}}: pass"
    );
}
