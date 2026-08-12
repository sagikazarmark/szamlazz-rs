//! Archiver tests against an in-memory `OpenDAL` operator.

#![cfg(feature = "opendal")]

use std::path::Path;

use opendal::Operator;
use szamlazz_adatkapcsolat::archive::{Archiver, Layout, Redelivery};
use szamlazz_adatkapcsolat::{Document, Handler as _};

const OUTGOING_INVOICE: &str = include_str!("synthetic/szamla.xml");

const BANK_TRANSACTION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<banktranz xmlns="http://www.szamlazz.hu/banktranz">
  <id>987</id>
  <bankszamla>11111111-22222222-33333333</bankszamla>
  <erteknap>2026-07-03</erteknap>
  <irany>BE</irany>
  <technikai>false</technikai>
  <osszeg>12700.0</osszeg>
  <devizanem>HUF</devizanem>
</banktranz>"#;

const RECEIPT_BATCH: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv">
  <nyugta>
    <alap><id>1</id><nyugtaszam>NYGTA-2026-1</nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-03</kelt><fizmod>készpénz</fizmod><penznem>HUF</penznem><fokonyvVevo>311</fokonyvVevo><teszt>false</teszt><adoszam>12345678-1-42</adoszam></alap>
    <tetelek><tetel><megnevezes>Item</megnevezes><nettoEgysegar>100</nettoEgysegar><mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>100</netto><afakulcs>27</afakulcs><afa>27</afa><brutto>127</brutto><fokonyv><arbevetel>911</arbevetel><afa>467</afa></fokonyv></tetel></tetelek>
    <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>100</netto><afa>27</afa><brutto>127</brutto></afakulcsossz><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek>
  </nyugta>
  <nyugta>
    <alap><id>2</id><nyugtaszam></nyugtaszam><tipus>NY</tipus><stornozott>false</stornozott><kelt>2026-07-03</kelt><fizmod>bankkártya</fizmod><penznem>HUF</penznem><teszt>false</teszt><adoszam>12345678-1-42</adoszam></alap>
    <tetelek><tetel><megnevezes>Item</megnevezes><nettoEgysegar>100</nettoEgysegar><mennyiseg>1</mennyiseg><mennyisegiEgyseg>db</mennyisegiEgyseg><netto>100</netto><afakulcs>27</afakulcs><afa>27</afa><brutto>127</brutto></tetel></tetelek>
    <osszegek><afakulcsossz><afakulcs>27</afakulcs><netto>100</netto><afa>27</afa><brutto>127</brutto></afakulcsossz><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek>
  </nyugta>
</xmlnyugtaarchiv>"#;

fn memory() -> Operator {
    Operator::new(opendal::services::Memory::default()).expect("memory operator")
}

/// The synthetic invoice (number 2015-123, issued 2015-12-01) with a real
/// base64 PDF payload spliced in place of the fixture's empty element.
fn invoice_with_pdf() -> szamlazz_adatkapcsolat::InvoiceDocument {
    let start = OUTGOING_INVOICE.find("<pdf>").expect("pdf start");
    let end = OUTGOING_INVOICE.find("</pdf>").expect("pdf end") + "</pdf>".len();
    let body = format!(
        "{}<pdf>JVBERi0=</pdf>{}",
        &OUTGOING_INVOICE[..start],
        &OUTGOING_INVOICE[end..]
    )
    .replacen(
        "<megjegyzes></megjegyzes>",
        "<megjegyzes></megjegyzes><afatipus>EU-OSS</afatipus>",
        1,
    )
    .replacen(
        "<osszegek>",
        "<qutetek><qutet><nev>Fee</nev><afakulcs>27</afakulcs><netto>10</netto><afa>2.7</afa><brutto>12.7</brutto><afalevon>1</afalevon><cimkek><cimke>finance</cimke></cimkek></qutet></qutetek><cimkek><cimke>priority</cimke></cimkek><osszegek>",
        1,
    );
    match Document::parse(body.as_bytes()).expect("parse") {
        Document::OutgoingInvoice(invoice) => invoice,
        other => panic!("expected outgoing invoice, got {other:?}"),
    }
}

