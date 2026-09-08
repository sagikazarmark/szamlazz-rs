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

An invoice Ack echoes its document id and can include the registration number assigned by your system. `KEY_ERR` and `KEY_DEL` are deliberate control codes, not errors: `KEY_ERR` tells szamlazz.hu that the key is wrong, and **a bank transaction or receipt answered `KEY_ERR` is never resent** (an invoice only when it next changes); `KEY_DEL` severs the connection. Anything uncertain must be a non-200 status instead, which szamlazz.hu retries for up to 72 hours. The router follows that rule for you, in this order: a missing `X-Szamlazzhu-Key` header is `401` before the body is read, a body over the limit is `413`, an unknown root element is `400`, an unknown key is `200` + `KEY_ERR` in the Ack shape of the pushed kind, a key the resolver *could not check* is `503`, an authenticated body that is not the pushed document at all is `400`, and a handler failure is `500`. Only `200` ends a delivery.

## Shape, Not Content

A push is at-most-N-times delivery: szamlazz.hu retries a non-200 identically for 72 hours and then drops the record (for a bank transaction or a receipt, for good). A deterministic refusal therefore loses data, so `Document::parse` refuses only what it cannot Ack: a body that is not UTF-8 or XML, an unknown root or namespace, a missing document id (and, for an invoice, its number), a number or boolean that is not one. Everything else is content and reads as the wire delivers it: an element the XSD requires but the push omits is `None`, an unknown `<irany>` is `TransactionDirection::Other`, a `<pdf>` that does not decode is `None` with the encoded text still in `raw_xml()`, a date that is not a date is `None` with its text likewise in `raw_xml()`, an empty receipt batch has no receipts. Nearly every field is therefore an `Option`; only the identity (`info.id`, `info.invoice_number`, a transaction's `id`) is not.

The XSD's requirements are a signal, not a gate: `Document::validate` (and `InvoiceDocument::validate`, `BankTransaction::validate`, `ReceiptBatch::validate` from inside a `Handler`) reports the first one a parsed document misses, and `Document::parse_strict` refuses such a document for a caller that would rather have szamlazz.hu retry it.

Breaking change in 0.4: before it, the parse enforced the XSD's requirements and refused a non-conforming push. `BankTransaction`'s `bank_account`, `value_date`, `direction`, `technical`, `amount` and `currency` were required fields and are `Option`s now; `TransactionDirection` gained `Other(String)` (and is no longer `Copy`); `InvoiceInfo::invoice_number` is a `String` where it was an `Option` the parse required anyway; `ParseError::Validation` carries a `ValidationError`. A receiver that relied on the old refusals calls `Document::parse_strict`.

Several accounts behind one URL use `axum::router_with_resolver` with a `KeyResolver`, whose `resolve` is async and returns `Ok(Some(handler))`, `Ok(None)` for a key that is definitely unknown (→ `KEY_ERR`), or `Err(_)` when the lookup itself failed (→ `503`, so the record stays retryable). A resolver backed by a database or a secrets service must return `Err` on a timeout, never `Ok(None)`.

The router caps request bodies at `BodyLimit::DEFAULT` (64 MiB; over it is `413`). Számlázz.hu publishes no maximum and receipt batches are unbounded in principle, so `axum::router_with_body_limit` / `router_with_resolver_and_body_limit` take a `BodyLimit` to raise the cap or, as an explicit choice, lift it with `BodyLimit::Unlimited`.

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
