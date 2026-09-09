//! End-to-end tests of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service against a real Restate server with wiremock
//! standing in for szamlazz.hu: the **sequence test** (one scenario per
//! handler path of [`harness::run_names::RUN_NAMES`], proving the steps run
//! in that order under Restate) and the **durable-execution proof** (what
//! only a server can show: the `Idempotency-Key` replay, run retries and
//! their exhaustion, the re-executed closure, the per-key lock, the scope
//! namespacing keys, the `account` entry across a re-execution, a rotation,
//! purge, kill, the flag day, the journal scan and the run-name pin). The
//! decisions each handler takes are unit tests of `service`; the wire of each
//! gateway step is `tests/gateway.rs`.
//!
//! The two end-to-end tests are ignored by default:
//! `cargo test -p restate-szamlazz --test e2e -- --ignored`.
//! The server comes from the environment, decided once (the server gate of
//! the `restate-e2e-harness` crate): `RESTATE_ADMIN_URL` /
//! `RESTATE_INGRESS_URL` reuse a running server (with the three experimental
//! flags; `compose.yaml` sets them; the main suite only), `RESTATE_SERVER_BIN`
//! names a `restate-server` binary the harness spawns on the loopback (what
//! the Dagger check does; `dagger call ci restate-server export --path
//! ./restate-server` gives a developer the same binary). With neither the
//! suite skips with a message, and fails when `CI` is set, since a skipped
//! run in CI proves nothing. A server the harness starts binds ports chosen
//! free at launch, none fixed, so two runs on one host collide with nothing;
//! it is stopped when the run ends and on a SIGINT or SIGTERM to the test
//! process. Unix only, as `restate-server` itself is: the server's process
//! group and the stop signals are.
//! The Restate half of the harness (the gate, the spawned server, the
//! deployment, the ingress, the admin API and the run-name matcher) is that
//! crate, tested there; the szamlazz half under [`harness`] has its own
//! tests (the fetch hold and the resolution script, the stub helpers) beside
//! what they test, needing only wiremock and running un-ignored.
//!
//! One binary, one tree: this file holds the two tests and the order the
//! scenarios run in; [`harness`] is everything the scenarios drive; and every
//! other module is one handler family's scenarios: the creates
//! ([`create_invoice`], [`create_proforma`], [`create_prepayment`],
//! [`create_final`], [`correct_invoice`]), [`storno`] and [`delete_proforma`],
//! [`get`], the run retry policies and a cancellation mid-send
//! ([`policies`]), the order-key lock and the in-flight `Idempotency-Key`
//! under one scope ([`concurrency`]), the `Szamlazz.Agent` reads and writes
//! under a scope ([`agent_reads`], [`agent_writes`]), the wire faults
//! ([`faults`]), the prologue's account step ([`prologue`]), the flag day and
//! scope isolation ([`multi_account`]) and the run-wide pins ([`pins`]). A
//! scenario is a `pub(crate) async fn` taking the harness; a family file is
//! where a new scenario of that handler goes.
//!
//! The main run has two phases on one Restate server. The first registers a
//! **single-account** deployment (the static resolver's `[account]`) and runs
//! its scenarios **concurrently**, unscoped: every scenario owns its order
//! keys, numbers and `Idempotency-Key`s, every stub is mounted once and
//! discriminated by them, nothing is reset between scenarios, and every
//! scenario's failure is reported (a panic in one does not hide the rest). The
//! second performs the documented single → multi **flag day** (private,
//! drain, register the **multi-account** deployment (two accounts, reachable
//! by scope only, behind a test-local mutable resolver and store), public)
//! and proves, in sequence, the isolation properties multi-account mode leans
//! on: the same order key and the same `Idempotency-Key` under two scopes
//! being two objects and two invocations, and, under one scope, the order-key
//! lock and the in-flight attach (the first invocation held at its credential
//! fetch, which the mutable store of this phase can do), the scoped reads and
//! writes, an order Restate has no memory of, the resolve policy and a kill on
//! the `account` step, credential rotation and account changes between
//! executions; and, last, over the whole run, that the object kept no state,
//! that no agent key was ever journaled (the hex-decoded `raw` of every
//! journal entry of every invocation), and that every invocation's `ctx.run`
//! names are a prefix of its handler's pinned path and every path was walked
//! in full. The second test is the protocol-v7 canary on a server of its own
//! without the flag.

