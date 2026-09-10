//! Monetary metadata through real operation entry points.
use rust_decimal::dec;
use szamlazz_agent::InvoiceSelector;
use szamlazz_agent::ops::{
    credit_entry::RegisterCreditEntry, query_pdf::QueryInvoicePdf, storno::StornoInvoice,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};

fn response(value: Option<&str>, inner: &str) -> RawResponse {
    let mut headers = vec![("szlahu_szamlaszam", " I-1 ")];
    if let Some(value) = value {
        for name in [
            "szlahu_nettovegosszeg",
            "szlahu_bruttovegosszeg",
            "szlahu_kintlevoseg",
        ] {
            headers.push((name, value));
        }
    }
    RawResponse::new(headers, format!(r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><pdf>JVBERi0=</pdf>{inner}</xmlszamlavalasz>"#).into_bytes())
}

#[test]
fn monetary_headers_are_ungrouped_decimals_across_operations() {
    for (value, expected) in [
        ("100,01", dec!(100.01)),
        ("1,234", dec!(1.234)),
        ("-0,5", dec!(-0.5)),
        ("+1.5", dec!(1.5)),
        ("0", dec!(0)),
        ("-0", dec!(0)),
        (".5", dec!(0.5)),
        ("1.", dec!(1)),
        ("1,25e+2", dec!(125)),
        ("1E-2", dec!(0.01)),
        (" \t1.2345\t ", dec!(1.2345)),
    ] {
        let raw = response(Some(value), "");
        let issued = StornoInvoice::new("I-1")
            .parse(&raw)
            .unwrap_or_else(|e| panic!("{value:?}: {e}"));
        assert_eq!(
            (issued.net_total, issued.gross_total, issued.outstanding),
            (Some(expected), Some(expected), Some(expected))
        );
        assert_eq!(issued.invoice_number.as_str(), "I-1");
        let balance = RegisterCreditEntry::new("I-1")
            .parse(&raw)
            .expect("balance");
        assert_eq!(
            (balance.net_total, balance.gross_total, balance.outstanding),
            (Some(expected), Some(expected), Some(expected))
        );
        let pdf = QueryInvoicePdf::new(InvoiceSelector::OrderNumber("O-1".into()))
            .parse(&raw)
            .expect("PDF");
        assert_eq!(
            (pdf.net_total, pdf.gross_total),
            (Some(expected), Some(expected))
        );
    }
}

#[test]
fn malformed_headers_and_body_precedence() {
    let request = StornoInvoice::new("I-1");
    assert_eq!(
        request
            .parse(&response(None, ""))
            .expect("absent")
            .gross_total,
        None
    );
    for value in [
        "",
        " \t",
        "1_000",
        "1 000",
        "1,234.56",
        "1.234,56",
        "1,,2",
        "1..2",
        "1e",
        "1e999",
        "79228162514264337593543950336",
        "\n1",
        "1\u{a0}",
    ] {
        assert!(
            request.parse(&response(Some(value), "")).is_err(),
            "{value:?}"
        );
        let body = "<szamlanetto>2</szamlanetto><szamlabrutto>3</szamlabrutto><kintlevoseg>1</kintlevoseg>";
        assert_eq!(
            request
                .parse(&response(Some(value), body))
                .expect("body wins")
                .gross_total,
            Some(dec!(3))
        );
    }
    for body in ["<szamlabrutto/>", "<szamlabrutto> \t </szamlabrutto>"] {
        assert_eq!(
            request
                .parse(&response(Some("1,25"), body))
                .expect("header fallback")
                .gross_total,
            Some(dec!(1.25))
        );
    }
    assert!(
        request
            .parse(&response(Some("1,25"), "<szamlabrutto>bad</szamlabrutto>"))
            .is_err()
    );
    assert!(
        request
            .parse(&response(Some("1,25"), "<szamlabrutto>1,25</szamlabrutto>"))
            .is_err()
    );
}

#[test]
fn numbered_56_retains_comma_metadata_and_drops_malformed_metadata() {
    let raw = RawResponse::new(
        [
            ("szlahu_error_code", "56"),
            ("szlahu_szamlaszam", "I-1"),
            ("szlahu_nettovegosszeg", "100,01"),
            ("szlahu_bruttovegosszeg", "bad"),
            ("szlahu_kintlevoseg", "0,01"),
        ],
        b"notification failed".to_vec(),
    );
    let issued = StornoInvoice::new("I-1").parse(&raw).expect("issued");
    assert!(issued.notification_delivery_failed);
    assert_eq!(
        (issued.net_total, issued.gross_total, issued.outstanding),
        (Some(dec!(100.01)), None, Some(dec!(0.01)))
    );
}
