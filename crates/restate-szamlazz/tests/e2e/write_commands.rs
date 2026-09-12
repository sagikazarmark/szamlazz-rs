//! Complete state/run ordering of the protected protocol, inspected through admin.
use restate_e2e_harness::JournalEntry;

/// A successful protected write or recovery must actually clear its marker,
/// not merely order a clearance correctly if one happens to exist.
pub(crate) fn check_settled(journal: &[JournalEntry], settlement: &str) {
    assert!(
        !journal.is_empty(),
        "completed invocation journal is required"
    );
    check(journal);
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.entry_type.contains("ClearState"))
            .count(),
        1
    );
    let result =
        restate_e2e_harness::run_result(journal, settlement).expect("settlement completion");
    assert!(
        !result.raw_contains("Unresolved"),
        "uncertainty cannot clear the marker"
    );
    if settlement == "reconcile-write" {
        assert_eq!(
            journal
                .iter()
                .filter(|entry| entry.entry_type.contains("SetState"))
                .count(),
            1
        );
        assert!(
            result.raw_contains("Reconciled") || result.raw_contains("Reversed"),
            "matching positive document evidence"
        );
    }
}

pub(crate) fn check(journal: &[JournalEntry]) {
    let command = |name: &str| {
        journal
            .iter()
            .find(|entry| entry.is_run() && entry.name.as_deref() == Some(name))
    };
    let completion = |run: &JournalEntry| {
        assert!(run.run_completion_id.is_some(), "run completion identity");
        journal
            .iter()
            .find(|entry| {
                entry.entry_type == "Notification: Run"
                    && entry.run_completion_id == run.run_completion_id
            })
            .expect("recorded run completion")
            .index
    };
    let states: Vec<_> = journal
        .iter()
        .filter(|entry| entry.entry_type.contains("SetState"))
        .collect();
    assert!(states.len() <= 1, "one unresolved marker per invocation");
    let clears: Vec<_> = journal
        .iter()
        .filter(|entry| entry.entry_type.contains("ClearState"))
        .collect();
    assert!(clears.len() <= 1, "one marker clearance");
    if command("arm-write").is_some() {
        assert_eq!(states.len(), 1, "arming requires a durable marker");
    }
    if let Some(marker) = states.first() {
        assert!(marker.raw_contains("unresolved-write"));
        let prepared = command("prepare-write").expect("marker preparation");
        assert!(completion(prepared) < marker.index);
        let read = journal
            .iter()
            .find(|entry| entry.entry_type.contains("Get") && entry.entry_type.contains("State"))
            .expect("entry marker guard");
        assert!(read.raw_contains("unresolved-write"));
        assert!(read.index < prepared.index);
        if let Some(arm) = command("arm-write") {
            assert!(marker.index < arm.index);
            let write = journal.iter().find(|entry| {
                entry.is_run()
                    && entry.name.as_deref().is_some_and(|name| {
                        name.starts_with("create-")
                            || name.starts_with("storno-")
                            || name.starts_with("delete-proforma-")
                    })
            });
            if let Some(write) = write {
                assert!(completion(arm) < write.index);
                if let Some(reconcile) = command("reconcile-write") {
                    assert!(completion(write) < reconcile.index);
                }
            }
            if let Some(clear) = clears.first() {
                let settled = command("reconcile-write")
                    .or(write)
                    .expect("settlement run");
                assert!(completion(settled) < clear.index);
            }
        } else {
            assert!(clears.is_empty());
        }
    } else if let Some(clear) = clears.first() {
        let recovery = command("record-recovery").expect("operator settlement before clear");
        assert!(completion(recovery) < clear.index);
    }
    for clear in clears {
        assert!(clear.raw_contains("unresolved-write"));
    }
}

/// Every create adapter traverses read-only reconciliation and records its
/// settlement before state clearance. The table sees these paths in the main run.
pub(crate) async fn all_create_kinds_record_reconciliation_before_clear(
    h: &crate::harness::Harness,
) {
    use crate::common::{Doc, create_for, external_id_query, not_found, number_query, order_query};
    use rust_decimal::dec;
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use wiremock::ResponseTemplate;
    for (handler, kind, token) in [
        ("create_proforma", "proforma", "D"),
        ("create_invoice", "invoice", "SZ"),
        ("create_prepayment", "prepayment", "ES"),
        ("create_final", "final", "VS"),
        ("correct_invoice", "corrective:c1", "HS"),
    ] {
        let key = format!("COMMANDS-{token}");
        let visible = Arc::new(AtomicBool::new(false));
        let state = visible.clone();
        let order = key.clone();
        external_id_query(&format!("acct:{key}:{kind}"))
            .respond_with(move |_: &wiremock::Request| {
                if state.load(Ordering::SeqCst) {
                    Doc {
                        referenced_invoice: (token == "HS").then_some("COMMANDS-BASE"),
                        ..Doc::of("COMMANDS-ISSUED", token, &order)
                    }
                    .response()
                } else {
                    not_found()
                }
            })
            .mount(&h.mock)
            .await;
        for other in ["proforma", "invoice", "prepayment", "final"] {
            if other != kind {
                external_id_query(&format!("acct:{key}:{other}"))
                    .respond_with(if token == "VS" && other == "prepayment" {
                        Doc::of("COMMANDS-ADVANCE", "ES", &key).response()
                    } else {
                        not_found()
                    })
                    .mount(&h.mock)
                    .await;
            }
        }
        order_query(&key)
            .respond_with(not_found())
            .mount(&h.mock)
            .await;
        if token == "HS" {
            number_query("COMMANDS-BASE")
                .respond_with(Doc::of("COMMANDS-BASE", "SZ", &key).response())
                .mount(&h.mock)
                .await;
        }
        create_for(&key)
            .respond_with(move |_: &wiremock::Request| {
                visible.store(true, Ordering::SeqCst);
                ResponseTemplate::new(500)
            })
            .expect(1)
            .mount(&h.mock)
            .await;
        let mut body = crate::harness::create_body(dec!(1000));
        if token == "HS" {
            body = json!({"invoice_number":"COMMANDS-BASE","correction_id":"c1","document":body["document"]});
        }
        let reply = h.call(&key, handler, &body, &key).await;
        assert_eq!(reply.body["outcome"], "reconciled", "{}", reply.body);
        check_settled(
            &h.admin().journal(reply.invocation_id()).await,
            "reconcile-write",
        );
        h.assert_state_absent(None, &key).await;
    }
}
