# szamlazz-adatkapcsolat

[![crates.io](https://img.shields.io/crates/v/szamlazz-adatkapcsolat?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-adatkapcsolat)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-adatkapcsolat?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-adatkapcsolat)

**Receiver toolkit for the szamlazz.hu [Online Pénzügyi Adatkapcsolat](https://docs.szamlazz.hu/penzugyi-adatkapcsolat/).**

Adatkapcsolat pushes outgoing invoices, incoming invoices, bank transactions, and daily receipt batches as XML to one registered receiver URL. The `X-Szamlazzhu-Key` header authenticates the connection, and the XML root identifies the document type.

## Quick Start

After verifying `X-Szamlazzhu-Key`, parse the body and return the matching Ack XML:

```rust
use szamlazz_adatkapcsolat::{Ack, Document, InvoiceAck, InvoiceDirection};

fn acknowledge(body: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    match Document::parse(body)? {
        Document::OutgoingInvoice(invoice) => {
            Ok(InvoiceAck::accept(invoice.info.id).to_xml(InvoiceDirection::Outgoing)?)
        }
        Document::IncomingInvoice(invoice) => {
            Ok(InvoiceAck::accept(invoice.info.id).to_xml(InvoiceDirection::Incoming)?)
        }
        Document::BankTransaction(_) => Ok(Ack::accept().to_bank_transaction_xml()),
        Document::Receipts(_) => Ok(Ack::accept().to_receipts_xml()),
    }
}
```

Use the `axum` feature when you want `axum::router` to verify the key, dispatch documents to a `Handler`, and render Acks for you.

## Feature Flags

No features are enabled by default. Serde serialization and deserialization of the parsed invoice, transaction, and receipt types are part of the core crate and do not require a feature. The [crate documentation](https://docs.rs/szamlazz-adatkapcsolat/latest/szamlazz_adatkapcsolat/#features) is authoritative for feature semantics and platform constraints.

- **`axum`** provides the ready-made receiver router, including key verification; it supports native Rust and Cloudflare Workers.
- **`opendal`** provides `Archiver`, a `Handler` that persists documents through an OpenDAL operator. Enable the required storage services on your own `opendal` dependency.

## Receiver Contract

Implement every `Handler` method and return success only after the document has been durably accepted. Requiring all methods prevents a newly enabled document stream from being silently acknowledged and discarded. The router handles key verification, root-element dispatch, and Ack rendering.

`axum::nest_at` accepts both the configured base path and `/{key}` beneath it for accounts using `addkeytourl`. Register that receiver URL with a trailing slash because szamlazz.hu appends the key by literal concatenation.

An invoice Ack echoes its document id and can include the registration number assigned by your system. `KEY_ERR` tells szamlazz.hu that the key is wrong and stops sending until it changes; `KEY_DEL` severs the connection. These are deliberate control codes. Transient failures should return a non-200 status so szamlazz.hu retries for up to 72 hours.

The core is framework-free and `wasm32`-clean. On wasm, `Handler` drops its `Send` bounds so JavaScript futures can implement it. The axum router applies the same single-thread `Send` assertion as `#[worker::send]`; your own routes still need their usual Workers integration. Invoice Ack rendering is fallible so an invalid registration number cannot produce malformed XML.

## Archiving

With `opendal`, `Archiver` can store the exact pushed XML, an embedded invoice PDF, and typed JSON independently; all three default to enabled, and JSON omits the PDF bytes. Paths default to `{type}/{YYYY}/{MM}/{name}`, relative to the OpenDAL operator root and dated from the document. Invoices use their Adatkapcsolat document id; receipts use their business number with an id fallback. Receipt-batch XML is stored once as `batch-{first-id}-{last-id}.xml`, while receipts retain individual JSON files.

Storage failures become handler failures, producing a non-200 status and a later redelivery. `Redelivery::Timestamped` and `Redelivery::Both` use conditional create-only writes so concurrent receiver instances cannot overwrite a historical version; those modes require an OpenDAL service with `if_not_exists` support.

## Composing Handlers

`Handler` is not dyn-compatible, so the always-available `Fanout` type erases handler types and delivers every document to all registered handlers:

```text
let handler = Fanout::new()
    .with(Archiver::new(operator))
    .with(MyBusinessLogic { database });
```

All members run even if one fails. The delivery then fails with a per-handler report, and szamlazz.hu redelivers to every member, so handlers must tolerate redelivery. Acks merge by taking the strongest control code, or otherwise the first registration number supplied.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
