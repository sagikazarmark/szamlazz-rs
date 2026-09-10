//! End-to-end tests of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service against a real Restate server with wiremock
//! standing in for szamlazz.hu: the **sequence test** (one scenario per
//! handler path of [`harness::run_names::RUN_NAMES`], proving the steps run
//! in that order under Restate) and the **durable-execution proof** (what
//! only a server can show: the `Idempotency-Key` replay, run retries and
//! their exhaustion, the re-executed closure, the per-key lock, the scope
//! namespacing keys, the `account` entry across a re-execution, a rotation,
//! purge, kill, the flag day, the journal scan and the step-name table check). The
//! decisions each handler takes are unit tests of `service`; the wire of each
//! gateway step is `tests/gateway/`.
//!
//! The two end-to-end tests are ignored by default:
//! `cargo test -p restate-szamlazz --test e2e -- --ignored`; `E2E_ONLY=<needle,…>`
//! runs one family's scenarios and their prerequisites ([`Only`]).
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
//! deployment, the ingress, the admin API and the step-name table check) is that
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
//! scope isolation ([`multi_account`]) and the run-wide checks
//! ([`invariants`]). A
//! scenario is a `pub(crate) async fn` taking the harness; a family file is
//! where a new scenario of that handler goes.
//!
//! The main run has two phases on one Restate server. The first registers a
//! **single-account** deployment (the static resolver's `[account]`) and runs
//! its scenarios **concurrently**, unscoped: every scenario owns its order
//! keys, numbers and `Idempotency-Key`s, every stub is mounted once and
//! discriminated by them, nothing is reset between scenarios, and every
//! scenario's failure is reported (a panic in one does not hide the rest, and
//! the run goes on to the second phase). The
//! second performs the documented single → multi **flag day** (private,
//! drain, register the **multi-account** deployment (two accounts, reachable
//! by scope only, behind a test-local mutable resolver and store), public;
//! the one step whose failure ends the run, since everything after it runs
//! on that deployment) and proves, in sequence and likewise every failure
//! reported, the isolation properties multi-account mode leans on: the same order key and the same `Idempotency-Key` under two scopes
//! being two objects and two invocations, and, under one scope, the order-key
//! lock and the in-flight attach (the first invocation held at its credential
//! fetch, which the mutable store of this phase can do), the scoped reads and
//! writes, an order Restate has no memory of, the resolve policy and a kill on
//! the `account` step, credential rotation and account changes between
//! executions; and, last, over the whole run, that the object kept no state,
//! that no agent key was ever journaled (the hex-decoded `raw` of every
//! journal entry of every invocation), and that every invocation's `ctx.run`
//! names are a prefix of one of its handler's paths and every path was walked
//! in full. The second test is the protocol-v7 canary on a server of its own
//! without the flag.

#![cfg(unix)]

/// The fixtures shared with `tests/gateway/` and the crate's unit tests.
#[path = "../common/mod.rs"]
mod common;
mod harness;

mod agent_reads;
mod agent_writes;
mod cancellation;
mod concurrency;
mod correct_invoice;
mod create_final;
mod create_invoice;
mod create_prepayment;
mod create_proforma;
mod delete_proforma;
mod expected_document;
mod faults;
mod get;
mod invariants;
mod multi_account;
mod policies;
mod prologue;
mod storno;
mod unresolved;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use restate_szamlazz::contract::TerminalCode;
use serde_json::json;
use tokio::task::JoinSet;

