//! `Szamlazz.Order.correct_invoice`: the corrective under its
//! `correction_id`, the base verified like every document found by number,
//! no order-number hint, and a 152 as a plain rejection.

use rust_decimal::dec;
use serde_json::{Value, json};
use wiremock::matchers::body_string_contains;

use crate::harness::szamlazz::{
    Doc, create, created, duplicate_order_number, external_id_query, not_found, number_query,
    order_query,
};
use crate::harness::{Harness, document};

/// (vii-c) `correct_invoice`: the base is verified by number (it must carry
/// this order's number); then the corrective is issued
/// under `{namespace}:{order}:corrective:{correction_id}` with the base named
/// on the wire (`helyesbitettSzamlaszam`), through `verify-base-{number}`,
/// `lookup-corrective` and `create-corrective`; the same `correction_id` again
/// (new key) finds it: `already_issued`.
pub(crate) async fn corrective_is_issued_under_its_correction_id(h: &Harness) {
    h.reset().await;
    h.holds(&Doc::new("SZ-C1", "SZ", "E2E-C1")).await;
    // The corrective's id: absent for the lookup step and the create step's
    // leading query, then the issued corrective.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-C1:corrective:fix-1"),
            referenced_invoice: Some("SZ-C1"),
            ..Doc::new("HS-C1", "HS", "E2E-C1")
        },
    )
    .await;
    create()
        .and(body_string_contains(
            "<helyesbitettSzamlaszam>SZ-C1</helyesbitettSzamlaszam>",
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
        h.runs(reply.invocation_id()).await,
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
    eprintln!(
        "(vii-c) correct_invoice → issued under the correction id with the base on the wire; again → already_issued: pass"
    );
}

/// (vii-c') `correct_invoice` verifies its base like every document found by
/// number and settles every refusal after the verify alone, with
/// nothing sent: a base szamlazz.hu does not know (code 7) is 404 `not_found`
/// naming the invoice and carrying the corrective's identity; a reversed base
/// is `conflict{base_reversed, existing_number}`; a base carrying another
/// order's number is `conflict{not_managed, existing_number}`. A live base of
/// this order is corrected under `{namespace}:{order}:corrective:{correction_id}`
/// on the wire (`szamlaKulsoAzon`), a second `correction_id` is a second
/// corrective under its own id, and neither takes the order-number hint
/// (correctives are exempt from it), so a live foreign invoice under the order
/// is never met (the verify's arms were previously pinned by nothing).
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the three refusals of the verify, then two correctives on the wire"
)]
pub(crate) async fn correctives_verify_their_base_and_take_no_hint(h: &Harness) {
    let body = |base: &str, correction_id: &str| {
        json!({
            "invoice_number": base,
            "correction_id": correction_id,
            "document": document(dec!(-1000)),
        })
    };

    // Code 7 on the base.
    h.reset().await;
    number_query("SZ-C7")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("HS-X", "-1000", "-1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-C2",
            "correct_invoice",
            &body("SZ-C7", "fix-7"),
            "e2e-c2-k1",
        )
        .await;
    assert_eq!(reply.status, 404, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "not_found", "{fault:?}");
    assert!(fault.message.contains("SZ-C7"), "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-C2"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("corrective"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-C2:corrective:fix-7"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-base-SZ-C7"]
    );
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");

    // A reversed base, then a base of another order.
    for (base, order, reason, correction_id) in [
        ("SZ-C8", "E2E-C2", "base_reversed", "fix-8"),
        ("SZ-C9", "OTHER-C", "not_managed", "fix-9"),
    ] {
        h.reset().await;
        number_query(base)
            .respond_with(
                Doc {
                    reversed: reason == "base_reversed",
                    ..Doc::new(base, "SZ", order)
                }
                .response(),
            )
            .expect(1)
            .mount(&h.mock)
            .await;
        create()
            .respond_with(created("HS-X", "-1000", "-1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-C2",
                "correct_invoice",
                &body(base, correction_id),
                &format!("e2e-c2-{base}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{base}: {}", reply.body);
        let conflict = &reply.body;
        assert_eq!(conflict["outcome"], "conflict", "{base}: {conflict}");
        assert_eq!(conflict["conflict_reason"], reason, "{base}: {conflict}");
        assert_eq!(conflict["existing_number"], base, "{base}: {conflict}");
        assert_eq!(conflict["kind"], "corrective", "{base}");
        assert_eq!(
            conflict["external_id"],
            format!("acct:E2E-C2:corrective:{correction_id}"),
            "{base}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", &format!("verify-base-{base}")],
            "{base}: the verify is the last step journaled"
        );
        assert_eq!(h.requests_seen().await, 1, "{base}: nothing sent");
    }

    // Two correctives of one live base, each under its own id; the hint is
    // never taken, so a live foreign invoice under the order is never met.
    h.reset().await;
    number_query("SZ-C3")
        .respond_with(Doc::new("SZ-C3", "SZ", "E2E-C3").response())
        .mount(&h.mock)
        .await;
    order_query("E2E-C3")
        .respond_with(Doc::new("SZ-FOREIGN-C3", "SZ", "E2E-C3").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    for correction_id in ["fix-a", "fix-b"] {
        let external_id = format!("acct:E2E-C3:corrective:{correction_id}");
        external_id_query(&external_id)
            .respond_with(not_found())
            .mount(&h.mock)
            .await;
        create()
            .and(body_string_contains(format!(
                "<szamlaKulsoAzon>{external_id}</szamlaKulsoAzon>"
            )))
            .and(body_string_contains(
                "<helyesbitettSzamlaszam>SZ-C3</helyesbitettSzamlaszam>",
            ))
            .and(body_string_contains(
                "<helyesbitoszamla>true</helyesbitoszamla>",
            ))
            .respond_with(created(&format!("HS-C3-{correction_id}"), "-1000", "-1270"))
            .expect(1)
            .mount(&h.mock)
            .await;
    }
    for correction_id in ["fix-a", "fix-b"] {
        let reply = h
            .call(
                "E2E-C3",
                "correct_invoice",
                &body("SZ-C3", correction_id),
                &format!("e2e-c3-{correction_id}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{correction_id}: {}", reply.body);
        assert_eq!(
            reply.body["outcome"], "issued",
            "{correction_id}: {}",
            reply.body
        );
        assert_eq!(
            reply.body["invoice_number"],
            format!("HS-C3-{correction_id}"),
            "{correction_id}"
        );
        assert_eq!(
            reply.body["external_id"],
            format!("acct:E2E-C3:corrective:{correction_id}"),
            "{correction_id}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            [
                "namespace",
                "account",
                "verify-base-SZ-C3",
                "lookup-corrective",
                "create-corrective",
            ],
            "{correction_id}"
        );
    }
    assert_eq!(
        h.create_bodies().await.len(),
        2,
        "one create per correction id"
    );
    assert_eq!(
        h.bodies_of("action-szamla_agent_xml")
            .await
            .iter()
            .filter(|body| body.contains("<rendelesSzam>"))
            .count(),
        0,
        "no order-number query for a corrective"
    );
    eprintln!(
        "(vii-c') correct_invoice: base 7 → not_found; reversed → conflict{{base_reversed}}; another order's → conflict{{not_managed}}; two correction ids → two correctives under their ids, no hint: pass"
    );
}

/// (vii-c'') a duplicate-order-number answer (152) to a corrective's create
/// is `rejected{152}`, never `conflict{duplicate_order_number}`: storno and
/// corrective invoices are exempt from the order-number-repetition rule, so
/// the answer is a plain refusal; the external-id re-query (which could
/// still reconcile a corrective that landed) misses, and no order-number
/// query follows it; one send. (The gateway pins the closure; this pins the
/// handler's response.)
pub(crate) async fn a_duplicate_order_number_on_a_corrective_is_rejected(h: &Harness) {
    h.reset().await;
    number_query("SZ-C4")
        .respond_with(Doc::new("SZ-C4", "SZ", "E2E-C4").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-C4:corrective:fix-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    order_query("E2E-C4")
        .respond_with(Doc::new("SZ-C4", "SZ", "E2E-C4").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(duplicate_order_number("E2E-C4"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let rejected = h
        .ok(
            "E2E-C4",
            "correct_invoice",
            &json!({
                "invoice_number": "SZ-C4",
                "correction_id": "fix-1",
                "document": document(dec!(-1000)),
            }),
            "e2e-c4-k1",
        )
        .await;
    assert_eq!(rejected["outcome"], "rejected", "{rejected}");
    assert_eq!(rejected["conflict_reason"], Value::Null, "{rejected}");
    assert_eq!(rejected["code"], "152", "{rejected}");
    assert!(
        rejected["message"]
            .as_str()
            .is_some_and(|message| message.contains("E2E-C4")),
        "{rejected}"
    );
    assert_eq!(rejected["kind"], "corrective");
    assert_eq!(h.create_bodies().await.len(), 1, "one send");
    assert_eq!(
        h.requests_seen().await,
        5,
        "the verify, the lookup, the leading query, the send and the external-id re-query; no order-number query"
    );
    eprintln!("(vii-c'') 152 on a corrective → rejected{{152}}, no order query: pass");
}
