//! Synthetic journal-v2 robustness cases, including interleavings outside
//! Rust SDK 0.12's immediate-await rule. Live coverage is in `e2e_smoke`.

use restate_e2e_harness::{JournalEntry, run_result};
use serde_json::{Value, json};

fn row(index: u64, entry_type: &str, name: Option<&str>, entry: &Value) -> JournalEntry {
    JournalEntry::from_row(&json!({
        "index": index, "version": 2, "entry_type": entry_type,
        "name": name, "entry_json": entry.to_string(), "raw": ""
    }))
}

fn command(index: u64, name: &str, completion: u32) -> JournalEntry {
    row(
        index,
        "Command: Run",
        Some(name),
        &json!({
            "Command": {"Run": {"completion_id": completion, "name": name}}
        }),
    )
}

fn notification(index: u64, completion: u32) -> JournalEntry {
    row(
        index,
        "Notification: Run",
        None,
        &json!({
            "Notification": {"Completion": {"Run": {
                "completion_id": completion, "result": {"Success": []}
            }}}
        }),
    )
}

#[test]
fn interleaved_runs_find_their_own_notifications_in_either_completion_order() {
    for (first, second) in [(41, 7), (7, 41)] {
        let journal = [
            command(1, "A", 41),
            command(2, "B", 7),
            notification(3, first),
            notification(4, second),
        ];
        assert_eq!(
            run_result(&journal, "A").expect("A completed").index,
            if first == 41 { 3 } else { 4 }
        );
        assert_eq!(
            run_result(&journal, "B").expect("B completed").index,
            if first == 7 { 3 } else { 4 }
        );
    }
}

#[test]
#[should_panic(expected = "ambiguous run name")]
fn a_repeated_name_requires_an_explicit_occurrence() {
    let journal = [
        command(1, "lookup", 0),
        notification(2, 0),
        command(3, "lookup", 1),
        notification(4, 1),
    ];
    let _ = run_result(&journal, "lookup");
}

#[test]
fn explicit_occurrences_select_sequential_runs_with_repeated_names() {
    use restate_e2e_harness::run_result_at;
    let journal = [
        command(1, "lookup", 0),
        notification(2, 0),
        command(3, "lookup", 1),
        notification(4, 1),
        command(5, "lookup", 2),
    ];
    assert_eq!(
        run_result_at(&journal, "lookup", 0).expect("first").index,
        2
    );
    assert_eq!(
        run_result_at(&journal, "lookup", 1).expect("second").index,
        4
    );
    assert!(run_result_at(&journal, "lookup", 2).is_none(), "incomplete");
    assert!(
        run_result_at(&journal, "lookup", 3).is_none(),
        "absent occurrence"
    );
    assert!(run_result(&journal, "absent").is_none());
}

#[test]
fn unsupported_or_ambiguous_evidence_is_never_read_as_a_result_or_absence() {
    let valid = vec![command(1, "A", 0), notification(2, 0)];
    let mut cases = Vec::new();
    for version in [None, Some(1), Some(3)] {
        let mut journal = valid.clone();
        journal[0].version = version;
        cases.push(journal);
    }
    for index in [0, 1] {
        let mut journal = valid.clone();
        journal[index].run_completion_id = None;
        cases.push(journal);
    }
    cases.push(vec![
        command(1, "A", 0),
        command(2, "B", 0),
        notification(3, 0),
    ]);
    cases.push(vec![
        command(1, "A", 0),
        notification(2, 0),
        notification(3, 0),
    ]);
    cases.push(vec![command(1, "A", 0), notification(2, 1)]);
    cases.push(vec![notification(1, 0), command(2, "A", 0)]);
    cases.push(vec![notification(2, 0), command(1, "A", 0)]);
    cases.push(vec![command(1, "A", 0), notification(1, 0)]);
    for journal in cases {
        assert!(
            std::panic::catch_unwind(|| run_result(&journal, "A")).is_err(),
            "unsupported journal must fail explicitly: {journal:?}"
        );
        assert!(
            std::panic::catch_unwind(|| run_result(&journal, "absent")).is_err(),
            "unsupported journal cannot establish absence: {journal:?}"
        );
    }
}

