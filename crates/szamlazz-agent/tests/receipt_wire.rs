//! Receipt wire semantics from the first-party query/send XML and amount docs.
//! Structural assertions establish crate emission, not server behavior.

use rust_decimal::dec;
use serde::Deserialize;
use szamlazz_agent::ops::receipt::{
    CreateReceipt, QueryReceipt, ReceiptEmail, ReceiptSelector, SendReceipt,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{Credentials, Currency, LineItem, PaymentMethod, VatRate};

#[test]
fn receipt_reversal_requires_a_boolean_fact() {
    let request = QueryReceipt::new(ReceiptSelector::ReceiptNumber("NY-1".into()));
    let sample = include_str!("synthetic/xmlnyugtavalasz.xml");
    assert!(sample.contains("<stornozott>false</stornozott>"));
    for (value, reversed) in [("false", false), ("0", false), ("true", true), ("1", true)] {
        let body = sample.replace(
            "<stornozott>false</stornozott>",
            &format!("<stornozott>{value}</stornozott>"),
        );
        let receipt = request
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("boolean");
        assert_eq!(receipt.reversed, reversed);
    }
    for element in [
        "",
        "<stornozott/>",
        "<stornozott></stornozott>",
        "<stornozott> \t\n </stornozott>",
        "<stornozott>unknown</stornozott>",
    ] {
        let body = sample.replace("<stornozott>false</stornozott>", element);
        assert!(
            request
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .is_err(),
            "{element:?}"
        );
    }
}

#[test]
fn blank_receipt_pdf_is_absent_without_losing_receipt_identity() {
    let request = QueryReceipt {
        download_pdf: true,
        ..QueryReceipt::new(ReceiptSelector::ReceiptNumber("NY-1".into()))
    };
    let sample = include_str!("synthetic/xmlnyugtavalasz.xml");
    for element in [
        "",
        "<nyugtaPdf/>",
        "<nyugtaPdf></nyugtaPdf>",
        "<nyugtaPdf> \t\n </nyugtaPdf>",
    ] {
        let body = sample.replace("<nyugta>", &format!("{element}<nyugta>"));
        let receipt = request
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("receipt retained");
        assert_eq!(receipt.receipt_number.as_str(), "NYGT-TST-2026-123");
        assert_eq!(receipt.pdf, None, "{element:?}");
    }
}

#[derive(Debug, Deserialize)]
struct SendXml {
    #[serde(rename = "emailKuldes", default)]
    email: Vec<EmailXml>,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
struct EmailXml {
    email: Option<String>,
    #[serde(rename = "emailReplyto")]
    reply_to: Option<String>,
    #[serde(rename = "emailTargy")]
    subject: Option<String>,
    #[serde(rename = "emailSzoveg")]
    body: Option<String>,
}

fn email_xml(request: &SendReceipt) -> EmailXml {
    let xml = request.write_xml(&Credentials::agent_key("key"));
    let mut blocks = quick_xml::de::from_reader::<_, SendXml>(xml.as_slice())
        .expect("send XML")
        .email;
    assert_eq!(blocks.len(), 1, "exactly one present emailKuldes block");
    blocks.pop().expect("one block")
}

#[test]
fn email_presence_observer_distinguishes_absent_paired_and_self_closing_blocks() {
    for (xml, count) in [
        ("<xmlnyugtasend/>", 0),
        ("<xmlnyugtasend><emailKuldes/></xmlnyugtasend>", 1),
        (
            "<xmlnyugtasend><emailKuldes></emailKuldes></xmlnyugtasend>",
            1,
        ),
        (
            "<xmlnyugtasend><emailKuldes/><emailKuldes/></xmlnyugtasend>",
            2,
        ),
    ] {
        let parsed: SendXml = quick_xml::de::from_str(xml).expect("send XML");
        assert_eq!(parsed.email.len(), count, "{xml}");
        assert!(
            parsed
                .email
                .iter()
                .all(|email| *email == EmailXml::default())
        );
    }
}

#[test]
fn resend_keeps_a_present_empty_email_block_and_none_omits_only_its_child() {
    assert_eq!(email_xml(&SendReceipt::new("NY-1")), EmailXml::default());
    let mut request = SendReceipt {
        email: Some(ReceiptEmail::default()),
        ..SendReceipt::new("NY-1")
    };
    assert_eq!(email_xml(&request), EmailXml::default());
    request.email = Some(ReceiptEmail {
        to: Some("buyer@example.com".into()),
        reply_to: None,
        subject: Some(String::new()),
        body: Some("Ár & érték".into()),
    });
    assert_eq!(
        email_xml(&request),
        EmailXml {
            email: Some("buyer@example.com".into()),
            reply_to: None,
            subject: Some(String::new()),
            body: Some("Ár & érték".into()),
        }
    );
    request.email = Some(ReceiptEmail {
        to: None,
        reply_to: Some(String::new()),
        subject: None,
        body: None,
    });
    assert_eq!(
        email_xml(&request),
        EmailXml {
            reply_to: Some(String::new()),
            ..EmailXml::default()
        }
    );
}

#[derive(Debug, Deserialize)]
struct QueryXml {
    fejlec: QueryHeader,
}

#[derive(Debug, Deserialize, PartialEq, Eq, Default)]
#[serde(deny_unknown_fields)]
struct QueryHeader {
    nyugtaszam: Option<String>,
    #[serde(rename = "rendelesSzam")]
    order: Option<String>,
    #[serde(rename = "hivasAzonosito")]
    call_id: Option<String>,
}

#[test]
fn ordinary_number_and_order_queries_omit_call_id_but_explicit_value_is_written() {
    for (selector, expected) in [
        (
            ReceiptSelector::ReceiptNumber("NY-1".into()),
            QueryHeader {
                nyugtaszam: Some("NY-1".into()),
                ..QueryHeader::default()
            },
        ),
        (
            ReceiptSelector::OrderNumber("ORD-1".into()),
            QueryHeader {
                order: Some("ORD-1".into()),
                ..QueryHeader::default()
            },
        ),
    ] {
        let mut query = QueryReceipt::new(selector);
        let xml = query.write_xml(&Credentials::agent_key("key"));
        let parsed: QueryXml = quick_xml::de::from_reader(xml.as_slice()).expect("query XML");
        assert_eq!(parsed.fejlec, expected);
        query.call_id = Some("persisted-id".into());
        let xml = query.write_xml(&Credentials::agent_key("key"));
        let parsed: QueryXml = quick_xml::de::from_reader(xml.as_slice()).expect("query XML");
        assert_eq!(
            parsed.fejlec,
            QueryHeader {
                call_id: Some("persisted-id".into()),
                ..expected
            }
        );
    }
}

#[test]
fn explicit_receipt_amounts_preserve_fractional_net_and_vat_with_whole_gross() {
    #[derive(Deserialize)]
    struct Amounts {
        netto: String,
        afa: String,
        brutto: String,
    }
    #[derive(Deserialize)]
    struct Items {
        tetel: Amounts,
    }
    #[derive(Deserialize)]
    struct CreateXml {
        tetelek: Items,
    }

    let request = CreateReceipt::new(
        "NY",
        PaymentMethod::Cash,
        Currency::HUF,
        vec![LineItem::new(
            "Item",
            dec!(1),
            "db",
            dec!(787.40),
            VatRate::percent(27),
            dec!(787.40),
            dec!(212.60),
            dec!(1000),
        )],
    );
    let xml = request.write_xml(&Credentials::agent_key("key"));
    let parsed: CreateXml = quick_xml::de::from_reader(xml.as_slice()).expect("receipt XML");
    assert_eq!(parsed.tetelek.tetel.netto, "787.40");
    assert_eq!(parsed.tetelek.tetel.afa, "212.60");
    assert_eq!(parsed.tetelek.tetel.brutto, "1000");
    assert!(request.to_wire(&Credentials::agent_key("key")).is_ok());
}
