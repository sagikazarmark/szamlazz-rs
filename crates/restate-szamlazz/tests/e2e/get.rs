//! `Szamlazz.Order.get` under Restate: the fact every retry budget of the
//! worker rests on, that run retries spend no invocation attempts (#87). The
//! `get` path itself is walked by the proforma scenario; the projection and
//! the consumed-proforma derivation are unit tests of `service::storno`.

use std::time::{Duration, Instant};

use restate_sdk::service::Discoverable;
use restate_szamlazz::Order;
use serde_json::Value;

use crate::harness::Harness;
use crate::harness::szamlazz::Doc;
use crate::harness::szamlazz::{holds, loses_reply_once};

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
        .unwrap_or_else(|| panic!("{handler} sets no max_attempts"))
}

/// **Run retries do not spend invocation attempts** (#87): a re-execution
/// the SDK asks for with a delay (`next_retry_delay`, a run retry policy) is
/// re-dispatched by the server without advancing the handler's
/// `invocation_retry_policy` iterator, whose attempts are spent only on
/// worker-side failures. Proved on `get`: all four of its reads lose their
/// reply once, so the read policy re-executes the invocation four times, more
/// re-executions than the handler's `max_attempts` (read from discovery)
/// would allow if they counted, and the status still completes with what
/// szamlazz.hu holds, instead of the invocation being killed.
/// `sys_invocation.retry_count` is the invoker's count of starts
/// (`start_count`; verified against 1.7.8), so it is seen past the handler's
/// `max_attempts` while the invocation is in flight.
pub(crate) async fn run_retries_do_not_spend_invocation_attempts(h: &Harness) {
    // Each of the four reads loses its reply once: mounted before the steady
    // answers, which take over from the second query on.
    for kind in ["proforma", "invoice", "prepayment", "final"] {
        loses_reply_once(&h.mock, &format!("acct:E2E-30:{kind}")).await;
    }
    h.absent("E2E-30", &["proforma", "prepayment", "final"])
        .await;
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-30:invoice"),
            ..Doc::of("SZ-30", "SZ", "E2E-30")
        },
    )
    .await;

    // Four re-executions must be more than the handler would tolerate as
    // attempts, or completing proves nothing.
    let get_max_attempts = discovered_max_attempts::<Order>("get");
    assert!(
        get_max_attempts < 4,
        "`get` allows {get_max_attempts} attempts; this scenario needs to re-execute more often than that"
    );

    let started = Instant::now();
    let watch = h.watch("E2E-30");
    let reply = h.get_reply("E2E-30").await;
    let elapsed = started.elapsed();
    let retries = watch.finish().await;
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
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.completion_failure, None, "{invocation:?}");
    let runs = h.admin().runs(reply.invocation_id()).await;
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
        "  (get: {} starts against max_attempts = {get_max_attempts}, completed)",
        retries.max_retry_count
    );
}
