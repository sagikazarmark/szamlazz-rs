//! Strict civil-date input spelling is independent of Jiff's permissive parser.

use restate_szamlazz::contract::{CreditEntryInput, DocumentInput, QueryResponse};
use serde_json::{Value, json};

fn document() -> Value {
    json!({
        "buyer": {"name":"A", "zip":"1", "city":"B", "address":"C"},
        "items": [], "fulfillment_date":"2026-09-14",
        "due_date":"2026-09-21", "payment_method":"transfer"
    })
}

#[test]
fn caller_dates_require_canonical_calendar_dates() {
    for date in [
        json!("2026-09-14T23:59:00-12:00"),
        json!("2026-09-14[Europe/Budapest]"),
        json!("20260914"),
        json!("2026-9-14"),
        json!("2026-09-14 "),
        json!(" 2026-09-14"),
        json!("0000-01-01"),
        json!("-000001-01-01"),
        json!("+010000-01-01"),
        json!("2026-02-29"),
        json!("2026-13-01"),
        json!("2026-01-00"),
        json!("２０２６-09-14"),
        json!(20_260_914),
        json!([2026, 9, 14]),
    ] {
        for field in ["fulfillment_date", "due_date", "issue_date"] {
            let mut input = document();
            input[field] = date.clone();
            assert!(
                serde_json::from_value::<DocumentInput>(input).is_err(),
                "{field}: {date}"
            );
        }
        let input = json!({"date":date, "title":"transfer", "amount":"1"});
        assert!(
            serde_json::from_value::<CreditEntryInput>(input).is_err(),
            "{date}"
        );
    }
}

#[test]
fn canonical_dates_round_trip_including_range_and_leap_boundaries() {
    for date in ["0001-01-01", "9999-12-31", "2024-02-29", "2000-02-29"] {
        let mut input = document();
        for field in ["fulfillment_date", "due_date", "issue_date"] {
            input[field] = json!(date);
        }
        let decoded: DocumentInput = serde_json::from_value(input).expect(date);
        let encoded = serde_json::to_value(decoded).expect("serialize");
        for field in ["fulfillment_date", "due_date", "issue_date"] {
            assert_eq!(encoded[field], date);
        }
        let entry: CreditEntryInput = serde_json::from_value(json!({
            "date":date, "title":"transfer", "amount":"1"
        }))
        .expect(date);
        assert_eq!(
            serde_json::to_value(entry).expect("serialize")["date"],
            date
        );
    }
}

#[test]
fn only_optional_issue_date_accepts_null_or_omission() {
    let mut input = document();
    assert!(
        serde_json::from_value::<DocumentInput>(input.clone())
            .expect("omitted issue date")
            .issue_date
            .is_none()
    );
    input["issue_date"] = Value::Null;
    assert!(
        serde_json::from_value::<DocumentInput>(input.clone())
            .expect("null issue date")
            .issue_date
            .is_none()
    );
    for field in ["fulfillment_date", "due_date"] {
        let mut missing = input.clone();
        missing
            .as_object_mut()
            .expect("document object")
            .remove(field);
        assert!(serde_json::from_value::<DocumentInput>(missing).is_err());
        let mut null = input.clone();
        null[field] = Value::Null;
        assert!(serde_json::from_value::<DocumentInput>(null).is_err());
    }
    for input in [
        json!({"title":"transfer", "amount":"1"}),
        json!({"date":null, "title":"transfer", "amount":"1"}),
    ] {
        assert!(serde_json::from_value::<CreditEntryInput>(input).is_err());
    }
}

#[test]
fn response_date_decoding_remains_tolerant() {
    let response: QueryResponse = serde_json::from_value(json!({
        "invoice_number":"SZ-1", "document_type":"SZ", "issue_date":"2026-09-14T23:59:00-12:00"
    }))
    .expect("response dates retain the existing reader");
    assert_eq!(
        response.issue_date.expect("reported date").to_string(),
        "2026-09-14"
    );
}

#[cfg(feature = "schemars")]
#[test]
fn discovery_describes_strict_dates_and_optional_issue_date() {
    let schema = serde_json::to_value(schemars::schema_for!(DocumentInput)).expect("schema");
    for field in ["fulfillment_date", "due_date", "issue_date"] {
        let date = &schema["properties"][field];
        assert_eq!(date["format"], "date");
        assert!(date["pattern"].as_str().is_some());
        assert_eq!(
            date["type"],
            if field == "issue_date" {
                json!(["string", "null"])
            } else {
                json!("string")
            }
        );
    }
    let schema = serde_json::to_value(schemars::schema_for!(CreditEntryInput)).expect("schema");
    assert_eq!(schema["properties"]["date"]["type"], "string");
    assert_eq!(schema["properties"]["date"]["format"], "date");
}
