//! Provider record IDs at the public response boundary.
use restate_szamlazz::contract::{CreateResponse, DocumentStatus, QueryResponse, StornoResponse};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn optional_ids<T: DeserializeOwned + Serialize>(legacy: Value, fields: &[&str]) {
    let decoded: T = serde_json::from_value(legacy.clone()).expect("older response");
    let encoded = serde_json::to_value(decoded).expect("encode");
    for field in fields {
        assert_eq!(encoded.get(*field), Some(&Value::Null), "{field}");
    }
    let mut present = legacy;
    for (field, id) in fields.iter().zip([i64::MAX, i64::MIN]) {
        present[*field] = json!(id);
    }
    let decoded: T = serde_json::from_value(present.clone()).expect("integer IDs");
    let retained = serde_json::to_string(&decoded).expect("record");
    let replay: T = serde_json::from_str(&retained).expect("replay");
    let encoded = serde_json::to_value(replay).expect("encode");
    for field in fields {
        assert_eq!(
            encoded[*field], present[*field],
            "exact i64, including replay"
        );
        let mut invalid = present.clone();
        invalid[*field] = json!(1.5);
        assert!(serde_json::from_value::<T>(invalid).is_err());
    }
}

#[test]
fn provider_ids_are_optional_exact_integers_associated_with_their_numbers() {
    optional_ids::<CreateResponse>(
        json!({"outcome":"issued", "kind":"invoice", "external_id":"acct:O:invoice", "invoice_number":"SZ-1"}),
        &["document_id"],
    );
    optional_ids::<QueryResponse>(
        json!({"invoice_number":"SZ-1", "document_type":"SZ"}),
        &["document_id"],
    );
    optional_ids::<DocumentStatus>(
        json!({"number":"D-1", "state":"consumed", "by":"SZ-1"}),
        &["document_id"],
    );
    optional_ids::<StornoResponse>(
        json!({"outcome":"reversed", "invoice_number":"SZ-1", "storno_number":"SS-1"}),
        &["invoice_document_id", "storno_document_id"],
    );
}

#[test]
fn number_only_reversal_evidence_retains_absent_metadata_on_replay() {
    use restate_szamlazz::gateway::{
        StornoLookupOutcome, StornoOutcome, recovery::ReconciliationOutcome,
    };
    optional_ids::<StornoResponse>(
        json!({"outcome":"reversed", "invoice_number":"SZ-1"}),
        &["invoice_document_id", "storno_document_id"],
    );
    let legacy = json!({"AlreadyReversed":{"storno_number":"SS-1"}});
    let lookup: StornoLookupOutcome =
        serde_json::from_value(legacy.clone()).expect("number only lookup");
    let write: StornoOutcome = serde_json::from_value(legacy).expect("number only write");
    let recovery: ReconciliationOutcome =
        serde_json::from_value(json!({"Reversed":{"storno_number":"SS-1"}}))
            .expect("number only recovery");
    for (recorded, variant) in [
        (
            serde_json::to_value(lookup).expect("record"),
            "AlreadyReversed",
        ),
        (
            serde_json::to_value(write).expect("record"),
            "AlreadyReversed",
        ),
        (serde_json::to_value(recovery).expect("record"), "Reversed"),
    ] {
        assert_eq!(recorded[variant]["storno_number"], "SS-1");
        assert_eq!(recorded[variant]["storno_document_id"], Value::Null);
    }
}

#[cfg(feature = "schemars")]
#[test]
fn provider_id_schemas_are_optional_nullable_int64() {
    for (schema, fields) in [
        (schemars::schema_for!(CreateResponse), vec!["document_id"]),
        (schemars::schema_for!(QueryResponse), vec!["document_id"]),
        (schemars::schema_for!(DocumentStatus), vec!["document_id"]),
        (
            schemars::schema_for!(StornoResponse),
            vec!["invoice_document_id", "storno_document_id"],
        ),
    ] {
        let schema = serde_json::to_value(schema).expect("schema");
        for field in fields {
            let property = &schema["properties"][field];
            assert_eq!(property["type"], json!(["integer", "null"]));
            assert_eq!(property["format"], "int64");
            assert!(
                !schema["required"]
                    .as_array()
                    .expect("required")
                    .contains(&json!(field))
            );
        }
    }
}
