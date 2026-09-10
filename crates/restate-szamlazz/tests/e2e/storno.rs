//! `Szamlazz.Order.storno_invoice` under Restate: the storno path (verify,
//! lookup, the storno step) with the original's `telj` repeated on the wire
//! and the reissue that follows a reversal; the hint path, an original the
//! verify already reports reversed answered from the order-number hint; a
//! storno whose first send loses its reply re-executed under the issue policy
//! with a byte-identical body (the date a pure function of the journaled
//! verify); and, in phase 2, an order Restate has no memory of. The verdicts
//! (`not_managed`, `not_stornoable`, a `telj`-less original, the storno's
//! `eszamla`) are unit tests of `service::storno` and the storno intent;
//! szamlazz.hu's typed refusals (14, 221) are `tests/gateway/`'s.

use rust_decimal::dec;
use serde_json::json;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{
    Doc, agent_key_tag, create_for, create_with_key, created, created_without_totals,
    external_id_query, holds, not_found, number_query, order_query, original_telj_tag,
    storno_never_sent, storno_of, storno_of_number_repeating_telj,
};
use crate::harness::{Harness, create_body};

/// storno ⇒ `reversed{storno_number}` through `verify-original-{number}`,
/// `lookup-storno-{number}` and `storno-{number}`, the storno carrying the
/// original's `telj` as `teljesitesDatum`, the original's appearance as
/// `eszamla` and no `keltDatum` (ADR 0007); then a create with `reissue` ⇒
/// `issued` as the newest holder of the same external id (the lookup passes
/// the reversed document and its hint sees the storno; the create step's
/// leading query sees the same reversed document and issues).
#[allow(
    clippy::too_many_lines,
    reason = "storno with identity verification, then explicit reissue"
)]
pub(crate) async fn storno_then_reissue(h: &Harness) {
    // The storno: the original by number, nothing under the storno id.
    // An e-invoice original (`eszamla` 3, what szamlazz.hu reports for a
    // create with `eszamla=true`) on an account whose default is paper.
    number_query("SZ-4")
        .respond_with(
            Doc {
                eszamla: Some(3),
                ..Doc::of("SZ-4", "SZ", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-4:storno:SZ-4")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_of_number_repeating_telj("SZ-4")
        .respond_with(created_without_totals("SS-4"))
        .expect(1)
        .mount(&h.mock)
        .await;
    // Optional reply totals are absent: identity verification stays inside
    // storno-SZ-4, so the durable path below is still the same five entries.
    number_query("SS-4")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-4"),
                ..Doc::of("SS-4", "SS", "E2E-4")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    // The reissue: the reversed original under our id, its storno the newest
    // document under the order, nothing under the other ids.
    h.absent("E2E-4", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-4:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-4", "SZ", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-4")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-4"),
                ..Doc::of("SS-4", "SS", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    create_for("E2E-4")
        .respond_with(created("SZ-4B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4"), "e2e-4-s1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let reversed = &reply.body;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["storno_number"], "SS-4");
    assert_eq!(reversed["invoice_number"], "SZ-4");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-original-SZ-4",
            "lookup-storno-SZ-4",
            "storno-SZ-4",
        ]
    );
    let stornos = h.storno_bodies_of("SZ-4").await;
    assert_eq!(stornos.len(), 1);
    assert!(stornos[0].contains(&original_telj_tag()), "{}", stornos[0]);
    assert!(
        !stornos[0].contains("<keltDatum>"),
        "no issue date on a storno: {}",
        stornos[0]
    );
    assert!(
        stornos[0].contains("<eszamla>true</eszamla>"),
        "an e-invoice original (eszamla 3) is reversed as an e-invoice, whatever the account default (paper): {}",
        stornos[0]
    );

    let reissued = h
        .ok(
            "E2E-4",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-4-k1",
        )
        .await;
    assert_eq!(reissued["outcome"], "issued", "{reissued}");
    assert_eq!(reissued["external_id"], "acct:E2E-4:invoice");
    assert_eq!(reissued["invoice_number"], "SZ-4B");
    assert_eq!(h.create_bodies_of("E2E-4").await.len(), 1);
}

/// The storno handler's other path and its re-execution. An original the
/// verify already reports `sztornozott` is answered `reversed` with the storno
/// number from the order-number hint, through `verify-original-{number}` and
/// `hint-storno-{number}` alone, nothing sent. And a storno whose first send
/// loses its reply (the immediate re-query still missing) is re-executed under
/// the issue policy with a **byte-identical** body (the `teljesitesDatum` is a
/// pure function of the journaled verify, which the re-execution replays)
/// under one `storno-{number}` entry.
pub(crate) async fn storno_answers_from_the_hint_or_re_executes_a_lost_send(h: &Harness) {
    // The hint path.
    number_query("SZ-4D")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-4D", "SZ", "E2E-4D")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    order_query("E2E-4D")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-4D"),
                ..Doc::of("SS-4D", "SS", "E2E-4D")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock, "SZ-4D").await;
    // The lost reply: the first send answers 500 and its re-query still
    // misses; the second execution's send lands.
    number_query("SZ-4E")
        .respond_with(Doc::of("SZ-4E", "SZ", "E2E-4E").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-4E:storno:SZ-4E")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_of_number_repeating_telj("SZ-4E")
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_of_number_repeating_telj("SZ-4E")
        .respond_with(created("SS-4E", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call("E2E-4D", "storno_invoice", &storno_of("SZ-4D"), "e2e-4d-s1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4D", "{}", reply.body);
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-original-SZ-4D",
            "hint-storno-SZ-4D"
        ]
    );
    assert_eq!(
        h.requests_of_order("E2E-4D").await.len(),
        1,
        "the hint is the one request naming the order; the verify names the number"
    );

    let reply = h
        .call("E2E-4E", "storno_invoice", &storno_of("SZ-4E"), "e2e-4e-s1")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4E", "{}", reply.body);
    let stornos = h.storno_bodies_of("SZ-4E").await;
    assert_eq!(stornos.len(), 2, "two executions of the storno step");
    assert_eq!(
        stornos[0], stornos[1],
        "the re-executed storno is byte-identical"
    );
    assert!(stornos[0].contains(&original_telj_tag()));
    assert!(!stornos[0].contains("<keltDatum>"));
    let runs = h.admin().runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "storno-SZ-4E").count(),
        1,
        "one storno step entry: {runs:?}"
    );
}

