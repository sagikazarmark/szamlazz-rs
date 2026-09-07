//! `Szamlazz.Order.storno_invoice`: reversed then stale create then
//! `reissue`, the original's `telj` repeated or the storno refused (ADR
//! 0007), the storno step's rejections and exhaustion, an order Restate has
//! no memory of (Phase 2), and — on both storno handlers — the storno issued
//! in its original's form (`eszamla`).

use std::time::{Duration, Instant};

use rust_decimal::dec;
use serde_json::{Value, json};
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{
    Doc, agent_key_tag, api_error, create, create_with_key, created, external_id_query, not_found,
    number_query, order_query, original_telj_tag, storno, storno_never_sent, storno_of,
    storno_repeating_telj,
};
use crate::harness::{Harness, create_body};

/// After the storno of `SZ-1` on `E2E-1`: the reversed document under our
/// external id, its storno `SS-1` as the newest document under the order,
/// nothing under the other ids.
async fn mount_reversed_sz1(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-1:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-1", "SZ", "E2E-1")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-1")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-1"),
                ..Doc::new("SS-1", "SS", "E2E-1")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
}

/// (iv) storno ⇒ `reversed{storno_number}` with the storno carrying the
/// original's `telj` as `teljesitesDatum` and no `keltDatum` (ADR 0007); a
/// create ⇒ `reversed`; a create with `reissue` ⇒ `issued` as the newest
/// holder of the same external id.
pub(crate) async fn storno_then_stale_create_then_reissue(h: &Harness) {
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-1", "SZ", "E2E-1")
    })
    .await;
    external_id_query("acct:E2E-1:storno:SZ-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(created("SS-1", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reversed = h
        .ok(
            "E2E-1",
            "storno_invoice",
            &json!({ "invoice_number": "SZ-1" }),
            "e2e-1-s1",
        )
        .await;
    assert_eq!(reversed["outcome"], "reversed", "{reversed}");
    assert_eq!(reversed["storno_number"], "SS-1");
    assert_eq!(reversed["invoice_number"], "SZ-1");
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1);
    assert!(
        !stornos[0].contains("<keltDatum>"),
        "no issue date on a storno: {}",
        stornos[0]
    );

    // The stale create: the lookup finds the reversed document.
    mount_reversed_sz1(h).await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let stale = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k3",
        )
        .await;
    assert_eq!(stale["outcome"], "reversed", "{stale}");
    assert_eq!(stale["invoice_number"], "SZ-1");
    assert_eq!(stale["storno_number"], "SS-1");

    // Reissue: the lookup passes the reversed document and its hint sees the
    // storno; the create step's leading query sees the same reversed
    // document and issues the next one under the same id.
    mount_reversed_sz1(h).await;
    create()
        .respond_with(created("SZ-2", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reissued = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-1-k4",
        )
        .await;
    assert_eq!(reissued["outcome"], "issued", "{reissued}");
    assert_eq!(reissued["external_id"], "acct:E2E-1:invoice");
    assert_eq!(reissued["invoice_number"], "SZ-2");
    eprintln!("(iv) storno → reversed; stale create → reversed; reissue → issued: pass");
}

