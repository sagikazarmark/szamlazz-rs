//! `Szamlazz.Order.get`: the status shape, its reads retried under the read
//! policy (run retries spending no invocation attempts; #87), and a purged
//! invocation querying szamlazz.hu again.

use std::time::{Duration, Instant};

use restate_sdk::service::Discoverable;
use restate_szamlazz::Order;
use restate_szamlazz::contract::{DocumentState, OrderStatus};
use rust_decimal::dec;
use serde_json::{Value, json};

use crate::harness::Harness;
use crate::harness::szamlazz::{Doc, external_id_query, not_found};

/// The `max_attempts` of `handler`'s invocation retry policy as the service
/// discovers it: what the deployment registered with the server, read from
/// the same source rather than copied.
fn discovered_max_attempts<S: Discoverable>(handler: &str) -> u64 {
    S::discover()
        .handlers
        .into_iter()
        .find(|h| h.name.as_str() == handler)
        .unwrap_or_else(|| panic!("no handler {handler}"))
        .retry_policy_max_attempts
        .unwrap_or_else(|| panic!("{handler} pins no max_attempts"))
}

/// (viii) the `get` live view after (iv)/(v).
pub(crate) async fn status_shape(h: &Harness) {
    h.reset().await;
    h.absent("E2E-1", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-1:invoice"),
        ..Doc::new("SZ-2", "SZ", "E2E-1")
    })
    .await;
    let status = h.get("E2E-1").await;
    assert_eq!(status["invoice"]["number"], "SZ-2", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["invoice"]["gross"], "1270");
    assert_eq!(status["invoice"]["net"], "1000");
    assert_eq!(status["invoice"]["payments"], json!([]));
    assert_eq!(status["invoice"]["e_invoice"], true);
    assert_eq!(status["proforma"], Value::Null);
    assert_eq!(status["prepayment"], Value::Null);
    assert_eq!(status["final"], Value::Null);
    let status: OrderStatus = serde_json::from_value(status).expect("status deserialises");
    let invoice = status.invoice.expect("invoice");
    assert_eq!(invoice.state, DocumentState::Live);
    assert_eq!(invoice.gross, Some(dec!(1270)));
    assert!(status.proforma.is_none());
    eprintln!("(viii) get shape: pass");
}

/// (xi-d) `get` under the same fault injection: one of its four reads loses
/// its reply once, the read policy re-executes it, and the status completes
/// with what szamlazz.hu holds.
pub(crate) async fn flaky_get_read_is_retried_by_the_read_policy(h: &Harness) {
    h.reset().await;
    h.absent("E2E-29", &["prepayment", "final"]).await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-29:invoice"),
        ..Doc::new("SZ-29", "SZ", "E2E-29")
    })
    .await;
    h.loses_reply_once("acct:E2E-29:proforma").await;
    external_id_query("acct:E2E-29:proforma")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;

    let watch = h.watch("E2E-29");
    let reply = h.get_reply("E2E-29").await;
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let status = &reply.body;
    assert_eq!(status["invoice"]["number"], "SZ-29", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["proforma"], Value::Null);
    assert_eq!(status["prepayment"], Value::Null);
    assert_eq!(status["final"], Value::Null);

    assert!(retries.max_retry_count >= 1, "{retries:?}");
    assert_eq!(
        retries.failing_commands,
        ["get-proforma"],
        "the proforma read is the failing command: {retries:?}"
    );
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ],
        "{runs:?}"
    );
    eprintln!("(xi-d) flaky get read → retried by the read policy, status complete: pass");
}

