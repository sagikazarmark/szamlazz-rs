//! Read retries and protected Order writes under Restate: read-only
//! reconciliation after a lost create answer, retained prerequisite retries
//! and pause/resume, and cancellation while a write's reply is in flight.
//! These scenarios exercise the production protected-write boundary, including
//! cancellation fault context and the retained marker guarding later mutations.
//! They also check re-execution and its delay, `retry_count` and the failing
//! command on `sys_invocation`, one journal entry per step, and retained
//! completions under the caller's `Idempotency-Key`.

use std::time::{Duration, Instant};

use restate_e2e_harness::Call;
use rust_decimal::dec;
use wiremock::ResponseTemplate;

use restate_szamlazz::contract::{IssuedKind, TerminalCode};

use crate::harness::szamlazz::{
    Doc, create_for, create_lands_slowly, created, external_id_query, holds_after_misses,
    loses_reply_once, not_found, order_query,
};
use crate::harness::{Harness, create_body};

/// Protected reconciliation and retained prerequisites, on three orders at once:
///
/// - **a lost create answer settles through read-only reconciliation**
///   (`E2E-11`): one create loses its reply and the first reconciliation finds
///   nothing. The invocation re-executes `reconcile-write` after the test's
///   one-second delay, finds the document and answers `reconciled`, clearing
///   its marker. The same `Idempotency-Key` replays that completion without a
///   request; a new key then answers `already_issued` from the target lookup.
/// - **the invocation policy re-executes a prerequisite** (`E2E-27`): the target lookup's
///   external-id query answers 500 once and code 7 afterwards; the create
///   completes `issued` in one invocation with `lookup-invoice` the failing
///   command, one journal entry per step, exactly one create;
/// - **an unanswered Order prerequisite pauses its invocation** (`E2E-28`):
///   after three executions, no completion and no create; restoring the read
///   and resuming that same invocation issues exactly once.
///
/// The resolver's twin (the `account` step) runs in phase 2, where a
/// resolution can be scripted per scope.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: reconciliation and retained prerequisites on separate orders, concurrently"
)]
pub(crate) async fn order_retries_retain_prerequisites_and_reconcile_writes(h: &Harness) {
    // The lost create answer: target lookup, full lookup, leading query and
    // first reconciliation miss (four queries). Read-only reconciliation
    // finds the document on its next execution; no second send is permitted.
    h.absent("E2E-11", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-11")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    holds_after_misses(
        &h.mock,
        4,
        &Doc {
            external_id: Some("acct:E2E-11:invoice"),
            ..Doc::of("SZ-11", "SZ", "E2E-11")
        },
    )
    .await;
    create_for("E2E-11")
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    // The flaky target lookup: the first execution loses its reply; the
    // second, the full lookup and the create step's leading query miss cleanly.
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
    // The retained target lookup: three executions, then pause before prerequisites.
    h.absent("E2E-28", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-28")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-28:invoice")
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(3)
        .expect(3)
        .mount(&h.mock)
        .await;
    external_id_query("acct:E2E-28:invoice")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for("E2E-28")
        .respond_with(created("SZ-28", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    tokio::join!(
        reconciled_create_then_the_key_replays(h),
        flaky_read_is_re_executed(h),
        retained_read_resumes(h),
    );
}

/// The protected create on `E2E-11`: read-only reconciliation, the completion
/// the same key replays, and the next key's `already_issued`.
#[allow(
    clippy::too_many_lines,
    reason = "the reconciliation, the stored completion, the next key"
)]
async fn reconciled_create_then_the_key_replays(h: &Harness) {
    let started = Instant::now();
    let watch = h.watch("E2E-11");
    let reply = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000)),
            "e2e-11-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(60),
        "the test invocation policy's reconciliation delay (1 s initial) was honoured: {elapsed:?}"
    );
    assert_eq!(reply.body["outcome"], "reconciled");
    h.assert_state_absent(None, "E2E-11").await;
    // Read-only reconciliation re-executes while the invocation is in flight:
    // `retry_count` (the invoker's count of starts) counts it, and the failing
    // command is reconcile-write, never a second create send.
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["reconcile-write"],
        "only read-only reconciliation retried: {retries:?}"
    );
    assert!(
        retries
            .failures
            .iter()
            .all(|failure| failure.contains("unresolved")),
        "reconciliation reports retained uncertainty: {retries:?}"
    );
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.handler, "create_invoice");
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert!(invocation.completion_failure.is_none(), "{invocation:?}");
    let runs = h.admin().runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "create-invoice").count(),
        1,
        "the create step was recorded once: {runs:?}"
    );
    assert_eq!(
        h.create_bodies_of("E2E-11").await.len(),
        1,
        "one send across every execution"
    );

    // The same key: the stored reconciled completion, nothing read.
    let before = h.requests_of_order("E2E-11").await.len();
    let replayed = h
        .call(
            "E2E-11",
            "create_invoice",
            &create_body(dec!(1000)),
            "e2e-11-k1",
        )
        .await;
    assert_eq!(replayed.status, 200, "{}", replayed.body);
    assert_eq!(replayed.body, reply.body);
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
            &create_body(dec!(1000)),
            "e2e-11-k2",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "already_issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-11");
    assert_eq!(reply.body["external_id"], "acct:E2E-11:invoice");
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account", "lookup-invoice"],
        "the target lookup answered before prerequisites"
    );
    assert_eq!(
        h.create_bodies_of("E2E-11").await.len(),
        1,
        "nothing more was sent"
    );
}

