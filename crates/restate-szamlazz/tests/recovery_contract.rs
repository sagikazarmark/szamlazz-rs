//! Public recovery wire contract: exact identity and fail-closed evidence.
use restate_szamlazz::contract::recovery::{RecoveryRequest, UnresolvedWrite};
use serde_json::json;

fn marker() -> serde_json::Value {
    json!({
        "version": 1, "token": "inv-owner", "owner_invocation": "inv-owner",
        "created_at": "2026-09-10T12:00:00Z", "scope": "shop", "order": "ORD-1",
        "namespace": "acct", "external_id": "acct:ORD-1:corrective:c1",
        "account_id": "seller", "endpoint": "https://www.szamlazz.hu/szamla/",
        "credential_ref": "seller-key",
        "operation": {"type": "create", "kind": "corrective", "expected_number": null,
                      "corrected_number": "BASE"}
    })
}

#[test]
fn marker_identity_round_trips_and_unknown_state_fails_closed() {
    let wire = marker();
    let parsed: UnresolvedWrite = serde_json::from_value(wire.clone()).expect("known marker");
    assert_eq!(
        serde_json::to_value(parsed).expect("marker serializes"),
        wire
    );
    for (field, value) in [("version", json!(2)), ("future_permission", json!(true))] {
        let mut unknown = marker();
        unknown[field] = value;
        assert!(serde_json::from_value::<UnresolvedWrite>(unknown).is_err());
    }
}

#[test]
fn recovery_requires_exact_marker_and_explicit_evidence() {
    let request = json!({"marker": marker(), "evidence": {
        "type": "not_executed", "audit_reference": "INC-216",
        "did_not_execute_and_cannot_execute_later": true
    }});
    assert!(serde_json::from_value::<RecoveryRequest>(request.clone()).is_ok());
    for evidence in [
        json!({"type": "forget"}),
        json!({"type": "elapsed", "seconds": 3600}),
        json!({"type": "not_executed", "audit_reference": "INC-216",
               "did_not_execute_and_cannot_execute_later": false}),
    ] {
        let mut refused = request.clone();
        refused["evidence"] = evidence;
        assert!(serde_json::from_value::<RecoveryRequest>(refused).is_err());
    }
}

#[test]
fn completed_write_attestation_requires_an_explicit_conclusion_and_identity() {
    let evidence = json!({"type":"completed", "audit_reference":"INC-300",
        "completion":{"type":"issued", "number":"HS-1"},
        "completed_and_cannot_execute_later":true});
    let request = json!({"marker":marker(), "evidence":evidence});
    let parsed: RecoveryRequest =
        serde_json::from_value(request.clone()).expect("completed attestation");
    assert_eq!(serde_json::to_value(parsed).expect("round trip"), request);
    for replacement in [json!(false), json!(null), json!("true")] {
        let mut refused = request.clone();
        refused["evidence"]["completed_and_cannot_execute_later"] = replacement;
        assert!(serde_json::from_value::<RecoveryRequest>(refused).is_err());
    }
    for completion in [
        json!({"type":"issued"}),
        json!({"type":"issued","number":" "}),
        json!({"type":"unknown","number":"HS-1"}),
    ] {
        let mut refused = request.clone();
        refused["evidence"]["completion"] = completion;
        assert!(serde_json::from_value::<RecoveryRequest>(refused).is_err());
    }
}

#[test]
fn newer_observations_remain_inspectable_without_becoming_clearance() {
    use restate_szamlazz::contract::recovery::UnresolvedObservation;
    let mut newer = marker();
    newer["version"] = json!(2);
    for wire in [
        json!({"state":"unresolved","marker":newer}),
        json!({"state":"held_by_operator","incident":"INC-216"}),
    ] {
        let observed: UnresolvedObservation =
            serde_json::from_value(wire.clone()).expect("open observation");
        assert!(matches!(observed, UnresolvedObservation::Other { .. }));
        assert_eq!(serde_json::to_value(observed).expect("observation"), wire);
    }
}

#[cfg(feature = "schemars")]
#[test]
fn recovery_discovery_requires_the_same_version_and_attestation_as_the_decoder() {
    use restate_szamlazz::contract::recovery::{MarkerVersion, NonExecutionAttestation};
    let version = serde_json::to_value(
        schemars::generate::SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<MarkerVersion>(),
    )
    .expect("schema");
    assert_eq!(version["type"], "integer");
    assert_eq!(version["const"], 1);
    let attestation = serde_json::to_value(
        schemars::generate::SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<NonExecutionAttestation>(),
    )
    .expect("schema");
    assert_eq!(attestation["type"], "boolean");
    assert_eq!(attestation["const"], true);
}