#![cfg(unix)]

/// The fixtures shared with `tests/gateway.rs` and the crate's unit tests.
#[path = "../common/mod.rs"]
mod common;
mod harness;

mod agent_reads;
mod agent_writes;
mod concurrency;
mod correct_invoice;
mod create_final;
mod create_invoice;
mod create_prepayment;
mod create_proforma;
mod delete_proforma;
mod faults;
mod get;
mod multi_account;
mod pins;
mod policies;
mod prologue;
mod storno;

use std::collections::HashMap;
use std::sync::Arc;

use restate_szamlazz::contract::TerminalCode;
use serde_json::json;
use tokio::task::JoinSet;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{not_found, probe_with_key};
use crate::harness::{Harness, MAIN_SERVER, Reuse, WITHOUT_PROTOCOL_V7, launcher_or_skip};

/// The scenarios of phase 1, spawned on one runtime and joined together:
/// each runs against the shared harness on order keys of its own, and every
/// failure is collected and reported at the end rather than the first one
/// ending the run.
struct Concurrently {
    set: JoinSet<&'static str>,
    names: HashMap<tokio::task::Id, &'static str>,
}

impl Concurrently {
    fn new() -> Self {
        Self {
            set: JoinSet::new(),
            names: HashMap::new(),
        }
    }