/// The invocation policy's re-execution, on `E2E-27`: one lost reply, `issued` in
/// one invocation.
async fn flaky_read_is_re_executed(h: &Harness) {
    let started = Instant::now();
    let watch = h.watch("E2E-27");
    let reply = h
        .call(
            "E2E-27",
            "create_invoice",
            &create_body(dec!(1000)),
            "e2e-27-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-27");
    h.assert_state_absent(None, "E2E-27").await;
    assert!(
        elapsed < Duration::from_secs(60),
        "the test invocation policy's delay was honoured: {elapsed:?}"
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
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
            "prepare-write",
            "arm-write",
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

/// The invocation policy pauses the unfinished read; resume keeps its identity.
async fn retained_read_resumes(h: &Harness) {
    let watch = h.watch("E2E-28");
    let call = Call::object("Szamlazz.Order", "E2E-28", "create_invoice");
    let body = create_body(dec!(1000));
    let owner = h.invoke(&call.send(), Some(&body), Some("e2e-28-k1")).await;
    assert_eq!(owner.status, 202, "{}", owner.body);
    assert_eq!(
        h.admin()
            .await_status(owner.invocation_id(), &["paused", "completed"])
            .await,
        "paused"
    );
    let retries = watch.finish().await;
    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["lookup-invoice"],
        "the lookup step is the failing command: {retries:?}"
    );
    let invocation = h.admin().invocation(owner.invocation_id()).await;
    assert!(invocation.completion_failure.is_none(), "{invocation:?}");
    assert_eq!(
        h.admin().runs(owner.invocation_id()).await,
        ["namespace", "account", "lookup-invoice"],
        "the target lookup is journaled; prerequisites and create never ran"
    );
    assert!(
        h.create_bodies_of("E2E-28").await.is_empty(),
        "nothing was created"
    );
    h.assert_state_absent(None, "E2E-28").await;
    h.admin().resume(owner.invocation_id()).await;
    assert_eq!(
        h.admin()
            .await_status(owner.invocation_id(), &["completed", "paused"])
            .await,
        "completed"
    );
    let reply = h.invoke(&call, Some(&body), Some("e2e-28-k1")).await;
    assert_eq!(reply.invocation_id(), owner.invocation_id());
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued");
    assert_eq!(h.create_bodies_of("E2E-28").await.len(), 1);
    h.assert_state_absent(None, "E2E-28").await;
}

/// A **cancellation**
/// (`PATCH /invocations/{id}/cancel`, the SDK's 409) while the create step is
/// mid-send is `outcome_unknown` with `cause: cancelled` (ADR 0011). The
/// send is delayed by szamlazz.hu; the cancel arrives while the reply is in
/// flight; the SDK does not interrupt the closure, so the step's own send
/// completes, and the cancel is what the step's result await sees. The
/// invocation completes with the fault (never `issued`, never a kill), the
/// order key is released by that completion, and its unresolved marker refuses
/// the next mutation before any lookup or send. The cancelled invocation's runs are the full create
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

    let body = create_body(dec!(1000));
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
    assert_eq!(fault.is_cancelled(), Some(true));
    assert_eq!(
        fault.cause,
        Some(restate_szamlazz::contract::FaultCause::Cancelled)
    );
    assert_eq!(fault.order.as_deref(), Some("E2E-L4"));
    assert_eq!(fault.kind, Some(IssuedKind::Invoice));
    assert_eq!(fault.external_id.as_deref(), Some("acct:E2E-L4:invoice"));
    assert!(
        fault.message.contains("409") && fault.message.contains("cancelled"),
        "the fault names how the run ended: {fault:?}"
    );
    assert!(
        fault.message.contains("unresolved marker is retained"),
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
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
            "prepare-write",
            "arm-write",
            "create-invoice",
        ],
        "the create step's command was journaled; the cancel ended its await"
    );
    assert_eq!(
        h.create_bodies_of("E2E-L4").await.len(),
        1,
        "the one send that landed"
    );

    // The key is released by the completion: the next mutation runs at once
    // but the retained marker refuses it before any external operation.
    let started = Instant::now();
    let next = h.call("E2E-L4", "create_invoice", &body, "e2e-l4-k2").await;
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the key was released by the cancelled invocation's completion"
    );
    assert_eq!(next.status, 500, "{}", next.body);
    assert_eq!(next.fault().code, TerminalCode::OutcomeUnknown);
    assert!(h.admin().runs(next.invocation_id()).await.is_empty());
    assert_eq!(
        h.create_bodies_of("E2E-L4").await.len(),
        1,
        "nothing more was sent"
    );
    h.expect_unresolved(None, "E2E-L4").await;
}

