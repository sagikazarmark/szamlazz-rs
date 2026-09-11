//! Namespace identity must survive the serde boundary.
use szamlazz_agent::InvoiceSelector;
use szamlazz_agent::ops::{
    proforma::{DeleteProforma, ProformaSelector},
    query_xml::QueryInvoiceXml,
    receipt::{CreateReceipt, QueryReceipt, ReceiptSelector, SendReceipt, StornoReceipt},
    storno::StornoInvoice,
    taxpayer::QueryTaxpayer,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};

fn raw(body: String) -> RawResponse {
    RawResponse::new::<&str, &str>([], body.into_bytes())
}

/// Duplicate a complete row without changing its business content. Namespace
/// aliases and ignored container children must not change list membership.
fn repeated_row(body: &str, row: &str, namespace: &str, between: &str) -> String {
    let start = body.find(&format!("<{row}>")).expect("row start");
    let end = start + body[start..].find(&format!("</{row}>")).expect("row end") + row.len() + 3;
    let original = &body[start..end];
    let alias = original
        .replacen(
            &format!("<{row}>"),
            &format!(r#"<p:{row} xmlns:p="{namespace}">"#),
            1,
        )
        .replace(&format!("</{row}>"), &format!("</p:{row}>"));
    format!(
        "{}{original}{between}{alias}{}",
        &body[..start],
        &body[end..]
    )
}

#[test]
fn repeated_invoice_and_receipt_rows_use_expanded_names() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    let invoice = include_str!("synthetic/szamla_query.xml").replace("</szamla>",
        "<qutetek><qutet><nev>Ledger row</nev><afakulcs>20</afakulcs><netto>10</netto><afa>2</afa><brutto>12</brutto><afalevon>1</afalevon></qutet></qutetek></szamla>");
    let receipt = include_str!("synthetic/xmlnyugtavalasz.xml");
    let get = QueryReceipt::new(ReceiptSelector::ReceiptNumber("R".into()));
    for between in ["", "<extension/>", r#"<x:extension xmlns:x="urn:future"/>"#] {
        for row in ["tetel", "kifizetes", "afakulcsossz", "qutet"] {
            let doc = query
                .parse(&raw(repeated_row(
                    &invoice,
                    row,
                    "http://www.szamlazz.hu/szamla",
                    between,
                )))
                .expect("invoice rows");
            assert_eq!(doc.items.len(), if row == "tetel" { 2 } else { 1 });
            assert_eq!(
                doc.credit_entries.len(),
                if row == "kifizetes" { 2 } else { 1 }
            );
            assert_eq!(
                doc.totals.by_vat_rate.len(),
                if row == "afakulcsossz" { 2 } else { 1 }
            );
            assert_eq!(doc.items[0].name, "Synthetic service");
            assert_eq!(
                doc.financial_items.len(),
                if row == "qutet" { 2 } else { 1 }
            );
            if row == "qutet" {
                continue;
            }
            let response = raw(repeated_row(
                receipt,
                row,
                "http://www.szamlazz.hu/xmlnyugtavalasz",
                between,
            ));
            let doc = get.parse(&response).expect("receipt rows");
            assert_eq!(doc.items.len(), if row == "tetel" { 3 } else { 2 });
            assert_eq!(doc.payments.len(), if row == "kifizetes" { 3 } else { 2 });
            assert_eq!(
                doc.totals.by_vat_rate.len(),
                if row == "afakulcsossz" { 2 } else { 1 }
            );
            assert_eq!(doc.items.last().expect("last row").name, "Synthetic item B");
            assert_eq!(
                StornoReceipt::new("R")
                    .parse(&response)
                    .expect("storno rows"),
                doc
            );
            assert_eq!(
                CreateReceipt::new(
                    "R",
                    szamlazz_agent::PaymentMethod::Cash,
                    szamlazz_agent::Currency::HUF,
                    vec![]
                )
                .parse(&response)
                .expect("create rows"),
                doc
            );
        }
    }
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
fn namespace_reserved_names_are_checked_even_in_ignored_extensions() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    let taxpayer = QueryTaxpayer::new("12345678").expect("prefix");
    for (extension, valid) in [
        ("<xmlns:extension/>", false),
        ("<xmlns:extension></xmlns:extension>", false),
        ("<extension><xmlns:child/></extension>", false),
        ("<?p:target data?>", false),
        ("<?é data?>", true),
        ("<xml:extension/>", true),
        (
            r#"<xmlFuture:extension xmlns:xmlFuture="urn:future"/>"#,
            true,
        ),
        (
            r#"<xmlnsFuture:extension xmlns:xmlnsFuture="urn:future"/>"#,
            true,
        ),
    ] {
        let invoice = include_str!("synthetic/szamla_query.xml")
            .replace("</szamla>", &format!("{extension}</szamla>"));
        assert_eq!(query.parse(&raw(invoice)).is_ok(), valid, "{extension}");
        for (namespace, result) in [
            (
                "http://schemas.nav.gov.hu/OSA/2.0/api",
                "<result><funcCode>OK</funcCode></result>",
            ),
            (
                "http://schemas.nav.gov.hu/OSA/3.0/api",
                r#"<result xmlns="http://schemas.nav.gov.hu/NTCA/1.0/common"><funcCode>OK</funcCode></result>"#,
            ),
        ] {
            let body = format!(
                r#"<QueryTaxpayerResponse xmlns="{namespace}">{result}<taxpayerValidity>true</taxpayerValidity>{extension}</QueryTaxpayerResponse>"#
            );
            let parsed = taxpayer.parse(&raw(body));
            assert_eq!(parsed.is_ok(), valid, "{namespace}: {extension}");
            if let Ok(info) = parsed {
                assert!(info.valid);
            }
        }
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

#[test]
fn namespace_checks_normalize_bindings_and_check_expanded_attributes() {
    let query = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
    let taxpayer = QueryTaxpayer::new("12345678").expect("prefix");
    for (attributes, valid) in [
        (
            r#"xmlns:xml="http://www.w3.org/XML/1998/n&#97;mespace""#,
            true,
        ),
        (r#"xmlns:a="urn:a" xmlns:b="urn:b" a:x="1" b:x="2""#, true),
        (
            r#"xmlns:a="urn:same" xmlns:b="urn:s&#97;me" a:x="1" b:x="2""#,
            false,
        ),
        (
            r#"xmlns:x="http://www.w3.org/XML/1998/n&#97;mespace""#,
            false,
        ),
        (r#"xmlns="http://www.w3.org/XML/1998/namespace""#, false),
        (r#"xmlns="http://www.w3.org/2000/xmlns/""#, false),
        (r#"xmlns:x="""#, false),
        (r#"xmlns:xml="urn:wrong""#, false),
        // XML attribute normalization precedes comparison, but references
        // producing whitespace are not normalized again.
        (
            "xmlns:a=\"urn:a\nb\" xmlns:b=\"urn:a b\" a:x=\"1\" b:x=\"2\"",
            false,
        ),
        (
            r#"xmlns:a="urn:a&#10;b" xmlns:b="urn:a b" a:x="1" b:x="2""#,
            true,
        ),
        (
            r#"xmlns:a="urn:a&amp;b" xmlns:b="urn:a&#38;b" a:x="1" b:x="2""#,
            false,
        ),
        (
            r#"xmlns:a="urn:a&amp;amp;b" xmlns:b="urn:a&amp;b" a:x="1" b:x="2""#,
            true,
        ),
    ] {
        let extension = format!("<extension {attributes}/>");
        let invoice = include_str!("synthetic/szamla_query.xml")
            .replace("</szamla>", &format!("{extension}</szamla>"));
        assert_eq!(query.parse(&raw(invoice)).is_ok(), valid, "{attributes}");
        let nav = include_str!("synthetic/taxpayer.xml")
            .replace("<result>", &format!("{extension}<result>"));
        assert_eq!(taxpayer.parse(&raw(nav)).is_ok(), valid, "{attributes}");
    }
}

#[test]
fn aliases_and_interleaved_extensions_do_not_weaken_singleton_checks() {
    for contents in [
        "<sikeres>true</sikeres><extension/><p:sikeres>true</p:sikeres><szamlaszam>I</szamlaszam>",
        "<sikeres>true</sikeres><szamlaszam>I</szamlaszam><extension/><p:szamlaszam>J</p:szamlaszam>",
        "<sikeres>false</sikeres><hibakod>56</hibakod><extension/><p:hibakod>3</p:hibakod><szamlaszam>I</szamlaszam>",
    ] {
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz" xmlns:p="http://www.szamlazz.hu/xmlszamlavalasz">{contents}</xmlszamlavalasz>"#
        );
        assert!(StornoInvoice::new("I").parse(&raw(body)).is_err());
    }
}

#[test]
fn legal_attribute_quoting_survives_protocol_projection() {
    for attributes in [r#"note='a"b'"#, "note='a&quot;b &amp; c'", r#"note="a'b""#] {
        let body = format!(
            r#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz" {attributes}><sikeres>true</sikeres><extension {attributes}/><szamlaszam>I-1</szamlaszam></xmlszamlavalasz>"#
        );
        let doc = StornoInvoice::new("I")
            .parse(&raw(body))
            .expect("legal attribute");
        assert_eq!(doc.invoice_number.as_str(), "I-1");
    }
}
