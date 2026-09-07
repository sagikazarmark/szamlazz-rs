//! The run-wide pins, last: the `Szamlazz.Order` object keeps no state, no
//! agent key in any journal of the run, and every handler journals its
//! pinned run names ([`RUN_NAMES`](crate::harness::run_names::RUN_NAMES)).

use std::collections::BTreeSet;

use crate::harness::Harness;
use crate::harness::accounts::AGENT_KEYS;
use crate::harness::run_names::{RUN_NAMES, is_prefix_of_path, run_pattern};

/// (xx-b) the `Szamlazz.Order` object keeps no state (design §3, ADR 0005):
/// after every create, storno, delete and read of the run, on both
/// deployments, the `state` table holds no row for the service — szamlazz.hu
/// is the only record, and there is nothing a redeploy could leave behind.
/// Checked over the run's invocations so that the empty table is not vacuous.
pub(crate) async fn the_order_keeps_no_state(h: &Harness) {
    // Far under what the run issues; a floor against an empty table proving
    // nothing (a purge or a retention change emptying `sys_invocation`).
    const ENOUGH_ORDER_INVOCATIONS: usize = 40;
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
    eprintln!(
        "(xx-b) the state table holds nothing for Szamlazz.Order after {orders} invocations: pass"
    );
}

/// (xxi) the leak check over the whole run: the hex-decoded `raw` of every
/// journal entry of every invocation the server holds, and every
/// `completion_failure`, contain none of the agent keys the run put on the
/// wire — while the scan does find the positive control's sentinel from
/// (xii), so it reads real bytes.
pub(crate) async fn no_agent_key_in_any_journal_of_the_run(h: &Harness) {
    const POSITIVE_CONTROL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";
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
        "(xxi) no agent key in {entries} journal entries of {} invocations ({scoped} scoped); positive control found: pass",
        invocations.len()
    );
}

/// (xxii) the run-name pin over the whole run ([`RUN_NAMES`]): for every
/// invocation the server still holds, the `ctx.run` names in journal order
/// are a prefix of one of its handler's pinned paths, every handler seen is
/// pinned, and every pinned path was walked in full by at least one
/// invocation — so a renamed, inserted, reordered or dropped step, on any
/// handler of either service, fails here rather than stranding an in-flight
/// invocation on the next deploy.
pub(crate) async fn every_handler_journals_its_pinned_run_names(h: &Harness) {
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
        "handlers with no pinned run names: {unpinned:?} — pin their steps in RUN_NAMES"
    );
    assert!(
        unexplained.is_empty(),
        "run sequences no pinned path of their handler explains:\n  {}\n\n\
         A renamed, inserted, reordered or dropped step strands every in-flight invocation of the \
         previous deployment on replay (ADR 0005). Keep the names and their order; a step that must \
         change is a new row in RUN_NAMES and a deploy that drains first, never an edited row.",
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
        "pinned paths no invocation of the run walked in full:\n  {}\n\n\
         Either a scenario must exercise the path or its last step was dropped from the handler.",
        not_walked.join("\n  ")
    );
    eprintln!(
        "(xxii) every run sequence of {} invocations is a prefix of its handler's pinned path; all {} paths walked in full: pass",
        invocations.len(),
        RUN_NAMES.len()
    );
}
