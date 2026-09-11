//! Caller intent is mandatory and uses the bounded invoice-number contract.

use restate_szamlazz::contract::{CreateOptions, DeleteProformaRequest};
use serde_json::json;

#[test]
fn documented_reissue_request_decodes_with_its_expected_document() {
    let readme = include_str!("../README.md");
    let section = readme
        .split("### Expected-document intent")
        .nth(1)
        .expect("migration section");
    let example = section
        .split("```json\n")
        .nth(1)
        .expect("JSON example")
        .split("```")
        .next()
        .expect("example body");
    let request: restate_szamlazz::CreateRequest =
        serde_json::from_str(example).expect("complete request");
    assert_eq!(
        request
            .options
            .reissue
            .expect("replacement intent")
            .expected_number
            .as_str(),
        "SZ-A"
    );
}

#[test]
fn deletion_requires_a_bounded_expected_number_even_when_forced() {
    for wire in [
        json!({}),
        json!({"force": true}),
        json!({"expected_number": ""}),
        json!({"expected_number": "A".repeat(41)}),
        json!({"expected_number": "D-1", "typo": true}),
    ] {
        assert!(
            serde_json::from_value::<DeleteProformaRequest>(wire.clone()).is_err(),
            "{wire}"
        );
    }
    let request: DeleteProformaRequest =
        serde_json::from_value(json!({"expected_number": "D-1"})).expect("named target");
    assert!(!request.force);
}

#[test]
fn reissue_names_the_document_and_refuses_legacy_or_incomplete_intent() {
    let wire = json!({"reissue": {"expected_number": "SZ-A"}});
    let options: CreateOptions = serde_json::from_value(wire).expect("explicit intent");
    assert_eq!(
        serde_json::to_value(options).expect("serialize")["reissue"],
        json!({"expected_number": "SZ-A"})
    );
    for reissue in [
        json!(true),
        json!(false),
        json!({}),
        json!({"expected_number": "SZ A"}),
        json!({"expected_number": "A".repeat(41)}),
        json!({"expected_number": "SZ-A", "typo": true}),
    ] {
        assert!(
            serde_json::from_value::<CreateOptions>(json!({"reissue": reissue})).is_err(),
            "{reissue}"
        );
    }
    serde_json::from_value::<CreateOptions>(json!({})).expect("ordinary create");
}

#[cfg(feature = "schemars")]
#[test]
fn discovery_schemas_require_closed_bounded_intent() {
    use restate_szamlazz::contract::{CorrectRequest, Reissue};
    for schema in [
        schemars::schema_for!(Reissue),
        schemars::schema_for!(DeleteProformaRequest),
    ] {
        let wire = serde_json::to_value(schema).expect("schema");
        assert_eq!(wire["additionalProperties"], false);
        assert!(
            wire["required"]
                .as_array()
                .expect("required")
                .contains(&json!("expected_number"))
        );
        assert_eq!(
            wire["properties"]["expected_number"]["$ref"],
            "#/$defs/InvoiceNumber"
        );
        assert_eq!(wire["$defs"]["InvoiceNumber"]["maxLength"], 40);
    }
    let options = serde_json::to_value(schemars::schema_for!(CreateOptions)).expect("schema");
    assert_eq!(
        options["properties"]["reissue"]["anyOf"],
        json!([{"$ref": "#/$defs/Reissue"}, {"type": "null"}])
    );
    let correct = serde_json::to_value(schemars::schema_for!(CorrectRequest)).expect("schema");
    assert_eq!(correct["additionalProperties"], false);
    assert!(correct["properties"].get("reissue").is_none());
}

#[cfg(feature = "schemars")]
#[test]
fn expected_number_schemas_and_runtime_refuse_controls_and_allow_unicode() {
    use restate_szamlazz::contract::Reissue;

    for schema in [
        schemars::schema_for!(Reissue),
        schemars::schema_for!(DeleteProformaRequest),
    ] {
        let schema = serde_json::to_value(schema).expect("schema");
        let number_schema = &schema["$defs"]["InvoiceNumber"];
        let forbidden =
            regex::Regex::new(number_schema["not"]["pattern"].as_str().expect("pattern"))
                .expect("schema regex");
        for control in (0..=0x1f).chain(0x7f..=0x9f) {
            let character = char::from_u32(control).expect("control");
            for number in [
                format!("{character}SZ-1"),
                format!("SZ{character}1"),
                format!("SZ-1{character}"),
            ] {
                let wire = json!({"expected_number": number});
                assert!(
                    forbidden.is_match(&number),
                    "schema accepts U+{control:04X}"
                );
                assert!(serde_json::from_value::<Reissue>(wire.clone()).is_err());
                assert!(serde_json::from_value::<DeleteProformaRequest>(wire).is_err());
            }
        }
        for number in ["É-2026-1".to_owned(), "é".repeat(20), "é".repeat(21)] {
            assert!(!forbidden.is_match(&number));
            assert!(
                number.chars().count() as u64
                    <= number_schema["maxLength"].as_u64().expect("bound")
            );
            let wire = json!({"expected_number": number});
            let within_byte_limit = number.len() <= 40;
            assert_eq!(
                serde_json::from_value::<Reissue>(wire.clone()).is_ok(),
                within_byte_limit
            );
            assert_eq!(
                serde_json::from_value::<DeleteProformaRequest>(wire).is_ok(),
                within_byte_limit
            );
            assert!(
                number_schema["description"]
                    .as_str()
                    .expect("description")
                    .contains("40 UTF-8 bytes")
            );
        }
    }
}
