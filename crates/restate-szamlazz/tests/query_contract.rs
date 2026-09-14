//! Exact provider-number reads share recovery's number domain.

use restate_szamlazz::contract::recovery::EvidenceNumber;
use restate_szamlazz::contract::{QueryRequest, Selector};
use serde_json::json;

#[test]
fn exact_queries_accept_recovery_numbers_without_normalizing() {
    for number in [
        "SZ-1".to_owned(),
        "  Külső: invoice 1  ".to_owned(),
        "é".repeat(80),
        "e\u{301}:\t1\n".to_owned(),
    ] {
        let evidence: EvidenceNumber = number.parse().expect("recovery number");
        let body = json!({"selector": {"invoice_number": evidence}});
        let request: QueryRequest = serde_json::from_value(body.clone()).expect("query number");
        let Selector::InvoiceNumber(ref parsed) = request.selector else {
            panic!("exact selector");
        };
        assert_eq!(parsed.as_str(), number);
        request.selector.validate().expect("XML-safe number");
        assert_eq!(serde_json::to_value(request).expect("encode"), body);
    }
}

#[test]
fn exact_queries_reject_blank_or_xml_invalid_numbers_but_mutations_stay_bounded() {
    use restate_szamlazz::contract::InvoiceNumber;

    for number in ["", " \t\n", "\u{a0}", "SZ-\0", "SZ-\u{b}", "SZ-\u{ffff}"] {
        assert!(
            serde_json::from_value::<QueryRequest>(json!({"selector": {"invoice_number": number}}))
                .is_err(),
            "{number:?}"
        );
    }
    for number in ["SZ:1".to_owned(), " SZ-1 ".to_owned(), "S".repeat(41)] {
        assert!(number.parse::<InvoiceNumber>().is_err(), "{number:?}");
        assert!(
            serde_json::from_value::<QueryRequest>(json!({"selector": {"invoice_number": number}}))
                .is_ok()
        );
    }
}

#[test]
fn managed_numbers_and_recovery_evidence_can_be_used_in_exact_selectors() {
    use restate_szamlazz::contract::{InvoiceNumber, ProviderDocumentNumber};

    let managed: InvoiceNumber = "SZ-1".parse().expect("managed number");
    let selector = Selector::InvoiceNumber(managed.into());
    assert_eq!(
        serde_json::to_value(selector).expect("encode"),
        json!({"invoice_number": "SZ-1"})
    );

    let evidence: EvidenceNumber = " vendor:1 ".parse().expect("evidence");
    let number: ProviderDocumentNumber = evidence;
    assert_eq!(number.as_str(), " vendor:1 ");
}

#[cfg(feature = "schemars")]
#[test]
fn exact_query_discovery_uses_the_recovery_number_domain() {
    let schema = serde_json::to_value(schemars::schema_for!(QueryRequest)).expect("schema");
    let number = &schema["$defs"]["ProviderDocumentNumber"];
    assert_eq!(number["type"], "string");
    assert_eq!(number["minLength"], 1);
    assert!(number.get("maxLength").is_none());
    assert_eq!(
        number["pattern"],
        json!(
            r"[^\u0009-\u000D\u0020\u0085\u00A0\u1680\u2000-\u200A\u2028\u2029\u202F\u205F\u3000]"
        )
    );
    assert!(
        number["not"]["pattern"]
            .as_str()
            .expect("XML exclusions")
            .contains(r"\u0000")
    );
}