/// (iv-b) the storno's `teljesitesDatum` (ADR 0007), end to end. A verified
/// original without a `telj` is 503 `unavailable` about the storno — order,
/// kind and the storno external id — with only the prologue and the verify
/// journaled and nothing sent; the fault comes **after** the answers that
/// need no send, so a `telj`-less document of another order is still
/// `conflict{not_managed}`, a `telj`-less proforma still
/// `rejected{not_stornoable}` and a `telj`-less reversed invoice still
/// `reversed` with its storno number from the hint. And a storno whose first
/// reply is lost is re-executed under the issue policy with a byte-identical
/// body — the date is a pure function of the journaled verify — under one
/// `storno-{number}` entry.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the fault, its three predecessors and the re-executed step"
)]
pub(crate) async fn storno_repeats_the_originals_fulfillment_date_or_refuses(h: &Harness) {
    let without_telj = |number: &'static str, order: &'static str| Doc {
        fulfillment_date: None,
        ..Doc::new(number, "SZ", order)
    };

    // The fault: a live invoice of this order without a `telj`.
    h.reset().await;
    number_query("SZ-4A")
        .respond_with(without_telj("SZ-4A", "E2E-4").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4A"), "e2e-4-s1")
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("SZ-4A"), "{fault:?}");
    assert!(fault.message.contains("fulfillment date"), "{fault:?}");
    assert!(fault.message.contains("nothing was sent"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-4"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("invoice"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-4:storno:SZ-4A"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "verify-storno-SZ-4A"],
        "the verify is the last step journaled"
    );
    assert_eq!(h.requests_seen().await, 1, "the verify, nothing else");

    // Before the fault: another order's document is `conflict{not_managed}`.
    h.reset().await;
    number_query("SZ-4B")
        .respond_with(without_telj("SZ-4B", "OTHER-4").response())
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let conflict = h
        .ok("E2E-4", "storno_invoice", &storno_of("SZ-4B"), "e2e-4-s2")
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "not_managed", "{conflict}");
    assert_eq!(h.requests_seen().await, 1);

    // Before the fault: a proforma, and a delivery note, are
    // `rejected{not_stornoable}`.
    for (number, tipus) in [("D-4C", "D"), ("SL-4C", "SL")] {
        h.reset().await;
        number_query(number)
            .respond_with(
                Doc {
                    fulfillment_date: None,
                    ..Doc::new(number, tipus, "E2E-4")
                }
                .response(),
            )
            .mount(&h.mock)
            .await;
        storno_never_sent(&h.mock).await;
        let rejected = h
            .ok(
                "E2E-4",
                "storno_invoice",
                &storno_of(number),
                &format!("e2e-4-s3-{tipus}"),
            )
            .await;
        assert_eq!(rejected["outcome"], "rejected", "{tipus}: {rejected}");
        assert_eq!(rejected["code"], "not_stornoable", "{tipus}: {rejected}");
        assert_eq!(h.requests_seen().await, 1, "{tipus}");
    }

    // Before the fault: an already reversed invoice is `reversed`, with the
    // storno number from the hint.
    h.reset().await;
    number_query("SZ-4D")
        .respond_with(
            Doc {
                reversed: true,
                ..without_telj("SZ-4D", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-4")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-4D"),
                ..Doc::new("SS-4D", "SS", "E2E-4")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4D"), "e2e-4-s4")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4D", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-storno-SZ-4D",
            "hint-storno-SZ-4D"
        ]
    );
    assert_eq!(h.requests_seen().await, 2, "the verify and the hint");

    // A lost reply: the first execution's send answers 500 and its re-query
    // still misses; the second execution's send lands. Both sends carry the
    // same bytes — the same `teljesitesDatum` — under one run entry.
    h.reset().await;
    number_query("SZ-4E")
        .respond_with(Doc::new("SZ-4E", "SZ", "E2E-4").response())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-4:storno:SZ-4E")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(created("SS-4E", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-4", "storno_invoice", &storno_of("SZ-4E"), "e2e-4-s5")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-4E", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 2, "two executions of the storno step");
    assert_eq!(
        stornos[0], stornos[1],
        "the re-executed storno is byte-identical"
    );
    assert!(stornos[0].contains(&original_telj_tag()));
    assert!(!stornos[0].contains("<keltDatum>"));
    assert!(
        stornos[0].contains("<eszamla>true</eszamla>"),
        "an e-invoice original (eszamla 2) is reversed as an e-invoice, whatever the account default (paper): {}",
        stornos[0]
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "storno-SZ-4E").count(),
        1,
        "one storno step entry: {runs:?}"
    );
    eprintln!(
        "(iv-b) telj-less original → unavailable about the storno with nothing sent, after not_managed / not_stornoable / reversed; lost storno reply → byte-identical re-execution: pass"
    );
}

