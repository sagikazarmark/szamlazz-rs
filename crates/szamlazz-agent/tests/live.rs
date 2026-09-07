//! Live tests against a real szamlazz.hu **test-mode** account.
//!
//! Ignored by default: they need `SZAMLAZZ_AGENT_KEY` set to an agent key of
//! an account switched into test mode (rate limit: 100 invoices/hour), and
//! they create real (test) documents. Run explicitly:
//!
//! ```sh
//! SZAMLAZZ_AGENT_KEY=... cargo test -p szamlazz-agent --features client-reqwest --test live -- --ignored
//! ```
//!
//! These are the tests that answer what the docs leave open: that the
//! whole-forint HUF line totals of `Rounding::minor_unit` stay inside the
//! tolerance of szamlazz.hu's `net = price × qty` check, rejected
//! `InvoiceKind` combinations, and empty-vs-omitted element handling.

#![cfg(feature = "client-reqwest")]

use jiff::civil::Date;
use rust_decimal::dec;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::query_pdf::InvoiceSelector;
use szamlazz_agent::ops::query_xml::{InvoiceAppearance, InvoiceDocument, QueryInvoiceXml};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::{
    Client, Credentials, Currency, InvoiceNumber, Language, LineItem, PaymentMethod, Rounding,
    VatRate,
};

fn client() -> Client {
    let key = std::env::var("SZAMLAZZ_AGENT_KEY")
        .expect("SZAMLAZZ_AGENT_KEY must point at a test-mode account");
    Client::new(Credentials::agent_key(key)).expect("client")
}

fn today() -> Date {
    // Live tests run on real infrastructure; wall clock is fine here.
    jiff::Zoned::now().date()
}

fn document(kind: InvoiceKind) -> CreateInvoice {
    let mut invoice = CreateInvoice::new(
        kind,
        InvoiceHeader::new(
            today(),
            today(),
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        ),
        Buyer::new("Teszt Vevő Kft.", "1010", "Budapest", "Teszt utca 1."),
        vec![
            // The worst case of minor-unit rounding: 2 × 1234.25 = 2468.5 →
            // 2469, a net half a forint off `price × qty`, inside the 259
            // tolerance szamlazz.hu was observed to have (P60: 2 accepted, 5
            // refused).
            LineItem::try_calculated(
                "Integrációs teszt tétel",
                dec!(2),
                "db",
                dec!(1234.25),
                VatRate::percent(27),
                Rounding::minor_unit(&Currency::HUF),
            )
            .expect("fits"),
        ],
    );
    invoice.download_pdf = true;
    invoice
}

#[tokio::test]
#[ignore = "requires SZAMLAZZ_AGENT_KEY for a test-mode account"]
async fn taxpayer_query() {
    // KBOSS.HU Kft. — the operator of szamlazz.hu itself.
    let info = client()
        .send(&QueryTaxpayer::new("13421739").expect("valid prefix"))
        .await
        .expect("query");
    assert!(info.valid);
    assert!(info.name.is_some());
}

#[tokio::test]
#[ignore = "requires SZAMLAZZ_AGENT_KEY for a test-mode account"]
async fn invoice_lifecycle() {
    let client = client();

    let created = client
        .send(&document(InvoiceKind::invoice()))
        .await
        .expect("create");
    assert!(created.pdf.is_some(), "requested PDF must be present");
    // HUF totals round to whole forints at each monetary step:
    // 2 × 1234.25 = 2468.5 → 2469; VAT 27% = 666.63 → 667; gross 3136.
    assert_eq!(created.net_total, Some(dec!(2469)));
    assert_eq!(created.gross_total, Some(dec!(3136)));

    let created_number = created
        .invoice_number
        .clone()
        .expect("issued invoice number");
    let storno = client
        .send(&StornoInvoice::new(created_number.clone()))
        .await
        .expect("storno");
    assert_ne!(storno.invoice_number, created_number);
}

#[tokio::test]
#[ignore = "requires SZAMLAZZ_AGENT_KEY for a test-mode account"]
async fn proforma_lifecycle() {
    let client = client();

    let created = client
        .send(&document(InvoiceKind::Proforma))
        .await
        .expect("create proforma");

    client
        .send(&DeleteProforma::new(ProformaSelector::InvoiceNumber(
            created.invoice_number.expect("issued proforma number"),
        )))
        .await
        .expect("delete proforma");
}

// ----- `eszamla` semantics (issue #73)

/// A `<eszamla>` code as szamlazz.hu reports it, for the probe table.
fn appearance_cell(document: &InvoiceDocument) -> String {
    let appearance = document.info.e_invoice;
    format!("{} ({appearance:?})", appearance.code())
}

