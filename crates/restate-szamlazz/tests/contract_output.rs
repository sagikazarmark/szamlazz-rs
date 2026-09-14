//! Output discovery describes emitted money; readers remain exact and tolerant.
#![cfg(feature = "schemars")]

use restate_szamlazz::contract::recovery::UnresolvedObservation;
use restate_szamlazz::contract::{
    CreateResponse, DocumentKind, DocumentStatus, OrderStatus, QueryResponse, RecordedCreditEntry,
    SetCreditEntriesResponse, VatTotal,
};
use schemars::{JsonSchema, generate::SchemaSettings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn schema<T: JsonSchema>() -> Value {
    serde_json::to_value(
        SchemaSettings::draft2020_12()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<T>(),
    )
    .expect("output schema")
}

#[test]
fn status_reports_colliding_kinds_separately_from_null_document_slots() {
    let wire = json!({
        "proforma": null, "invoice": null, "prepayment": null, "final": null,
        "collisions": ["proforma", "invoice"]
    });
    let status: OrderStatus = serde_json::from_value(wire.clone()).expect("status");
    assert_eq!(
        status.collisions,
        [DocumentKind::Proforma, DocumentKind::Invoice]
    );
    assert_eq!(serde_json::to_value(status).expect("encode"), wire);
    let older: OrderStatus = serde_json::from_value(json!({})).expect("older status");
    assert!(older.collisions.is_empty());
    let order_schema = schema::<OrderStatus>();
    assert_eq!(order_schema["properties"]["collisions"]["type"], "array");
    assert!(
        schema::<DocumentStatus>()["properties"]
            .get("credit_entries")
            .is_none()
    );
}

fn money_schema(schema: &Value, optional: bool) {
    assert_eq!(
        schema["type"],
        if optional {
            json!(["string", "null"])
        } else {
            json!("string")
        }
    );
    let pattern = regex::Regex::new(schema["pattern"].as_str().expect("decimal grammar"))
        .expect("valid pattern");
    let forbidden = regex::Regex::new(schema["not"]["pattern"].as_str().expect("line endings"))
        .expect("valid exclusion");
    for valid in [
        "0",
        "-0.00",
        "79228162514264337593543950335",
        "0.0000000000000000000000000001",
    ] {
        assert!(
            pattern.is_match(valid) && !forbidden.is_match(valid),
            "{valid}"
        );
    }
    for invalid in [
        "",
        "NaN",
        "Infinity",
        "+1",
        "1e2",
        ".1",
        "1.",
        " 1",
        "1\n",
        "1\u{2028}",
    ] {
        assert!(
            !pattern.is_match(invalid) || forbidden.is_match(invalid),
            "{invalid:?}"
        );
    }
    if optional {
        assert_eq!(schema["not"]["type"], "string", "null must survive not");
    }
}

#[test]
fn every_response_money_field_is_a_string_or_nullable_string() {
    for (schema, fields) in [
        (
            schema::<QueryResponse>(),
            &["net_total", "vat_total", "gross_total", "outstanding"][..],
        ),
        (
            schema::<CreateResponse>(),
            &["net_total", "gross_total", "outstanding"][..],
        ),
        (
            schema::<SetCreditEntriesResponse>(),
            &["gross_total", "outstanding"][..],
        ),
        (schema::<DocumentStatus>(), &["gross", "net"][..]),
    ] {
        for field in fields {
            money_schema(&schema["properties"][field], true);
        }
    }
    for field in ["net", "vat", "gross"] {
        money_schema(&schema::<VatTotal>()["properties"][field], false);
    }
    money_schema(
        &schema::<RecordedCreditEntry>()["properties"]["amount"],
        false,
    );
    let status = schema::<DocumentStatus>();
    assert_eq!(
        status["properties"]["credit_entry_amounts"]["type"],
        "array"
    );
    money_schema(
        &status["properties"]["credit_entry_amounts"]["items"],
        false,
    );

    // Nested discovery must carry the same promises as the stand-alone types.
    let query = schema::<QueryResponse>();
    money_schema(&query["$defs"]["VatTotal"]["properties"]["net"], false);
    money_schema(
        &query["$defs"]["RecordedCreditEntry"]["properties"]["amount"],
        false,
    );
}

fn round_trip_money<T: DeserializeOwned + Serialize>(mut wire: Value, fields: &[&str]) {
    // Direct JSON text retains the full token (not an f64 approximation).
    for token in [
        "9007199254740993.01",
        "1e-28",
        "79228162514264337593543950335",
    ] {
        for field in fields {
            wire[field] = serde_json::from_str(token).expect("JSON number");
        }
        let response: T = serde_json::from_str(&wire.to_string()).expect("exact numeric response");
        let output = serde_json::to_value(response).expect("emitted response");
        let expected = restate_szamlazz::szamlazz_agent::parse_decimal(token).expect("exact");
        for field in fields {
            assert_eq!(output[field], expected.to_string(), "{field}");
        }
        let decoded: T = serde_json::from_value(output.clone()).expect("string response");
        assert_eq!(serde_json::to_value(decoded).expect("round trip"), output);
    }
    for field in fields {
        wire[field] = json!("0.00000000000000000000000000001");
    }
    assert!(
        serde_json::from_value::<T>(wire).is_err(),
        "never round excess precision"
    );
}

#[test]
fn output_schema_narrowing_does_not_narrow_exact_response_decoders() {
    round_trip_money::<QueryResponse>(
        json!({"invoice_number":"SZ-1", "document_type":"SZ"}),
        &["net_total", "vat_total", "gross_total", "outstanding"],
    );
    round_trip_money::<CreateResponse>(
        json!({"outcome":"issued", "kind":"invoice", "external_id":"acct:ORD:invoice"}),
        &["net_total", "gross_total", "outstanding"],
    );
    round_trip_money::<SetCreditEntriesResponse>(
        json!({"invoice_number":"SZ-1"}),
        &["gross_total", "outstanding"],
    );
    round_trip_money::<VatTotal>(json!({"vat_rate_code":"27"}), &["net", "vat", "gross"]);
    round_trip_money::<RecordedCreditEntry>(json!({}), &["amount"]);
    round_trip_money::<DocumentStatus>(json!({"number":"SZ-1", "state":"live"}), &["gross", "net"]);

    let status: DocumentStatus = serde_json::from_str(
        r#"{"number":"SZ-1","state":"live","credit_entry_amounts":[9007199254740993, "1e-28"]}"#,
    )
    .expect("buffered state retains exact integers and strings");
    let wire = serde_json::to_value(status).expect("status");
    assert_eq!(
        wire["credit_entry_amounts"],
        json!(["9007199254740993", "0.0000000000000000000000000001"])
    );
    assert_eq!(wire["gross"], Value::Null);
    assert_eq!(wire["net"], Value::Null);
    let absent = serde_json::to_value(QueryResponse::new("SZ-1", "SZ")).expect("query");
    for field in ["net_total", "vat_total", "gross_total", "outstanding"] {
        assert_eq!(absent[field], Value::Null);
    }
}

#[test]
fn observation_schema_requires_marker_without_closing_future_state() {
    let schema = schema::<UnresolvedObservation>();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["required"], json!(["state"]));
    assert_eq!(schema["properties"]["state"]["type"], "string");
    assert_eq!(
        schema["if"],
        json!({"properties":{"state":{"const":"unresolved"}}})
    );
    assert_eq!(schema["then"], json!({"required":["marker"]}));
    assert!(schema.get("additionalProperties").is_none());
    assert!(
        schema["properties"].get("marker").is_none(),
        "opaque future marker"
    );
    for wire in [
        json!({}),
        json!({"state":null}),
        json!({"state":42}),
        json!({"state":"unresolved"}),
    ] {
        assert!(serde_json::from_value::<UnresolvedObservation>(wire).is_err());
    }
    for wire in [
        json!({"state":"absent"}),
        json!({"state":"unreadable"}),
        json!({"state":"unresolved","marker":{"version":2,"new_intent":true}}),
        json!({"state":"unresolved","marker":null}),
        json!({"state":"held_by_operator","incident":{"number":216}}),
    ] {
        let decoded: UnresolvedObservation =
            serde_json::from_value(wire.clone()).expect("open observation");
        assert_eq!(serde_json::to_value(decoded).expect("preserved"), wire);
    }
}