use crate::harness::accounts::AGENT_KEY;
use crate::harness::szamlazz::{not_found, probe_with_key};
use crate::harness::{Harness, MAIN_SERVER, ReusePolicy, WITHOUT_PROTOCOL_V7, launcher_or_skip};

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

    /// Spawns `scenario` under `name`: the name is what a failure is reported
    /// under, since a panicked task's `JoinError` carries only its id.
    fn spawn(&mut self, name: &'static str, scenario: impl Future<Output = ()> + Send + 'static) {
        let handle = self.set.spawn(async move {
            scenario.await;
            name
        });
        self.names.insert(handle.id(), name);
    }

    /// Joins every scenario, then verifies every mock the scenarios mounted
    /// (`expect(n)`; wiremock checks the counts on `verify`, which panics
    /// naming the mock, so it runs as a task of its own and its failure is one
    /// more line). The failures, each naming its scenario with its panic
    /// message, or the expectations that were not met: the run goes on to
    /// phase 2 and the checks and reports them all together
    /// ([`Sequentially::finish`]). Verified here, not at the flag day's
    /// `reset`, so a failed scenario cannot hide another's unmet expectation,
    /// and so the report is phase 1's.
    async fn join_all(mut self, h: &Arc<Harness>) -> Vec<String> {
        let mut failures = Vec::new();
        while let Some(joined) = self.set.join_next_with_id().await {
            match joined {
                Ok((_, name)) => eprintln!("[phase 1] {name}: pass"),
                Err(error) => {
                    let name = self
                        .names
                        .get(&error.id())
                        .copied()
                        .unwrap_or("<a task this run did not spawn>");
                    eprintln!("[phase 1] {name}: FAIL");
                    failures.push(format!("{name}: {error}"));
                }
            }
        }
        let mocks = Arc::clone(h);
        if let Err(error) = tokio::spawn(async move { mocks.mock.verify().await }).await {
            eprintln!("[phase 1] wiremock expectations: FAIL");
            failures.push(format!("wiremock expectations: {error}"));
        } else {
            eprintln!("[phase 1] wiremock expectations: pass");
        }
        failures
    }
}

/// What a scenario runs as: a future on the shared harness.
type Run = Box<dyn FnOnce(Arc<Harness>) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// One scenario as a phase lists it: its name (`family::scenario`, what a
/// report and the `E2E_ONLY` filter read) and what it runs as.
type Scenario = (&'static str, Run);

/// The scenarios of a phase, in the order they are listed, each under its own
/// name.
macro_rules! scenarios {
    ($($scenario:path),+ $(,)?) => {{
        let scenarios: Vec<Scenario> = vec![$((
            stringify!($scenario),
            Box::new(|h: Arc<Harness>| -> Pin<Box<dyn Future<Output = ()> + Send>> {
                Box::pin(async move { $scenario(&h).await })
            }),
        ),)+];
        scenarios
    }};
}

/// The `E2E_ONLY` filter: a developer iterating on one handler runs that
/// family's scenarios and their prerequisites rather than the whole run.
/// Comma-separated needles, each a **substring** of a scenario's
/// `family::scenario` name (`E2E_ONLY=storno,create_final`; `storno` selects
/// the `storno` family and `agent_writes::agent_storno_and_…`); a scenario is
/// selected when any needle matches. The prerequisites follow from what was
/// selected: the flag day runs when a phase-2 scenario is selected; the three
/// run-wide checks are **skipped** under any filter, since they count over
/// the whole run, and a needle that selects nothing is a failure (a typo must
/// not pass as an empty run). Unset or empty, the run is unchanged.
#[derive(Debug, PartialEq, Eq)]
struct Only(Vec<String>);

impl Only {
    /// The filter `E2E_ONLY` holds, `None` when unset or empty.
    fn from_env() -> Option<Self> {
        std::env::var("E2E_ONLY")
            .ok()
            .and_then(|value| Self::parse(&value))
    }

    /// The needles of `value`: comma-separated, trimmed, the empty ones
    /// dropped; `None` when none is left.
    fn parse(value: &str) -> Option<Self> {
        let needles: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|needle| !needle.is_empty())
            .map(str::to_owned)
            .collect();
        (!needles.is_empty()).then_some(Self(needles))
    }

    /// Whether any needle is a substring of `name`.
    fn selects(&self, name: &str) -> bool {
        self.0.iter().any(|needle| name.contains(needle.as_str()))
    }

    /// Splits `scenarios` into the selected ones (in their order) and the
    /// names of the skipped; `None` selects everything and skips nothing.
    fn select<T>(
        only: Option<&Self>,
        scenarios: Vec<(&'static str, T)>,
    ) -> (Vec<(&'static str, T)>, Vec<&'static str>) {
        let Some(only) = only else {
            return (scenarios, Vec::new());
        };
        let mut selected = Vec::new();
        let mut skipped = Vec::new();
        for (name, scenario) in scenarios {
            if only.selects(name) {
                selected.push((name, scenario));
            } else {
                skipped.push(name);
            }
        }
        (selected, skipped)
    }
}

/// Reports the scenarios a phase skips under `E2E_ONLY`.
fn report_skipped(phase: &str, skipped: &[&str]) {
    for name in skipped {
        eprintln!("[{phase}] {name}: skip (E2E_ONLY)");
    }
}

/// What a sequential step of the run is: a phase-2 scenario, or one of the
/// run-wide checks. The difference decides what a failure does: a failed
/// scenario resets the mock so the next starts clean and makes the checks'
/// counts suspect; a failed check mounts nothing and resets nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Scenario,
    Check,
}

impl Step {
    /// The tag a report line carries.
    fn phase(self) -> &'static str {
        match self {
            Self::Scenario => "phase 2",
            Self::Check => "checks",
        }
    }
}

