//! Deployment configuration for explicit document-query retries.

use restate_szamlazz::config::{QueryConfig, WorkerConfig, WorkerConfigError};
use serde_json::json;

#[test]
fn explicit_query_policy_accepts_a_single_attempt_independently_of_reads() {
    let config: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct",
        "read": {"max_attempts": 3, "initial_delay": "1s"},
        "query": {"max_attempts": 1}
    }))
    .expect("independent query policy");
    let config = config.validate().expect("valid single-attempt deployment");
    assert_eq!(
        config.query.as_ref().expect("override").max_attempts,
        Some(1)
    );
    assert_eq!(config.read.max_attempts, Some(3));
}

#[test]
fn omitted_query_inherits_reads_while_a_present_table_has_its_own_defaults() {
    let absent: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct", "read": {"max_attempts": 2}
    }))
    .expect("old configuration");
    assert!(absent.query.is_none());
    let present: WorkerConfig = serde_json::from_value(json!({
        "namespace": "acct", "read": {"max_attempts": 2}, "query": {}
    }))
    .expect("explicit default query policy");
    assert_eq!(present.query, Some(QueryConfig::default()));
    assert_eq!(present.query.expect("query").max_attempts, Some(5));
}

#[test]
fn query_configuration_is_closed_and_validated_under_its_own_name() {
    assert!(
        serde_json::from_value::<WorkerConfig>(json!({
            "namespace": "acct", "query": {"max_atempts": 1}
        }))
        .is_err()
    );
    for (query, expected) in [
        (json!({"max_attempts": 0}), "attempts"),
        (json!({"initial_delay": "2m", "max_delay": "1m"}), "delay"),
        (json!({"factor": 0.5}), "factor"),
    ] {
        let config: WorkerConfig = serde_json::from_value(json!({
            "namespace": "acct", "query": query
        }))
        .expect("parse");
        match (
            config.validate().expect_err("invalid query policy"),
            expected,
        ) {
            (WorkerConfigError::ZeroMaxAttempts { table }, "attempts")
            | (WorkerConfigError::DelayOrder { table, .. }, "delay")
            | (WorkerConfigError::InvalidFactor { table, .. }, "factor") => {
                assert_eq!(table, "query");
            }
            (error, _) => panic!("wrong validation: {error}"),
        }
    }
}
