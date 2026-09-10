//! The prologue's `account` step under Restate, in phase 2 where a scope
//! selects the account and the scripts are per scope: a resolver that fails
//! then answers is re-executed under the resolve policy with one `account`
//! entry, and an invocation standing still after its `account` step holds
//! the order key until it is killed. The prologue's decisions (unscoped and
//! unknown as `unknown_account`, the store's `gone` and `unavailable`, the
//! fetch's in-process retry under a paused clock) are unit tests of
//! `service::prologue`.

use std::time::{Duration, Instant};

use rust_decimal::dec;
use serde_json::json;

use crate::harness::accounts::KEY_B;
use crate::harness::szamlazz::{create_with_key, created, not_found, order_query};
use crate::harness::{Harness, create_body};

/// The scope every call of this family goes under: `beta`, so a scripted
/// resolution or a held fetch disturbs no scenario of `acme`.
const SCOPE: &str = "beta";

/// The resolver fails twice, then answers: the `account` step is re-executed
/// under the resolve policy (one second apart under the test policy, not the
/// handler's two-minute `initial_interval`), the invocation completes with the
/// outcome, `sys_invocation.retry_count` shows the run's retries with
/// `account` as the failing command and the resolver's own message never
/// echoed, and the journal holds one `account` entry: the failed executions
/// journaled nothing.
pub(crate) async fn a_flaky_resolver_is_retried_by_the_resolve_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-14", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-14")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(created("SZ-14", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let resolutions_before = h.multi().resolutions(SCOPE);
    h.multi().fail_next_resolutions(SCOPE, 2);
    let started = Instant::now();
    let watch = h.watch("E2E-14");
    let reply = h
        .call_scoped(
            SCOPE,
            "E2E-14",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-14-k1",
        )
        .await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert!(
        elapsed < Duration::from_secs(60),
        "the resolve policy's delay was honoured, not the handler's: {elapsed:?}"
    );
    assert_eq!(
        h.multi().resolutions(SCOPE) - resolutions_before,
        3,
        "two failures, then the answer"
    );
    // Two run failures: the server counts at least both (its exact
    // accounting is its own; 3 was observed).
    assert!(retries.max_retry_count >= 2, "{retries:?}");
    assert_eq!(retries.failing_commands, ["account"], "{retries:?}");
    assert!(
        retries.failures.iter().all(|failure| failure
            .contains("the account resolver is unavailable")
            && !failure.contains("scripted")),
        "the resolver's own message is never echoed: {retries:?}"
    );

    let runs = h.admin().runs(reply.invocation_id()).await;
    assert_eq!(
        runs.iter().filter(|name| *name == "account").count(),
        1,
        "the failed executions journaled nothing: {runs:?}"
    );
    assert_eq!(
        runs.last().map(String::as_str),
        Some("create-invoice"),
        "{runs:?}"
    );
}

/// A killed invocation releases the order key. An invocation that will not
/// finish holds the Virtual Object's lock; the same order's next exclusive
/// handler queues behind it. The kill (what an operator does to a stuck
/// invocation, and what `on_max_attempts = kill` does once a handler's
/// attempts are spent) ends it as a failed completion, and the queued
/// `delete_proforma` runs at once, answering from szamlazz.hu. The stuck
/// invocation stands still at its credential fetch, **held** by the store
/// (`hold_fetch`): after its `account` step, before its gateway opens, so it
/// journals `namespace`, `account` and its unfinished `lookup-proforma`, and
/// the moment it stands is the hold's signal, not a clock's. The operation's
/// fetch deadline (10 s) would cut a hold that stayed and retry the fetch; the
/// kill lands well inside it. (`get` would prove nothing here: it is shared
/// and never waits for the lock.)
pub(crate) async fn a_killed_invocation_releases_the_order_key(h: &Harness) {
    h.reset().await;
    h.absent("E2E-K", &["proforma"]).await;
    let hold = h.multi().hold_fetch(SCOPE, 1);
    let stuck = h
        .submit_scoped(SCOPE, "E2E-K", "delete_proforma", &json!({}))
        .await;
    hold.reached().await;
    h.admin().await_status(&stuck, &["running"]).await;

    // The queued call: submitted while the lock is held, answered after the
    // kill.
    let queued = h
        .submit_scoped(SCOPE, "E2E-K", "delete_proforma", &json!({}))
        .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let waiting = h.admin().invocation(&queued).await;
    assert_ne!(
        waiting.status, "completed",
        "the queued call waits behind the lock: {waiting:?}"
    );
    assert!(
        h.requests_of_order("E2E-K").await.is_empty(),
        "nothing read while the lock is held"
    );

    h.admin().kill(&stuck).await;
    let killed = h
        .admin()
        .await_status(&stuck, &["completed", "killed"])
        .await;
    let stuck_row = h.admin().invocation(&stuck).await;
    assert!(
        stuck_row
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.to_lowercase().contains("kill")),
        "the kill is the completion ({killed}): {stuck_row:?}"
    );
    assert_eq!(
        h.admin().runs(&stuck).await,
        ["namespace", "account", "lookup-proforma"],
        "the fetch is inside the unfinished operation command"
    );
    // The hold is on the position, so the queued invocation's fetch would
    // park behind it too: released now that the kill is the completion.
    hold.release();

    let started = Instant::now();
    h.admin().await_status(&queued, &["completed"]).await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "the key was released by the kill, not by a timeout: {elapsed:?}"
    );
    let done = h.admin().invocation(&queued).await;
    assert_eq!(done.completion_failure, None, "{done:?}");
    assert_eq!(
        h.admin().runs(&queued).await,
        ["namespace", "account", "lookup-proforma"],
        "the queued delete ran to its answer (absent)"
    );
    assert_eq!(
        h.requests_mentioning("acct:E2E-K:proforma").await.len(),
        1,
        "the queued delete's lookup"
    );
}