/// The scenarios of phase 2 and the run-wide checks, run one after another
/// on one harness: each as a task of its own, so a panic in one is a
/// `JoinError` recorded under the scenario's name and the next scenario runs;
/// the failures are reported together at the end ([`Sequentially::finish`])
/// rather than the first one ending the run.
struct Sequentially {
    h: Arc<Harness>,
    failures: Vec<String>,
    /// Whether a scenario (not a run-wide check) has failed: the run-wide
    /// checks count over the whole run, and after a failed scenario their
    /// counts are suspect, which their report says.
    a_scenario_failed: bool,
}

impl Sequentially {
    /// The collector after phase 1, carrying its `failures` (a failed phase-1
    /// scenario makes the checks' counts suspect too).
    fn after_phase_1(h: Arc<Harness>, failures: Vec<String>) -> Self {
        Self {
            h,
            a_scenario_failed: !failures.is_empty(),
            failures,
        }
    }

    /// Runs `scenario` under `name` to its end and records its outcome. After
    /// a failed scenario the mock is reset **without** verifying its
    /// expectations (the failed scenario's unmet `expect(n)` would otherwise
    /// be blamed on the next scenario's `reset`), and said so; a failed
    /// run-wide check mounts nothing and resets nothing.
    async fn run(
        &mut self,
        step: Step,
        name: &'static str,
        scenario: impl Future<Output = ()> + Send + 'static,
    ) {
        let phase = step.phase();
        match tokio::spawn(scenario).await {
            Ok(()) => eprintln!("[{phase}] {name}: pass"),
            Err(error) => {
                eprintln!("[{phase}] {name}: FAIL");
                let suspect = if step == Step::Check && self.a_scenario_failed {
                    " (an earlier scenario failed; the run-wide counts are suspect)"
                } else {
                    ""
                };
                self.failures.push(format!("{name}{suspect}: {error}"));
                if step == Step::Scenario {
                    self.a_scenario_failed = true;
                    self.h.mock.reset().await;
                    eprintln!(
                        "[{phase}] the mock was reset without verifying its expectations, so the \
                         next scenario starts clean"
                    );
                }
            }
        }
    }

    /// Runs `scenarios` in order, each as a `step`.
    async fn run_all(&mut self, step: Step, scenarios: Vec<Scenario>) {
        for (name, scenario) in scenarios {
            let h = Arc::clone(&self.h);
            self.run(step, name, scenario(h)).await;
        }
    }

