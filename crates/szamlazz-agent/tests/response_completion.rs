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
fn unexpected_root_diagnostics_bound_upstream_names() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    for name in ["A".repeat(23_000), "é".repeat(23_000)] {
        let raw = RawResponse::new::<&str, &str>([], format!("<{name}/>").into_bytes());
        let error = query.parse(&raw).expect_err("wrong root");
        assert!(matches!(
            error,
            szamlazz_agent::ResponseError::Parse(szamlazz_agent::ParseError::UnexpectedBody(_))
        ));
        let diagnostic = error.to_string();
        assert!(diagnostic.len() < 1024, "length {}", diagnostic.len());
        assert!(diagnostic.contains("expected"));
    }
}

#[test]
fn xml_lexical_rules_apply_even_to_ignored_extensions() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    let sample = include_str!("synthetic/szamla_query.xml");
    for extension in [
        "<extension>\0</extension>",
        "<extension>&#x1;</extension>",
        "<extension>&#xD800;</extension>",
        "<extension>&#x110000;</extension>",
        "<extension>A]]>B</extension>",
        "<extension>&unknown;</extension>",
        "<extension a='1'b='2'/>",
        "<extension a='<bad'/>",
        "<1invalid/>",
        "<extension a='&#x1;'/>",
        "<extension><![CDATA[\0]]></extension>",
    ] {
        let body = sample.replace("</szamla>", &format!("{extension}</szamla>"));
        assert!(
            query
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .is_err(),
            "{extension:?}"
        );
    }
    for name in ["A\0B", "A&#x1;B", "A]]>B"] {
        let body = sample.replace("Synthetic Supplier Kft.", name);
        assert_ne!(sample, body);
        assert!(
            query
                .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
                .is_err()
        );
    }
    for extension in [
        "<árvíz a='&lt;&amp;&#x1F600;'>text&#13;<![CDATA[<&]]></árvíz>",
        "<extension xmlns='urn:future'>unknown &amp; valid</extension>",
    ] {
        let body = sample.replace("</szamla>", &format!("{extension}</szamla>"));
        query
            .parse(&RawResponse::new::<&str, &str>([], body.into_bytes()))
            .expect("well-formed extension");
    }
}

#[test]
fn declaration_checks_cover_every_xml_separator() {
    let request = StornoInvoice::new("I");
    let body = r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres><szamlaszam>I-2</szamlaszam></xmlszamlavalasz>"#;
    for separator in [' ', '\t', '\n', '\r'] {
        for (attributes, valid) in [
            ("version='1.0'encoding='UTF-8'", false),
            ("version='1.0' encoding='UTF-8'", true),
        ] {
            let xml = format!("<?xml{separator}{attributes}?>{body}");
            assert_eq!(
                request
                    .parse(&RawResponse::new::<&str, &str>([], xml.into_bytes()))
                    .is_ok(),
                valid,
                "{separator:?} {attributes}"
            );
        }
    }
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
