//! `Szamlazz.Order.create_invoice` under Restate: the durable sequence of a
//! first create and its `already_issued` twin, the `Idempotency-Key` replaying
//! the stored completion, and the create step's leading query on a
//! **re-executed** closure meeting a reversal that happened between the two
//! executions. The decisions the sequence takes (`decide_lookup`,
//! `respond_to`, the duplicate order number, the proforma link) are unit
//! tests of `service::create`; the wire of the create step is
//! `tests/gateway.rs`.

use restate_e2e_harness::run_result;
use rust_decimal::dec;
use serde_json::Value;
use wiremock::ResponseTemplate;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{Doc, create_for, created, not_found, order_query};
use crate::harness::{Harness, create_body};

/// The first create of an order is `issued` through the full create path
/// (`namespace`, `account`, the two exclusivity reads, the proforma link, the
/// lookup, the create step), with exactly one `account` entry whose journaled
/// account carries its id and never the agent key; a second call with a
/// **new** `Idempotency-Key` is `already_issued` from the lookup step, with no
/// second send; and the **same** `Idempotency-Key` replays the stored
/// completion, byte for byte, without a single request to szamlazz.hu: the
/// create mock's `expect(1)` holds over all three calls.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the first create, the new key, the same key"
)]
pub(crate) async fn issued_already_issued_and_the_key_replays(h: &Harness) {
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step and the create step's own leading query both miss;
    // the second call's lookup finds the document.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-1:invoice"),
            ..Doc::of("SZ-1", "SZ", "E2E-1")
        },
    )
    .await;
    create_for("E2E-1")
        .respond_with(created("SZ-1", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let first = &reply.body;
    assert_eq!(first["outcome"], "issued", "{first}");
    assert_eq!(first["invoice_number"], "SZ-1");
    assert_eq!(first["kind"], "invoice");
    assert_eq!(first["external_id"], "acct:E2E-1:invoice");
    assert_eq!(first["gross_total"], "1270");
    assert_eq!(first.get("request_id"), None);

    // The prologue: the namespace pin and exactly one `account` entry, both
    // before the operation's first step; the journaled account carries its
    // id and never the agent key.
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
            "create-invoice",
        ],
        "{runs:?}"
    );
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains("\"id\":\"acct\""),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );
    assert!(
        !journal.iter().any(|entry| entry.raw_contains(AGENT_KEY)),
        "the agent key is in no journal entry"
    );

    // A new key: the lookup finds the document, no create step runs.
    let again = h
        .call(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k2",
        )
        .await;
    assert_eq!(again.status, 200, "{}", again.body);
    assert_eq!(again.body["outcome"], "already_issued", "{}", again.body);
    assert_eq!(again.body["invoice_number"], "SZ-1");
    assert_eq!(again.body["gross_total"], "1270");
    assert_eq!(again.body["outstanding"], "1270");
    assert_eq!(
        h.runs(again.invocation_id())
            .await
            .last()
            .map(String::as_str),
        Some("lookup-invoice"),
        "the lookup answered; no create step"
    );

    // The same key: the stored completion, no request of this order.
    let before = h.requests_mentioning("E2E-1").await.len();
    let stored = h
        .call(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(stored.status, 200, "{}", stored.body);
    assert_eq!(stored.body, *first, "the stored completion, byte for byte");
    assert_eq!(
        stored.invocation_id(),
        reply.invocation_id(),
        "the same invocation answered"
    );
    assert_eq!(
        h.requests_mentioning("E2E-1").await.len(),
        before,
        "a replayed completion reaches neither the query nor the create mock"
    );
    assert_eq!(
        h.create_bodies_of("E2E-1").await.len(),
        1,
        "one send in all"
    );
}

/// The hole #36 closes, end to end: the lookup sees nothing, the first
/// execution of the create step sends (the document lands, the reply is lost,
/// the immediate re-query still misses), and before the run policy
/// re-executes the step the document is reversed in the szamlazz.hu UI. The
/// second execution's leading query, inside the re-executed closure, finds it
/// reversed and **does not send again**: `outcome: reversed`, exactly one
/// create on the wire, one `create-invoice` entry in the journal. What the
/// gateway's table pins for one execution, here across two executions of one
/// journaled step.
pub(crate) async fn reversal_between_executions_is_reversed_not_reissued(h: &Harness) {
    h.absent("E2E-6B", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-6B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step, the first execution's leading query and its re-query
    // miss; the second execution's leading query finds the document
    // reversed.
    h.holds_after_misses(
        3,
        &Doc {
            external_id: Some("acct:E2E-6B:invoice"),
            reversed: true,
            ..Doc::of("SZ-6B", "SZ", "E2E-6B")
        },
    )
    .await;
    create_for("E2E-6B")
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reply = h
        .call(
            "E2E-6B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6b-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let response = &reply.body;
    assert_eq!(response["outcome"], "reversed", "{response}");
    assert_eq!(response["invoice_number"], "SZ-6B");
    assert_eq!(response["storno_number"], Value::Null);

    // The create step was re-executed (one run retry) and journaled once.
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "create-invoice").count(),
        1,
        "one create step entry: {runs:?}"
    );
    assert_eq!(
        h.create_bodies_of("E2E-6B").await.len(),
        1,
        "one create on the wire"
    );
}
