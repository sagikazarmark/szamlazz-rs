//! Expected retained state comes from scenario intent, never from SQL observations.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
pub(super) struct StateKey {
    pub(super) scope: Option<String>,
    pub(super) service_key: String,
    key: String,
}

impl StateKey {
    pub(super) fn unresolved(scope: Option<&str>, order: &str) -> Self {
        Self {
            scope: scope.map(str::to_owned),
            service_key: order.to_owned(),
            key: "unresolved-write".to_owned(),
        }
    }
}

pub(super) fn check_inventory(expected: &BTreeSet<StateKey>, rows: Vec<Value>) {
    let rows: Vec<StateKey> = rows
        .into_iter()
        .map(|row| serde_json::from_value(row).expect("scope, service_key and key state columns"))
        .collect();
    let actual: BTreeSet<_> = rows.iter().cloned().collect();
    assert_eq!(
        actual.len(),
        rows.len(),
        "duplicate Order state rows: {rows:?}"
    );
    let unexpected: Vec<_> = actual.difference(expected).collect();
    let missing: Vec<_> = expected.difference(&actual).collect();
    assert!(
        unexpected.is_empty() && missing.is_empty(),
        "Order state inventory mismatch; unexpected: {unexpected:?}; missing: {missing:?}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(scope: Option<&str>, order: &str, key: &str) -> Value {
        json!({"scope": scope, "service_key": order, "key": key})
    }

    #[test]
    fn inventory_is_exact_across_scopes_and_state_keys() {
        let expected = BTreeSet::from([
            StateKey::unresolved(None, "same-order"),
            StateKey::unresolved(Some("beta"), "same-order"),
        ]);
        check_inventory(
            &expected,
            vec![
                row(Some("beta"), "same-order", "unresolved-write"),
                row(None, "same-order", "unresolved-write"),
            ],
        );
        for rows in [
            vec![row(None, "same-order", "unresolved-write")], // missing scoped marker
            vec![
                row(None, "same-order", "unresolved-write"),
                row(Some("beta"), "same-order", "unresolved-write"),
                row(None, "surprise", "unresolved-write"),
            ], // extra marker beside all expected ones
            vec![
                row(Some("acme"), "same-order", "unresolved-write"),
                row(None, "same-order", "unresolved-write"),
            ], // wrong scope
            vec![
                row(Some("beta"), "same-order", "unresolved-write"),
                row(None, "surprise", "unresolved-write"),
            ], // same count, wrong order
            vec![
                row(Some("beta"), "same-order", "unresolved-write"),
                row(None, "same-order", "other-state"),
            ],
        ] {
            assert!(std::panic::catch_unwind(|| check_inventory(&expected, rows)).is_err());
        }
    }

    #[test]
    fn unselected_scenarios_expect_no_state() {
        check_inventory(&BTreeSet::new(), vec![]);
        assert!(
            std::panic::catch_unwind(|| check_inventory(
                &BTreeSet::new(),
                vec![row(None, "surprise", "unresolved-write")]
            ))
            .is_err()
        );
    }
}