/// (xi-e) **run retries do not spend invocation attempts**, the fact every
/// retry budget of the worker rests on (#87): a re-execution the SDK asks for
/// with a delay (`next_retry_delay`, a run retry policy) is re-dispatched by
/// the server without advancing the handler's `invocation_retry_policy`
/// iterator, whose attempts are spent only on worker-side failures. Proved on
/// `get`: all four of its reads lose their reply once, so the read policy
/// re-executes the invocation four times, more re-executions than the
/// handler's `max_attempts` (read from discovery) would allow if they counted,
/// and the status still completes with what szamlazz.hu holds, instead of the
/// invocation being killed. `sys_invocation.retry_count` is the invoker's
/// count of starts (`start_count`; verified against 1.7.8), so it is seen past
/// the handler's `max_attempts` while the invocation is in flight.
pub(crate) async fn run_retries_do_not_spend_invocation_attempts(h: &Harness) {
    h.reset().await;
    // Each of the four reads loses its reply once: mounted before the steady
    // answers, which take over from the second query on.
    for kind in ["proforma", "invoice", "prepayment", "final"] {
        h.loses_reply_once(&format!("acct:E2E-30:{kind}")).await;
    }
    h.absent("E2E-30", &["proforma", "prepayment", "final"])
        .await;
    h.holds(&Doc {
        external_id: Some("acct:E2E-30:invoice"),
        ..Doc::new("SZ-30", "SZ", "E2E-30")
    })
    .await;

    // Four re-executions must be more than the handler would tolerate as
    // attempts, or completing proves nothing.
    let get_max_attempts = discovered_max_attempts::<Order>("get");
    assert!(
        get_max_attempts < 4,
        "`get` allows {get_max_attempts} attempts; this scenario needs to re-execute more often than that"
    );

    let started = Instant::now();
    let watch = h.watch_for("E2E-30", Duration::from_secs(12));
    let reply = h.get_reply("E2E-30").await;
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let status = &reply.body;
    assert_eq!(status["invoice"]["number"], "SZ-30", "{status}");
    assert_eq!(status["invoice"]["state"], "live");
    assert_eq!(status["proforma"], Value::Null);
    assert!(
        elapsed < Duration::from_secs(60),
        "four run retries one second apart, not the handler's policy: {elapsed:?}"
    );

    // Four re-executions on top of the first start: the count is seen past
    // the handler's budget (each re-execution is visible for the 1 s back-off
    // before it), and the invocation completes rather than being killed;
    // run retries spent none of its attempts.
    assert!(
        retries.max_retry_count > get_max_attempts,
        "retry_count must exceed get's max_attempts ({get_max_attempts}): {retries:?}"
    );
    assert_eq!(
        retries.failing_commands,
        ["get-proforma", "get-invoice", "get-prepayment", "get-final"],
        "each read failed once, in order: {retries:?}"
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.runs(reply.invocation_id()).await;
    assert_eq!(
        runs,
        [
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
        ],
        "one journal entry per step, the retries invisible in the journal: {runs:?}"
    );
    eprintln!(
        "(xi-e) run retries do not spend invocation attempts: {} starts against max_attempts = {get_max_attempts}, completed: pass",
        retries.max_retry_count
    );
}

/// (xiii) an order Restate has no memory of: the `get` invocation is purged
/// and a second `get` queries szamlazz.hu again (nothing is served from a
/// retained journal or a Virtual Object state).
pub(crate) async fn purged_invocation_queries_szamlazz_again(h: &Harness) {
    h.reset().await;
    h.absent("E2E-13", &["proforma", "invoice", "prepayment", "final"])
        .await;
    let reply = h
        .invoke("/restate/call/Szamlazz.Order/E2E-13/get", None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let first = h.requests_seen().await;
    assert_eq!(first, 4, "four external-id queries");
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(
        h.journal(reply.invocation_id())
            .await
            .iter()
            .filter(|entry| entry.is_run())
            .count(),
        6,
        "get's journal is retained and inspectable: the prologue's two steps and four queries"
    );

    h.purge(reply.invocation_id()).await;
    assert!(
        h.journal(reply.invocation_id()).await.is_empty(),
        "the journal is gone with the invocation"
    );

    let reply = h
        .invoke("/restate/call/Szamlazz.Order/E2E-13/get", None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        h.requests_seen().await,
        first + 4,
        "szamlazz.hu is queried again"
    );
    eprintln!("(xiii) purged invocation → szamlazz.hu queried again: pass");
}
