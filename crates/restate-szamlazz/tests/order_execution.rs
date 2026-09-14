//! Public deployment selection is independent of host and test utilities.
use restate_szamlazz::{
    Order,
    account::{Accounts, StaticConfig, StaticResolver},
    config::{OrderExecution, WorkerConfig},
};
use serde_json::json;

#[test]
fn execution_selection_is_explicit_and_defaults_to_protected() {
    let config: WorkerConfig =
        serde_json::from_value(json!({"namespace":"deployment"})).expect("config");
    assert_eq!(config.order_execution, OrderExecution::Protected);
    assert_eq!(
        WorkerConfig::new("deployment".parse().expect("namespace")).order_execution,
        OrderExecution::Protected
    );
    let config: WorkerConfig = serde_json::from_value(
        json!({"namespace":"deployment","order_execution":"replay_enabled"}),
    )
    .expect("config");
    let resolver = StaticResolver::try_from(
        serde_json::from_value::<StaticConfig>(
            json!({"account":{"id":"test","agent_key":"NOT-A-REAL-KEY"}}),
        )
        .expect("account"),
    )
    .expect("resolver");
    let order = Order::from_parts(Accounts::from(resolver), config.validate().expect("valid"));
    assert_eq!(
        order.config().order_execution,
        OrderExecution::ReplayEnabled
    );
    assert!(
        serde_json::from_value::<WorkerConfig>(
            json!({"namespace":"deployment","order_execution":"wasm"})
        )
        .is_err()
    );
}
