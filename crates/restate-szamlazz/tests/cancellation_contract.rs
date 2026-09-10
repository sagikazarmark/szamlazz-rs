//! Caller-visible cancellation and uncertainty are independent facts.

use restate_szamlazz::contract::{Fault, FaultCause, TerminalCode};
use serde_json::json;

#[test]
fn cancellation_is_distinct_from_unavailability_without_erasing_write_uncertainty() {
    let read: Fault = serde_json::from_value(json!({
        "code": "cancelled", "message": "stopped"
    }))
    .expect("read fault");
    assert_eq!(read.code, TerminalCode::Cancelled);
    assert_eq!(read.status(), Some(409));
    assert_eq!(read.is_cancelled(), Some(true));
    assert_eq!(read.code.is_outcome_unknown(), Some(false));

    let write = Fault::new(TerminalCode::OutcomeUnknown, "may have landed")
        .with_cause(FaultCause::Cancelled);
    assert_eq!(write.status(), Some(500));
    assert_eq!(write.is_cancelled(), Some(true));
    assert_eq!(write.code.is_outcome_unknown(), Some(true));
    let wire = serde_json::to_value(&write).expect("serialize");
    assert_eq!(wire["cause"], "cancelled");
    assert_eq!(
        serde_json::from_value::<Fault>(wire).expect("decode"),
        write
    );

    let unavailable = Fault::new(TerminalCode::Unavailable, "dependency did not answer");
    assert_eq!(unavailable.is_cancelled(), Some(false));
    assert_eq!(unavailable.status(), Some(503));
    for wire in [
        json!({"code": "future", "message": "unknown"}),
        json!({"code": "outcome_unknown", "message": "unknown", "cause": "future"}),
    ] {
        let unknown: Fault = serde_json::from_value(wire.clone()).expect("open fault");
        assert_eq!(unknown.is_cancelled(), None);
        let encoded = serde_json::to_value(unknown).expect("preserve tokens");
        assert_eq!(encoded["code"], wire["code"]);
        assert_eq!(encoded["cause"], wire["cause"]);
    }
}
