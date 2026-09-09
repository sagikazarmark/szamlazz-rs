//! The run retry policies under Restate, driven through `create_invoice`: a
//! read the issue or read policy re-executes and a step it gives up on, the
//! exhaustion stored under the caller's `Idempotency-Key` as a structured
//! fault and replayed by it, and a cancellation while a send's reply is in
//! flight (the other `Err` a write run can end with). What a policy decides
//! on a given answer is the gateway's (`Unanswered`, `Unconfirmed`) and the
//! handlers' (`create_outcome_unknown`, the exhausted read's fault), unit
//! tested; what is proved here is Restate's part: the re-execution and its
//! delay, `retry_count` and the failing command on `sys_invocation` while in
//! flight, one journal entry per step whatever the retries, and the stored
//! completion.

use std::time::{Duration, Instant};

use rust_decimal::dec;
use wiremock::ResponseTemplate;

use restate_szamlazz::contract::{IssuedKind, TerminalCode};

use crate::harness::szamlazz::{
    Doc, create_for, create_lands_slowly, create_never_sent, created, external_id_query,
    holds_after_misses, loses_reply_once, not_found, order_query,
};
use crate::harness::{Harness, create_body};

/// The three policies' Restate half, on three orders at once:
///
/// - **the issue policy re-executes a write and its exhaustion is a
///   structured fault the key replays** (`E2E-11`): every execution of the
///   create step loses its reply and the re-query finds nothing, so the step
///   is re-executed once (one second later under the test policy, not the
///   handler's two-minute `initial_interval`; `retry_count` moves and
///   `create-invoice` is the failing command while in flight) and its
///   exhaustion is `outcome_unknown` (500) naming the order, kind and
///   external id; the same `Idempotency-Key` then replays the stored fault
///   without a request, and a new key finds the document that landed after
///   all and answers `already_issued` from the lookup step with nothing sent;
/// - **the read policy re-executes a read** (`E2E-27`): the lookup's
///   external-id query answers 500 once and code 7 afterwards; the create
///   completes `issued` in one invocation with `lookup-invoice` the failing
///   command, one journal entry per step, exactly one create;
/// - **the read policy's exhaustion is a structured `unavailable`**
///   (`E2E-28`): a read szamlazz.hu never answers is, after three executions,
///   `unavailable` (503) naming the step, the order, kind and external id,
///   the create step never run and nothing sent.
///
/// The resolve policy's twin (the `account` step) runs in phase 2, where a
/// resolution can be scripted per scope.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the three policies, each on its own order, concurrently"
)]
pub(crate) async fn run_retries_re_execute_a_step_and_exhaustion_is_a_structured_fault(
    h: &Harness,
) {
    // The exhausted create: the lookup, then two executions' leading query
    // and re-query miss (five queries); the document that landed after all
    // is found by the next call's lookup.
    h.absent("E2E-11", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-11")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    holds_after_misses(
        &h.mock,
        5,
        &Doc {
            external_id: Some("acct:E2E-11:invoice"),
            ..Doc::of("SZ-11", "SZ", "E2E-11")
        },
    )
    .await;
    create_for("E2E-11")
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&h.mock)
        .await;
    // The flaky lookup: the first execution loses its reply; the second, and
    // the create step's own leading query, miss cleanly.
    h.absent("E2E-27", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-27")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    loses_reply_once(&h.mock, "acct:E2E-27:invoice").await;
    external_id_query("acct:E2E-27:invoice")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for("E2E-27")
        .respond_with(created("SZ-27", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    // The exhausted lookup: three executions, no answer.
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
    create_never_sent(&h.mock, "E2E-28").await;

    tokio::join!(
        exhausted_create_then_the_key_replays(h),
        flaky_read_is_re_executed(h),
        exhausted_read_is_unavailable(h),
    );
}

/// The issue policy's half of the scenario above, on `E2E-11`: the
/// exhaustion, the stored fault the same key replays, the next key's
/// `already_issued`.
#[allow(
    clippy::too_many_lines,
    reason = "the exhaustion, the stored fault, the next key"
)]
async fn exhausted_create_then_the_key_replays(h: &Harness) {
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
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::OutcomeUnknown, "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-11"));
    assert_eq!(fault.kind, Some(IssuedKind::Invoice));
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
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("outcome_unknown")),
        "{invocation:?}"
    );
    let runs = h.admin().runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "create-invoice").count(),
        1,
        "the re-executed step is one entry: {runs:?}"
    );
    assert_eq!(
        h.create_bodies_of("E2E-11").await.len(),
        2,
        "one send per execution"
    );

    // The same key: the stored fault, nothing read.
    let before = h.requests_of_order("E2E-11").await.len();
    let replayed = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-11-k1",
        )
        .await;
    assert_eq!(replayed.status, 500, "{}", replayed.body);
    assert_eq!(replayed.fault().code, TerminalCode::OutcomeUnknown);
    assert_eq!(replayed.invocation_id(), reply.invocation_id());
    assert_eq!(
        h.requests_of_order("E2E-11").await.len(),
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
        h.admin()
            .runs(reply.invocation_id())
            .await
            .last()
            .map(String::as_str),
        Some("lookup-invoice"),
        "the lookup answered; no create step"
    );
    assert_eq!(
        h.create_bodies_of("E2E-11").await.len(),
        2,
        "nothing more was sent"
    );
}

