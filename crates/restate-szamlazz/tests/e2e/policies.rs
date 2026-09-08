//! The issue and read policies at the two durable steps of issuing, driven
//! through `create_invoice`: an exhausted create step as a structured
//! `outcome_unknown` and the next call's `already_issued`, a flaky and an
//! exhausted lookup read, and a szamlazz.hu code answered to the create
//! step's leading query or to the lookup's hint (#63).

use std::time::{Duration, Instant};

use rust_decimal::dec;
use wiremock::ResponseTemplate;

use crate::harness::szamlazz::{
    Doc, api_error, create, created, external_id_query, not_found, order_query,
};
use crate::harness::{Harness, create_body};

/// (xi) every execution of the create step loses its reply and the re-query
/// finds nothing ⇒ the run retry policy re-executes the step (one second
/// later under the test policy, not the handler's two-minute
/// `initial_interval`), and its exhaustion is a structured `outcome_unknown`
/// fault naming the order, kind and external id. That run retries spend
/// none of the handler's `invocation_retry_policy` attempts is (xi-e)'s
/// proof; here `retry_count` is only checked to have moved.
pub(crate) async fn exhausted_create_step_is_a_structured_outcome_unknown(h: &Harness) {
    h.reset().await;
    h.absent("E2E-11", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-11")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let watch = h.watch("E2E-11");
    let reply = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 500, "{}", reply.body);
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(60),
        "the run policy's delay (1 s initial) was honoured, not the handler's: {elapsed:?}"
    );

    // The ingress wraps the handler's terminal error; the fault is the JSON
    // in its message.
    let fault = reply.fault();
    assert_eq!(fault.code, "outcome_unknown", "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-11"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-11:invoice"));
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    // The run's re-execution is visible while the invocation is in flight:
    // `retry_count` (the invoker's count of starts) counts it, with the
    // create step named as the failing command, and the completed invocation
    // carries the structured fault.
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["create-invoice"],
        "the run, not the handler, is what retried: {retries:?}"
    );
    assert!(
        retries
            .failures
            .iter()
            .all(|failure| failure.contains("transport failure")),
        "the last failure is the Unconfirmed message: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("outcome_unknown")),
        "{invocation:?}"
    );
    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert!(
        runs.contains(&"lookup-invoice") && runs.contains(&"create-invoice"),
        "the two steps are journaled by name: {runs:?}"
    );
    eprintln!("(xi) exhausted create step → structured outcome_unknown; run retries visible: pass");
}

/// (xi-a) what `outcome_unknown` asks the caller to do works: the next call
/// on the same order with a **new** `Idempotency-Key`, after (xi) left the
/// outcome of `E2E-11`'s create unknown, finds the document that landed
/// after all under its external id and answers `already_issued` from the
/// lookup step, with nothing sent; the same key would replay (xi)'s fault.
/// (`reconciled` is the create step's own answer to a 152, see (iii); a lookup
/// that finds the document never reaches the create step.)
pub(crate) async fn after_an_outcome_unknown_the_next_call_answers_already_issued(h: &Harness) {
    h.reset().await;
    h.absent("E2E-11", &["prepayment", "final", "proforma"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-11:invoice"),
        ..Doc::new("SZ-11", "SZ", "E2E-11")
    })
    .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    // The same key: the stored fault, nothing read.
    let before = h.requests_seen().await;
    let replayed = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k1",
        )
        .await;
    assert_eq!(replayed.status, 500, "{}", replayed.body);
    assert_eq!(replayed.fault().code, "outcome_unknown");
    assert_eq!(
        h.requests_seen().await,
        before,
        "a replayed completion reaches nothing"
    );

    // A new key: the lookup finds what landed.
    let reply = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "already_issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-11");
    assert_eq!(reply.body["external_id"], "acct:E2E-11:invoice");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
        ],
        "the lookup answered; no create step"
    );
    assert!(h.create_bodies().await.is_empty(), "nothing sent");
    eprintln!(
        "(xi-a) after outcome_unknown: the same key replays the fault; a new key → already_issued from the lookup, nothing sent: pass"
    );
}