/// (iv-c) the storno step's answers at the order's handler (design §6 steps
/// 3–4): szamlazz.hu's typed refusals — 14 (a storno of a storno) and 221
/// (the invoice has a corrective) — are `rejected{code, message}`, settled
/// after one send that carries the caller's comment; every execution's send
/// losing its reply exhausts the issue policy into the structured
/// `outcome_unknown` naming the storno's identity, with the `storno-{number}`
/// run as the retried command and one send per execution; and the next call
/// — a new `Idempotency-Key` — is answered by the storno lookup (design §6
/// step 2) when the `SS` is under the storno external id while the verify
/// still reports the original live — szamlazz.hu's query surface behind the
/// storno that landed — `reversed` with the storno number and nothing sent,
/// through `verify-storno-{number}` and `lookup-storno-{number}` alone. (A
/// verify that already reports `sztornozott` answers before the lookup, from
/// the hint — (iv-b).)
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two typed refusals, the exhaustion, then the reconciling call"
)]
pub(crate) async fn storno_rejections_and_exhaustion_at_the_orders_handler(h: &Harness) {
    let with_comment = |number: &str| json!({ "invoice_number": number, "comment": "wrong buyer" });

    // 14 and 221: szamlazz.hu's typed refusals, settled after one send.
    for (code, message) in [
        ("14", "Sztornó számla nem sztornózható."),
        (
            "221",
            "Ez a számla nem sztornózható (van helyesbítő számlája).",
        ),
    ] {
        h.reset().await;
        h.holds(&Doc::new("SZ-44", "SZ", "E2E-44")).await;
        external_id_query("acct:E2E-44:storno:SZ-44")
            .respond_with(not_found())
            .mount(&h.mock)
            .await;
        storno_repeating_telj()
            .and(body_string_contains("<megjegyzes>wrong buyer</megjegyzes>"))
            .respond_with(api_error(code, message))
            .expect(1)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-44",
                "storno_invoice",
                &with_comment("SZ-44"),
                &format!("e2e-44-s{code}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{code}: {}", reply.body);
        let rejected = &reply.body;
        assert_eq!(rejected["outcome"], "rejected", "{code}: {rejected}");
        assert_eq!(rejected["code"], code, "{code}: {rejected}");
        assert_eq!(rejected["message"], message, "{code}: {rejected}");
        assert_eq!(rejected["invoice_number"], "SZ-44", "{code}");
        assert_eq!(rejected["storno_number"], Value::Null, "{code}");
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            [
                "namespace",
                "account",
                "verify-storno-SZ-44",
                "lookup-storno-SZ-44",
                "storno-SZ-44",
            ],
            "{code}"
        );
        assert_eq!(h.storno_bodies().await.len(), 1, "{code}: one send");
    }

    // Exhaustion: every execution's send loses its reply and the re-query
    // still misses.
    h.reset().await;
    h.holds(&Doc::new("SZ-45", "SZ", "E2E-45")).await;
    external_id_query("acct:E2E-45:storno:SZ-45")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&h.mock)
        .await;
    let started = Instant::now();
    let watch = h.watch("E2E-45");
    let reply = h
        .call("E2E-45", "storno_invoice", &storno_of("SZ-45"), "e2e-45-s1")
        .await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 500, "{}", reply.body);
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(60),
        "the issue policy's delay (1 s) was honoured, not the handler's: {elapsed:?}"
    );
    let fault = reply.fault();
    assert_eq!(fault.code, "outcome_unknown", "{fault:?}");
    assert!(fault.message.contains("storno step"), "{fault:?}");
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );
    assert_eq!(fault.order.as_deref(), Some("E2E-45"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("invoice"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-45:storno:SZ-45"),
        "{fault:?}"
    );
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["storno-SZ-45"],
        "the storno step is what retried: {retries:?}"
    );
    assert_eq!(
        h.storno_bodies().await.len(),
        2,
        "one send per execution of the storno step"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("outcome_unknown")),
        "{invocation:?}"
    );

    // The next call: the storno landed after all and its `SS` is under the
    // storno external id, while the verify still reports the original live
    // — so the lookup, not the verify, is what answers, and nothing is sent.
    h.reset().await;
    h.holds(&Doc::new("SZ-45", "SZ", "E2E-45")).await;
    external_id_query("acct:E2E-45:storno:SZ-45")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-45"),
                ..Doc::new("SS-45", "SS", "E2E-45")
            }
            .response(),
        )
        .expect(1)
        .mount(&h.mock)
        .await;
    storno_never_sent(&h.mock).await;
    let reply = h
        .call("E2E-45", "storno_invoice", &storno_of("SZ-45"), "e2e-45-s2")
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-45", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-45");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "verify-storno-SZ-45",
            "lookup-storno-SZ-45",
        ],
        "the lookup is what answered, no storno step"
    );
    assert_eq!(h.requests_seen().await, 2, "the verify and the lookup");
    eprintln!(
        "(iv-c) storno 14 / 221 → rejected{{code}} after one send; exhaustion → outcome_unknown about the storno; next call finds the SS → reversed, nothing sent: pass"
    );
}

