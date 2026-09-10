//! Malformed evidence must not look like a successful, unscoped invocation.

use restate_e2e_harness::Invocation;
use serde_json::{Value, json};

#[test]
fn invocation_columns_distinguish_wire_nulls_from_malformed_evidence() {
    let valid = json!({
        "status": "future-status", "target_service_name": "Svc", "target_handler_name": "h",
        "completion_failure": null, "scope": null
    });
    let invocation = Invocation::from_row(&valid);
    assert_eq!(invocation.status, "future-status");
    assert_eq!(invocation.completion_failure, None);
    assert_eq!(invocation.scope, None);
    for column in [
        "status",
        "target_service_name",
        "target_handler_name",
        "completion_failure",
        "scope",
    ] {
        for value in [
            None,
            Some(json!(42)),
            Some(json!({"message": "leak-sentinel"})),
        ] {
            let mut row = valid.clone();
            row.as_object_mut().expect("an object row").remove(column);
            if let Some(value) = value {
                row[column] = value;
            }
            if row.get(column).is_none() && matches!(column, "completion_failure" | "scope") {
                assert_eq!(
                    Invocation::from_row(&row),
                    invocation,
                    "SQL nulls are omitted on the wire"
                );
            } else {
                assert!(
                    std::panic::catch_unwind(|| Invocation::from_row(&row)).is_err(),
                    "{column}: {row}"
                );
            }
        }
    }
    for column in ["status", "target_service_name", "target_handler_name"] {
        let mut row = valid.clone();
        row[column] = Value::Null;
        assert!(std::panic::catch_unwind(|| Invocation::from_row(&row)).is_err());
    }
    let mut row = valid;
    row["completion_failure"] = json!("leak-sentinel");
    row["scope"] = json!("scope-a");
    let invocation = Invocation::from_row(&row);
    assert_eq!(
        invocation.completion_failure.as_deref(),
        Some("leak-sentinel")
    );
    assert_eq!(invocation.scope.as_deref(), Some("scope-a"));
}