async fn keys(op: &Operator) -> Vec<String> {
    let mut entries: Vec<String> = op
        .list_with("")
        .recursive(true)
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.path().to_owned())
        .filter(|path| !path.ends_with('/'))
        .collect();
    entries.sort();
    entries
}

#[tokio::test]
async fn archives_invoice_pdf_and_data_monthly() {
    let op = memory();
    let archiver = Archiver::new(op.clone());
    let invoice = invoice_with_pdf();
    let source_xml = invoice.raw_xml().expect("source XML").to_owned();

    let ack = archiver.outgoing_invoice(invoice).await.expect("archive");
    let ack_xml = String::from_utf8(
        ack.to_xml(szamlazz_adatkapcsolat::InvoiceDirection::Outgoing)
            .expect("valid Ack"),
    )
    .expect("utf-8");
    assert!(ack_xml.contains("<id>123456</id>"));

    assert_eq!(
        keys(&op).await,
        vec![
            "outgoing-invoices/2015/12/123456.json".to_owned(),
            "outgoing-invoices/2015/12/123456.pdf".to_owned(),
            "outgoing-invoices/2015/12/123456.xml".to_owned(),
        ]
    );

    let xml = op
        .read("outgoing-invoices/2015/12/123456.xml")
        .await
        .expect("read XML");
    assert_eq!(xml.to_vec(), source_xml.as_bytes());

    let pdf = op
        .read("outgoing-invoices/2015/12/123456.pdf")
        .await
        .expect("read pdf");
    assert_eq!(pdf.to_vec(), b"%PDF-");

    // The JSON keeps the data but not the embedded PDF.
    let json = op
        .read("outgoing-invoices/2015/12/123456.json")
        .await
        .expect("read json");
    let value: serde_json::Value = serde_json::from_slice(&json.to_vec()).expect("json");
    assert_eq!(value["info"]["invoice_number"], "2015-123");
    assert_eq!(value["info"]["source"], 34);
    assert_eq!(value["info"]["e_invoice"], 1);
    assert_eq!(value["info"]["kata_ledger"], false);
    assert_eq!(value["info"]["vat_type"], "EU-OSS");
    assert_eq!(value["buyer"]["location"], 1);
    assert_eq!(value["buyer"]["buyer_ledger"]["customer"], "12345A");
    assert_eq!(value["items"][0]["vat_type"], "ÁKK");
    assert_eq!(value["items"][0]["vat_rate"], "0");
    assert_eq!(value["items"][0]["ordering"], 1);
    assert_eq!(value["items"][0]["ledger"]["revenue"], "12345A");
    assert_eq!(value["financial_items"][0]["name"], "Fee");
    assert_eq!(value["financial_items"][0]["tags"][0], "finance");
    assert_eq!(value["tags"][0], "priority");
    assert_eq!(value["payments"][0]["exchange_rate"], "275");
    assert!(value.get("pdf").is_none());
}

#[tokio::test]
async fn flat_layout_and_toggles() {
    let op = memory();
    let archiver = Archiver::builder(op.clone())
        .layout(Layout::Flat)
        .save_pdf(false)
        .build();

    let _ = archiver
        .outgoing_invoice(invoice_with_pdf())
        .await
        .expect("archive");

    assert_eq!(
        keys(&op).await,
        vec![
            "outgoing-invoices/123456.json".to_owned(),
            "outgoing-invoices/123456.xml".to_owned(),
        ]
    );
}

#[tokio::test]
async fn data_toggle_off_still_writes_source_xml_and_pdf() {
    let op = memory();
    let archiver = Archiver::builder(op.clone()).save_data(false).build();

    let _ = archiver
        .outgoing_invoice(invoice_with_pdf())
        .await
        .expect("archive");

    assert_eq!(
        keys(&op).await,
        vec![
            "outgoing-invoices/2015/12/123456.pdf".to_owned(),
            "outgoing-invoices/2015/12/123456.xml".to_owned(),
        ]
    );
}

