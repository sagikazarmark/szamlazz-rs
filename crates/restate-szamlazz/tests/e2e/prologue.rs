//! The prologue's account steps on the single-account deployment: a scoped
//! call answered `unknown_account` (with the leak check's positive control),
//! a flaky resolver retried under the resolve policy, a failing credential
//! store as a terminal `unavailable`, and a killed invocation releasing the
//! order key.

use std::time::{Duration, Instant};

use rust_decimal::dec;
use serde_json::json;

use crate::harness::introspection::run_result;
use crate::harness::szamlazz::{
    Doc, api_error, create, created, not_found, number_query, order_query,
};
use crate::harness::{Harness, create_body};

/// (xii) the harness capabilities of #29 that the multi-account tickets
/// assert through: a scoped call reaches the handler (the scope needs no
/// Virtual Object routing for `Szamlazz.Agent`, and is on the invocation
/// either way), and on this single-account deployment the prologue answers it
/// with `unknown_account` and nothing reaches szamlazz.hu; the leak check
/// has a positive control: a sentinel string in a wiremock rejection is
/// found in the hex-decoded `raw` of the create run's journal entry.
pub(crate) async fn harness_scoped_call_and_leak_positive_control(h: &Harness) {
    const SENTINEL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";
    h.reset().await;
    number_query("SZ-12")
        .respond_with(Doc::new("SZ-12", "SZ", "E2E-12").response())
        .expect(0)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_agent_scoped(
            "acme-events",
            "query",
            &json!({ "selector": { "invoice_number": "SZ-12" } }),
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unknown_account", "{fault:?}");
    assert!(fault.message.contains("acme-events"), "{fault:?}");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(
        invocation.scope.as_deref(),
        Some("acme-events"),
        "{invocation:?}"
    );
    assert_eq!(invocation.handler, "query");

    // The same through the Virtual Object: the journal has the `account`
    // entry (the resolution is data), and nothing after it.
    let reply = h
        .call_scoped(
            "acme-events",
            "E2E-12",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-12-scoped",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    assert_eq!(reply.fault().code, "unknown_account", "{}", reply.body);
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");

    // Positive control: the sentinel travels through szamlazz.hu's rejection
    // message into the create run's journaled result.
    h.reset().await;
    h.absent("E2E-12", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-12")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(api_error("259", SENTINEL))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call(
            "E2E-12",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-12-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "rejected", "{}", reply.body);
    assert_eq!(reply.body["message"], SENTINEL);

    let journal = h.journal(reply.invocation_id()).await;
    let create_result = run_result(&journal, "create-invoice")
        .unwrap_or_else(|| panic!("the create-invoice run's result entry: {journal:?}"));
    assert!(
        create_result.raw_contains(SENTINEL),
        "the sentinel is found in the hex-decoded raw of entry {}: {:?}",
        create_result.index,
        String::from_utf8_lossy(&create_result.raw)
    );
    let lookup_result = run_result(&journal, "lookup-invoice").expect("the lookup's result");
    assert!(
        !lookup_result.raw_contains(SENTINEL),
        "the sentinel is not in an entry it did not pass through"
    );
    let leaked: Vec<u64> = journal
        .iter()
        .filter(|entry| entry.raw_contains(SENTINEL))
        .map(|entry| entry.index)
        .collect();
    assert_eq!(
        leaked,
        [create_result.index, journal.last().expect("output").index],
        "the sentinel is in exactly the create result and the output"
    );
    eprintln!(
        "(xii) scoped call on a single-account deployment → unknown_account; leak positive control: pass"
    );
}

/// (xiv) the resolver fails twice, then answers: the `account` step is
/// re-executed under the resolve policy (one second apart under the test
/// policy, not the handler's two-minute `initial_interval`), the invocation
/// completes with the outcome, `sys_invocation.retry_count` shows the run's
/// retries with `account` as the failing command, and the journal holds one
/// `account` entry.
pub(crate) async fn flaky_resolver_is_retried_by_the_resolve_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-14", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-14")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-14", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let resolutions_before = h.script.resolutions();
    h.script.fail_next_resolutions(2);
    let started = Instant::now();
    let watch = h.watch("E2E-14");
    let reply = h
        .call(
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
        h.script.resolutions() - resolutions_before,
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

    let journal = h.journal(reply.invocation_id()).await;
    let runs: Vec<_> = journal
        .iter()
        .filter(|entry| entry.is_run())
        .filter_map(|entry| entry.name.as_deref())
        .collect();
    assert_eq!(
        runs.iter().filter(|name| **name == "account").count(),
        1,
        "the failed executions journaled nothing: {runs:?}"
    );
    assert!(runs.contains(&"create-invoice"), "{runs:?}");
    eprintln!("(xiv) flaky resolver → retried under the resolve policy, one account entry: pass");
}

/// (xv) the credential store fails on every fetch: the handler ends with a
/// terminal `unavailable` (503) after the in-process retry, without a single
/// szamlazz.hu request; the `account` step is journaled (the resolution
/// succeeded), nothing after it.
pub(crate) async fn failing_credential_store_is_a_terminal_unavailable(h: &Harness) {
    h.reset().await;
    h.absent("E2E-15", &["prepayment", "final", "proforma", "invoice"])
        .await;
    create()
        .respond_with(created("SZ-15", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let fetches_before = h.script.fetches();
    h.script.set_store_down(true);
    let started = Instant::now();
    let reply = h
        .call(
            "E2E-15",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-15-k1",
        )
        .await;
    let elapsed = started.elapsed();
    h.script.set_store_down(false);
    assert_eq!(reply.status, 503, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, "unavailable", "{fault:?}");
    assert!(fault.message.contains("credentials"), "{fault:?}");
    assert!(!fault.message.contains("scripted"), "{fault:?}");
    // No response names the account, nor the store's reference (#65).
    assert!(!fault.message.contains("acct"), "{fault:?}");
    assert!(
        elapsed < Duration::from_secs(30),
        "terminal, not routed into the handler's retries: {elapsed:?}"
    );
    assert_eq!(
        h.script.fetches() - fetches_before,
        3,
        "the short in-process retry: three fetches"
    );
    assert_eq!(h.requests_seen().await, 0, "zero szamlazz.hu requests");

    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert!(
        invocation
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.contains("unavailable")),
        "{invocation:?}"
    );

    // The store is back: the same order issues on the next call.
    h.reset().await;
    h.absent("E2E-15", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-15")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create()
        .respond_with(created("SZ-15", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let issued = h
        .ok(
            "E2E-15",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-15-k2",
        )
        .await;
    assert_eq!(issued["outcome"], "issued", "{issued}");
    eprintln!(
        "(xv) failing credential store → terminal unavailable, zero szamlazz.hu requests: pass"
    );
}

/// (xv-b) a killed invocation releases the order key. An invocation that
/// will not finish (its `account` step hangs on a resolver that never
/// answers) holds the Virtual Object's lock; the same order's next exclusive
/// handler queues behind it. The kill (what an operator does to a stuck
/// invocation, and what `on_max_attempts = kill` does once a handler's
/// attempts are spent) ends it as a failed completion, and the
/// queued `delete_proforma` runs at once, answering from szamlazz.hu.
/// Journaled by the killed one: `namespace`, and the `account` command it
/// never completed, a prefix of every handler's path. (`get` would prove
/// nothing here: it is shared and never waits for the lock.)
pub(crate) async fn a_killed_invocation_releases_the_order_key(h: &Harness) {
    h.reset().await;
    h.absent("E2E-K", &["proforma"]).await;
    let resolutions_before = h.script.resolutions();
    h.script.hang_next_resolutions(1);
    let stuck = h.submit("E2E-K", "delete_proforma", &json!({})).await;
    // The hung resolution is the next one asked for; nothing else resolves
    // until it is.
    let deadline = Instant::now() + Duration::from_secs(30);
    while h.script.resolutions() == resolutions_before {
        assert!(
            Instant::now() < deadline,
            "the hung invocation never resolved"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    h.await_status(&stuck, &["running"]).await;

    // The queued call: submitted while the lock is held, answered after the
    // kill.
    let queued = h.submit("E2E-K", "delete_proforma", &json!({})).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let waiting = h.invocation(&queued).await;
    assert_ne!(
        waiting.status, "completed",
        "the queued call waits behind the lock: {waiting:?}"
    );
    assert_eq!(
        h.requests_seen().await,
        0,
        "nothing read while the lock is held"
    );

    h.kill(&stuck).await;
    let killed = h.await_status(&stuck, &["completed", "killed"]).await;
    let stuck_row = h.invocation(&stuck).await;
    assert!(
        stuck_row
            .completion_failure
            .as_deref()
            .is_some_and(|failure| failure.to_lowercase().contains("kill")),
        "the kill is the completion ({killed}): {stuck_row:?}"
    );
    assert_eq!(
        h.runs(&stuck).await,
        ["namespace", "account"],
        "the killed invocation journaled the prologue's two commands"
    );

    let started = Instant::now();
    h.await_status(&queued, &["completed"]).await;
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(30),
        "the key was released by the kill, not by a timeout: {elapsed:?}"
    );
    let done = h.invocation(&queued).await;
    assert_eq!(done.completion_failure, None, "{done:?}");
    assert_eq!(
        h.runs(&queued).await,
        ["namespace", "account", "proforma-for-delete"],
        "the queued delete ran to its answer (absent)"
    );
    assert_eq!(h.requests_seen().await, 1, "the queued delete's lookup");
    eprintln!(
        "(xv-b) a killed invocation releases the order key; the queued call runs at once: pass"
    );
}