    /// Joins every scenario; panics naming each one that failed, with its
    /// panic message.
    async fn join_all(mut self) {
        let mut failures = Vec::new();
        while let Some(joined) = self.set.join_next_with_id().await {
            match joined {
                Ok((_, name)) => eprintln!("[phase 1] {name}: pass"),
                Err(error) => {
                    let name = self.names.get(&error.id()).copied().unwrap_or("?");
                    eprintln!("[phase 1] {name}: FAIL");
                    failures.push(format!("{name}: {error}"));
                }
            }
        }
        assert!(
            failures.is_empty(),
            "{} phase-1 scenario(s) failed:\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }
}

/// Spawns each scenario of the list on the harness.
macro_rules! concurrently {
    ($h:expr; $($scenario:path),+ $(,)?) => {{
        let mut run = Concurrently::new();
        $(
            let h = Arc::clone(&$h);
            let handle = run.set.spawn(async move {
                $scenario(&h).await;
                stringify!($scenario)
            });
            run.names.insert(handle.id(), stringify!($scenario));
        )+
        run
    }};
}

/// Runs each scenario of the list in sequence, reporting each.
macro_rules! sequentially {
    ($h:expr; $($scenario:path),+ $(,)?) => {{
        $(
            $scenario(&$h).await;
            eprintln!("[phase 2] {}: pass", stringify!($scenario));
        )+
    }};
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN or RESTATE_ADMIN_URL / RESTATE_INGRESS_URL"]
async fn e2e_order_protocol() {
    let Some(launcher) = launcher_or_skip(Reuse::Allowed) else {
        return;
    };
    let h = Arc::new(Harness::start(launcher.launch(&MAIN_SERVER).await).await);

    // Phase 1: the single-account deployment, unscoped, every scenario at
    // once on its own order keys.
    concurrently!(h;
        create_invoice::issued_already_issued_and_the_key_replays,
        create_invoice::reversal_between_executions_is_reversed_not_reissued,
        create_proforma::proforma_then_the_invoice_naming_it_then_get_consumed,
        create_prepayment::prepayment_converts_the_proforma_under_auto_and_by_number,
        create_final::create_final_names_its_live_prepayment_invoice,
        correct_invoice::corrective_is_issued_under_its_correction_id,
        storno::storno_then_reissue,
        storno::storno_answers_from_the_hint_or_re_executes_a_lost_send,
        delete_proforma::proforma_is_deleted_by_the_orders_handler,
        policies::run_retries_re_execute_a_step_and_exhaustion_is_a_structured_fault,
        policies::a_cancellation_mid_send_is_outcome_unknown_and_releases_the_key,
        get::run_retries_do_not_spend_invocation_attempts,
        faults::refusals_and_szamlazz_codes_travel_as_structured_faults,
        pins::plant_the_leak_positive_control,
    )
    .join_all()
    .await;
    let mut h = Arc::try_unwrap(h)
        .ok()
        .expect("every phase-1 scenario has been joined");

    // Phase 2: the flag day, then the multi-account deployment by scope, in
    // sequence (the scenarios script the shared resolver and store).
    multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls(&mut h).await;
    eprintln!(
        "[phase 2] multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls: pass"
    );
    sequentially!(h;
        multi_account::the_scope_namespaces_the_order_key_and_the_idempotency_key,
        concurrency::same_key_same_scope_concurrent_creates_issue_once,
        concurrency::same_key_same_scope_second_call_between_the_first_calls_executions,
        concurrency::same_idempotency_key_in_flight_attaches_to_the_invocation,
        agent_reads::the_scope_selects_the_account_for_every_agent_read,
        agent_writes::agent_storno_and_set_payments_run_on_the_scoped_account,
        storno::purged_order_is_stornoed_and_reissued,
        prologue::a_flaky_resolver_is_retried_by_the_resolve_policy,
        prologue::a_killed_invocation_releases_the_order_key,
        multi_account::account_change_between_executions_does_not_reach_the_invocation,
        multi_account::credential_rotation_between_executions_is_picked_up,
    );

    // The run-wide pins, last.
    sequentially!(h;
        pins::the_order_keeps_no_state,
        pins::no_agent_key_in_any_journal_of_the_run,
        pins::every_handler_journals_its_pinned_run_names,
    );
}

/// The deploy-time canary for protocol v7, provoked: on a server without
/// `RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7` the ingress accepts a scoped path
/// (it does not refuse one for the flag), and the server keys the invocation
/// by the scope (`sys_invocation.scope`), but the SDK sees no scope. So a
/// scoped `check_account` reports `scope: null`: on the single-account
/// deployment with its account and `credentials: ok` as a 200 (the signal a
/// deploy pipeline reads, since the worker has no per-request way to tell
/// "unscoped" from "scope not forwarded"), and on the multi-account deployment
/// as `unknown_account` naming the unscoped case, with nothing sent: every
/// scoped call fails closed, no account is reached under the wrong scope. A
/// server of its own, on ports of its own; never a reused one, whose flags
/// are the main suite's.
#[tokio::test]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN"]
async fn e2e_check_account_without_protocol_v7() {
    let Some(launcher) = launcher_or_skip(Reuse::Never) else {
        return;
    };
    let mut h = Harness::start(launcher.launch(&WITHOUT_PROTOCOL_V7).await).await;

    // The single-account deployment: the scoped probe answers the account
    // and reports the scope it saw: none.
    probe_with_key(AGENT_KEY)
        .respond_with(not_found())
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h.check_account(Some("acme")).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(
        reply.body,
        json!({
            "scope": null,
            "account": { "id": "acct" },
            "namespace": "acct",
            "credentials": { "state": "ok" },
        }),
        "the canary: a scoped call reported without its scope"
    );
    assert_eq!(
        h.requests_mentioning("acct:check-account").await.len(),
        1,
        "one probe query, nothing else"
    );
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account", "probe"]
    );
    let invocation = h.invocation(reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(invocation.handler, "check_account");
    // The hazard, in one row: the server keyed the invocation by the scope
    // (`sys_invocation.scope`, the partition key) and the handler never saw
    // it; the response above is the only place the discrepancy shows.
    assert_eq!(
        invocation.scope.as_deref(),
        Some("acme"),
        "the server keyed the invocation by the scope it did not forward: {invocation:?}"
    );

    // The multi-account deployment on the same server: the scope selects no
    // account because none arrives; `unknown_account`, nothing sent.
    h.switch_to_multi_account().await;
    h.reset().await;
    let reply = h.check_account(Some("acme")).await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::UnknownAccount, "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert_eq!(
        h.runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert!(
        h.requests_mentioning("acct:check-account").await.is_empty(),
        "nothing reached szamlazz.hu"
    );
    eprintln!(
        "(canary) without protocol v7: scoped check_account → scope: null on the single-account deployment, unknown_account on the multi-account one: pass"
    );
}