#[tokio::test]
async fn redelivery_both_keeps_history_and_latest() {
    let op = memory();
    let archiver = Archiver::builder(op.clone())
        .save_pdf(false)
        .save_xml(false)
        .redelivery(Redelivery::Both)
        .build();

    let _ = archiver
        .outgoing_invoice(invoice_with_pdf())
        .await
        .expect("archive");

    let keys = keys(&op).await;
    assert_eq!(keys.len(), 2, "latest + timestamped copy: {keys:?}");
    assert!(keys.contains(&"outgoing-invoices/2015/12/123456.json".to_owned()));
    let timestamped = keys
        .iter()
        .find(|k| k.as_str() != "outgoing-invoices/2015/12/123456.json")
        .expect("timestamped copy");
    assert!(
        timestamped.starts_with("outgoing-invoices/2015/12/123456.2")
            && Path::new(timestamped)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json")),
        "unexpected timestamped key: {timestamped}"
    );
}

#[tokio::test]
async fn incoming_invoices_with_the_same_business_number_do_not_overwrite() {
    let op = memory();
    let archiver = Archiver::builder(op.clone())
        .save_pdf(false)
        .save_xml(false)
        .build();
    let first = invoice_with_pdf();
    let mut second = invoice_with_pdf();
    second.info.id += 1;

    let _ = archiver
        .incoming_invoice(first)
        .await
        .expect("first archive");
    let _ = archiver
        .incoming_invoice(second)
        .await
        .expect("second archive");

    assert_eq!(
        keys(&op).await,
        vec![
            "incoming-invoices/2015/12/123456.json".to_owned(),
            "incoming-invoices/2015/12/123457.json".to_owned(),
        ]
    );
}

#[tokio::test]
async fn timestamped_redeliveries_always_get_distinct_versions() {
    let op = memory();
    let archiver = Archiver::builder(op.clone())
        .save_pdf(false)
        .save_xml(false)
        .redelivery(Redelivery::Timestamped)
        .build();

    let _ = archiver
        .outgoing_invoice(invoice_with_pdf())
        .await
        .expect("first delivery");
    let _ = archiver
        .outgoing_invoice(invoice_with_pdf())
        .await
        .expect("second delivery");

    assert_eq!(keys(&op).await.len(), 2);
}

#[tokio::test]
async fn bank_transactions_and_receipt_batches() {
    let op = memory();
    let archiver = Archiver::new(op.clone());

    let Document::BankTransaction(tx) =
        Document::parse(BANK_TRANSACTION.as_bytes()).expect("parse")
    else {
        panic!("expected bank transaction");
    };
    let _ = archiver.bank_transaction(tx).await.expect("archive tx");

    let Document::Receipts(batch) = Document::parse(RECEIPT_BATCH.as_bytes()).expect("parse")
    else {
        panic!("expected receipts");
    };
    let _ = archiver.receipts(batch).await.expect("archive receipts");

    let bank_xml = op
        .read("bank-transactions/2026/07/987.xml")
        .await
        .expect("read bank XML");
    assert_eq!(bank_xml.to_vec(), BANK_TRANSACTION.as_bytes());
    let receipt_xml = op
        .read("receipts/2026/07/batch-1-2.xml")
        .await
        .expect("read receipt XML");
    assert_eq!(receipt_xml.to_vec(), RECEIPT_BATCH.as_bytes());

    let receipt = op
        .read("receipts/2026/07/NYGTA-2026-1.json")
        .await
        .expect("read receipt");
    let receipt: serde_json::Value =
        serde_json::from_slice(&receipt.to_vec()).expect("receipt json");
    assert_eq!(receipt["info"]["customer_ledger"], "311");
    assert_eq!(receipt["items"][0]["ledger"]["revenue"], "911");

    assert_eq!(
        keys(&op).await,
        vec![
            "bank-transactions/2026/07/987.json".to_owned(),
            "bank-transactions/2026/07/987.xml".to_owned(),
            // Receipt with a number uses it; the numberless one falls back to id.
            "receipts/2026/07/2.json".to_owned(),
            "receipts/2026/07/NYGTA-2026-1.json".to_owned(),
            "receipts/2026/07/batch-1-2.xml".to_owned(),
        ]
    );
}