/// One line of a case for an assertion message.
fn case_label(create_e_invoice: bool, storno_e_invoice: bool) -> String {
    format!("created eszamla={create_e_invoice}, storno eszamla={storno_e_invoice}")
}

async fn query_by_number(client: &Client, number: &InvoiceNumber) -> InvoiceDocument {
    client
        .send(&QueryInvoiceXml::new(InvoiceSelector::InvoiceNumber(
            number.clone(),
        )))
        .await
        .expect("query by number")
}

/// What `<eszamla>` means in a queried document, and whether a storno's
/// `eszamla` must match its original's — settled live (P73 in
/// `docs/szamlazz-hu-behaviour.md`): an invoice created with
/// `<eszamla>true</eszamla>` and one with `false`, each queried back, then
/// stornoed with a matching and a mismatching `eszamla` (four originals, since
/// a repeat storno only echoes the existing storno). Asserts what was
/// observed — the mapping the crate publishes as [`InvoiceAppearance`] (`1`
/// paper, `2`/`3` e-invoice), and that every storno is accepted and issued in
/// the *request's* form — and prints the four cases as a table
/// (`--nocapture`). A create refused by the account (no e-invoice feature)
/// fails the test with the code.
///
/// Every document is stornoed by the probe itself; nothing is left to clean up.
#[tokio::test]
#[ignore = "requires SZAMLAZZ_AGENT_KEY for a test-mode account"]
async fn eszamla_semantics() {
    let client = client();
    let tag = jiff::Timestamp::now().as_second() % 1_000_000;

    // (created as e-invoice?, storno as e-invoice?)
    let cases = [(true, true), (true, false), (false, true), (false, false)];

    println!("\nrun tag: {tag}");
    println!(
        "| Created `eszamla` | Original `<eszamla>` | Storno `eszamla` | Storno result | `SS` `<eszamla>` |"
    );
    println!("|---|---|---|---|---|");

    for (index, (create_e_invoice, storno_e_invoice)) in cases.into_iter().enumerate() {
        let label = case_label(create_e_invoice, storno_e_invoice);
        let order = format!("ESZ-{tag}-{index}");
        let mut invoice = document(InvoiceKind::invoice());
        invoice.header.order_number = Some(order.clone());
        invoice.external_id = Some(format!("esz-{tag}:{index}"));
        invoice.e_invoice = create_e_invoice;
        invoice.download_pdf = false;

        let created = client
            .send(&invoice)
            .await
            .unwrap_or_else(|error| panic!("{label}: create refused: {error}"));
        let number = created
            .invoice_number
            .clone()
            .expect("issued invoice number");
        let original = query_by_number(&client, &number).await;
        assert_eq!(original.info.document_type, "SZ", "{label}");
        assert_eq!(
            original.info.order_number.as_deref(),
            Some(order.as_str()),
            "{label}"
        );

        let mut storno = StornoInvoice::new(number.clone());
        storno.e_invoice = storno_e_invoice;
        storno.fulfillment_date = original.info.fulfillment_date;
        storno.external_id = Some(format!("esz-{tag}:{index}:storno"));
        let reversal = client
            .send(&storno)
            .await
            .unwrap_or_else(|error| panic!("{label}: storno of {number} refused: {error}"));
        assert!(
            reversal.reverses(&number),
            "{label}: storno of {number} echoed the original"
        );
        let storno_document = query_by_number(&client, &reversal.invoice_number).await;
        assert_eq!(storno_document.info.document_type, "SS", "{label}");

        println!(
            "| `{create_e_invoice}` | `{number}`: {} | `{storno_e_invoice}` | `sikeres=true`, `{}` | {} |",
            appearance_cell(&original),
            reversal.invoice_number,
            appearance_cell(&storno_document),
        );

        // The mapping the crate publishes: a document created as an e-invoice
        // reports an e-invoice code, a paper one reports `1`.
        assert_eq!(
            original.info.e_invoice.is_e_invoice(),
            create_e_invoice,
            "{label}: {number} queried as {:?}",
            original.info.e_invoice
        );
        if !create_e_invoice {
            assert_eq!(original.info.e_invoice, InvoiceAppearance::Paper, "{label}");
        }
        // The storno takes the request's form, whatever the original's: a
        // mismatch is neither refused nor corrected, so a caller that wants
        // the reversal in its original's form must derive the flag itself.
        assert_eq!(
            storno_document.info.e_invoice.is_e_invoice(),
            storno_e_invoice,
            "{label}: storno {} queried as {:?}",
            reversal.invoice_number,
            storno_document.info.e_invoice
        );
    }
}
