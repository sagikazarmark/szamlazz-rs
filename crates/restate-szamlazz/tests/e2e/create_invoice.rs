//! `Szamlazz.Order.create_invoice`: issued and already issued, the
//! `Idempotency-Key` replay, two concurrent creates on one key and the same
//! `Idempotency-Key` sent while the first is in flight, the duplicate order
//! number (152) reconciled or
//! settled as a conflict, `reissue` on a live document, a reversal seen by
//! the lookup or between two executions of the create step, a lost create
//! reply settled by the immediate re-query, the proforma link and the
//! secondary-lookup collision.

use std::time::{Duration, Instant};

use rust_decimal::dec;
use serde_json::{Value, json};
use wiremock::ResponseTemplate;
use wiremock::matchers::body_string_contains;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::introspection::run_result;
use crate::harness::szamlazz::{
    Doc, api_error, create, created, duplicate_order_number, external_id_query, not_found,
    number_query, order_query,
};
use crate::harness::{Harness, create_body, document};

/// (i) create ⇒ `issued`; a second call with a **new** key ⇒
/// `already_issued` from the lookup step.
pub(crate) async fn issued_then_already_issued(h: &Harness) {
    h.reset().await;
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
            ..Doc::new("SZ-1", "SZ", "E2E-1")
        },
    )
    .await;
    create()
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
    assert_eq!(first.get("gen"), None);
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
        &runs[..3],
        ["namespace", "account", "exclusivity-prepayment"],
        "{runs:?}"
    );
    assert_eq!(
        runs.iter().filter(|name| **name == "account").count(),
        1,
        "one account entry per invocation: {runs:?}"
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

    let again = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k2",
        )
        .await;
    assert_eq!(again["outcome"], "already_issued", "{again}");
    assert_eq!(again["invoice_number"], "SZ-1");
    assert_eq!(again["gross_total"], "1270");
    assert_eq!(again["outstanding"], "1270");
    eprintln!("(i) issued → already_issued (new key): pass");
}

/// (ii) the same `Idempotency-Key` ⇒ the stored completion, byte for byte,
/// without a single call to szamlazz.hu.
pub(crate) async fn idempotency_key_replays_without_calling_szamlazz(h: &Harness) {
    let before = h.requests_seen().await;
    let replay = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-k1",
        )
        .await;
    assert_eq!(replay["outcome"], "issued", "{replay}");
    assert_eq!(replay["invoice_number"], "SZ-1");
    assert_eq!(
        h.requests_seen().await,
        before,
        "a replayed completion reaches neither the query nor the create mock"
    );
    // The create mock's `expect(1)` is verified by the next scenario's
    // `reset`.
    eprintln!("(ii) same key → identical response, no szamlazz.hu call: pass");
}

/// How long the create's reply is held back in the two concurrency
/// scenarios below: the window in which the first invocation is in flight
/// (its create is on the wire, unanswered) while the second call is made;
/// each scenario reads the first's `sys_invocation` row inside the window
/// and asserts it is not completed, so "in flight" is asserted, not assumed.
/// Well under the Számla Agent client's request timeout and the handler's
/// inactivity timeout, and the one cost these scenarios add to the run.
const IN_FLIGHT: Duration = Duration::from_secs(2);

