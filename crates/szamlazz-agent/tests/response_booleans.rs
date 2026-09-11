//! Required verdicts and optional reported facts through public operations.
use szamlazz_agent::ops::{
    credit_entry::ClearCreditEntries,
    proforma::{DeleteProforma, ProformaSelector},
    query_pdf::QueryInvoicePdf,
    query_xml::QueryInvoiceXml,
    receipt::{QueryReceipt, ReceiptSelector, SendReceipt},
    storno::StornoInvoice,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, InvoiceSelector, OutcomeClass, ResponseError};

#[test]
fn invoice_indicators_preserve_unreported_values_in_json() {
    use serde_json::{Value, json};
    use szamlazz_agent::ops::query_xml::InvoiceDocument;

    let request = QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber("I-1".into()));
    let sample = include_str!("synthetic/szamla_query.xml")
        .replace("<penzforg>false</penzforg>", "")
        .replace("<kata>true</kata>", "");
    let fields = [
        ("keszpenz", "info", "cash_payment", "</alap>"),
        ("penzforg", "info", "cash_accounting", "</alap>"),
        ("kata", "info", "kata", "</alap>"),
        ("katafokonyv", "info", "kata_ledger", "</alap>"),
        (
            "privatePersonIndicator",
            "buyer",
            "private_person",
            "</vevo>",
        ),
    ];
    for (tag, section, field, close) in fields {
        for (text, expected) in [
            (None, Value::Null),
            (Some(""), Value::Null),
            (Some(" \t\n "), Value::Null),
            (Some("false"), json!(false)),
            (Some("0"), json!(false)),
            (Some("true"), json!(true)),
            (Some("1"), json!(true)),
        ] {
            let element = text.map_or_else(String::new, |value| format!("<{tag}>{value}</{tag}>"));
            let body = sample.replace(close, &format!("{element}{close}"));
            let document = request
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .expect("reported indicator");
            let mut json = serde_json::to_value(&document).expect("JSON");
            assert_eq!(json[section][field], expected, "{tag}: {text:?}");
            assert_eq!(
                serde_json::from_value::<InvoiceDocument>(json.clone()).expect("roundtrip"),
                document
            );
            // Old boolean JSON still decodes; omitted or null fields mean unknown.
            json[section]
                .as_object_mut()
                .expect("section")
                .remove(field);
            let omitted: InvoiceDocument = serde_json::from_value(json).expect("omitted indicator");
            assert_eq!(
                serde_json::to_value(omitted).expect("JSON")[section][field],
                Value::Null
            );
        }
        for bad in ["unknown", "2", "TRUE"] {
            let body = sample.replace(close, &format!("<{tag}>{bad}</{tag}>{close}"));
            assert!(
                matches!(
                    request.parse(&RawResponse::new::<&str, &str>([], body.into_bytes())),
                    Err(ResponseError::Parse(_))
                ),
                "{tag}: {bad}"
            );
        }
    }
}

#[test]
fn a_malformed_required_verdict_cannot_settle_an_outcome() {
    let invoice = InvoiceSelector::InvoiceNumber("I-1".into());
    for verdict in [
        "",
        "<sikeres/>",
        "<sikeres></sikeres>",
        "<sikeres> \t\n </sikeres>",
        "<sikeres>garbage</sikeres>",
        "<sikeres>&#160;</sikeres>",
    ] {
        for code in ["", "3", "53", "57", "463", "56", "FUTURE"] {
            let response = |root: &str| {
                let body = format!(
                    "<{root} xmlns='http://www.szamlazz.hu/{root}'>{verdict}<hibakod>{code}</hibakod><szamlaszam>I-2</szamlaszam></{root}>"
                );
                RawResponse::new::<&str, &str>([], body.into_bytes()).with_status(200)
            };
            let raw = response("xmlszamlavalasz");
            let errors = [
                ClearCreditEntries::new("I-1")
                    .parse(&raw)
                    .expect_err("invalid verdict"),
                StornoInvoice::new("I-1")
                    .parse(&raw)
                    .expect_err("invalid verdict"),
                QueryInvoiceXml::new(invoice.clone())
                    .parse(&raw)
                    .expect_err("invalid verdict"),
                QueryInvoicePdf::new(invoice.clone())
                    .parse(&raw)
                    .expect_err("invalid verdict"),
                DeleteProforma::new(ProformaSelector::InvoiceNumber("D-1".into()))
                    .parse(&response("xmlszamladbkdelvalasz"))
                    .expect_err("invalid verdict"),
                QueryReceipt::new(ReceiptSelector::ReceiptNumber("NY-1".into()))
                    .parse(&response("xmlnyugtavalasz"))
                    .expect_err("invalid verdict"),
                SendReceipt::new("NY-1")
                    .parse(&response("xmlnyugtasendvalasz"))
                    .expect_err("invalid verdict"),
            ];
            for error in errors {
                assert!(
                    matches!(error, ResponseError::Parse(_)),
                    "{verdict:?} / {code}: {error:?}"
                );
                assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
            }
        }
    }
}

#[test]
fn valid_verdicts_and_authoritative_headers_keep_their_meaning() {
    let request = ClearCreditEntries::new("I-1");
    for (verdict, success) in [("true", true), ("1", true), ("false", false), ("0", false)] {
        let raw = RawResponse::new::<&str, &str>([], format!(
            "<xmlszamlavalasz xmlns='http://www.szamlazz.hu/xmlszamlavalasz'><sikeres> \t{verdict}\n </sikeres><hibakod>57</hibakod><szamlaszam>I-1</szamlaszam><hibauzenet><bad/></hibauzenet></xmlszamlavalasz>"
        ).into_bytes());
        if success {
            assert_eq!(
                request
                    .parse(&raw)
                    .expect("success")
                    .invoice_number
                    .as_ref()
                    .map(szamlazz_agent::InvoiceNumber::as_str),
                Some("I-1")
            );
        } else {
            let error = request.parse(&raw).expect_err("refusal");
            assert_eq!(error.outcome_class(), OutcomeClass::Rejected);
            assert!(
                matches!(error, ResponseError::Api(api) if api.code == ErrorCode::MalformedXml)
            );
        }
    }
    let body = b"<xmlszamlavalasz xmlns='http://www.szamlazz.hu/xmlszamlavalasz'><sikeres/></xmlszamlavalasz>";
    let header = RawResponse::new([("szlahu_error_code", "3")], body.to_vec());
    assert!(
        matches!(request.parse(&header), Err(ResponseError::Api(api)) if api.code == ErrorCode::InvalidCredentials)
    );
    let down = RawResponse::new(
        [("szlahu_error_code", "3"), ("szlahu_down", "maintenance")],
        body.to_vec(),
    );
    assert!(matches!(
        request.parse(&down),
        Err(ResponseError::ServiceUnavailable(_))
    ));
}