/// Submit with a known retry identity, wait until szamlazz.hu has the write,
/// then cancel while its reply is delayed. Attaching with the same key reads
/// the cancelled invocation's stored completion, rather than issuing again.
pub(crate) async fn cancel_after_send(
    h: &Harness,
    call: Call<'_>,
    body: &serde_json::Value,
    idempotency: &str,
    received: &tokio::sync::Notify,
) -> crate::harness::ingress::Reply {
    let submitted = h.invoke(&call.send(), Some(body), Some(idempotency)).await;
    assert_eq!(submitted.status, 202, "{}", submitted.body);
    tokio::time::timeout(Duration::from_secs(30), received.notified())
        .await
        .expect("the write reached szamlazz.hu");
    h.admin().cancel(submitted.invocation_id()).await;
    let reply = h.invoke(&call, Some(body), Some(idempotency)).await;
    assert_eq!(reply.invocation_id(), submitted.invocation_id());
    assert_eq!(reply.status, 500, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::OutcomeUnknown, "{fault:?}");
    assert_eq!(fault.is_cancelled(), Some(true));
    assert_eq!(
        fault.cause,
        Some(restate_szamlazz::contract::FaultCause::Cancelled)
    );
    assert!(
        fault.message.contains("409") && fault.message.contains("cancelled"),
        "{fault:?}"
    );
    assert_eq!(
        h.admin().invocation(reply.invocation_id()).await.status,
        "completed"
    );
    reply
}

/// A deletion has reached szamlazz.hu when cancelled: its successful delayed
/// reply must not make the invocation claim success. The next invocation's
/// lookup reconciles the deletion, and the cancelled completion sends once.
pub(crate) async fn cancelled_one_shot_deletion_is_unknown_and_get_reconciles(h: &Harness) {
    use crate::harness::szamlazz::{delete_of, number_query, proforma_deleted};
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let landed = Arc::new(AtomicBool::new(false));
    let received = Arc::new(tokio::sync::Notify::new());
    let query_landed = Arc::clone(&landed);
    external_id_query("acct:E2E-CANCEL-DELETE:proforma")
        .respond_with(move |_: &wiremock::Request| {
            if query_landed.load(Ordering::SeqCst) {
                not_found()
            } else {
                Doc::of("D-CANCEL", "D", "E2E-CANCEL-DELETE").response()
            }
        })
        .mount(&h.mock)
        .await;
    h.absent("E2E-CANCEL-DELETE", &["invoice", "prepayment", "final"])
        .await;
    number_query("D-CANCEL")
        .respond_with(Doc::of("D-CANCEL", "D", "E2E-CANCEL-DELETE").response())
        .mount(&h.mock)
        .await;
    let signal = Arc::clone(&received);
    delete_of("D-CANCEL")
        .respond_with(move |_: &wiremock::Request| {
            landed.store(true, Ordering::SeqCst);
            signal.notify_one();
            proforma_deleted().set_delay(Duration::from_secs(4))
        })
        .expect(1)
        .mount(&h.mock)
        .await;
    let call = Call::object("Szamlazz.Order", "E2E-CANCEL-DELETE", "delete_proforma");
    let reply = cancel_after_send(
        h,
        call,
        &json!({"expected_number": "D-CANCEL"}),
        "cancel-delete-k1",
        &received,
    )
    .await;
    let fault = reply.fault();
    assert_eq!(fault.order.as_deref(), Some("E2E-CANCEL-DELETE"));
    assert!(
        fault.message.contains("unresolved marker is retained"),
        "{fault:?}"
    );
    assert_eq!(
        fault.external_id.as_deref(),
        Some("acct:E2E-CANCEL-DELETE:proforma")
    );
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "lookup-proforma",
            "prepare-write",
            "arm-write",
            "delete-proforma-D-CANCEL"
        ]
    );
    // The cancelled order lock is released, and a read sees the landed delete.
    let get = h.get_reply("E2E-CANCEL-DELETE").await;
    assert_eq!(get.status, 200, "{}", get.body);
    assert!(get.body["proforma"].is_null(), "{}", get.body);
    let again = h
        .invoke(
            &call,
            Some(&json!({"expected_number": "D-CANCEL"})),
            Some("cancel-delete-k2"),
        )
        .await;
    assert_eq!(again.status, 500, "{}", again.body);
    assert!(h.admin().runs(again.invocation_id()).await.is_empty());
    h.expect_unresolved(None, "E2E-CANCEL-DELETE").await;
}
