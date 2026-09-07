//! The `Szamlazz.Agent` writes: `storno` acting on what the verify finds and
//! repeating the original's `telj` (ADR 0007), and `set_payments` replacing
//! or appending.

use serde_json::{Value, json};
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

use crate::harness::Harness;
use crate::harness::accounts::{AGENT_KEY, AGENT_KEYS};
use crate::harness::szamlazz::{
    Doc, SUPPLIER_B, agent_key_tag, api_error, created, credit, credited, external_id_query,
    not_found, number_query, storno_never_sent, storno_of, storno_repeating_telj,
};

/// (xviii-b) `Szamlazz.Agent.storno` under a scope acts on what the verify
/// finds and compares it with nothing about the account (ADR 0006,
/// account-pin amendment): a document whose seller block carries another
/// `szallito/id` and whose `teszt` says a live account issued it is reversed
/// with `acme`'s key like any of the account's own; a document carrying an
/// order number is `managed_by_order` with nothing sent.
pub(crate) async fn agent_storno_acts_on_what_the_verify_finds(h: &Harness) {
    // A document whose `teszt` and seller record id are not what `acme`'s
    // documents carry: nothing compares them, the storno proceeds with
    // `acme`'s key.
    h.reset().await;
    h.holds(&Doc {
        test: false,
        supplier_id: SUPPLIER_B,
        ..Doc::unmanaged("SZ-22", "SZ")
    })
    .await;
    external_id_query("acct:by-number:SZ-22:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-22", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-22"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-22", "{}", reply.body);

    // A document as `acme`'s own read: reversed, as before, through verify,
    // lookup and storno.
    h.reset().await;
    h.holds(&Doc::unmanaged("SZ-23", "SZ")).await;
    external_id_query("acct:by-number:SZ-23:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-23", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-23"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-23", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-SZ-23",
            "lookup-storno-SZ-23",
            "storno-SZ-23"
        ]
    );

    // A document carrying an order number is `managed_by_order`, nothing
    // sent.
    h.reset().await;
    number_query("SZ-25")
        .respond_with(Doc::new("SZ-25", "SZ", "E2E-25").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-25"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "managed_by_order", "{}", reply.body);
    assert_eq!(reply.body["order_key"], "E2E-25", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");
    eprintln!(
        "(xviii-b) Szamlazz.Agent.storno under a scope: another teszt / seller record id compared with nothing → reversed with acme's key; own → reversed; order-bearing → managed_by_order, nothing sent: pass"
    );
}

/// (xviii-c'') `Szamlazz.Agent.set_payments` under a scope, its one step
/// `set-payments-{number}` with no query before it: a replacing call puts
/// `<additiv>false</additiv>` on the wire with that account's key and the
/// entries as sent, an additive one `<additiv>true</additiv>`, and the answer
/// is the invoice's totals as szamlazz.hu reported them — `outstanding`
/// distinct from `gross_total`. A replacing call with no entries would clear
/// the invoice's payments: refused as `invalid_input` before the wire. A lost
/// reply is `outcome_unknown` after exactly one send — the step has no retry
/// of its own — and the fault's advice follows `additive`: a replacing call
/// is repeated as is, an additive one may have landed its entries, so the
/// caller queries the invoice first (at-least-once).
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two wire shapes, the refusal, and the two lost-reply faults"
)]
pub(crate) async fn set_payments_replaces_or_appends_and_answers_a_lost_reply(h: &Harness) {
    let entry = json!({
        "date": "2026-09-05",
        "method": "transfer",
        "amount": "1000",
        "description": "first instalment",
    });
    let request = |number: &str, additive: bool| {
        json!({
            "invoice_number": number,
            "entries": [entry.clone()],
            "additive": additive,
        })
    };

    // Replacing, then additive: the flag on the wire, the totals in the
    // answer.
    for (number, additive) in [("SZ-50", false), ("SZ-51", true)] {
        h.reset().await;
        credit()
            .and(body_string_contains(format!(
                "<szamlaszam>{number}</szamlaszam>"
            )))
            .and(body_string_contains(format!(
                "<additiv>{additive}</additiv>"
            )))
            .and(body_string_contains(agent_key_tag(AGENT_KEY)))
            .and(body_string_contains("<osszeg>1000</osszeg>"))
            .and(body_string_contains("<jogcim>átutalás</jogcim>"))
            .and(body_string_contains("<leiras>first instalment</leiras>"))
            .respond_with(credited(number, "1270", "270"))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call_agent_scoped("acme", "set_payments", &request(number, additive))
            .await;
        assert_eq!(reply.status, 200, "{number}: {}", reply.body);
        assert_eq!(reply.body["invoice_number"], number, "{}", reply.body);
        assert_eq!(reply.body["outstanding"], "270", "{}", reply.body);
        assert_eq!(reply.body["gross_total"], "1270", "{}", reply.body);
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", &format!("set-payments-{number}")],
            "{number}: one step, no query before it"
        );
        assert_eq!(h.requests_seen().await, 1, "{number}: the one send");
    }

    // A replacing call with no entries: refused before the wire.
    h.reset().await;
    credit()
        .respond_with(api_error("999", "never"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped(
            "acme",
            "set_payments",
            &json!({ "invoice_number": "SZ-52", "entries": [] }),
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert_eq!(fault.szamlazz_code, None, "{fault:?}");
    assert!(fault.message.contains("at least one entry"), "{fault:?}");
    assert!(fault.message.contains("nothing was sent"), "{fault:?}");
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // A lost reply, replacing then additive: one send each, and the advice
    // follows the flag.
    for (number, additive, advice, never) in [
        (
            "SZ-53",
            false,
            "call set_payments again",
            "query the invoice",
        ),
        (
            "SZ-54",
            true,
            "query the invoice before re-sending",
            "call set_payments again",
        ),
    ] {
        h.reset().await;
        credit()
            .and(body_string_contains(format!(
                "<szamlaszam>{number}</szamlaszam>"
            )))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call_agent_scoped("acme", "set_payments", &request(number, additive))
            .await;
        assert_eq!(reply.status, 500, "{number}: {}", reply.body);
        let fault = reply.fault();
        assert_eq!(fault.code, "outcome_unknown", "{number}: {fault:?}");
        assert!(
            fault
                .message
                .contains("credit entry registration outcome unknown"),
            "{number}: {fault:?}"
        );
        assert!(fault.message.contains(advice), "{number}: {fault:?}");
        assert!(!fault.message.contains(never), "{number}: {fault:?}");
        assert_eq!(fault.order, None, "{number}: a by-number fault: {fault:?}");
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            ["namespace", "account", &format!("set-payments-{number}")],
            "{number}"
        );
        assert_eq!(
            h.credit_bodies().await.len(),
            1,
            "{number}: the step has no retry of its own"
        );
    }
    eprintln!(
        "(xviii-c'') set_payments: additiv false/true on the wire with the totals answered; empty replace → invalid_input before the wire; lost reply → outcome_unknown after one send, advice by additive: pass"
    );
}

/// (xviii-e) `Szamlazz.Agent.storno` repeats the original's `telj` too (ADR
/// 0007), under a scope: a document is reversed with the storno carrying
/// `teljesitesDatum` and `acme`'s key; one without a `telj` is 503
/// `unavailable` naming the invoice — without `order`, `kind` or
/// `external_id`, as this handler's other faults — with only the verify
/// journaled and nothing sent; and the fault comes after the answers that
/// need no send: a `telj`-less order-bearing document is still
/// `managed_by_order`, a reversed one still `reversed`.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the date on the wire, the fault and its two predecessors"
)]
pub(crate) async fn agent_storno_repeats_the_originals_fulfillment_date_or_refuses(h: &Harness) {
    let without_telj = |number: &'static str| Doc {
        fulfillment_date: None,
        ..Doc::unmanaged(number, "SZ")
    };

    // The date on the wire.
    h.reset().await;
    number_query("SZ-31")
        .respond_with(Doc::unmanaged("SZ-31", "SZ").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-31:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains(agent_key_tag(AGENT_KEY)))
        .respond_with(created("SS-31", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-31"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-31", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1);
    assert!(!stornos[0].contains("<keltDatum>"), "{}", stornos[0]);

    // The fault, without an order identity.
    h.reset().await;
    number_query("SZ-32")
        .respond_with(without_telj("SZ-32").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-32"))
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("SZ-32"), "{fault:?}");
    assert!(fault.message.contains("fulfillment date"), "{fault:?}");
    assert_eq!(fault.order, None, "{fault:?}");
    assert_eq!(fault.kind, None, "{fault:?}");
    assert_eq!(fault.external_id, None, "{fault:?}");
    for key in AGENT_KEYS {
        assert!(!fault.message.contains(key), "{fault:?}");
    }
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-SZ-32"],
        "the verify is the only step journaled"
    );
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");

    // Before the fault: an order-bearing document is `managed_by_order`.
    h.reset().await;
    number_query("SZ-34")
        .respond_with(
            Doc {
                fulfillment_date: None,
                ..Doc::new("SZ-34", "SZ", "E2E-34")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-34"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "managed_by_order", "{}", reply.body);
    assert_eq!(reply.body["order_key"], "E2E-34", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 1);

    // Before the fault: a reversed document is `reversed`, with the storno
    // number the by-number storno lookup names — nothing under the id (a
    // reversal from the UI) leaves it unknown; the verify and the lookup are
    // the only requests, nothing is sent (J25, #65).
    h.reset().await;
    number_query("SZ-35")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-35")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-35:storno")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-35"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], Value::Null, "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-SZ-35",
            "lookup-storno-SZ-35"
        ]
    );
    assert_eq!(h.requests_seen().await, 2, "the verify and the lookup");

    // Reversed by a storno of ours (a lost reply, a retry with a new key):
    // the lookup names it.
    h.reset().await;
    number_query("SZ-36")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-36")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    external_id_query("acct:by-number:SZ-36:storno")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-36"),
                ..Doc::unmanaged("SS-36", "SS")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-36"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-36", "{}", reply.body);
    assert_eq!(h.requests_seen().await, 2, "the verify and the lookup");
    eprintln!(
        "(xviii-e) Szamlazz.Agent.storno: teljesitesDatum on the wire; telj-less → unavailable without an order identity, after managed_by_order / reversed (storno number from the by-number lookup): pass"
    );
}
