//! The run-wide checks, last: the `Szamlazz.Order` object keeps no state, no
//! agent key in any journal of the run, and every handler journals its
//! steps in the table ([`RUN_NAMES`](crate::harness::run_names::RUN_NAMES));
//! and, in phase 1, the leak scan's positive control planted.

use std::collections::BTreeSet;

use rust_decimal::dec;

use crate::harness::accounts::AGENT_KEYS;
use crate::harness::run_names::{RUN_NAMES, is_prefix_of_path, run_pattern};
use crate::harness::szamlazz::{api_error, create_for, not_found, order_query};
use crate::harness::{Harness, create_body};

/// The sentinel the leak scan must find: planted in phase 1
/// ([`plant_the_leak_positive_control`]), looked for by
/// [`no_agent_key_in_any_journal_of_the_run`].
const POSITIVE_CONTROL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";

/// The leak scan's positive control (phase 1): a sentinel string in a
/// szamlazz.hu rejection's message travels into the create run's journaled
/// result and the output, and nowhere else (the lookup's result never saw
/// it), so a scan that finds no agent key is known to read real bytes. Under
/// journal v2 the `Command: Run` row carries only the name; the result is in
/// the notification that follows, which `run_result` reads.
pub(crate) async fn plant_the_leak_positive_control(h: &Harness) {
    h.absent("E2E-12", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-12")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for("E2E-12")
        .respond_with(api_error("259", POSITIVE_CONTROL))
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
    assert_eq!(reply.body["code"], "259", "{}", reply.body);
    assert_eq!(reply.body["message"], POSITIVE_CONTROL);

    let journal = h.journal(reply.invocation_id()).await;
    let create_result = restate_e2e_harness::run_result(&journal, "create-invoice")
        .unwrap_or_else(|| panic!("the create-invoice run's result entry: {journal:?}"));
    assert!(
        create_result.raw_contains(POSITIVE_CONTROL),
        "the sentinel is found in the hex-decoded raw of entry {}: {:?}",
        create_result.index,
        String::from_utf8_lossy(&create_result.raw)
    );
    let lookup_result =
        restate_e2e_harness::run_result(&journal, "lookup-invoice").expect("the lookup's result");
    assert!(
        !lookup_result.raw_contains(POSITIVE_CONTROL),
        "the sentinel is not in an entry it did not pass through"
    );
    let leaked: Vec<u64> = journal
        .iter()
        .filter(|entry| entry.raw_contains(POSITIVE_CONTROL))
        .map(|entry| entry.index)
        .collect();
    assert_eq!(
        leaked,
        [create_result.index, journal.last().expect("output").index],
        "the sentinel is in exactly the create result and the output"
    );
}

/// The `Szamlazz.Order` object keeps no state: after every create, storno,
/// delete and read of the run, on both deployments, the `state` table holds
/// no row for the service; szamlazz.hu is the only record, and there is
/// nothing a redeploy could leave behind. Checked over the run's invocations
/// so that the empty table is not vacuous.
pub(crate) async fn the_order_keeps_no_state(h: &Harness) {
    // Under what the run issues; a floor against an empty table proving
    // nothing (a purge or a retention change emptying `sys_invocation`).
    const ENOUGH_ORDER_INVOCATIONS: usize = 30;
    let orders = h
        .all_invocations()
        .await
        .into_iter()
        .filter(|(_, invocation)| invocation.service == "Szamlazz.Order")
        .count();
    assert!(
        orders >= ENOUGH_ORDER_INVOCATIONS,
        "{orders} Szamlazz.Order invocations were run"
    );
    let state = h
        .sql("SELECT service_name, service_key, key FROM state WHERE service_name = 'Szamlazz.Order'")
        .await;
    assert!(
        state.is_empty(),
        "Szamlazz.Order keeps no state, yet the state table holds: {state:?}"
    );
    eprintln!("  (the state table holds nothing for Szamlazz.Order after {orders} invocations)");
}

/// The leak check over the whole run: the hex-decoded `raw` of every journal
/// entry of every invocation the server holds, and every
/// `completion_failure`, contain none of the agent keys the run put on the
/// wire, while the scan does find the positive control's sentinel planted in
/// phase 1, so it reads real bytes.
pub(crate) async fn no_agent_key_in_any_journal_of_the_run(h: &Harness) {
    let journals = h.all_journals().await;
    let invocations = h.all_invocations().await;
    assert!(
        journals.len() >= 20 && invocations.len() >= journals.len(),
        "the scan covers the run: {} journals, {} invocations",
        journals.len(),
        invocations.len()
    );
    let entries = journals.values().map(Vec::len).sum::<usize>();
    assert!(entries >= 100, "{entries} journal entries");

    let mut leaks = Vec::new();
    for (id, journal) in &journals {
        for entry in journal {
            for key in AGENT_KEYS {
                if entry.raw_contains(key) {
                    leaks.push(format!(
                        "{id} entry {} ({}, {:?}) contains {key}",
                        entry.index, entry.entry_type, entry.name
                    ));
                }
            }
        }
    }
    for (id, invocation) in &invocations {
        if let Some(failure) = &invocation.completion_failure {
            for key in AGENT_KEYS {
                if failure.contains(key) {
                    leaks.push(format!("{id} completion_failure contains {key}"));
                }
            }
        }
    }
    assert!(leaks.is_empty(), "agent keys in Restate: {leaks:#?}");

    assert!(
        journals
            .values()
            .flatten()
            .any(|entry| entry.raw_contains(POSITIVE_CONTROL)),
        "the positive control's sentinel is found by the same scan"
    );
    let scoped = invocations
        .iter()
        .filter(|(_, invocation)| invocation.scope.is_some())
        .count();
    assert!(scoped >= 8, "{scoped} scoped invocations were scanned");
    eprintln!(
        "  (no agent key in {entries} journal entries of {} invocations, {scoped} scoped; positive control found)",
        invocations.len()
    );
}

/// The step-name table check over the whole run ([`RUN_NAMES`]): for every
/// invocation the server still holds, the `ctx.run` names in journal order
/// are a prefix of one of its handler's paths, every handler seen is in the
/// table, and every path was walked in full by at least one invocation, so a
/// renamed, inserted, reordered or dropped step, on any handler of either
/// service, fails here and shows in the table's diff (the sequence half of
/// what a pause-and-resume onto a new deployment depends on; ADR 0009). The
/// floor of the suite: a
/// scenario that is the only walker of a path stays, however plain its
/// decision.
pub(crate) async fn every_handler_journals_its_tabled_steps(h: &Harness) {
    let journals = h.all_journals().await;
    let invocations = h.all_invocations().await;
    let mut unpinned = BTreeSet::new();
    let mut unexplained = Vec::new();
    let mut walked = BTreeSet::new();
    for (id, invocation) in &invocations {
        let observed: Vec<String> = journals
            .get(id)
            .map(|journal| {
                journal
                    .iter()
                    .filter(|entry| entry.is_run())
                    .filter_map(|entry| entry.name.as_deref())
                    .map(run_pattern)
                    .collect()
            })
            .unwrap_or_default();
        let target = format!("{}.{}", invocation.service, invocation.handler);
        let paths: Vec<(usize, &[&str])> = RUN_NAMES
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                row.service == invocation.service && row.handler == invocation.handler
            })
            .map(|(index, row)| (index, row.path))
            .collect();
        if paths.is_empty() {
            unpinned.insert(target);
            continue;
        }
        if !paths
            .iter()
            .any(|(_, path)| is_prefix_of_path(&observed, path))
        {
            unexplained.push(format!("{id} {target}: {observed:?}"));
        }
        for (row, path) in paths {
            if observed.len() == path.len() && is_prefix_of_path(&observed, path) {
                walked.insert(row);
            }
        }
    }
    assert!(
        unpinned.is_empty(),
        "handlers not in the table: {unpinned:?}; add their steps to RUN_NAMES"
    );
    assert!(
        unexplained.is_empty(),
        "run sequences no path of their handler explains:\n  {}\n\n\
         The table is the record of which steps a handler journals and in what order: the sequence \
         half of what a pause-and-resume of a stuck invocation onto a new deployment replays (ADR \
         0009; the result types and the inputs are the other half, reviewed by hand). Bring \
         RUN_NAMES to match the code; a changed row means such a resume across this release fails.",
        unexplained.join("\n  ")
    );
    let not_walked: Vec<String> = RUN_NAMES
        .iter()
        .enumerate()
        .filter(|(row, _)| !walked.contains(row))
        .map(|(_, row)| format!("{}.{}: {:?}", row.service, row.handler, row.path))
        .collect();
    assert!(
        not_walked.is_empty(),
        "paths no invocation of the run walked in full:\n  {}\n\n\
         Either a scenario must exercise the path or its last step was dropped from the handler.",
        not_walked.join("\n  ")
    );
    eprintln!(
        "  (every run sequence of {} invocations is a prefix of one of its handler's paths; all {} paths walked in full)",
        invocations.len(),
        RUN_NAMES.len()
    );
}
