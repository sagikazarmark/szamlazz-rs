//! Explicit empty replacement at the checked sans-I/O boundary.
use szamlazz_agent::ops::credit_entry::ClearCreditEntries;
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{Credentials, ErrorCode, ResponseError};

#[test]
fn clear_preserves_target_options_through_serde_and_checks_xml() {
    let request: ClearCreditEntries = serde_json::from_value(serde_json::json!({
        "invoice_number": "I&1",
        "issuer_tax_number": "12345678-1-13",
        "aggregator": "A&B"
    }))
    .expect("explicit clearing intent");
    let restored: ClearCreditEntries =
        serde_json::from_slice(&serde_json::to_vec(&request).expect("serialize"))
            .expect("deserialize");
    let credentials = Credentials::agent_key("key");
    let wire = restored.to_wire(&credentials).expect("checked request");
    let xml = String::from_utf8(wire.body).expect("UTF-8");
    assert!(xml.contains("<szamlaszam>I&amp;1</szamlaszam>"));
    assert!(xml.contains("<aggregator>A&amp;B</aggregator>"));
    assert!(!xml.contains("<kifizetes>"));
    assert!(
        ClearCreditEntries::new("I\0")
            .to_wire(&credentials)
            .is_err()
    );

    // A registration body must not silently turn into a clearing request.
    for extra in [
        serde_json::json!({"entries": []}),
        serde_json::json!({"additive": true}),
    ] {
        let mut input = serde_json::json!({"invoice_number": "I-1"});
        input
            .as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        assert!(serde_json::from_value::<ClearCreditEntries>(input).is_err());
    }
}

#[test]
fn clear_retains_registration_refusals_and_uncertain_answers() {
    let request = ClearCreditEntries::new("I-1");
    let refused = RawResponse::new::<&str, &str>([], br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>false</sikeres><hibakod>463</hibakod><hibauzenet>reversed</hibauzenet></xmlszamlavalasz>"#.to_vec());
    assert!(
        matches!(request.parse(&refused), Err(ResponseError::Api(api))
        if api.code == ErrorCode::PaymentOnReversedInvoice && api.message == "reversed")
    );
    let incomplete = RawResponse::new::<&str, &str>([], b"<xmlszamlavalasz".to_vec());
    assert!(matches!(
        request.parse(&incomplete),
        Err(ResponseError::Parse(_))
    ));
}
