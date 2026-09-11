//! Archiver tests against an in-memory `OpenDAL` operator.

#![cfg(feature = "opendal")]

mod common;

use std::path::Path;

use common::{BANK_TRANSACTION, OUTGOING_INVOICE, RECEIPT_BATCH};
use opendal::Operator;
use szamlazz_adatkapcsolat::archive::{Archiver, Layout, Redelivery};
use szamlazz_adatkapcsolat::{Document, Handler as _};

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
    common::outgoing(&body).expect("parse")
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
    assert_eq!(
        value["info"]["appearance"], 1,
        "the code as an integer; `e_invoice` in 0.3"
    );
    assert_eq!(value["info"]["document_type"], "SZ", "`kind` in 0.3");
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
    assert_eq!(value["credit_entries"][0]["exchange_rate"], "275");
    assert_eq!(value["credit_entries"][0]["title"], "transfer");
    assert!(value.get("payments").is_none());
    assert!(value["credit_entries"][0].get("method").is_none());
    assert!(value.get("pdf").is_none());
}

#[tokio::test]
async fn archives_invoice_credit_entry_titles_as_received_in_both_directions() {
    for incoming in [false, true] {
        for title in [Some("új elszámolási mód"), None] {
            let op = memory();
            let archiver = Archiver::new(op.clone());
            let element =
                title.map_or_else(String::new, |title| format!("<jogcim>{title}</jogcim>"));
            let body = common::with(OUTGOING_INVOICE, "<jogcim>transfer</jogcim>", &element);
            let (body, directory) = if incoming {
                (common::as_incoming(&body), "incoming-invoices")
            } else {
                (body, "outgoing-invoices")
            };
            let _ = match Document::parse(body.as_bytes()).expect("parse fixture") {
                Document::OutgoingInvoice(invoice) => archiver.outgoing_invoice(invoice).await,
                Document::IncomingInvoice(invoice) => archiver.incoming_invoice(invoice).await,
                other => panic!("expected invoice, got {other:?}"),
            }
            .expect("archive invoice");

            let json = op
                .read(&format!("{directory}/2015/12/123456.json"))
                .await
                .expect("read JSON");
            let value: serde_json::Value = serde_json::from_slice(&json.to_vec()).expect("JSON");
            let entries = value["credit_entries"].as_array().expect("credit entries");
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].get("title"), Some(&serde_json::json!(title)));
            assert_eq!(entries[0]["amount"], "200");
            assert!(value.get("payments").is_none());
            assert!(entries[0].get("method").is_none());

            let xml = op
                .read(&format!("{directory}/2015/12/123456.xml"))
                .await
                .expect("read XML");
            assert_eq!(xml.to_vec(), body.as_bytes());
        }
    }
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

    let tx = common::bank_transaction(BANK_TRANSACTION).expect("parse");
    let _ = archiver.bank_transaction(tx).await.expect("archive tx");

    // Record ids remain the identity even when a business number is missing.
    let batch_xml = RECEIPT_BATCH.replacen(
        "<nyugtaszam>NYGTA-2026-2</nyugtaszam>",
        "<nyugtaszam></nyugtaszam>",
        1,
    );
    let batch = common::receipts(&batch_xml).expect("parse");
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
    assert_eq!(receipt_xml.to_vec(), batch_xml.as_bytes());

    let receipt = op
        .read("receipts/2026/07/1.json")
        .await
        .expect("read receipt");
    let receipt: serde_json::Value =
        serde_json::from_slice(&receipt.to_vec()).expect("receipt json");
    assert_eq!(receipt["info"]["customer_ledger"], "311");
    assert_eq!(receipt["items"][0]["ledger"]["revenue"], "911");
    assert_eq!(receipt["payments"][0]["method"], "készpénz");
    assert_eq!(receipt["payments"][0]["amount"], "25400");
    assert!(receipt.get("credit_entries").is_none());
    assert!(receipt["payments"][0].get("title").is_none());

    assert_eq!(
        keys(&op).await,
        vec![
            "bank-transactions/2026/07/987.json".to_owned(),
            "bank-transactions/2026/07/987.xml".to_owned(),
            "receipts/2026/07/1.json".to_owned(),
            "receipts/2026/07/2.json".to_owned(),
            "receipts/2026/07/batch-1-2.xml".to_owned(),
        ]
    );
}

#[tokio::test]
async fn receipt_record_ids_prevent_business_number_and_fallback_collisions() {
    for (first, second) in [("NY/1", "NY-1"), ("2", "")] {
        let op = memory();
        let archiver = Archiver::builder(op.clone()).save_xml(false).build();
        let xml = RECEIPT_BATCH
            .replace("NYGTA-2026-1", first)
            .replace("NYGTA-2026-2", second);
        let _ = archiver
            .receipts(common::receipts(&xml).expect("batch"))
            .await
            .expect("Ack");
        assert_eq!(
            keys(&op).await,
            ["receipts/2026/07/1.json", "receipts/2026/07/2.json"]
        );
        for id in [1, 2] {
            let bytes = op
                .read(&format!("receipts/2026/07/{id}.json"))
                .await
                .expect("receipt");
            let record: serde_json::Value = serde_json::from_slice(&bytes.to_vec()).expect("JSON");
            assert_eq!(record["info"]["id"], id);
        }
    }
}

// The parse no longer refuses an empty batch or a transaction without a
// value date; the archiver must take both without a panic.
#[tokio::test]
async fn archives_content_the_parse_no_longer_refuses() {
    let op = memory();
    let archiver = Archiver::new(op.clone());

    let Document::Receipts(empty) =
        Document::parse(br#"<xmlnyugtaarchiv xmlns="http://www.szamlazz.hu/xmlnyugtaarchiv"/>"#)
            .expect("parse")
    else {
        panic!("expected receipts");
    };
    let _ = archiver
        .receipts(empty)
        .await
        .expect("an empty batch has nothing to archive");
    assert!(keys(&op).await.is_empty());

    let Document::BankTransaction(undated) = Document::parse(
        br#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>5</id></banktranz>"#,
    )
    .expect("parse") else {
        panic!("expected bank transaction");
    };
    let _ = archiver
        .bank_transaction(undated)
        .await
        .expect("archive an undated transaction");
    assert_eq!(
        keys(&op).await,
        vec![
            "bank-transactions/undated/5.json".to_owned(),
            "bank-transactions/undated/5.xml".to_owned(),
        ]
    );
}