/// (xi-b) a read that szamlazz.hu fails to answer once is retried by the
/// **read policy**, not failed terminally: the lookup step's external-id
/// query answers 500 to its first execution and code 7 afterwards; the create
/// completes `issued` in one invocation, the `lookup-invoice` run is what
/// retried (`last_failure_related_command_name` while in flight), and the
/// create mock sees exactly one request.
pub(crate) async fn flaky_lookup_read_is_retried_by_the_read_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-27", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-27")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The first execution of the lookup step loses its reply; the second,
    // and the create step's own leading query, miss cleanly.
    h.loses_reply_once("acct:E2E-27:invoice").await;
    external_id_query("acct:E2E-27:invoice")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-27", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let watch = h.watch("E2E-27");
    let reply = h
        .call(
            "E2E-27",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-27-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-27");
    assert!(
        elapsed < Duration::from_secs(60),
        "the read policy's delay was honoured, not the handler's: {elapsed:?}"
    );

    // The run, not the handler, is what retried, and it was the lookup.
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["lookup-invoice"],
        "the lookup step is the failing command: {retries:?}"
    );
    assert!(
        retries
            .failures
            .iter()
            .all(|failure| failure.contains("transport failure")),
        "the last failure is the Unanswered message: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
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
        "one journal entry per step; the retried read is one entry: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 1, "exactly one create");
    eprintln!("(xi-b) flaky lookup read → retried by the read policy, issued once: pass");
}

/// (xi-c) a read that szamlazz.hu never answers is, after the read policy
/// is exhausted, a structured `unavailable` (503) naming the order, kind and
/// external id (within the read policy's delays), and the create mock sees
/// zero requests.
pub(crate) async fn exhausted_lookup_read_is_a_structured_unavailable(h: &Harness) {
    h.reset().await;
    h.absent("E2E-28", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-28")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-28:invoice")
        .respond_with(ResponseTemplate::new(500))
        .expect(3)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-28", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let started = Instant::now();
    let watch = h.watch("E2E-28");
    let reply = h
        .call(
            "E2E-28",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-28-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert!(
        elapsed < Duration::from_secs(60),
        "three executions one second apart, not the handler's policy: {elapsed:?}"
    );

    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-28"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-28:invoice"));
    assert!(fault.message.contains("lookup-invoice"), "{fault:?}");
    assert!(fault.message.contains("transport failure"), "{fault:?}");
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["lookup-invoice"],
        "the lookup step is the failing command: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert!(
        runs.contains(&"lookup-invoice".to_owned()),
        "the lookup is journaled by name: {runs:?}"
    );
    assert!(
        !runs.contains(&"create-invoice".to_owned()),
        "the create step never ran: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 0, "nothing was created");
    eprintln!("(xi-c) exhausted lookup read → structured unavailable, nothing created: pass");
}