    /// Panics naming each scenario and check of the run that failed, with its
    /// panic message.
    async fn finish(self) {
        assert!(
            self.failures.is_empty(),
            "{} scenario(s) or run-wide check(s) failed:\n  {}",
            self.failures.len(),
            self.failures.join("\n  ")
        );
        Arc::try_unwrap(self.h)
            .ok()
            .expect("every scenario has been joined")
            .finish()
            .await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::too_many_lines,
    reason = "the suite lists its scenarios and executes its two phases"
)]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN or RESTATE_ADMIN_URL / RESTATE_INGRESS_URL"]
async fn e2e_order_protocol() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Allowed) else {
        return;
    };
    let only = Only::from_env();
    let (phase1, skipped1) = Only::select(
        only.as_ref(),
        scenarios![
            create_invoice::issued_already_issued_and_the_key_replays,
            expected_document::purged_reissue_intent_cannot_replace_its_replacement,
            expected_document::purged_deletion_intent_cannot_delete_a_replacement,
            expected_document::expected_target_outcomes_precede_prerequisites,
            expected_document::reissue_rechecks_the_expected_holder_after_prerequisites,
            expected_document::purged_corrective_request_still_cannot_reissue,
            expected_document::deletion_preserves_ownership_and_consumed_target_outcomes,
            expected_document::missing_target_after_a_lost_reissue_preserves_uncertainty,
            create_invoice::reversal_between_executions_is_reversed_not_reissued,
            create_invoice::reversed_targets_answer_before_prerequisites,
            create_proforma::proforma_then_the_invoice_naming_it_then_get_consumed,
            create_prepayment::prepayment_converts_the_proforma_under_auto_and_by_number,
            create_final::create_final_names_its_live_prepayment_invoice,
            correct_invoice::corrective_is_issued_under_its_correction_id,
            storno::storno_then_reissue,
            storno::storno_answers_from_the_hint_or_re_executes_a_lost_send,
            storno::ambiguous_storno_retries_and_exhaustion_preserve_the_send,
            delete_proforma::proforma_is_deleted_by_the_orders_handler,
            delete_proforma::replay_refreshes_the_pinned_proformas_credit_entries,
            delete_proforma::deletion_answers_preserve_guard_failures_and_send_uncertainty,
            policies::run_retries_re_execute_a_step_and_exhaustion_is_a_structured_fault,
            policies::a_cancellation_mid_send_is_outcome_unknown_and_releases_the_key,
            policies::cancelled_one_shot_deletion_is_unknown_and_get_reconciles,
            cancellation::cancelled_reads_are_structured,
            cancellation::cancelled_storno_is_uncertain,
            get::run_retries_do_not_spend_invocation_attempts,
            faults::refusals_and_szamlazz_codes_travel_as_structured_faults,
            invariants::plant_the_leak_positive_control,
        ],
    );
    let (phase2, skipped2) = Only::select(
        only.as_ref(),
        scenarios![
            multi_account::the_scope_namespaces_the_order_key_and_the_idempotency_key,
            concurrency::same_key_same_scope_concurrent_creates_issue_once,
            concurrency::same_key_same_scope_second_call_between_the_first_calls_executions,
            concurrency::same_idempotency_key_in_flight_attaches_to_the_invocation,
            agent_reads::the_scope_selects_the_account_for_every_agent_read,
            agent_writes::agent_storno_and_set_credit_entries_run_on_the_scoped_account,
            agent_writes::inconclusive_credit_entry_answers_are_stored_unknown_outcomes,
            agent_writes::cancelled_credit_entries_are_unknown_with_mode_specific_guidance,
            storno::purged_order_is_stornoed_and_reissued,
            prologue::a_flaky_resolver_is_retried_by_the_resolve_policy,
            cancellation::cancelled_resolution_is_structured,
            prologue::a_killed_invocation_releases_the_order_key,
            multi_account::account_change_between_executions_does_not_reach_the_invocation,
            multi_account::credential_rotation_between_executions_is_picked_up,
            multi_account::credential_failure_on_replay_preserves_operation_commands,
            multi_account::initialization_failure_is_journaled_at_the_operation,
        ],
    );
    if let Some(only) = &only {
        assert!(
            !(phase1.is_empty() && phase2.is_empty()),
            "E2E_ONLY={:?} selects no scenario; a needle is a substring of a `family::scenario` name",
            only.0
        );
        eprintln!(
            "E2E_ONLY={:?}: {} of {} scenario(s) selected; the run-wide checks are skipped",
            only.0,
            phase1.len() + phase2.len(),
            phase1.len() + phase2.len() + skipped1.len() + skipped2.len()
        );
    }

    let h = Arc::new(Harness::start(launcher.launch(&MAIN_SERVER).await).await);

    // Phase 1: the single-account deployment, unscoped, every scenario at
    // once on its own order keys.
    report_skipped("phase 1", &skipped1);
    let mut run = Concurrently::new();
    for (name, scenario) in phase1 {
        run.spawn(name, scenario(Arc::clone(&h)));
    }
    let phase1_failures = run.join_all(&h).await;
    if !phase1_failures.is_empty() {
        // As after a failed phase-2 scenario: the flag day's `reset` verifies
        // the mock, and a failed scenario's unmet expectation was reported
        // above already; it must not end the run a second time there.
        h.mock.reset().await;
        eprintln!(
            "[phase 1] the mock was reset without verifying its expectations, so phase 2 starts \
             clean"
        );
    }
    let mut h = Arc::try_unwrap(h)
        .ok()
        .expect("every phase-1 scenario has been joined");

    // Phase 2: the flag day (the prerequisite of everything after it, so its
    // failure ends the run; skipped with the whole phase when the filter
    // selects nothing of it), then the multi-account deployment by scope, in
    // sequence (the scenarios script the shared resolver and store), every
    // failure collected. The run-wide checks, last; run whether or not a
    // scenario failed, and reported with it; skipped under `E2E_ONLY`, since
    // they count over the whole run.
    let checks = scenarios![
        invariants::the_order_keeps_no_state,
        invariants::no_agent_key_in_any_journal_of_the_run,
        invariants::every_handler_journals_its_tabled_steps,
    ];
    report_skipped("phase 2", &skipped2);
    if phase2.is_empty() {
        eprintln!(
            "[phase 2] multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls: skip \
             (E2E_ONLY selects no phase-2 scenario)"
        );
        report_skipped("checks", &names_of(&checks));
        Sequentially::after_phase_1(Arc::new(h), phase1_failures)
            .finish()
            .await;
        return;
    }
    multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls(&mut h).await;
    eprintln!(
        "[phase 2] multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls: pass"
    );
    let mut run = Sequentially::after_phase_1(Arc::new(h), phase1_failures);
    run.run_all(Step::Scenario, phase2).await;
    if only.is_some() {
        report_skipped("checks", &names_of(&checks));
    } else {
        run.run_all(Step::Check, checks).await;
    }
    run.finish().await;
}

