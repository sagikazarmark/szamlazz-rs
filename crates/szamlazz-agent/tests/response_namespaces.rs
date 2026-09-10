//! Namespace identity must survive the serde boundary.
use szamlazz_agent::InvoiceSelector;
use szamlazz_agent::ops::{
    proforma::{DeleteProforma, ProformaSelector},
    query_xml::QueryInvoiceXml,
    receipt::SendReceipt,
    storno::StornoInvoice,
    taxpayer::QueryTaxpayer,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};

fn raw(body: String) -> RawResponse {
    RawResponse::new::<&str, &str>([], body.into_bytes())
}

#[test]
fn foreign_verdicts_do_not_establish_success() {
    let delete = DeleteProforma::new(ProformaSelector::OrderNumber("O".into()));
    let send = SendReceipt::new("R");
    for verdict in [
        r#"<x:sikeres xmlns:x="urn:foreign">true</x:sikeres>"#,
        r#"<sikeres xmlns="">true</sikeres>"#,
    ] {
        assert!(delete.parse(&raw(format!(r#"<xmlszamladbkdelvalasz xmlns="http://www.szamlazz.hu/xmlszamladbkdelvalasz">{verdict}</xmlszamladbkdelvalasz>"#))).is_err());
        assert!(send.parse(&raw(format!(r#"<xmlnyugtasendvalasz xmlns="http://www.szamlazz.hu/xmlnyugtasendvalasz">{verdict}</xmlnyugtasendvalasz>"#))).is_err());
    }
}

#[test]
fn invoice_extensions_cannot_supply_identity_or_reversal() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    let original = include_str!("synthetic/szamla_query.xml");
    for extension in [
        r#"<x:sztornozott xmlns:x="urn:foreign">true</x:sztornozott>"#,
        r#"<x:extension xmlns:x="urn:foreign"><sztornozott>true</sztornozott></x:extension>"#,
        "<extension><sztornozott>true</sztornozott></extension>",
    ] {
        let doc = query
            .parse(&raw(
                original.replace("</alap>", &format!("{extension}</alap>"))
            ))
            .expect("extension ignored");
        assert_eq!(doc.info.reversed, None);
    }
    let body = original
        .replace("<szamlaszam>", r#"<x:szamlaszam xmlns:x="urn:foreign">"#)
        .replace("</szamlaszam>", "</x:szamlaszam>");
    assert!(query.parse(&raw(body)).is_err());
    let body = original.replace("<alap>", r#"<alap xmlns="">"#);
    assert!(query.parse(&raw(body)).is_err());
    let body = original.replace(
        "</alap>",
        r#"<a:sztornozott xmlns:a="http://www.szamlazz.hu/szamla">true</a:sztornozott></alap>"#,
    );
    assert_eq!(
        query
            .parse(&raw(body))
            .expect("correct alias")
            .info
            .reversed,
        Some(true)
    );
}

#[test]
fn correct_aliases_and_foreign_extensions_preserve_envelope_content() {
    let body = r#"<a:xmlszamlavalasz xmlns:a="http://www.szamlazz.hu/xmlszamlavalasz" xmlns:x="urn:extension"><x:sikeres>false</x:sikeres><a:sikeres>true</a:sikeres><a:szamlaszam>I-1</a:szamlaszam><x:szamlabrutto>999</x:szamlabrutto><a:szamlabrutto>127</a:szamlabrutto><a:vevoifiokurl>https://example.test/?a=1&amp;b=2</a:vevoifiokurl></a:xmlszamlavalasz>"#;
    let doc = StornoInvoice::new("I")
        .parse(&raw(body.into()))
        .expect("protocol fields");
    assert_eq!(doc.gross_total, Some(rust_decimal::dec!(127)));
    assert_eq!(
        doc.customer_account_url.as_deref(),
        Some("https://example.test/?a=1&b=2")
    );
}

#[test]
fn undeclared_prefixes_are_refused_even_in_unknown_subtrees() {
    let invoice = include_str!("synthetic/szamla_query.xml");
    let taxpayer = include_str!("synthetic/taxpayer.xml");
    for extension in [
        "<extension><x:unknown/></extension>",
        "<extension x:attribute='1'/>",
    ] {
        let body = invoice.replace("</szamla>", &format!("{extension}</szamla>"));
        assert!(
            QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
                .parse(&raw(body))
                .is_err()
        );
        // Inject at the known root's opening content, independent of its prefix.
        let at = taxpayer.find("<result>").expect("result");
        let mut body = taxpayer.to_owned();
        body.insert_str(at, extension);
        assert!(
            QueryTaxpayer::new("12345678")
                .expect("prefix")
                .parse(&raw(body))
                .is_err()
        );
    }
}

#[test]
fn escaped_namespace_uris_have_the_same_identity() {
    let original = include_str!("synthetic/szamla_query.xml");
    let body = original.replace("http://www.szamlazz.hu/szamla", "http://www.szamlazz.hu/sz&#97;mla")
        .replace("</alap>", r#"<p:sztornozott xmlns:p="http://www.szamlazz.hu/sz&#x61;mla">true</p:sztornozott></alap>"#);
    let doc = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()))
        .parse(&raw(body))
        .expect("equivalent namespace");
    assert_eq!(doc.info.reversed, Some(true));
    let body = include_str!("synthetic/taxpayer.xml")
        .replace("/api", "/&#97;pi")
        .replace("/data", "/d&#97;ta");
    assert!(
        QueryTaxpayer::new("12345678")
            .expect("prefix")
            .parse(&raw(body))
            .expect("NAV namespaces")
            .valid
    );
}

#[test]
fn ignoring_foreign_children_cannot_manufacture_scalar_values() {
    for (name, text) in [
        ("sikeres", "tr<x:extension/>ue"),
        ("szamlaszam", "I<x:extension/>-1"),
        ("szamlabrutto", "1<x:extension/>2"),
    ] {
        let body = r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz" xmlns:x="urn:extension"><sikeres>true</sikeres><szamlaszam>I</szamlaszam><szamlabrutto>127</szamlabrutto></xmlszamlavalasz>"#;
        let old = match name {
            "sikeres" => "true",
            "szamlaszam" => "I",
            _ => "127",
        };
        let body = body.replace(
            &format!("<{name}>{old}</{name}>"),
            &format!("<{name}>{text}</{name}>"),
        );
        assert!(StornoInvoice::new("I").parse(&raw(body)).is_err(), "{name}");
    }
}
