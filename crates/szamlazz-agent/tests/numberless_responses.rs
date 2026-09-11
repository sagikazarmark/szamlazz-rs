//! Successful acknowledgements retain metadata without inventing a vendor echo.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rust_decimal::dec;
use szamlazz_agent::ops::{
    credit_entry::{ClearCreditEntries, InvoiceBalance, RegisterCreditEntry},
    query_pdf::{InvoicePdf, QueryInvoicePdf},
    storno::{StornoInvoice, StornoResponse},
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, InvoiceSelector, OutcomeClass, PaymentMethod, ResponseError};

fn envelope(inner: &str, headers: &[(&str, &str)]) -> RawResponse {
    RawResponse::new(headers.iter().copied(), format!(
        r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">{inner}</xmlszamlavalasz>"#
    ).into_bytes())
}

#[test]
fn register_and_clear_keep_numberless_balance_metadata() {
    for (inner, headers) in [
        (
            "<szamlanetto>100</szamlanetto><szamlabrutto>127</szamlabrutto><kintlevoseg>0</kintlevoseg><vevoifiokurl>opaque:a%2Bb+c&amp;x=1</vevoifiokurl>",
            vec![
                ("szlahu_nettovegosszeg", "bad"),
                ("szlahu_bruttovegosszeg", "bad"),
                ("szlahu_kintlevoseg", "bad"),
                ("szlahu_vevoifiokurl", "wrong"),
                ("szlahu_fizetesmod", "%C3%A1tutal%C3%A1s"),
            ],
        ),
        (
            "",
            vec![
                ("szlahu_nettovegosszeg", "100"),
                ("szlahu_bruttovegosszeg", "127"),
                ("szlahu_kintlevoseg", "0"),
                ("szlahu_vevoifiokurl", "opaque%3Aa%252Bb%2Bc%26x%3D1"),
                ("szlahu_fizetesmod", "%C3%A1tutal%C3%A1s"),
            ],
        ),
    ] {
        let raw = envelope(&format!("<sikeres>true</sikeres>{inner}"), &headers);
        for balance in [
            RegisterCreditEntry::new("I-request")
                .parse(&raw)
                .expect("register acknowledgement"),
            ClearCreditEntries::new("I-request")
                .parse(&raw)
                .expect("clear acknowledgement"),
        ] {
            assert_eq!(
                balance.invoice_number, None,
                "request target is not an echo"
            );
            assert_eq!(balance.net_total, Some(dec!(100)));
            assert_eq!(balance.gross_total, Some(dec!(127)));
            assert_eq!(balance.outstanding, Some(dec!(0)));
            assert_eq!(balance.payment_method, Some(PaymentMethod::Transfer));
            assert_eq!(
                balance.customer_account_url.as_deref(),
                Some("opaque:a%2Bb+c&x=1")
            );
            let json = serde_json::to_value(&balance).expect("serialize");
            assert!(json["invoice_number"].is_null());
            assert_eq!(
                serde_json::from_value::<InvoiceBalance>(json).expect("roundtrip"),
                balance
            );
        }
    }
}

#[test]
fn numberless_pdf_query_returns_the_decoded_artifact() {
    // Deliberately synthetic decoded bytes: this asserts artifact transport,
    // not PDF-format validation or proof of a particular document's identity.
    let artifact = b"%PDF-1.4\nsynthetic decoded artifact\n%%EOF";
    let raw = envelope(
        &format!(
            "<sikeres>true</sikeres><szamlanetto>100</szamlanetto><szamlabrutto>127</szamlabrutto><pdf>{}</pdf>",
            STANDARD.encode(artifact)
        ),
        &[
            ("szlahu_kintlevoseg", "0"),
            ("szlahu_vevoifiokurl", "opaque%3Aartifact"),
        ],
    );
    for selector in [
        InvoiceSelector::InvoiceNumber("I-request".into()),
        InvoiceSelector::OrderNumber("O-request".into()),
        InvoiceSelector::ExternalId("external-request".into()),
    ] {
        let pdf = QueryInvoicePdf::new(selector)
            .parse(&raw)
            .expect("artifact without number");
        assert_eq!(pdf.invoice_number, None);
        assert_eq!(pdf.pdf.as_bytes(), artifact);
        assert_eq!(pdf.net_total, Some(dec!(100)));
        assert_eq!(pdf.gross_total, Some(dec!(127)));
        assert_eq!(pdf.outstanding, Some(dec!(0)));
        assert_eq!(pdf.customer_account_url.as_deref(), Some("opaque:artifact"));
        let json = serde_json::to_value(&pdf).expect("serialize");
        assert_eq!(json["pdf"], STANDARD.encode(artifact));
        assert_eq!(
            serde_json::from_value::<InvoicePdf>(json).expect("roundtrip"),
            pdf
        );
    }
    for inner in ["", "<pdf>not base64!</pdf>"] {
        let error = QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber("I-request".into()))
            .parse(&envelope(&format!("<sikeres>true</sikeres>{inner}"), &[]))
            .expect_err("an artifact is still required");
        assert!(matches!(error, ResponseError::Parse(_)));
    }
}

