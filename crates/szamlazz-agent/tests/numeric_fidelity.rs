//! Source numeric values must not silently change during interpretation.
use rust_decimal::{Decimal, dec};
use szamlazz_agent::ops::{
    query_xml::QueryInvoiceXml,
    receipt::{QueryReceipt, ReceiptSelector},
    storno::StornoInvoice,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{InvoiceSelector, LineItem, ReceiptNumber, Rounding, VatRate};

fn raw(body: String) -> RawResponse {
    RawResponse::new::<&str, &str>([], body.into_bytes())
}

#[test]
fn numeric_vat_tokens_calculate_the_percentage_they_send() {
    for token in ["27.00", " 27 ", "\t2.7E1\n", "+27"] {
        let item = LineItem::try_calculated(
            "item",
            dec!(1),
            "db",
            dec!(100),
            VatRate::Other(token.into()),
            Rounding::Scale(2),
        )
        .expect("representable percentage");
        assert_eq!(item.vat_value, dec!(27), "{token:?}");
        assert_eq!(item.gross_value, dec!(127));
        assert_eq!(item.vat_rate.as_wire(), token);
    }
    for token in ["1e-29", "0.00000000000000000000000000001", "1e100"] {
        assert!(
            LineItem::try_calculated(
                "item",
                dec!(1),
                "db",
                dec!(100),
                VatRate::Other(token.into()),
                Rounding::Exact
            )
            .is_err(),
            "{token}"
        );
    }
    assert_eq!(VatRate::from(" FUTURE "), VatRate::Other(" FUTURE ".into()));
    assert_eq!(VatRate::from(" AAM "), VatRate::Other(" AAM ".into()));
}

#[test]
fn response_vat_helpers_interpret_numeric_xml_whitespace_without_changing_raw_text() {
    for token in [" 27 ", "\t27\n", "2.7E1"] {
        let invoice = include_str!("synthetic/szamla_query.xml").replace(
            "<afakulcs>20</afakulcs>",
            &format!("<afakulcs>{token}</afakulcs>"),
        );
        let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
            .parse(&raw(invoice))
            .expect("invoice");
        assert_eq!(doc.items[0].vat_rate(), VatRate::percent(27));
        assert_eq!(doc.items[0].vat_rate_code, token);
        assert_eq!(doc.totals.by_vat_rate[0].vat_rate(), VatRate::percent(27));
        let receipt = include_str!("synthetic/xmlnyugtavalasz.xml").replace(
            "<afakulcs>27</afakulcs>",
            &format!("<afakulcs>{token}</afakulcs>"),
        );
        let doc = QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new("R")))
            .parse(&raw(receipt))
            .expect("receipt");
        assert_eq!(doc.items[0].vat_rate(), VatRate::percent(27));
        assert_eq!(doc.items[0].vat_rate_code, token);
    }
}

#[test]
fn financial_items_and_receipt_subtotals_keep_special_code_precedence() {
    for special in [None, Some("AAM"), Some(" FUTURE ")] {
        let code = special.map_or(String::new(), |s| format!("<afatipus>{s}</afatipus>"));
        let expected = special.map_or(VatRate::percent(27), VatRate::from);
        let invoice = include_str!("synthetic/szamla_query.xml").replace("</szamla>", &format!("<qutetek><qutet><nev>row</nev>{code}<afakulcs> 27 </afakulcs><netto>100</netto><afa>27</afa><brutto>127</brutto><afalevon>0</afalevon></qutet></qutetek></szamla>"));
        let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
            .parse(&raw(invoice))
            .expect("financial item");
        assert_eq!(doc.financial_items[0].vat_rate(), expected);
        assert_eq!(doc.financial_items[0].vat_rate_code, " 27 ");
        let receipt = include_str!("synthetic/xmlnyugtavalasz.xml").replace(
            "<afatipus>ÁKK</afatipus><afakulcs>0</afakulcs>",
            &format!("{code}<afakulcs> 27 </afakulcs>"),
        );
        let doc = QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new("R")))
            .parse(&raw(receipt))
            .expect("subtotal");
        assert_eq!(doc.totals.by_vat_rate[0].vat_rate(), expected);
        assert_eq!(doc.totals.by_vat_rate[0].vat_rate_code, " 27 ");
    }
}

#[test]
fn existing_representable_amount_scale_is_preserved() {
    for (token, scale) in [("100.00", 2), ("1.20e-2", 4), ("0.00", 2)] {
        let body = include_str!("synthetic/szamla_query.xml").replace(
            "<devizaarf>0</devizaarf>",
            &format!("<devizaarf>{token}</devizaarf>"),
        );
        let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
            .parse(&raw(body))
            .expect("number");
        assert_eq!(doc.info.exchange_rate.expect("rate").scale(), scale);
    }
}

#[test]
fn unrepresentable_response_numbers_are_refused_on_every_numeric_channel() {
    for token in [
        "0.00000000000000000000000000001",
        "1e-29",
        "1.23456789012345678901234567895",
        "1e100",
    ] {
        for (old, new) in [
            (
                "<devizaarf>0</devizaarf>".to_owned(),
                format!("<devizaarf>{token}</devizaarf>"),
            ),
            (
                "<osszeg>15</osszeg>".to_owned(),
                format!("<osszeg>{token}</osszeg>"),
            ),
            (
                "<mennyiseg>1</mennyiseg>".to_owned(),
                format!("<mennyiseg>{token}</mennyiseg>"),
            ),
        ] {
            let body = include_str!("synthetic/szamla_query.xml").replace(&old, &new);
            assert!(
                QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
                    .parse(&raw(body))
                    .is_err(),
                "{new}"
            );
        }
        let body = include_str!("synthetic/xmlnyugtavalasz.xml").replace(
            "<mennyiseg>2.0</mennyiseg>",
            &format!("<mennyiseg>{token}</mennyiseg>"),
        );
        assert!(
            QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new("R")))
                .parse(&raw(body))
                .is_err(),
            "receipt {token}"
        );
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I</szamlaszam><szamlabrutto>{token}</szamlabrutto></xmlszamlavalasz>"#
        );
        assert!(
            StornoInvoice::new("I").parse(&raw(body)).is_err(),
            "body {token}"
        );
        let body = br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I</szamlaszam></xmlszamlavalasz>"#.to_vec();
        assert!(
            StornoInvoice::new("I")
                .parse(&RawResponse::new([("szlahu_bruttovegosszeg", token)], body))
                .is_err(),
            "header {token}"
        );
    }
}

#[test]
fn exact_representable_numbers_accept_equivalent_spellings() {
    for (token, expected) in [
        ("1e-2", dec!(0.01)),
        ("1.0000000000000000000000000000000", Decimal::ONE),
        ("100e-30", dec!(0.0000000000000000000000000001)),
        ("0e-100", Decimal::ZERO),
        ("79228162514264337593543950335", Decimal::MAX),
    ] {
        let body = include_str!("synthetic/szamla_query.xml").replace(
            "<devizaarf>0</devizaarf>",
            &format!("<devizaarf>{token}</devizaarf>"),
        );
        let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
            .parse(&raw(body))
            .expect("exact representable number");
        assert_eq!(doc.info.exchange_rate, Some(expected), "{token}");
    }
}
