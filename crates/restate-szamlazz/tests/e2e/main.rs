//! End-to-end tests of the `Szamlazz.Order` Virtual Object and the
//! `Szamlazz.Agent` service against a real Restate server with wiremock
//! standing in for szamlazz.hu.
//!
//! The two end-to-end tests are ignored by default:
//! `cargo test -p restate-szamlazz --test e2e -- --ignored`.
//! The server comes from the environment, decided once (the server gate,
//! [`harness::gate`]): `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a
//! running server (with the three experimental flags; `compose.yaml` sets
//! them; the main suite only), `RESTATE_SERVER_BIN` names a `restate-server`
//! binary the harness spawns on the loopback (what the Dagger check does),
//! otherwise a docker daemon runs a container of the Restate image. With none
//! of them the suite skips with a message, and fails when `CI` is set, since
//! a skipped run in CI proves nothing. A server the harness starts binds
//! ports chosen free at launch, none fixed, so two runs on one host collide
//! with nothing; it is stopped when the run ends and on a SIGINT or SIGTERM to
//! the test process. Unix only, as `restate-server` itself is: the server's
//! process group, the stop signals and the liveness check behind the stale
//! container removal are.
//! The harness's own tests (the server gate, the sampler's decision, the
//! fetch hold, the run-pattern matching, the stub helpers) live beside what
//! they test under [`harness`], need only wiremock and run un-ignored.
//!
//! One binary, one tree: this file holds the two tests and the order the
//! scenarios run in; [`harness`] is everything the scenarios drive; and every
//! other module is one handler family's scenarios: the creates
//! ([`create_invoice`], [`create_proforma`], [`create_prepayment`],
//! [`create_final`], [`correct_invoice`]), [`storno`] and [`delete_proforma`],
//! [`get`], the issue and read policies at the two steps of issuing, and a
//! cancellation mid-send ([`policies`]), the order-key lock and the in-flight
//! `Idempotency-Key` under one scope ([`concurrency`]), the `Szamlazz.Agent`
//! reads and writes ([`agent_reads`], [`agent_writes`]), the contract's
//! refusals ([`faults`]), the prologue's account steps ([`prologue`]), the
//! flag day and scope isolation ([`multi_account`]) and the run-wide pins
//! ([`pins`]). A scenario is a `pub(crate) async fn` taking the harness; a
//! family file is where a new scenario of that handler goes.
//!
//! The main run has two phases on one Restate server. The first registers a
//! **single-account** deployment (the static resolver's `[account]` behind a
//! scripted resolver and store) and runs the order protocol unscoped. The
//! second performs the documented single → multi **flag day** (private,
//! drain, register the **multi-account** deployment (two accounts, reachable
//! by scope only, behind a test-local mutable resolver and store), public)
//! and proves the isolation properties multi-account mode leans on: the same
//! order key issuing concurrently under two scopes with each account's own
//! key on the wire, the same `Idempotency-Key` under two scopes being two
//! invocations, and, under one scope, the order-key lock and the in-flight
//! attach (the first invocation held at its credential fetch, which the
//! mutable store of this phase can do), credential rotation and account
//! changes between executions,
//! an order Restate has no memory of, and (over the hex-decoded `raw` of
//! every journal entry of every invocation in the run) that no agent key
//! was ever journaled; and, last, that every invocation's `ctx.run` names are
//! a prefix of its handler's pinned path ([`harness::run_names::RUN_NAMES`]).
//! The second test is the protocol-v7 canary on a server of its own without
//! the flag.
//!
//! The harness calls through the `/restate/call/…` and
//! `/restate/scope/{scope}/call/…` ingress paths (and `/restate/send/…` for a
//! call it does not wait for), reports the invocation id (`x-restate-id`) and
//! parses fault bodies, reads `sys_journal` / `sys_invocation` through the
//! SQL introspection API (`raw` hex-decoded to bytes, since run results are
//! stored as bytes), and purges, kills or cancels invocations through the
//! admin API.
//! What szamlazz.hu holds is stated per document
//! ([`harness::Harness::holds`] and its siblings), so one `<szamla>` body
//! answers every selector the document is reachable by; every mock's
//! `expect(n)` is verified at the next scenario's [`harness::Harness::reset`]
//! (the last scenario's when the harness is dropped).

#![cfg(unix)]

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

use restate_szamlazz::contract::TerminalCode;
use serde_json::json;

use crate::harness::Harness;
use crate::harness::accounts::AGENT_KEY;
use crate::harness::gate::{MAIN_SERVER, Reuse, WITHOUT_PROTOCOL_V7, launcher_or_skip};
use crate::harness::szamlazz::{not_found, probe_with_key};

