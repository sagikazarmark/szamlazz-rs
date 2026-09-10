//! Decoded business characters survive response parsing.
use szamlazz_agent::ops::{
    query_xml::QueryInvoiceXml,
    receipt::{QueryReceipt, ReceiptSelector},
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{InvoiceSelector, PaymentMethod, ReceiptNumber};

#[test]
fn invoice_optional_business_text_is_preserved() {
    for (wire, expected) in [
        ("  A&amp;B&#13;\r\nC&#160; ", Some("  A&B\r\nC\u{a0} ")),
        ("&#160;", Some("\u{a0}")),
        (" \t\r\n ", None),
        ("", None),
    ] {
        let body = include_str!("synthetic/szamla_query.xml")
            .replace("<megjegyzes/>", &format!("<megjegyzes>{wire}</megjegyzes>"))
            .replace("<megjegyzes></megjegyzes>", &format!("<megjegyzes>{wire}</megjegyzes>"))
            .replace("Synthetic item comment", wire)
            .replace("<bankszamla/>", &format!("<bankszamla>{wire}</bankszamla>"))
            .replace("<bankszamla></bankszamla>", &format!("<bankszamla>{wire}</bankszamla>"))
            .replace("</alap>", &format!("<rendelesszam>{wire}</rendelesszam><hivszamlaszam>{wire}</hivszamlaszam></alap>"))
            .replace("credit_card", wire);
        let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("invoice");
        assert_eq!(doc.info.comment.as_deref(), expected);
        assert_eq!(doc.items[0].comment.as_deref(), expected);
        assert_eq!(
            doc.supplier.bank.expect("bank").account.as_deref(),
            expected
        );
        assert_eq!(doc.info.order_number.as_deref(), expected);
        assert_eq!(
            doc.info
                .referenced_invoice_number
                .as_ref()
                .map(szamlazz_agent::InvoiceNumber::as_str),
            expected
        );
        assert_eq!(
            doc.info.payment_method,
            expected.map(|s| PaymentMethod::Other(s.into()))
        );
    }
}

#[test]
fn receipt_identifiers_bank_ledger_and_tender_text_are_preserved() {
    for (wire, expected) in [
        (" padded ", Some(" padded ")),
        ("&#160;", Some("\u{a0}")),
        (" \t\r\n ", None),
        ("", None),
    ] {
        let body = include_str!("synthetic/xmlnyugtavalasz.xml")
            .replace("</alap>", &format!("<hivasAzonosito>{wire}</hivasAzonosito><rendelesSzam>{wire}</rendelesSzam><devizabank>{wire}</devizabank><megjegyzes>{wire}</megjegyzes><fokonyvVevo>{wire}</fokonyvVevo></alap>"))
            .replace("NYGT-TST-2026-100", wire).replace("ITEM-1", wire).replace("911", wire).replace("Synthetic voucher", wire);
        let doc = QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new("R")))
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("receipt");
        assert_eq!(doc.call_id.as_deref(), expected);
        assert_eq!(doc.order_number.as_deref(), expected);
        assert_eq!(doc.exchange_bank.as_deref(), expected);
        assert_eq!(doc.comment.as_deref(), expected);
        assert_eq!(doc.ledger_customer.as_deref(), expected);
        assert_eq!(
            doc.reversed_receipt_number
                .as_ref()
                .map(ReceiptNumber::as_str),
            expected
        );
        assert_eq!(doc.items[0].id.as_deref(), expected);
        assert_eq!(
            doc.items[0]
                .ledger
                .as_ref()
                .expect("ledger")
                .revenue_account
                .as_deref(),
            expected
        );
        assert_eq!(doc.payments[0].description.as_deref(), expected);
    }
}
