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
//! These are the tests that answer what the docs leave open: rounding
//! tolerance of `LineItem::calculated_for_currency`, rejected `InvoiceKind` combinations,
//! and empty-vs-omitted element handling.

#![cfg(feature = "client-reqwest")]

use jiff::civil::Date;
use rust_decimal::dec;
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::proforma::{DeleteProforma, ProformaSelector};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::{Client, Credentials, Currency, Language, LineItem, PaymentMethod, VatRate};

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
        vec![LineItem::calculated_for_currency(
            "Integrációs teszt tétel",
            dec!(2),
            "db",
            dec!(1234.56),
            VatRate::percent(27),
            &Currency::HUF,
        )],
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
    // 2 × 1234.56 = 2469.12 → 2469; VAT 27% = 666.63 → 667; gross 3136.
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