/// Both storno shells retry an inconclusive numbered reply inside the
/// storno step. A later leading query finds the reversal without resending;
/// if it stays absent, exhaustion is `outcome_unknown`, stored under the key.
#[allow(
    clippy::too_many_lines,
    reason = "both storno shells, reconciliation and exhaustion with stored completion"
)]
pub(crate) async fn ambiguous_storno_retries_and_exhaustion_preserve_the_send(h: &Harness) {
    use restate_e2e_harness::Call;
    use restate_szamlazz::contract::TerminalCode;

    for (order, number, returned, managed, recovered) in [
        ("E2E-196-O", "SZ-196-O", "SS-196-O", true, true),
        ("E2E-196-A", "SZ-196-A", "SS-196-A", false, true),
        ("E2E-196-OX", "SZ-196-OX", "SS-196-OX", true, false),
        ("E2E-196-AX", "SZ-196-AX", "SS-196-AX", false, false),
    ] {
        let external_id = if managed {
            format!("acct:{order}:storno:{number}")
        } else {
            format!("acct:by-number:{number}:storno")
        };
        let call = if managed {
            Call::object("Szamlazz.Order", order, "storno_invoice")
        } else {
            Call::service("Szamlazz.Agent", "storno")
        };
        let original = if managed {
            Doc::of(number, "SZ", order)
        } else {
            Doc::unmanaged(number, "SZ")
        };
        number_query(number)
            .respond_with(original.response())
            .expect(1)
            .mount(&h.mock)
            .await;
        // Lookup, first execution's leading query and immediate reconciliation
        // all miss. Only the next execution's leading query sees what landed.
        if recovered {
            external_id_query(&external_id)
                .respond_with(not_found())
                .up_to_n_times(3)
                .expect(3)
                .mount(&h.mock)
                .await;
            external_id_query(&external_id)
                .respond_with(
                    Doc {
                        referenced_invoice: Some(number),
                        ..Doc::new(returned, "SS")
                    }
                    .response(),
                )
                .expect(1)
                .mount(&h.mock)
                .await;
        } else {
            external_id_query(&external_id)
                .respond_with(not_found())
                .expect(5)
                .mount(&h.mock)
                .await;
        }
        let sends = if recovered { 1 } else { 2 };
        storno_of_number_repeating_telj(number)
            .respond_with(created_without_totals(returned))
            .expect(sends)
            .mount(&h.mock)
            .await;
        number_query(returned)
            .respond_with(crate::common::body_error("135", "expired verification key"))
            .expect(sends)
            .mount(&h.mock)
            .await;

        let body = storno_of(number);
        let reply = h.invoke(&call, Some(&body), Some(order)).await;
        if recovered {
            assert_eq!(reply.status, 200, "{}", reply.body);
            assert_eq!(reply.body["outcome"], "reversed");
            assert_eq!(reply.body["storno_number"], returned);
        } else {
            assert_eq!(reply.status, 500, "{}", reply.body);
            let fault = reply.fault();
            assert_eq!(fault.code, TerminalCode::OutcomeUnknown);
            assert!(
                fault.message.contains(returned)
                    && fault.message.contains("135")
                    && fault.message.contains("expired verification key"),
                "{fault:?}"
            );
        }
        assert_eq!(
            h.admin().runs(reply.invocation_id()).await,
            [
                "namespace".to_owned(),
                "account".to_owned(),
                format!("verify-original-{number}"),
                format!("lookup-storno-{number}"),
                format!("storno-{number}"),
            ],
            "verification/retry belong to the one storno entry"
        );
        let stored = h.invoke(&call, Some(&body), Some(order)).await;
        assert_eq!(stored.invocation_id(), reply.invocation_id());
        assert_eq!(stored.body, reply.body);
        let bodies = h.storno_bodies_of(number).await;
        assert_eq!(bodies.len(), usize::try_from(sends).expect("one or two"));
        assert!(bodies.iter().all(|body| body == &bodies[0]
            && body.contains(&original_telj_tag())
            && body.contains("<eszamla>false</eszamla>")));
    }
}