#[tokio::test]
#[ignore = "needs a Restate server: docker, RESTATE_SERVER_BIN or RESTATE_ADMIN_URL / RESTATE_INGRESS_URL"]
async fn e2e_order_protocol() {
    let Some(launcher) = launcher_or_skip(Reuse::Allowed) else {
        return;
    };
    let mut h = Harness::start(launcher.launch(&MAIN_SERVER)).await;

    // Phase 1: the single-account deployment, unscoped.
    create_invoice::issued_then_already_issued(&h).await;
    create_invoice::idempotency_key_replays_without_calling_szamlazz(&h).await;
    create_invoice::duplicate_order_number_reconciles(&h).await;
    create_invoice::duplicate_order_number_with_nothing_of_ours_is_a_settled_conflict(&h).await;
    storno::storno_then_stale_create_then_reissue(&h).await;
    storno::storno_repeats_the_originals_fulfillment_date_or_refuses(&h).await;
    storno::storno_rejections_and_exhaustion_at_the_orders_handler(&h).await;
    create_invoice::reissue_on_live_is_a_conflict(&h).await;
    create_invoice::external_reversal_detected(&h).await;
    create_invoice::reversal_between_executions_is_reversed_not_reissued(&h).await;
    create_invoice::lost_create_reply_is_settled_by_the_immediate_requery(&h).await;
    create_proforma::proforma_auto_link_and_consumed(&h).await;
    create_invoice::proforma_by_number_is_checked_like_every_found_document(&h).await;
    correct_invoice::corrective_is_issued_under_its_correction_id(&h).await;
    correct_invoice::correctives_verify_their_base_and_take_no_hint(&h).await;
    correct_invoice::a_duplicate_order_number_on_a_corrective_is_rejected(&h).await;
    delete_proforma::proforma_is_deleted_by_the_orders_handler(&h).await;
    delete_proforma::delete_proforma_guards_paid_proformas_and_settles_every_answer(&h).await;
    get::status_shape(&h).await;
    create_invoice::secondary_lookup_collision_refuses_to_create(&h).await;
    create_prepayment::prepayment_converts_the_proforma_like_the_invoice(&h).await;
    create_proforma::proforma_after_the_orders_invoice_is_order_invoiced_not_foreign(&h).await;
    create_final::a_live_final_closes_the_order_to_the_other_creates(&h).await;
    create_prepayment::the_other_chains_live_document_refuses_the_create(&h).await;
    create_invoice::the_invoices_proforma_link_is_settled_before_any_send(&h).await;
    create_final::create_final_settles_its_prepayment_invoice_first(&h).await;
    faults::a_malformed_body_is_a_structured_invalid_input(&h).await;
    faults::an_untrimmed_order_key_is_refused(&h).await;
    faults::bounded_inputs_are_refused_and_disturb_no_other_invocation(&h).await;
    policies::exhausted_create_step_is_a_structured_outcome_unknown(&h).await;
    policies::after_an_outcome_unknown_the_next_call_answers_already_issued(&h).await;
    policies::a_cancellation_mid_send_is_outcome_unknown_and_releases_the_key(&h).await;
    policies::flaky_lookup_read_is_retried_by_the_read_policy(&h).await;
    policies::exhausted_lookup_read_is_a_structured_unavailable(&h).await;
    policies::answered_code_on_the_create_leading_query_is_an_immediate_unavailable(&h).await;
    policies::an_answered_code_on_the_hint_is_inconclusive_and_the_create_proceeds(&h).await;
    get::flaky_get_read_is_retried_by_the_read_policy(&h).await;
    get::run_retries_do_not_spend_invocation_attempts(&h).await;
    prologue::harness_scoped_call_and_leak_positive_control(&h).await;
    agent_reads::check_account_names_the_account_and_reports_the_credentials(&h).await;
    get::purged_invocation_queries_szamlazz_again(&h).await;
    prologue::flaky_resolver_is_retried_by_the_resolve_policy(&h).await;
    prologue::failing_credential_store_is_a_terminal_unavailable(&h).await;
    prologue::a_killed_invocation_releases_the_order_key(&h).await;

    // Phase 2: the flag day, then the multi-account deployment by scope.
    multi_account::flag_day_keeps_the_documents_and_refuses_unscoped_calls(&mut h).await;
    multi_account::same_order_key_under_two_scopes_issues_on_both_accounts(&h).await;
    multi_account::same_idempotency_key_under_two_scopes_is_two_invocations(&h).await;
    concurrency::same_key_same_scope_concurrent_creates_issue_once(&h).await;
    concurrency::same_key_same_scope_second_call_between_the_first_calls_executions(&h).await;
    concurrency::same_idempotency_key_in_flight_attaches_to_the_invocation(&h).await;
    agent_reads::check_account_under_each_scope_names_its_account(&h).await;
    storno::purged_order_is_stornoed_and_reissued(&h).await;
    agent_writes::agent_storno_acts_on_what_the_verify_finds(&h).await;
    agent_reads::agent_query_projects_what_it_finds(&h).await;
    faults::every_fault_carries_a_terminal_code_and_the_szamlazz_code_beside_it(&h).await;
    agent_writes::set_payments_replaces_or_appends_and_answers_a_lost_reply(&h).await;
    agent_reads::agent_query_taxpayer_runs_on_the_scoped_account(&h).await;
    agent_writes::agent_storno_repeats_the_originals_fulfillment_date_or_refuses(&h).await;
    storno::storno_is_issued_in_the_originals_form_not_the_accounts_default(&h).await;
    multi_account::account_change_between_executions_does_not_reach_the_invocation(&h).await;
    multi_account::credential_rotation_between_executions_is_picked_up(&h).await;
    pins::the_order_keeps_no_state(&h).await;
    pins::no_agent_key_in_any_journal_of_the_run(&h).await;
    pins::every_handler_journals_its_pinned_run_names(&h).await;
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
#[ignore = "needs a Restate server: docker or RESTATE_SERVER_BIN"]
async fn e2e_check_account_without_protocol_v7() {
    let Some(launcher) = launcher_or_skip(Reuse::Never) else {
        return;
    };
    let mut h = Harness::start(launcher.launch(&WITHOUT_PROTOCOL_V7)).await;

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
    assert_eq!(h.requests_seen().await, 1, "one probe query, nothing else");
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
    assert_eq!(h.requests_seen().await, 0, "nothing reached szamlazz.hu");
    eprintln!(
        "(canary) without protocol v7: scoped check_account → scope: null on the single-account deployment, unknown_account on the multi-account one: pass"
    );
}
