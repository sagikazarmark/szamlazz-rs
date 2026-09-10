//! Simplified image selection is plain request data, not a monetary projection.
use jiff::civil::date;
use rust_decimal::dec;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::wire::AgentRequest;
use szamlazz_agent::{
    Credentials, Currency, InvoiceTemplate, Language, LineItem, PaymentMethod, VatRate,
};

fn header() -> InvoiceHeader {
    InvoiceHeader::new(
        date(2026, 9, 10),
        date(2026, 9, 10),
        PaymentMethod::Transfer,
        Currency::HUF,
        Language::Hungarian,
    )
}

#[test]
fn simplified_image_and_preview_are_independent_optional_header_fields() {
    assert_eq!(header().simple_items, None);
    for (simple_items, simple_xml) in [
        (None, ""),
        (Some(false), "<simpleItems>false</simpleItems>"),
        (Some(true), "<simpleItems>true</simpleItems>"),
    ] {
        for (preview_pdf, preview_xml) in [
            (None, ""),
            (Some(false), "<elonezetpdf>false</elonezetpdf>"),
            (Some(true), "<elonezetpdf>true</elonezetpdf>"),
        ] {
            for kind in [
                InvoiceKind::invoice(),
                InvoiceKind::Proforma,
                InvoiceKind::prepayment(),
                InvoiceKind::Final {
                    prepayment_number: Some("I-1".into()),
                    proforma_number: None,
                },
                InvoiceKind::Corrective {
                    corrected_number: "I-1".into(),
                },
                InvoiceKind::DeliveryNote,
            ] {
                let request = CreateInvoice::new(
                    kind,
                    InvoiceHeader {
                        simple_items,
                        preview_pdf,
                        template: Some(InvoiceTemplate::NoEnvelope),
                        ..header()
                    },
                    Buyer {
                        group_id: Some("12345678".into()),
                        ..Buyer::new("Buyer", "1111", "Budapest", "Street 1")
                    },
                    vec![LineItem {
                        erasure_code_count: Some(1),
                        ..LineItem::new(
                            "Travel",
                            dec!(1),
                            "db",
                            dec!(100),
                            VatRate::percent(27),
                            dec!(100),
                            dec!(27),
                            dec!(127),
                        )
                    }],
                );
                // Content rules belong to szamlazz.hu, including the forbidden kinds.
                request.validate().expect("plain data");
                let xml = String::from_utf8(request.write_xml(&Credentials::agent_key("key")))
                    .expect("UTF-8");
                let template = if matches!(request.kind, InvoiceKind::DeliveryNote) {
                    "SzlaFuvarlevelesAlap"
                } else {
                    "SzlaNoEnv"
                };
                assert!(
                    xml.contains(&format!(
                        "<szamlaSablon>{template}</szamlaSablon>{preview_xml}{simple_xml}</fejlec>"
                    )),
                    "{xml}"
                );
                assert!(xml.contains("<csoportazonosito>12345678</csoportazonosito>"));
                assert!(xml.contains("<nettoEgysegar>100</nettoEgysegar><afakulcs>27</afakulcs><nettoErtek>100</nettoErtek><afaErtek>27</afaErtek><bruttoErtek>127</bruttoErtek><torloKod>1</torloKod>"));
            }
        }
    }
}

#[test]
fn missing_null_and_explicit_simple_items_json_decode() {
    let mut json = serde_json::to_value(header()).expect("JSON");
    json.as_object_mut().expect("object").remove("simple_items");
    assert_eq!(
        serde_json::from_value::<InvoiceHeader>(json.clone())
            .expect("old JSON")
            .simple_items,
        None
    );
    for value in [serde_json::Value::Null, false.into(), true.into()] {
        json["simple_items"] = value.clone();
        let decoded: InvoiceHeader = serde_json::from_value(json.clone()).expect("header");
        assert_eq!(decoded.simple_items, value.as_bool());
        assert_eq!(
            serde_json::to_value(decoded).expect("JSON")["simple_items"],
            value
        );
    }
}