/// An order Restate has no memory of (phase 2, under `acme`): an invoice is
/// issued and the invocation purged; `storno_invoice` → `reversed` (its verify
/// finds the document by number on szamlazz.hu), purged; `create_invoice
/// {reissue}` → `issued` as the newest holder of the same external id; the
/// scoped `get` sees the new holder. Nothing but the order key and the scope
/// was needed.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: issue, purge, storno, purge, reissue, get"
)]
pub(crate) async fn purged_order_is_stornoed_and_reissued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-18", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-18")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-18", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .call_scoped(
            "acme",
            "E2E-18",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-18-k1",
        )
        .await;
    assert_eq!(issued.status, 200, "{}", issued.body);
    assert_eq!(issued.body["outcome"], "issued", "{}", issued.body);
    h.admin().purge(issued.invocation_id()).await;

    // Storno: the invoice is verified by number and reversed.
    h.reset().await;
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-18:invoice"),
            ..Doc::of("SZ-18", "SZ", "E2E-18")
        },
    )
    .await;
    external_id_query("acct:E2E-18:storno:SZ-18")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_of_number_repeating_telj("SZ-18")
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-18", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .call_scoped(
            "acme",
            "E2E-18",
            "storno_invoice",
            &json!({ "invoice_number": "SZ-18" }),
            "e2e-18-s1",
        )
        .await;
    assert_eq!(reversed.status, 200, "{}", reversed.body);
    assert_eq!(reversed.body["outcome"], "reversed", "{}", reversed.body);
    assert_eq!(reversed.body["storno_number"], "SS-18");
    h.admin().purge(reversed.invocation_id()).await;
    assert!(
        h.admin().journal(issued.invocation_id()).await.is_empty()
            && h.admin().journal(reversed.invocation_id()).await.is_empty(),
        "Restate holds nothing of the order"
    );

    // Reissue: the lookup sees the reversed document and its storno; the
    // create step issues the next one under the same id.
    h.reset().await;
    h.absent("E2E-18", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-18:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::of("SZ-18", "SZ", "E2E-18")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-18")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-18"),
                ..Doc::of("SS-18", "SS", "E2E-18")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-18B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reissued = h
        .call_scoped(
            "acme",
            "E2E-18",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-18-k2",
        )
        .await;
    assert_eq!(reissued.status, 200, "{}", reissued.body);
    assert_eq!(reissued.body["outcome"], "issued", "{}", reissued.body);
    assert_eq!(reissued.body["invoice_number"], "SZ-18B");
    assert_eq!(reissued.body["external_id"], "acct:E2E-18:invoice");

    // The scoped live view sees the new holder.
    h.reset().await;
    h.absent("E2E-18", &["proforma", "prepayment", "final"])
        .await;
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-18:invoice"),
            ..Doc::of("SZ-18B", "SZ", "E2E-18")
        },
    )
    .await;
    let status = h.get_scoped("acme", "E2E-18").await;
    assert_eq!(status["invoice"]["number"], "SZ-18B", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
}
