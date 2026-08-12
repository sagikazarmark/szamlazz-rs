# szamlazz-agent

[![crates.io](https://img.shields.io/crates/v/szamlazz-agent?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-agent)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-agent?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-agent)

**Sans-IO Rust client for the [szamlazz.hu Számla Agent](https://docs.szamlazz.hu/agent/basics/what-is).**

The core performs no I/O: request types serialize into a ready-to-send `WireRequest`, and typed responses parse from raw headers and body bytes. Any HTTP client can drive it on native Rust or `wasm32-unknown-unknown`, including Cloudflare Workers.

## Quick Start

Enable `client-reqwest` to use the ready-made async client:

```rust
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, InvoiceHeader, InvoiceKind};
use szamlazz_agent::{
    Client, Credentials, Currency, Date, Language, LineItem, PaymentMethod, VatRate,
};

async fn issue_invoice() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Credentials::agent_key("your-agent-key"))?;
    let header = InvoiceHeader::new(
        "2026-07-04".parse::<Date>()?,
        "2026-07-12".parse::<Date>()?,
        PaymentMethod::Transfer,
        Currency::HUF,
        Language::Hungarian,
    );
    let item = LineItem::calculated_for_currency(
        "Development",
        1.into(),
        "hour",
        10_000.into(),
        VatRate::percent(27),
        &Currency::HUF,
    );
    let request = CreateInvoice::new(
        InvoiceKind::invoice(),
        header,
        Buyer::new("Example Kft.", "1111", "Budapest", "Example utca 1."),
        vec![item],
    );

    let created = client.send(&request).await?;
    println!("issued: {:?}", created.invoice_number);
    Ok(())
}
```

Without `client-reqwest`, call `AgentRequest::to_wire`, send the resulting URL, content type, and body with your HTTP stack, then pass its headers and bytes to `RawResponse` and `AgentRequest::parse`.

## Feature Flags

No features are enabled by default. The [crate documentation](https://docs.rs/szamlazz-agent/latest/szamlazz_agent/#features) is authoritative for feature semantics and platform constraints.

- **`client-reqwest`** provides the ready-made async `Client` on native Rust and browser wasm.

## Operations

| Operation | Type |
|---|---|
| Invoice, proforma, prepayment invoice, final invoice, corrective invoice, or delivery note | `ops::invoice::CreateInvoice`, with the kind selected by `InvoiceKind` |
| Storno an invoice | `ops::storno::StornoInvoice` |
| Register credit entries | `ops::credit_entry::RegisterCreditEntry` |
| Query invoice PDF or full XML | `ops::query_pdf::QueryInvoicePdf`, `ops::query_xml::QueryInvoiceXml` |
| Delete a proforma | `ops::proforma::DeleteProforma` |
| Create, storno, query, or send receipts | `ops::receipt::*` |
| Look up a taxpayer through NAV | `ops::taxpayer::QueryTaxpayer` |

## Protocol Notes

- Identifiers are English; Rustdoc search also finds types by Hungarian names such as `díjbekérő` and `kintlévőség` through doc aliases.
- Errors are typed as `ErrorCode` values while preserving the verbatim Hungarian message.
- Agent code 56 means issuance succeeded but notification delivery failed. It sets `notification_delivery_failed = true`; do not retry that issued document.
- Response version 2 carries requested PDFs as base64 inside XML. The crate decodes them and exposes raw bytes through `Pdf`.
- Invoice creation has no idempotency key. Receipt call IDs prevent duplicate issuance by returning error 338 when reused, but do not replay the original success. The client never retries automatically.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