/// (xi-c') an *answer* to the create step's leading query that is neither 7
/// nor a credential code (here 57) is settled data, not `Unconfirmed`
/// (#63): the handler answers the structured `unavailable` (503) at once with
/// the code beside it, the `create-invoice` run is journaled as data with no
/// failure and no failing command recorded (the issue policy is not spent on
/// a read), and the create mock sees zero requests. The lookup step's own
/// query misses cleanly so that the create step is reached.
pub(crate) async fn answered_code_on_the_create_leading_query_is_an_immediate_unavailable(
    h: &Harness,
) {
    h.reset().await;
    h.absent("E2E-29", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-29")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The lookup step's query: code 7. The create step's leading query, the
    // next query of the same id: code 57. Two queries in all.
    external_id_query("acct:E2E-29:invoice")
        .respond_with(not_found())
        .up_to_n_times(1)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-29:invoice")
        .respond_with(api_error("57", "Hibás XML."))
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-29", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let watch = h.watch("E2E-29");
    let reply = h
        .call(
            "E2E-29",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-29-k1",
        )
        .await;
    let retries = watch.finish().await;
    assert_eq!(reply.status, 503, "{}", reply.body);

    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-29"));
    assert_eq!(fault.kind.as_deref(), Some("invoice"));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-29:invoice"));
    assert!(fault.message.contains("code 57"), "{fault:?}");
    assert!(
        fault.message.contains("retry with a new Idempotency-Key"),
        "{fault:?}"
    );

    // Settled inside the one execution: no run failed, so no failure and no
    // failing command were recorded, and `retry_count` stayed at the first
    // execution's 1 (the server's count includes it, as (vi-c) observed);
    // the answer was data, not `Unconfirmed`.
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert!(retries.failing_commands.is_empty(), "{retries:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert!(
        runs.contains(&"lookup-invoice".to_owned()) && runs.contains(&"create-invoice".to_owned()),
        "the create step ran and journaled the answer: {runs:?}"
    );
    assert_eq!(h.create_bodies().await.len(), 0, "nothing was created");
    eprintln!(
        "(xi-c') answered code on the create step's leading query → immediate unavailable{{szamlazz_code}}, nothing created: pass"
    );
}

/// (xi-c'') the two queries of the lookup step answer a szamlazz.hu code
/// differently (`Gateway::lookup` steps 1–2). The order-number **hint**
/// answered with a code that is neither 7 nor a credential code (here 57)
/// is data the hint cannot conclude from: it looks for a foreign document,
/// and a code says nothing about one, so the lookup continues as on a miss
/// and the create proceeds to `issued` in one execution, no run failure
/// recorded (the read policy is not spent on an answer), the hint queried
/// exactly once, one create on the wire. The **external-id** query answered
/// with the same code is the lookup's own `unavailable` (503) at once, with
/// the code beside it and nothing sent; the hint is never asked.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the same code on the hint, then on the external-id query"
)]
pub(crate) async fn an_answered_code_on_the_hint_is_inconclusive_and_the_create_proceeds(
    h: &Harness,
) {
    // The hint.
    h.reset().await;
    h.absent("E2E-29B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-29B")
        .respond_with(api_error("57", "Hibás XML."))
        .expect(1)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-29B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let watch = h.watch("E2E-29B");
    let reply = h
        .call(
            "E2E-29B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-29b-k1",
        )
        .await;
    let retries = watch.finish().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-29B");
    assert!(retries.max_retry_count <= 1, "{retries:?}");
    assert!(retries.failures.is_empty(), "{retries:?}");
    assert!(retries.failing_commands.is_empty(), "{retries:?}");
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
    assert_eq!(
        h.requests_seen().await,
        7,
        "two exclusivity lookups, the link, the external id, the hint, the leading query and the create"
    );

    // The external-id query.
    h.reset().await;
    h.absent("E2E-29C", &["prepayment", "final", "proforma"])
        .await;
    external_id_query("acct:E2E-29C:invoice")
        .respond_with(api_error("57", "Hibás XML."))
        .expect(1)
        .mount(&h.mock)
        .await;
    order_query("E2E-29C")
        .respond_with(not_found())
        .expect(0)
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-29C",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-29c-k1",
        )
        .await;
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert_eq!(fault.szamlazz_code.as_deref(), Some("57"), "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-29C"), "{fault:?}");
    assert_eq!(fault.kind.as_deref(), Some("invoice"), "{fault:?}");
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-29C:invoice"),
        "{fault:?}"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
        ],
        "the lookup answered as data; no create step"
    );
    assert_eq!(
        h.requests_seen().await,
        4,
        "the three reads before the lookup and its external-id query; no hint, nothing sent"
    );
    eprintln!(
        "(xi-c'') code 57 on the hint → inconclusive, issued in one execution; on the lookup's external-id query → unavailable{{szamlazz_code}}, nothing sent: pass"
    );
}