/// (xviii) an order Restate has no memory of: `acme` issues an invoice, the
/// invocation is purged; `storno_invoice` → `reversed` (its lookup finds the
/// document by number on szamlazz.hu), purged; `create_invoice {reissue}` →
/// `issued` as the newest holder of the same external id. Nothing but the
/// order key and the scope was needed.
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
    h.purge(issued.invocation_id()).await;

    // Storno: the invoice is verified by number and reversed.
    h.reset().await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-18:invoice"),
        ..Doc::new("SZ-18", "SZ", "E2E-18")
    })
    .await;
    external_id_query("acct:E2E-18:storno:SZ-18")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
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
    h.purge(reversed.invocation_id()).await;
    assert!(
        h.journal(issued.invocation_id()).await.is_empty()
            && h.journal(reversed.invocation_id()).await.is_empty(),
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
                ..Doc::new("SZ-18", "SZ", "E2E-18")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-18")
        .respond_with(
            Doc {
                referenced_invoice: Some("SZ-18"),
                ..Doc::new("SS-18", "SS", "E2E-18")
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
    h.holds(&Doc {
        external_id: Some("acct:E2E-18:invoice"),
        ..Doc::new("SZ-18B", "SZ", "E2E-18")
    })
    .await;
    let status = h.get_scoped("acme", "E2E-18").await;
    assert_eq!(status["invoice"]["number"], "SZ-18B", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    eprintln!("(xviii) purged order → storno → reversed; purged → reissue → issued: pass");
}

/// (xviii-f) The storno's `eszamla` is the verified original's appearance,
/// not the account default — on both storno handlers. szamlazz.hu accepts a
/// mismatch silently and issues the storno in the *request's* form (P73), so
/// the derivation is the only thing keeping a reversal in its original's
/// form. `acme` is switched to issuing e-invoices by default; a **paper**
/// original (`<eszamla>1</eszamla>`) is still reversed with
/// `<eszamla>false</eszamla>` by `Szamlazz.Order.storno_invoice`, and — the
/// default switched back to paper — an **e-invoice** original (`3`, the code
/// szamlazz.hu was observed to report) is reversed with
/// `<eszamla>true</eszamla>` by `Szamlazz.Agent.storno`.
pub(crate) async fn storno_is_issued_in_the_originals_form_not_the_accounts_default(h: &Harness) {
    // A paper original under an account that defaults to e-invoices.
    h.reset().await;
    h.multi()
        .update("acme", |account| account.defaults.e_invoice = true);
    h.holds(&Doc {
        eszamla: Some(1),
        ..Doc::new("SZ-73", "SZ", "E2E-73")
    })
    .await;
    external_id_query("acct:E2E-73:storno:SZ-73")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains("<eszamla>false</eszamla>"))
        .respond_with(created("SS-73", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    storno()
        .and(body_string_contains("<eszamla>true</eszamla>"))
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-73",
            "storno_invoice",
            &storno_of("SZ-73"),
            "e2e-73-k1",
        )
        .await;
    h.multi()
        .update("acme", |account| account.defaults.e_invoice = false);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-73", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1, "{stornos:?}");
    assert!(
        stornos[0].contains("<eszamla>false</eszamla>"),
        "a paper original is reversed on paper under an e-invoice default: {}",
        stornos[0]
    );

    // An e-invoice original (`3`) under the paper default, by number.
    h.reset().await;
    h.holds(&Doc {
        eszamla: Some(3),
        ..Doc::unmanaged("SZ-74", "SZ")
    })
    .await;
    external_id_query("acct:by-number:SZ-74:storno")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    storno_repeating_telj()
        .and(body_string_contains("<eszamla>true</eszamla>"))
        .respond_with(created("SS-74", "-1000", "-1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    storno()
        .and(body_string_contains("<eszamla>false</eszamla>"))
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped("acme", "storno", &storno_of("SZ-74"))
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "reversed", "{}", reply.body);
    assert_eq!(reply.body["storno_number"], "SS-74", "{}", reply.body);
    let stornos = h.storno_bodies().await;
    assert_eq!(stornos.len(), 1, "{stornos:?}");
    assert!(
        stornos[0].contains("<eszamla>true</eszamla>"),
        "an e-invoice original is reversed as an e-invoice under a paper default: {}",
        stornos[0]
    );
    eprintln!(
        "(xviii-f) storno eszamla is the verified original's (1 → false under an e-invoice default; 3 → true under a paper default), on both handlers: pass"
    );
}
