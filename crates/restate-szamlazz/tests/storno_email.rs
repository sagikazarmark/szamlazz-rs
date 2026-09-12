//! Storno recipient intent at the public request/schema boundary.
use restate_szamlazz::contract::StornoRequest;
use serde_json::json;

#[cfg(feature = "schemars")]
#[test]
fn storno_schema_exposes_optional_bounded_recipient_and_open_warnings() {
    let schema = serde_json::to_value(schemars::schema_for!(StornoRequest)).expect("schema");
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["required"], json!(["invoice_number"]));
    assert_eq!(
        schema["properties"]["buyer_email"]["anyOf"],
        json!([
            {"$ref":"#/$defs/StornoRecipient"}, {"type":"null"}
        ])
    );
    let recipient = &schema["$defs"]["StornoRecipient"];
    assert_eq!(recipient["maxLength"], 254);
    let pattern =
        regex::Regex::new(recipient["pattern"].as_str().expect("pattern")).expect("regex");
    let local = regex::Regex::new(
        recipient["allOf"][0]["pattern"]
            .as_str()
            .expect("local limit"),
    )
    .expect("regex");
    for (address, accepted) in [
        ("Buyer+storno@example.com", true),
        ("a&b@example.com", true),
        ("a..b@example.com", false),
        ("a@-example.com", false),
        ("a@example.com,b@example.com", false),
        ("é@example.com", false),
    ] {
        assert_eq!(
            pattern.is_match(address) && local.is_match(address),
            accepted
        );
    }
    assert!(!local.is_match(&format!("{}@example.com", "a".repeat(65))));
    let response = serde_json::to_value(schemars::schema_for!(
        restate_szamlazz::contract::StornoResponse
    ))
    .expect("schema");
    assert_eq!(
        response["properties"]["warnings"]["items"]["$ref"],
        "#/$defs/Warning"
    );
    assert_eq!(response["$defs"]["Warning"]["type"], "string");
    assert!(response["$defs"]["Warning"].get("enum").is_none());
    let legacy: restate_szamlazz::contract::StornoResponse =
        serde_json::from_value(json!({"outcome":"reversed", "invoice_number":"SZ-1"}))
            .expect("older response");
    assert!(legacy.warnings.is_empty());
}

#[test]
fn optional_storno_recipient_is_validated_and_preserved() {
    for body in [
        json!({"invoice_number":"SZ-1"}),
        json!({"invoice_number":"SZ-1", "buyer_email":null}),
    ] {
        let request: StornoRequest = serde_json::from_value(body).expect("omitted recipient");
        assert_eq!(
            serde_json::to_value(request).expect("encode")["buyer_email"],
            json!(null)
        );
    }
    for address in ["Buyer+storno@example.com", "a&b@example.com"] {
        let body = json!({"invoice_number":"SZ-1", "buyer_email":address});
        let request: StornoRequest = serde_json::from_value(body).expect("explicit recipient");
        let encoded = serde_json::to_string(&request).expect("retained request");
        let replay: StornoRequest = serde_json::from_str(&encoded).expect("replay");
        assert_eq!(request, replay);
        assert_eq!(
            serde_json::to_value(replay).expect("encode")["buyer_email"],
            address
        );
    }
    for address in [
        "",
        " ",
        " buyer@example.com",
        "buyer@example.com\n",
        "no-at",
        "a@@example.com",
        "a@",
        "@example.com",
        "a..b@example.com",
        "a@-example.com",
        "a@example..com",
        "a@example.com,b@example.com",
        "Buyer <a@example.com>",
        "a\0@example.com",
        "é@example.com",
    ] {
        let error = serde_json::from_value::<StornoRequest>(
            json!({"invoice_number":"SZ-1", "buyer_email":address}),
        )
        .expect_err("unsupported recipient");
        assert!(
            !error.to_string().contains(address) || address.len() < 2,
            "input is not echoed"
        );
    }
    for address in [
        format!("{}@example.com", "a".repeat(65)),
        format!("a@{}.com", "a".repeat(64)),
    ] {
        assert!(
            serde_json::from_value::<StornoRequest>(
                json!({"invoice_number":"SZ-1", "buyer_email":address})
            )
            .is_err()
        );
    }
}