/// (i-b) two `create_invoice` calls on the **same key under the same scope**,
/// concurrently, with distinct `Idempotency-Key`s: the Virtual Object's
/// per-key lock serialises them, so one is `issued` and the other
/// `already_issued` from its lookup step, in either order, with exactly one
/// create on the wire and both invocations completed. What the exactly-once
/// argument rests on; the cross-scope scenario (xvii) is the contrast (two
/// objects, two documents). While the first's create is on the wire, both
/// invocations are on the server and neither is completed: the second is
/// queued behind the lock, not attached to the first (that is the same key's
/// case, (ii-b)).
pub(crate) async fn concurrent_creates_on_one_key_issue_once(h: &Harness) {
    h.reset().await;
    h.absent("E2E-2", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-2")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The document is the holder from the moment the create is received, so
    // whichever call runs second finds it in its lookup, however the two
    // were ordered by the lock; the create's reply is held back so the first
    // invocation is in flight while the other is queued.
    h.create_lands_answering(
        &Doc {
            external_id: Some("acct:E2E-2:invoice"),
            ..Doc::new("SZ-2", "SZ", "E2E-2")
        },
        created("SZ-2", "1000", "1270").set_delay(IN_FLIGHT),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let both = async {
        tokio::join!(
            h.call("E2E-2", "create_invoice", &body, "e2e-2-k1"),
            h.call("E2E-2", "create_invoice", &body, "e2e-2-k2"),
        )
    };
    let during = async {
        h.wait_for_creates(1).await;
        // Both calls reached the server before the create did (they were
        // sent together, the create came after the first's reads), and
        // neither can complete while the create's reply is held back.
        let on_key = h.invocations_on("E2E-2").await;
        (on_key, h.create_bodies().await.len())
    };
    let ((first, second), (on_key, creates_during)) = tokio::join!(both, during);

    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(second.status, 200, "{}", second.body);
    let mut outcomes = [
        first.body["outcome"].as_str().unwrap_or_default(),
        second.body["outcome"].as_str().unwrap_or_default(),
    ];
    outcomes.sort_unstable();
    assert_eq!(
        outcomes,
        ["already_issued", "issued"],
        "one issued, one already issued, in either order: {} / {}",
        first.body,
        second.body
    );
    for reply in [&first, &second] {
        assert_eq!(reply.body["invoice_number"], "SZ-2", "{}", reply.body);
        assert_eq!(reply.body["external_id"], "acct:E2E-2:invoice");
    }
    assert_ne!(
        first.invocation_id(),
        second.invocation_id(),
        "two keys, two invocations"
    );
    assert_eq!(
        h.create_bodies().await.len(),
        1,
        "exactly one create on the wire"
    );

    assert_eq!(creates_during, 1, "the barrier fired on the one create");
    assert_eq!(
        on_key.len(),
        2,
        "both invocations were on the server while the create was on the wire: {on_key:?}"
    );
    assert!(
        on_key.iter().all(|(_, row)| row.status != "completed"),
        "neither had completed while the create's reply was held back: {on_key:?}"
    );

    // Both completed; the `issued` one walked the whole path, the
    // `already_issued` one stopped at its lookup, both prefixes of the pinned
    // `create_invoice` path.
    for reply in [&first, &second] {
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.status, "completed", "{invocation:?}");
        let runs = h.runs(reply.invocation_id()).await;
        let expected_last = if reply.body["outcome"] == "issued" {
            "create-invoice"
        } else {
            "lookup-invoice"
        };
        assert_eq!(
            runs.last().map(String::as_str),
            Some(expected_last),
            "{}: {runs:?}",
            reply.body["outcome"]
        );
    }
    eprintln!(
        "(i-b) two concurrent creates on one key → one issued, one already_issued, one create on the wire: pass"
    );
}

/// (ii-b) the **same** `Idempotency-Key` sent while the first invocation is
/// still in flight (its create is on the wire, unanswered) attaches to that
/// invocation and receives its outcome: one invocation id on both replies,
/// one create on the wire, one `sys_invocation` row on the key. The "no
/// answer" half of the `Idempotency-Key` rule: a caller that timed out keeps
/// its key, and the retry gets the in-flight outcome instead of queueing a
/// second invocation behind the lock (which a new key would, (i-b)). The
/// completed half is (ii).
pub(crate) async fn same_idempotency_key_in_flight_attaches_to_the_invocation(h: &Harness) {
    h.reset().await;
    h.absent("E2E-2B", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-2B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_answering(
        &Doc {
            external_id: Some("acct:E2E-2B:invoice"),
            ..Doc::new("SZ-2B", "SZ", "E2E-2B")
        },
        created("SZ-2B", "1000", "1270").set_delay(IN_FLIGHT),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let first = h.call("E2E-2B", "create_invoice", &body, "e2e-2b-shared");
    let retry = async {
        // The first is in flight for `IN_FLIGHT` from here: its create is on
        // the wire, its reply held back. Read its row before the retry, so
        // "in flight" is asserted, not assumed: a retry after completion
        // would be the replay of (ii), which every assertion below also
        // holds for.
        h.wait_for_creates(1).await;
        let in_flight = h.invocations_on("E2E-2B").await;
        let sent = Instant::now();
        let reply = h
            .call("E2E-2B", "create_invoice", &body, "e2e-2b-shared")
            .await;
        (in_flight, sent.elapsed(), reply)
    };
    let (first, (in_flight, waited, retry)) = tokio::join!(first, retry);

    assert_eq!(first.status, 200, "{}", first.body);
    assert_eq!(retry.status, 200, "{}", retry.body);
    assert_eq!(first.body["outcome"], "issued", "{}", first.body);
    assert_eq!(first.body["invoice_number"], "SZ-2B");
    assert_eq!(
        in_flight.len(),
        1,
        "one invocation on the key when the retry was sent: {in_flight:?}"
    );
    assert_eq!(in_flight[0].0, first.invocation_id());
    assert_ne!(
        in_flight[0].1.status, "completed",
        "the retry was sent while the first was in flight: {:?}",
        in_flight[0].1
    );
    assert!(
        waited >= IN_FLIGHT / 2,
        "the retry waited for the in-flight outcome rather than reading a stored one: {waited:?}"
    );
    assert_eq!(
        retry.body, first.body,
        "the retry received the in-flight invocation's outcome"
    );
    assert_eq!(
        retry.invocation_id(),
        first.invocation_id(),
        "the same key while in flight attaches: one invocation"
    );
    assert_eq!(
        h.create_bodies().await.len(),
        1,
        "exactly one create on the wire"
    );
    let on_key = h.invocations_on("E2E-2B").await;
    assert_eq!(
        on_key.len(),
        1,
        "one invocation on the key, not a second queued behind the lock: {on_key:?}"
    );
    assert_eq!(on_key[0].0, first.invocation_id());
    assert_eq!(on_key[0].1.status, "completed", "{:?}", on_key[0].1);
    eprintln!(
        "(ii-b) same Idempotency-Key while in flight → attached: one invocation id, one create on the wire: pass"
    );
}

/// (iii) 152 on create, then the external-id re-query finds the document ⇒
/// `reconciled`.
pub(crate) async fn duplicate_order_number_reconciles(h: &Harness) {
    h.reset().await;
    h.absent("E2E-3", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-3")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // Lookup and the create step's leading query miss; the re-query after
    // the 152 finds the document.
    h.holds_after_misses(
        2,
        &Doc {
            external_id: Some("acct:E2E-3:invoice"),
            ..Doc::new("SZ-3", "SZ", "E2E-3")
        },
    )
    .await;
    create()
        .respond_with(api_error("152", "duplicate"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let reconciled = h
        .ok(
            "E2E-3",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-3-k1",
        )
        .await;
    assert_eq!(reconciled["outcome"], "reconciled", "{reconciled}");
    assert_eq!(reconciled["invoice_number"], "SZ-3");
    eprintln!("(iii) 152 + ext-id re-query → reconciled: pass");
}

/// (iii-b) a duplicate-order-number answer (152) whose external-id re-query
/// finds nothing of ours is a **settled** `conflict{duplicate_order_number}`
/// after one send (#41): szamlazz.hu refused the order number, so re-sending
/// would only repeat the refusal; never `Unconfirmed`, so no run retry is
/// spent on it. With nothing under the order at all (the contradiction, logged
/// at `warn`) the conflict names no `existing_number`; with a live invoice
/// another channel issued between our lookup step and our create (the very
/// race the rule exists for; the hint missed, the naming query after the 152
/// finds it), the conflict names it.
pub(crate) async fn duplicate_order_number_with_nothing_of_ours_is_a_settled_conflict(h: &Harness) {
    // Nothing under the order at all.
    h.reset().await;
    h.absent("E2E-3B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-3B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(duplicate_order_number("E2E-3B"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let watch = h.watch("E2E-3B");
    let reply = h
        .call(
            "E2E-3B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-3b-k1",
        )
        .await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(
        conflict["conflict_reason"], "duplicate_order_number",
        "{conflict}"
    );
    assert_eq!(conflict["existing_number"], Value::Null, "{conflict}");
    assert_eq!(conflict["code"], "152", "{conflict}");
    assert!(
        conflict["message"]
            .as_str()
            .is_some_and(|message| message.contains("E2E-3B")),
        "{conflict}"
    );
    // Settled inside the one execution: no run failed.
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert_eq!(h.create_bodies().await.len(), 1, "one send, no re-send");
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

    // A live invoice of another channel, issued between the lookup and the
    // create: the hint missed, the naming query finds it.
    h.reset().await;
    h.absent("E2E-3C", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-3C")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    order_query("E2E-3C")
        .respond_with(Doc::new("SZ-3C-OTHER", "SZ", "E2E-3C").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(duplicate_order_number("E2E-3C"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-3C",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-3c-k1",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(
        conflict["conflict_reason"], "duplicate_order_number",
        "{conflict}"
    );
    assert_eq!(conflict["existing_number"], "SZ-3C-OTHER", "{conflict}");
    assert_eq!(conflict["code"], "152");
    assert_eq!(h.create_bodies().await.len(), 1, "one send");
    eprintln!(
        "(iii-b) 152 with nothing of ours → conflict{{duplicate_order_number}} settled after one send, existing_number null / the other channel's invoice: pass"
    );
}

/// (v) `reissue: true` while the document is live ⇒ `conflict{live}`.
pub(crate) async fn reissue_on_live_is_a_conflict(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-2", "SZ", "E2E-1")
    })
    .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), true),
            "e2e-1-k5",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "live");
    assert_eq!(conflict["existing_number"], "SZ-2");
    eprintln!("(v) reissue on a live document → conflict{{live}}: pass");
}

/// (vi) the lookup returns `<sztornozott>true</sztornozott>` (a UI storno)
/// ⇒ `reversed`, storno number unknown.
pub(crate) async fn external_reversal_detected(h: &Harness) {
    h.reset().await;
    h.absent("E2E-6", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-6")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-6", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-6",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6-k1",
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");

    h.reset().await;
    h.absent("E2E-6", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-6:invoice")
        .respond_with(
            Doc {
                reversed: true,
                ..Doc::new("SZ-6", "SZ", "E2E-6")
            }
            .response(),
        )
        .mount(&h.mock)
        .await;
    order_query("E2E-6")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let detected = h
        .ok(
            "E2E-6",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6-k2",
        )
        .await;
    assert_eq!(detected["outcome"], "reversed", "{detected}");
    assert_eq!(detected["invoice_number"], "SZ-6");
    assert_eq!(detected["storno_number"], Value::Null);
    eprintln!("(vi) sztornozott on the lookup → reversed: pass");
}

/// (vi-b) the hole #36 closes, end to end: the lookup sees nothing, the
/// first execution of the create step sends (the document lands, the reply
/// is lost, the immediate re-query still misses), and before the run policy
/// re-executes the step the document is reversed in the szamlazz.hu UI. The
/// second execution's leading query finds it reversed and **does not send
/// again**: `outcome: reversed`, exactly one create on the wire.
pub(crate) async fn reversal_between_executions_is_reversed_not_reissued(h: &Harness) {
    h.reset().await;
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
            ..Doc::new("SZ-6B", "SZ", "E2E-6B")
        },
    )
    .await;
    create()
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
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        runs.iter()
            .filter(|name| **name == "create-invoice")
            .count(),
        1,
        "one create step entry: {runs:?}"
    );
    eprintln!(
        "(vi-b) document reversed between two executions of the create step → reversed, one create on the wire: pass"
    );
}

/// (vi-c) the create lands but its reply is lost: the create step's immediate
/// re-query finds the document under the external id and settles the step as
/// `issued` **within the same execution**, no run retry, exactly one create on
/// the wire, one create step entry. The harness drives the transition from the
/// create request itself: how many queries precede the send is the protocol's,
/// not the test's, to know.
pub(crate) async fn lost_create_reply_is_settled_by_the_immediate_requery(h: &Harness) {
    h.reset().await;
    h.absent("E2E-6C", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-6C")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_but_reply_lost(&Doc {
        external_id: Some("acct:E2E-6C:invoice"),
        ..Doc::new("SZ-6C", "SZ", "E2E-6C")
    })
    .await;

    let watch = h.watch("E2E-6C");
    let reply = h
        .call(
            "E2E-6C",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-6c-k1",
        )
        .await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let response = &reply.body;
    assert_eq!(response["outcome"], "issued", "{response}");
    assert_eq!(response["invoice_number"], "SZ-6C");
    assert_eq!(response["external_id"], "acct:E2E-6C:invoice");
    assert_eq!(response["gross_total"], "1270");

    // Settled inside the one execution: no run failed, so no failure and no
    // failing command were recorded, and `retry_count` stayed at the first
    // execution's 1 (the server's count includes it: (xiv) observed
    // failures + 1).
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert!(retries.failing_commands.is_empty(), "{retries:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "create-invoice").count(),
        1,
        "one create step entry: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    eprintln!(
        "(vi-c) create landed, reply lost, immediate re-query finds it → issued in one execution, one create on the wire: pass"
    );
}

/// (vii-b) `options.proforma: {number}` validates the named proforma like
/// every other document found by number: a proforma carrying another order's
/// number, or none, is `conflict{not_managed, existing_number}` after the
/// verify alone (another order's live proforma cannot be linked into this
/// order's invoice), nothing sent; a proforma of this order proceeds as before
/// and the create carries `dijbekeroSzamlaszam`, whatever its `teszt` says,
/// since the worker holds no account pin.
pub(crate) async fn proforma_by_number_is_checked_like_every_found_document(h: &Harness) {
    let body = |number: &str| {
        json!({
            "document": document(dec!(1000)),
            "options": { "proforma": { "number": number } },
        })
    };

    // Another order's proforma, and one carrying no order number at all.
    for (number, order) in [("D-31", Some("E2E-31")), ("D-32", None)] {
        h.reset().await;
        h.absent("E2E-30", &["prepayment", "final"]).await;
        number_query(number)
            .respond_with(
                Doc {
                    order,
                    ..Doc::unmanaged(number, "D")
                }
                .response(),
            )
            .expect(1)
            .mount(&h.mock)
            .await;
        create()
            .respond_with(created("SZ-30", "1000", "1270"))
            .expect(0)
            .mount(&h.mock)
            .await;
        let reply = h
            .call(
                "E2E-30",
                "create_invoice",
                &body(number),
                &format!("e2e-30-{number}"),
            )
            .await;
        assert_eq!(reply.status, 200, "{number}: {}", reply.body);
        let response = &reply.body;
        assert_eq!(response["outcome"], "conflict", "{number}: {response}");
        assert_eq!(
            response["conflict_reason"], "not_managed",
            "{number}: {response}"
        );
        assert_eq!(response["existing_number"], number, "{number}: {response}");
        assert_eq!(response["kind"], "invoice", "{number}: {response}");
        assert_eq!(
            response["external_id"], "acct:E2E-30:invoice",
            "{number}: {response}"
        );
        assert_eq!(
            h.runs(reply.invocation_id()).await,
            [
                "namespace",
                "account",
                "exclusivity-prepayment",
                "exclusivity-final",
                &format!("verify-proforma-{number}")
            ],
            "{number}: the verify is the last step journaled"
        );
        assert_eq!(
            h.requests_seen().await,
            3,
            "{number}: the two exclusivity lookups and the verify, nothing else"
        );
    }

    // A proforma of this order: the create proceeds and carries
    // `dijbekeroSzamlaszam`. Its `teszt` says a live account issued it;
    // nothing compares that with anything.
    h.reset().await;
    h.absent("E2E-30", &["prepayment", "final", "invoice"])
        .await;
    h.holds(&Doc {
        test: false,
        ..Doc::new("D-33", "D", "E2E-30")
    })
    .await;
    create()
        .and(body_string_contains(
            "<dijbekeroSzamlaszam>D-33</dijbekeroSzamlaszam>",
        ))
        .respond_with(created("SZ-30", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let invoice = h
        .ok("E2E-30", "create_invoice", &body("D-33"), "e2e-30-k4")
        .await;
    assert_eq!(invoice["outcome"], "issued", "{invoice}");
    assert_eq!(invoice["invoice_number"], "SZ-30");
    eprintln!(
        "(vii-b) proforma by number: another order's or an order-less proforma → conflict{{not_managed}} with nothing sent; this order's → issued with dijbekeroSzamlaszam, its teszt compared with nothing: pass"
    );
}

/// (ix) a valid-looking document under `…:prepayment` that carries another
/// order's number (an external-id collision on a *secondary* lookup) ⇒
/// `conflict{external_id_collision}` from `create_invoice`, nothing created:
/// the newest holder may hide a live prepayment of ours behind it. `get`
/// reports the same slot as absent (a read must not fail).
pub(crate) async fn secondary_lookup_collision_refuses_to_create(h: &Harness) {
    h.reset().await;
    h.absent("E2E-9", &["proforma", "invoice", "final"]).await;
    // Another order's prepayment invoice under our prepayment id.
    h.holds(&Doc {
        external_id: Some("acct:E2E-9:prepayment"),
        ..Doc::new("ES-X", "ES", "OTHER-ORDER")
    })
    .await;
    order_query("E2E-9")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let conflict = h
        .ok(
            "E2E-9",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-9-k1",
        )
        .await;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "external_id_collision");
    assert_eq!(conflict["existing_number"], "ES-X");
    assert_eq!(conflict["kind"], "invoice");
    assert_eq!(conflict["external_id"], "acct:E2E-9:invoice");

    let status = h.get("E2E-9").await;
    assert_eq!(status["prepayment"], Value::Null, "{status}");
    assert_eq!(status["invoice"], Value::Null);
    eprintln!("(ix) collision on the prepayment lookup → conflict{{external_id_collision}}: pass");
}

/// (x-g) `create_invoice`'s proforma link settles every case before anything
/// is sent: `options.proforma: none` while a live proforma of ours exists is
/// `conflict{proforma_live, existing_number}` from the `proforma-link` read;
/// szamlazz.hu would link the proforma by shared order number regardless, so
/// refusing is the honest answer; a named proforma szamlazz.hu does not know
/// (code 7) is `conflict{proforma_missing, existing_number}` after the verify
/// (an outcome, never `not_found`); and a named document of this order that is
/// not a proforma is the `invalid_input` fault naming the number and its
/// `tipus`. (The prepayment invoice's `none` is (x); the by-number
/// `not_managed` is (vii-b).)
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the conflict, the missing proforma and the invalid link"
)]
pub(crate) async fn the_invoices_proforma_link_is_settled_before_any_send(h: &Harness) {
    let by_number = |number: &str| {
        json!({
            "document": document(dec!(1000)),
            "options": { "proforma": { "number": number } },
        })
    };

    // `none` while a live proforma of ours exists.
    h.reset().await;
    h.absent("E2E-42", &["prepayment", "final"]).await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-42:proforma"),
        ..Doc::new("D-42", "D", "E2E-42")
    })
    .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-42",
            "create_invoice",
            &json!({ "document": document(dec!(1000)), "options": { "proforma": "none" } }),
            "e2e-42-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(conflict["conflict_reason"], "proforma_live", "{conflict}");
    assert_eq!(conflict["existing_number"], "D-42");
    assert_eq!(conflict["kind"], "invoice");
    assert_eq!(conflict["external_id"], "acct:E2E-42:invoice");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
        ],
        "the proforma link is what refused, nothing after it"
    );
    assert_eq!(
        h.requests_seen().await,
        3,
        "the two exclusivity lookups and the link, nothing else"
    );

    // A named proforma szamlazz.hu does not know.
    h.reset().await;
    h.absent("E2E-42", &["prepayment", "final"]).await;
    number_query("D-MISSING")
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-42",
            "create_invoice",
            &by_number("D-MISSING"),
            "e2e-42-k2",
        )
        .await;
    assert_eq!(
        reply.status, 200,
        "an outcome, not not_found: {}",
        reply.body
    );
    let conflict = &reply.body;
    assert_eq!(conflict["outcome"], "conflict", "{conflict}");
    assert_eq!(
        conflict["conflict_reason"], "proforma_missing",
        "{conflict}"
    );
    assert_eq!(conflict["existing_number"], "D-MISSING");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "verify-proforma-D-MISSING",
        ]
    );
    assert_eq!(h.requests_seen().await, 3);

    // A named document of this order that is not a proforma.
    h.reset().await;
    h.absent("E2E-42", &["prepayment", "final"]).await;
    number_query("SZ-42")
        .respond_with(Doc::new("SZ-42", "SZ", "E2E-42").response())
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call("E2E-42", "create_invoice", &by_number("SZ-42"), "e2e-42-k3")
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "invalid_input", "{fault:?}");
    assert!(
        fault.message.contains("SZ-42 is not a proforma (tipus SZ)"),
        "names the number and its tipus: {fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "verify-proforma-SZ-42",
        ]
    );
    assert_eq!(h.requests_seen().await, 3, "nothing sent");
    eprintln!(
        "(x-g) create_invoice: none with a live D → conflict{{proforma_live}}; {{number}} on 7 → conflict{{proforma_missing}}; {{number}} of an SZ → invalid_input; nothing sent: pass"
    );
}
