//! The run-wide checks, last: the `Szamlazz.Order` object keeps no state, no
//! agent key in any journal of the run, and every handler journals its
//! steps in the table ([`TABLE`]);
//! and, in phase 1, the leak scan's positive control planted.

use rust_decimal::dec;

use crate::harness::accounts::AGENT_KEYS;
use crate::harness::run_names::TABLE;
use crate::harness::szamlazz::{api_error, create_for, not_found, order_query};
use crate::harness::{Harness, SERVICES, create_body};

/// The sentinel the leak scan must find: planted in phase 1
/// ([`plant_the_leak_positive_control`]), looked for by
/// [`no_agent_key_in_any_journal_of_the_run`].
const POSITIVE_CONTROL: &str = "SENTINEL-8f3a2c-LEAK-CONTROL";

/// The leak scan's positive control (phase 1): a sentinel string in a
/// szamlazz.hu rejection's message travels into the create run's journaled
/// result and the output, and nowhere else (the lookup's result never saw
/// it), so a scan that finds no agent key is known to read real bytes. Under
/// journal v2 the `Command: Run` row carries the name and completion id; the
/// result is in the notification with that id, which `run_result` reads.
pub(crate) async fn plant_the_leak_positive_control(h: &Harness) {
    recovery_paths(h).await;
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
            &create_body(dec!(1000)),
            "e2e-12-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "rejected", "{}", reply.body);
    assert_eq!(reply.body["code"], "259", "{}", reply.body);
    assert_eq!(reply.body["message"], POSITIVE_CONTROL);

    let journal = h.admin().journal(reply.invocation_id()).await;
    let create_result = restate_e2e_harness::run_result(&journal, "create-invoice")
        .unwrap_or_else(|| panic!("the create-invoice run's result entry: {journal:?}"));
    assert!(
        create_result.raw_contains(POSITIVE_CONTROL),
        "the sentinel is found in the hex-decoded raw of entry {}: {:?}",
        create_result.index,
        String::from_utf8_lossy(&create_result.raw)
    );
    // The ownership and full lookup share this name; inspect both results.
    for occurrence in 0..2 {
        let lookup_result =
            restate_e2e_harness::run_result_at(&journal, "lookup-invoice", occurrence)
                .expect("the lookup's result");
        assert!(
            !lookup_result.raw_contains(POSITIVE_CONTROL),
            "the sentinel is not in an entry it did not pass through"
        );
    }
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

async fn recovery_paths(h: &Harness) {
    use crate::harness::szamlazz::{Doc, external_id_query};
    use restate_e2e_harness::Call;
    use serde_json::json;
    let key = "E2E-RECOVERY";
    h.absent(key, &["invoice", "prepayment", "final", "proforma"])
        .await;
    order_query(key)
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for(key)
        .respond_with(wiremock::ResponseTemplate::new(500))
        .expect(2)
        .mount(&h.mock)
        .await;
    let create = Call::object("Szamlazz.Order", key, "create_invoice");
    let observe = Call::object("Szamlazz.Order", key, "observe_unresolved");
    let recover = Call::object("Szamlazz.Order", key, "recover");
    let denied = h
        .invoke(
            &Call::object("Szamlazz.Order", "UNAUTHORIZED", "observe_unresolved"),
            None,
            None,
        )
        .await;
    assert_eq!(denied.status, 403);
    assert_eq!(
        denied.fault().code,
        restate_szamlazz::contract::TerminalCode::Forbidden
    );
    for (id, positive) in [("recovery-attested", false), ("recovery-positive", true)] {
        let body = create_body(dec!(1000));
        let submitted = h.invoke(&create.send(), Some(&body), Some(id)).await;
        h.admin()
            .await_status(submitted.invocation_id(), &["paused"])
            .await;
        let observed = h.invoke(&observe, None, None).await;
        assert_eq!(observed.body["state"], "unresolved");
        h.admin().kill(submitted.invocation_id()).await;
        let evidence = if positive {
            external_id_query("acct:E2E-RECOVERY:invoice")
                .respond_with(Doc::of("RECOVERED", "SZ", key).response())
                .with_priority(1)
                .mount(&h.mock)
                .await;
            json!({"type":"document", "number":"RECOVERED"})
        } else {
            json!({"type":"not_executed", "audit_reference":"INC-216", "did_not_execute_and_cannot_execute_later":true})
        };
        let response = h
            .invoke(
                &recover,
                Some(&json!({"marker":observed.body["marker"],"evidence":evidence})),
                None,
            )
            .await;
        assert_eq!(response.status, 200, "{}", response.body);
        assert_eq!(response.body["evidence"], evidence);
        assert_eq!(h.invoke(&observe, None, None).await.body["state"], "absent");
    }
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
        .admin()
        .all_invocations()
        .await
        .into_iter()
        .filter(|(_, invocation)| invocation.service == "Szamlazz.Order")
        .count();
    assert!(
        orders >= ENOUGH_ORDER_INVOCATIONS,
        "{orders} Szamlazz.Order invocations were run"
    );
    let state = h.admin().sql_or_panic("SELECT service_name, service_key, key FROM state WHERE service_name = 'Szamlazz.Order'")
        .await;
    assert!(!state.is_empty(), "cancelled writes retain markers");
    assert!(
        state.iter().all(|row| row["key"] == "unresolved-write"),
        "only uncertainty state: {state:?}"
    );
    eprintln!(
        "  ({} unresolved markers after {orders} invocations)",
        state.len()
    );
}

/// The leak check over the whole run: the hex-decoded `raw` of every journal
/// entry of every invocation the server holds, and every
/// `completion_failure`, contain none of the agent keys the run put on the
/// wire, while the scan does find the positive control's sentinel planted in
/// phase 1, so it reads real bytes.
pub(crate) async fn no_agent_key_in_any_journal_of_the_run(h: &Harness) {
    let journals = h.admin().all_journals().await;
    let invocations = h.admin().all_invocations().await;
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

/// The step-name table check over the whole run ([`TABLE`], the crate's
/// [`Table::check`](restate_e2e_harness::Table::check)): for every invocation
/// the server still holds, the `ctx.run` names in journal order are a prefix
/// of one of its handler's paths; every handler the deployments offer
/// (`GET /services`, both services) or an invocation names is in the table,
/// so a handler added with neither a row nor a scenario is not invisible; and
/// every path was walked in full by at least one invocation. A renamed,
/// inserted, reordered or dropped step, on any handler of either service,
/// fails here and shows in the table's diff: a regression signal when reviewing
/// exceptional replay, not proof of a particular old invocation's branch or
/// exact commands (ADR 0009). The floor
/// of the suite: a scenario that is the only walker of a path stays, however
/// plain its decision.
pub(crate) async fn every_handler_journals_its_tabled_steps(h: &Harness) {
    let deployed = h.admin().handlers().await;
    for service in SERVICES {
        assert!(
            deployed.iter().any(|handler| handler.service == service),
            "{service} is registered: {deployed:?}"
        );
    }
    let journals = h.admin().all_journals().await;
    let invocations = h.admin().all_invocations().await;
    let walked = TABLE
        .check(&deployed, &invocations, &journals)
        .unwrap_or_else(|violations| panic!("{violations}"));
    eprintln!("  ({walked})");
}