/// The read policy's re-execution, on `E2E-27`: one lost reply, `issued` in
/// one invocation.
async fn flaky_read_is_re_executed(h: &Harness) {
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
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
            "create-invoice",
        ],
        "one journal entry per step; the retried read is one entry"
    );
    assert_eq!(
        h.create_bodies_of("E2E-27").await.len(),
        1,
        "exactly one create"
    );
}

/// The read policy's exhaustion, on `E2E-28`: three unanswered executions,
/// the structured `unavailable`, nothing sent.
async fn exhausted_read_is_unavailable(h: &Harness) {
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
    assert_eq!(fault.code, TerminalCode::Unavailable, "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-28"));
    assert_eq!(fault.kind, Some(IssuedKind::Invoice));
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
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
        ],
        "the lookup is journaled by name, the create step never ran"
    );
    assert!(
        h.create_bodies_of("E2E-28").await.is_empty(),
        "nothing was created"
    );
}

/// The other `Err` a write run can end with: a **cancellation**
/// (`PATCH /invocations/{id}/cancel`, the SDK's 409) while the create step is
/// mid-send is `outcome_unknown` like an exhausted policy (#142, #125). The
/// send is delayed by szamlazz.hu; the cancel arrives while the reply is in
/// flight; the SDK does not interrupt the closure, so the step's own send
/// completes, and the cancel is what the step's result await sees. The
/// invocation completes with the fault (never `issued`, never a kill), the
/// order key is released by that completion, and the next call with a new key
/// finds the document that landed and answers `already_issued` from its lookup
/// with nothing sent. The cancelled invocation's runs are the full create
/// path (the step's command was journaled before the cancel arrived), so no
/// `RUN_NAMES` row is added: a cancellation anywhere on the path leaves a
/// prefix, which the step-name table admits.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the cancelled send, then the released key"
)]
pub(crate) async fn a_cancellation_mid_send_is_outcome_unknown_and_releases_the_key(h: &Harness) {
    h.absent("E2E-L4", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L4")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    let mut sends = create_lands_slowly(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-L4:invoice"),
            ..Doc::of("SZ-L4", "SZ", "E2E-L4")
        },
        Duration::from_secs(4),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let started = Instant::now();
    let (reply, cancelled) = tokio::join!(
        h.call("E2E-L4", "create_invoice", &body, "e2e-l4-k1"),
        async {
            // szamlazz.hu has the create (the stub signals its receipt); its
            // reply is four seconds away.
            sends.received(1).await;
            let in_flight = h.in_flight_on("E2E-L4").await;
            h.admin().cancel(&in_flight).await;
            in_flight
        },
    );
    let elapsed = started.elapsed();
    assert_eq!(
        reply.invocation_id(),
        cancelled,
        "the cancelled invocation answered"
    );
    assert_eq!(reply.status, 500, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::OutcomeUnknown, "{fault:?}");
    assert_eq!(fault.order.as_deref(), Some("E2E-L4"));
    assert_eq!(fault.kind, Some(IssuedKind::Invoice));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-L4:invoice"));
    assert!(
        fault.message.contains("(409)") && fault.message.contains("cancelled"),
        "the fault names how the run ended: {fault:?}"
    );
    assert!(
        fault
            .message
            .contains("a send may have landed: read get, then retry with a new Idempotency-Key"),
        "a cancelled write reconciles before it retries: {fault:?}"
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "answered when the send's reply came, not after a handler retry: {elapsed:?}"
    );
    let invocation = h.admin().invocation(&cancelled).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("outcome_unknown")),
        "the completion is the fault, not a kill: {invocation:?}"
    );
    assert_eq!(
        h.admin().runs(&cancelled).await,
        [
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
            "create-invoice",
        ],
        "the create step's command was journaled; the cancel ended its await"
    );
    assert_eq!(
        h.create_bodies_of("E2E-L4").await.len(),
        1,
        "the one send that landed"
    );

    // The key is released by the completion: the next call runs at once and
    // finds what landed.
    let started = Instant::now();
    let next = h.call("E2E-L4", "create_invoice", &body, "e2e-l4-k2").await;
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the key was released by the cancelled invocation's completion"
    );
    assert_eq!(next.status, 200, "{}", next.body);
    assert_eq!(next.body["outcome"], "already_issued", "{}", next.body);
    assert_eq!(next.body["invoice_number"], "SZ-L4");
    assert_eq!(
        h.admin()
            .runs(next.invocation_id())
            .await
            .last()
            .map(String::as_str),
        Some("lookup-invoice"),
        "the next call answered from its lookup"
    );
    assert_eq!(
        h.create_bodies_of("E2E-L4").await.len(),
        1,
        "nothing more was sent"
    );
}
