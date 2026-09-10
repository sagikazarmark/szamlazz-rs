//! A structured response must be one complete XML document.
use szamlazz_agent::ops::{
    proforma::{DeleteProforma, ProformaSelector},
    query_xml::QueryInvoiceXml,
    receipt::{QueryReceipt, ReceiptSelector},
    storno::StornoInvoice,
    taxpayer::QueryTaxpayer,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{InvoiceSelector, ReceiptNumber};

fn check(request: &impl AgentRequest, body: &str) {
    let body = body
        .trim_start_matches("<?xml version=\"1.0\" encoding=\"UTF-8\"?>")
        .trim();
    for valid in [
        body.to_owned(),
        format!(
            "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!--before--><?before ok?>{body} \t\r\n<!--after--><?after ok?>"
        ),
        format!(
            "<?xml version = '1.0'\tencoding = 'UTF-8'\nstandalone = 'yes' ?><?árvíz?>{body}<?ns:ok a='b'?>"
        ),
    ] {
        assert!(
            request
                .parse(&RawResponse::new::<&str, &str>([], valid.into_bytes()))
                .is_ok()
        );
    }
    for tail in [
        "<extra/>",
        "text",
        "<![CDATA[ ]]>",
        "&#32;",
        "<!--",
        "<?pi",
        "<??>",
        "<? ?>",
        "<?1bad?>",
        "<",
        "</wrong>",
        "<?xml version=\"1.0\"?>",
        "<!-- bad -- comment -->",
    ] {
        assert!(
            request
                .parse(&RawResponse::new::<&str, &str>(
                    [],
                    format!("{body}{tail}").into_bytes()
                ))
                .is_err(),
            "tail {tail:?}"
        );
    }
    for prefix in [
        "text",
        "&#32;",
        "<![CDATA[ ]]>",
        " <?xml version=\"1.0\"?>",
        "<?xml?>",
        "<?xml version=\"1.0\"encoding=\"UTF-8\"?>",
        "<?xml version='1.0' encoding='UTF-8'standalone='yes'?>",
    ] {
        assert!(
            request
                .parse(&RawResponse::new::<&str, &str>(
                    [],
                    format!("{prefix}{body}").into_bytes()
                ))
                .is_err(),
            "prefix {prefix:?}"
        );
    }
    let truncated = &body[..body.rfind("</").expect("root close")];
    assert!(
        request
            .parse(&RawResponse::new::<&str, &str>(
                [],
                truncated.as_bytes().to_vec()
            ))
            .is_err()
    );
}

#[test]
fn operation_parsers_require_completed_documents() {
    check(
        &QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into())),
        include_str!("synthetic/szamla_query.xml"),
    );
    check(
        &StornoInvoice::new("I"),
        include_str!("synthetic/querying_pdf_xmlszamlavalasz.xml"),
    );
    check(
        &QueryReceipt::new(ReceiptSelector::ReceiptNumber(ReceiptNumber::new("R"))),
        include_str!("synthetic/xmlnyugtavalasz.xml"),
    );
    check(
        &DeleteProforma::new(ProformaSelector::OrderNumber("O".into())),
        include_str!("synthetic/xmlszamladbkdelvalasz.xml"),
    );
    check(
        &QueryTaxpayer::new("12345678").expect("prefix"),
        include_str!("synthetic/taxpayer.xml"),
    );
}