/// The names of `scenarios`.
fn names_of(scenarios: &[Scenario]) -> Vec<&'static str> {
    scenarios.iter().map(|(name, _)| *name).collect()
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
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
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
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account", "probe"]
    );
    let invocation = h.admin().invocation(reply.invocation_id()).await;
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
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    assert!(
        h.requests_mentioning("acct:check-account").await.is_empty(),
        "nothing reached szamlazz.hu"
    );
    eprintln!(
        "(canary) without protocol v7: scoped check_account → scope: null on the single-account deployment, unknown_account on the multi-account one: pass"
    );
    h.finish().await;
}

// ----- the E2E_ONLY filter, without a server ---------------------------------------

#[cfg(test)]
mod only_tests {
    use super::Only;

    /// The needles are comma-separated and trimmed; nothing left is no
    /// filter.
    #[test]
    fn e2e_only_parses_comma_separated_needles() {
        assert_eq!(
            Only::parse("storno, create_final,"),
            Some(Only(vec!["storno".to_owned(), "create_final".to_owned()]))
        );
        assert_eq!(Only::parse(""), None);
        assert_eq!(Only::parse(" , "), None);
    }

    /// A needle is a substring of the `family::scenario` name: a family
    /// selects every scenario of its file and any scenario naming it; the
    /// skipped keep their order for the report; no filter selects everything.
    #[test]
    fn e2e_only_selects_by_substring_and_reports_the_skipped() {
        let scenarios = || {
            vec![
                ("storno::storno_then_reissue", ()),
                (
                    "create_final::create_final_names_its_live_prepayment_invoice",
                    (),
                ),
                (
                    "agent_writes::agent_storno_and_set_credit_entries_run_on_the_scoped_account",
                    (),
                ),
                ("get::run_retries_do_not_spend_invocation_attempts", ()),
            ]
        };
        let only = Only::parse("storno").expect("a filter");
        let (selected, skipped) = Only::select(Some(&only), scenarios());
        assert_eq!(
            selected.iter().map(|(name, ())| *name).collect::<Vec<_>>(),
            [
                "storno::storno_then_reissue",
                "agent_writes::agent_storno_and_set_credit_entries_run_on_the_scoped_account",
            ]
        );
        assert_eq!(
            skipped,
            [
                "create_final::create_final_names_its_live_prepayment_invoice",
                "get::run_retries_do_not_spend_invocation_attempts",
            ]
        );
        let (selected, skipped) = Only::select(None, scenarios());
        assert_eq!(selected.len(), 4);
        assert!(skipped.is_empty());
        let none = Only::parse("nothing-named-so").expect("a filter");
        let (selected, skipped) = Only::select(Some(&none), scenarios());
        assert!(selected.is_empty());
        assert_eq!(
            skipped.len(),
            4,
            "the run refuses a filter that selects nothing"
        );
    }
}