#[test]
fn storno_acknowledgement_and_numbered_response_roundtrip_distinctly() {
    for (identity, state) in [
        ("", "unnumbered"),
        ("<szamlaszam>I-reversal</szamlaszam>", "numbered"),
    ] {
        let raw = envelope(
            &format!(
                "<sikeres>true</sikeres>{identity}<szamlanetto>-100</szamlanetto><szamlabrutto>-127</szamlabrutto><kintlevoseg>0</kintlevoseg><vevoifiokurl>opaque:storno</vevoifiokurl><pdf>JVBERi0=</pdf>"
            ),
            &[
                ("szlahu_id", "42"),
                ("szlahu_fizetesmod", "%C3%A1tutal%C3%A1s"),
            ],
        );
        let response = StornoInvoice::new("I-request")
            .parse(&raw)
            .expect("acknowledged");
        let json = serde_json::to_value(&response).expect("serialize");
        assert_eq!(json["state"], state);
        assert_eq!(
            serde_json::from_value::<StornoResponse>(json).expect("roundtrip"),
            response
        );
        if state == "numbered" {
            assert!(matches!(&response, StornoResponse::Numbered(_)));
            assert!(response.numbered().is_some());
            let issued = response.into_numbered().expect("numbered");
            assert_eq!(issued.invoice_number.as_str(), "I-reversal");
        } else {
            assert!(matches!(&response, StornoResponse::Unnumbered(_)));
            assert_eq!(response.numbered(), None);
            let ack = response
                .into_numbered()
                .expect_err("unnumbered acknowledgement");
            assert_eq!(ack.document_id, Some(42));
            assert_eq!(ack.net_total, Some(dec!(-100)));
            assert_eq!(ack.gross_total, Some(dec!(-127)));
            assert_eq!(ack.outstanding, Some(dec!(0)));
            assert_eq!(ack.customer_account_url.as_deref(), Some("opaque:storno"));
            assert_eq!(ack.payment_method, Some(PaymentMethod::Transfer));
            assert_eq!(ack.pdf.expect("decoded artifact").as_bytes(), b"%PDF-");
        }
    }
}

#[test]
fn numberless_56_remains_uncertain_rather_than_an_acknowledgement() {
    for raw in [
        envelope(
            "<sikeres>false</sikeres><hibakod>56</hibakod><hibauzenet>notification failed</hibauzenet>",
            &[],
        ),
        envelope("<sikeres>true</sikeres>", &[("szlahu_error_code", "56")]),
    ] {
        let error = StornoInvoice::new("I-request")
            .parse(&raw)
            .expect_err("no numbered evidence");
        assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
        assert!(
            matches!(error, ResponseError::Api(api) if api.code == ErrorCode::InvoiceNotificationDeliveryFailed)
        );
    }
}

#[test]
fn optional_identity_does_not_accept_malformed_identity_or_refusals() {
    for inner in [
        "<sikeres>true</sikeres><szamlaszam><bad/></szamlaszam>",
        "<sikeres>true</sikeres><szamlaszam>I-1</szamlaszam><szamlaszam>I-2</szamlaszam>",
        "<sikeres>false</sikeres><hibakod>3</hibakod>",
        "<sikeres>false</sikeres><hibakod><bad/></hibakod>",
        "<sikeres>false</sikeres><hibakod>3</hibakod><hibakod>56</hibakod>",
    ] {
        let raw = envelope(&format!("{inner}<pdf>JVBERi0=</pdf>"), &[]);
        assert!(
            RegisterCreditEntry::new("I-request").parse(&raw).is_err(),
            "{inner}"
        );
        assert!(
            ClearCreditEntries::new("I-request").parse(&raw).is_err(),
            "{inner}"
        );
        assert!(
            StornoInvoice::new("I-request").parse(&raw).is_err(),
            "{inner}"
        );
        assert!(
            QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber("I-request".into()))
                .parse(&raw)
                .is_err(),
            "{inner}"
        );
    }
}