#[test]
fn unrelated_entries_and_another_runs_completion_do_not_complete_an_unfinished_run() {
    let journal = [
        command(1, "A", 9),
        row(
            2,
            "Command: Sleep",
            None,
            &json!({"Command": {"Sleep": {"completion_id": 2}}}),
        ),
        command(3, "B", 3),
        row(
            4,
            "Notification: Sleep",
            None,
            &json!({"Notification": {"Completion": {"Sleep": {"completion_id": 2}}}}),
        ),
        notification(5, 3),
        row(
            6,
            "Notification: Signal",
            None,
            &json!({"Notification": {"Signal": {"id": {"SignalIndex": 1}}}}),
        ),
    ];
    assert!(run_result(&journal, "A").is_none());
    assert_eq!(run_result(&journal, "B").expect("B completed").index, 5);
}

#[test]
fn missing_or_malformed_sql_identity_metadata_has_no_adjacency_fallback() {
    for metadata in [
        Value::Null,
        json!("not json"),
        json!(r#"{"Command":{"Run":{}}}"#),
        json!(r#"{"Command":{"Run":{"completion_id":"0"}}}"#),
        json!(r#"{"Command":{"Run":{"completion_id":4294967296}}}"#),
    ] {
        let command = JournalEntry::from_row(&json!({
            "index": 1, "version": 2, "entry_type": "Command: Run",
            "name": "A", "entry_json": metadata, "raw": ""
        }));
        let journal = [command, notification(2, 0)];
        assert!(std::panic::catch_unwind(|| run_result(&journal, "A")).is_err());
    }
}

#[test]
fn unavailable_raw_cannot_pass_a_negative_content_assertion() {
    for raw in [
        None,
        Some(Value::Null),
        Some(json!([115, 101, 99, 114, 101, 116])),
        Some(json!("zz")),
        Some(json!("+1")),
        Some(json!("+1+2")),
        Some(json!("é")),
    ] {
        let mut row = json!({
            "index": 2, "version": 2, "entry_type": "Notification: Run",
            "entry_json": json!({"Notification": {"Completion": {"Run": {
                "completion_id": 0, "result": {"Success": [115, 101, 99, 114, 101, 116]}
            }}}}).to_string()
        });
        if let Some(raw) = raw {
            row["raw"] = raw;
        }
        assert!(
            std::panic::catch_unwind(|| JournalEntry::from_row(&row)).is_err(),
            "{row}"
        );
    }
    // An explicitly supplied empty byte string remains usable evidence.
    let empty = row(
        0,
        "Command: Input",
        None,
        &json!({"Command": {"Input": {}}}),
    );
    assert!(empty.raw.is_empty());
    assert!(!empty.raw_contains("secret"));
}

#[tokio::test]
async fn unsupported_journals_cannot_establish_no_runs_or_table_conformance() {
    use restate_e2e_harness::{Admin, Handler, Invocation, RunPath, Table};
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

    const PATHS: &[RunPath] = &[RunPath::new("Svc", "h", &[])];
    let server = MockServer::start().await;
    let http = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .build()
        .expect("loopback client");
    let table = Table::new(PATHS);
    let handlers = [Handler {
        service: "Svc".into(),
        name: "h".into(),
    }];
    let invocations = [(
        "inv".into(),
        Invocation::from_row(&json!({
            "status": "completed", "target_service_name": "Svc", "target_handler_name": "h",
            "completion_failure": null, "scope": null
        })),
    )];
    for version in [Value::Null, json!(1), json!(3)] {
        server.reset().await;
        let row = json!({
            "index": 0, "version": version, "entry_type": "Run", "name": "unexpected", "raw": "00"
        });
        Mock::given(path("/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [row.clone()]})))
            .mount(&server)
            .await;
        let admin = Admin::new(server.uri(), http.clone());
        assert!(
            tokio::spawn(async move { admin.runs("inv").await })
                .await
                .expect_err("unsupported evidence must panic, not answer no runs")
                .is_panic()
        );
        let journals = [("inv".into(), vec![JournalEntry::from_row(&row)])].into();
        assert!(
            std::panic::catch_unwind(|| table.check(&handlers, &invocations, &journals)).is_err()
        );
    }
}

#[tokio::test]
async fn unavailable_names_and_unknown_classifications_cannot_erase_runs() {
    use restate_e2e_harness::{Admin, Handler, Invocation, RunPath, Table, run_result_at};
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

    const PATHS: &[RunPath] = &[RunPath::new("Svc", "h", &[])];
    let server = MockServer::start().await;
    let admin = Admin::new(server.uri(), reqwest::Client::new());
    let table = Table::new(PATHS);
    let handlers = [Handler {
        service: "Svc".into(),
        name: "h".into(),
    }];
    let invocations = [(
        "inv".into(),
        Invocation {
            status: "completed".into(),
            completion_failure: None,
            scope: None,
            service: "Svc".into(),
            handler: "h".into(),
        },
    )];
    let valid = json!({
        "index": 0, "version": 2, "entry_type": "Command: Run", "name": "A", "raw": "",
        "entry_json": r#"{"Command":{"Run":{"completion_id":0,"name":"A"}}}"#
    });
    let mut cases = Vec::new();
    for name in [None, Some(Value::Null), Some(json!(42)), Some(json!("B"))] {
        let mut row = valid.clone();
        row.as_object_mut().expect("an object row").remove("name");
        if let Some(name) = name {
            row["name"] = name;
        }
        cases.push(row);
    }
    for ty in [
        "Command: FutureRun",
        "Notification: FutureRun",
        "Command: Sleep",
        "Notification: Run",
    ] {
        let mut row = valid.clone();
        row["entry_type"] = json!(ty);
        cases.push(row);
    }
    for row in cases {
        assert!(
            std::panic::catch_unwind(|| JournalEntry::from_row(&row)).is_err(),
            "{row}"
        );
        server.reset().await;
        Mock::given(path("/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [row]})))
            .mount(&server)
            .await;
        let admin = admin.clone();
        assert!(
            tokio::spawn(async move { admin.runs("inv").await })
                .await
                .expect_err("unusable run evidence")
                .is_panic()
        );
    }
    // Plain public rows must obey the same rules during semantic inspection.
    for (ty, name) in [
        ("Command: Run", None),
        ("Command: FutureRun", Some("A")),
        ("Notification: FutureRun", None),
    ] {
        let mut entry = command(0, "A", 0);
        entry.entry_type = ty.into();
        entry.name = name.map(str::to_owned);
        let journal = vec![entry];
        for name in ["A", "absent"] {
            assert!(std::panic::catch_unwind(|| run_result(&journal, name)).is_err());
            assert!(std::panic::catch_unwind(|| run_result_at(&journal, name, 0)).is_err());
        }
        let journals = [("inv".into(), journal)].into();
        assert!(
            std::panic::catch_unwind(|| table.check(&handlers, &invocations, &journals)).is_err()
        );
    }
    let unnamed = [command(0, "", 0), notification(1, 0)];
    assert_eq!(
        run_result(&unnamed, "").expect("unnamed run result").index,
        1
    );
}

#[test]
fn missing_classification_cannot_hide_a_malformed_run() {
    for entry_type in [None, Some(Value::Null), Some(json!(42)), Some(json!(""))] {
        let mut row = json!({
            "index": 0, "version": 2, "name": "A", "raw": "00",
            "entry_json": r#"{"Command":{"Run":{}}}"#
        });
        if let Some(entry_type) = entry_type {
            row["entry_type"] = entry_type;
        }
        assert!(
            std::panic::catch_unwind(|| JournalEntry::from_row(&row)).is_err(),
            "{row}"
        );
    }
}
