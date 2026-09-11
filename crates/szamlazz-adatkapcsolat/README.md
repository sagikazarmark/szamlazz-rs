# szamlazz-adatkapcsolat

[![crates.io](https://img.shields.io/crates/v/szamlazz-adatkapcsolat?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-adatkapcsolat)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-adatkapcsolat?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-adatkapcsolat)

**Receiver toolkit for the szamlazz.hu [Online Pénzügyi Adatkapcsolat](https://docs.szamlazz.hu/penzugyi-adatkapcsolat/).**

Adatkapcsolat pushes outgoing invoices, incoming invoices, bank transactions, and daily receipt batches as XML to one registered receiver URL. The `X-Szamlazzhu-Key` header authenticates the connection, and the XML root identifies the document type.

## Quick Start

Identify the pushed kind, verify `X-Szamlazzhu-Key`, then parse the body and return the matching Ack XML; an unknown key is answered `KEY_ERR` in the Ack shape of the pushed kind, with nothing of the body parsed:

```rust
use szamlazz_adatkapcsolat::{Ack, ControlCode, Document, InvoiceAck, InvoiceDirection, keys_match};

fn acknowledge(
    presented_key: &str,
    configured_key: &str,
    body: &[u8],
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let kind = Document::identify(body)?;
    if !keys_match(presented_key, configured_key) {
        return Ok(ControlCode::KeyUnknown.to_xml(kind));
    }
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
#
# let body = br#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>987</id></banktranz>"#;
# let ack = String::from_utf8(acknowledge("k-1", "k-1", body)?)?;
# assert!(ack.contains("<banktranzvalasz") && !ack.contains("hibakod"));
# let ack = String::from_utf8(acknowledge("k-2", "k-1", body)?)?;
# assert!(ack.contains("<hibakod>KEY_ERR</hibakod>"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

`Document` is exhaustive: the four variants are the four streams a connection can push, and a fifth would be a new `Handler` method, a breaking change by design. Use the `axum` feature when you want `axum::router` to verify the key, dispatch documents to a `Handler`, and render Acks for you.

## Feature Flags

No features are enabled by default. Serde serialization and deserialization of the parsed invoice, transaction, and receipt types are part of the core crate and do not require a feature. The [crate documentation](https://docs.rs/szamlazz-adatkapcsolat/latest/szamlazz_adatkapcsolat/#features) is authoritative for feature semantics and platform constraints.

- **`axum`** provides the ready-made receiver router, including key verification; it supports native Rust and Cloudflare Workers.
- **`tracing`** makes the router log a handler's or a key resolver's error at `warn` (the response is a bare status either way; without the feature the error is dropped, so log your own).
- **`opendal`** provides `Archiver`, a `Handler` that persists documents through an OpenDAL operator. Enable the required storage services on your own `opendal` dependency.

## Receiver Contract

Implement every `Handler` method and return success only after the document has been durably accepted. Requiring all methods prevents a newly enabled document stream from being silently acknowledged and discarded. The router handles key verification, root-element dispatch, and Ack rendering.

`axum::nest_at` accepts both the configured base path and `/{key}` beneath it for accounts using `addkeytourl`. Register that receiver URL with a trailing slash because szamlazz.hu appends the key by literal concatenation.

An invoice Ack echoes its document id and can include the registration number assigned by your system. `KEY_ERR` and `KEY_DEL` are deliberate control codes, not errors: `KEY_ERR` tells szamlazz.hu that the key is wrong, and **a bank transaction or receipt answered `KEY_ERR` is never resent** (an invoice only when it next changes); `KEY_DEL` severs the connection. Anything uncertain must be a non-200 status instead, which szamlazz.hu retries for up to 72 hours. The router follows that rule for you, in this order: a missing `X-Szamlazzhu-Key` header is `401` before the body is read, a body over the limit is `413`, an unknown root element is `400`, an unknown key is `200` + `KEY_ERR` in the Ack shape of the pushed kind, a key the resolver *could not check* is `503`, an authenticated body that is not the pushed document at all is `400`, and a handler failure is `500`. Only `200` ends a delivery.

## Shape, Not Content

A push is at-most-N-times delivery: szamlazz.hu retries a non-200 identically for 72 hours and then drops the record (for a bank transaction or a receipt, for good). A deterministic refusal therefore loses data, so `Document::parse` refuses only a body that is not the pushed record: one that is not UTF-8 or XML, an unknown root or namespace, a record without its identity (an invoice's `alap/id` and `szamlaszam`, a bank transaction's `id`, a receipt's `alap/id`; missing or not an integer), a number or boolean that is not one. Everything else is content and reads as the wire delivers it: an element the XSD requires but the push omits is `None`, an unknown `<irany>` is `TransactionDirection::Other`, a `<pdf>` that does not decode is `None` with the encoded text still in `raw_xml()`, a date that is not a date is `None` with its text likewise in `raw_xml()`, an empty receipt batch has no receipts. Nearly every field is therefore an `Option`; only the identity (`info.id`, `info.invoice_number`, a transaction's `id`) is not.

The identity is shape because it is what you key the record by, not because every Ack echoes it: an invoice Ack echoes `alap/id`, a bank transaction's or a receipt batch's Ack carries no id at all, yet redelivery is the protocol's normal case (a lost Ack, a failed fan-out member) and a receiver tolerates it by the id (the archiver names its objects by it). szamlazz.hu assigns the id and its schemas type it an integer, so a push without one is not a record it holds. The cost: a receipt batch with one id-less `<nyugta>` is refused whole.

The XSD's requirements are a signal, not a gate: `Document::validate` (and `InvoiceDocument::validate`, `BankTransaction::validate`, `ReceiptBatch::validate` from inside a `Handler`) reports the first one a parsed document misses as a typed `ValidationError` (`MissingRequired { path }`, `UnknownToken { path, token }`, `Negative { path }`, `Empty { path, child }`, each displaying as before: `missing required invoice alap/kelt`), and `Document::parse_strict` refuses such a document for a caller that would rather have szamlazz.hu retry it.

Several connections behind one URL (several szamlazz.hu accounts, each pushing under its own key) use `axum::router_with_resolver` with a `KeyResolver`, whose `resolve` is async and returns `Ok(Some(handler))` (an `Arc`, so the handler may be one the resolver holds or one built per request from what the lookup found), `Ok(None)` for a key that is definitely unknown (→ `KEY_ERR`), or `Err(_)` when the lookup itself failed (→ `503`, so the record stays retryable). A resolver backed by a database or a secrets service must return `Err` on a timeout, never `Ok(None)`. Compare keys that are secrets with `keys_match`, in constant time.

Without the router, `Document::identify` names the pushed kind (`RootKind`) from the root element alone, so a receiver can follow the protocol's order (identify, authenticate, then parse) and answer an unknown key with `ControlCode::KeyUnknown.to_xml(kind)` having parsed nothing.

The router caps request bodies at `BodyLimit::DEFAULT` (64 MiB; over it is `413`). Számlázz.hu publishes no maximum and receipt batches are unbounded in principle, so `axum::router_with_body_limit` / `router_with_resolver_and_body_limit` take a `BodyLimit` to raise the cap or, as an explicit choice, lift it with `BodyLimit::Unlimited`.

The core is framework-free and `wasm32`-clean. On wasm, `Handler` drops its `Send` bounds so JavaScript futures can implement it. The axum router applies the same single-thread `Send` assertion as `#[worker::send]`; your own routes still need their usual Workers integration. Invoice Ack rendering is fallible so an invalid registration number cannot produce malformed XML.

## Breaking Changes in 0.4

One release, so a receiver pays the migration once:

- The parse is lenient (above): `BankTransaction`'s `bank_account`, `value_date`, `direction`, `technical`, `amount` and `currency` were required fields and are `Option`s; `TransactionDirection` gained `Other(String)` (and is no longer `Copy`); `InvoiceInfo::invoice_number` is a `String` where it was an `Option` the parse required anyway. A receiver that relied on the old refusals calls `Document::parse_strict`.
- `ValidationError` is an enum naming the element (`MissingRequired`, `UnknownToken`, `Negative`, `Empty`), not an opaque string; its `Display` text is unchanged.
- `ControlCode::KeyError`, `InvoiceAck::key_error()` and `Ack::key_error()` are `KeyUnknown` / `key_unknown()`: a control code is not an error.
- `Document`, `InvoiceDirection` and the new public `RootKind` are exhaustive; a `_ =>` arm over them is now an unreachable-pattern warning.
- `KeyResolver::resolve` returns `Option<Arc<Self::Handler>>` (owned) where it returned `Option<&Self::Handler>`; a resolver that held handlers wraps them in `Arc` once and clones the `Arc` per request.
- `Handler::Error` and `KeyResolver::Error` are bound by `std::error::Error` where they were bound by `Display`; `String` no longer qualifies, `std::convert::Infallible` and any `thiserror` type do. `HandlerFailure::error` is the member's boxed error (`BoxError`), not a `String`.
- `Fanout::with` requires the member's `Error` to be `'static`, and `Send + Sync` on native targets (not on `wasm32`).
- `InvoiceAck::for_document` is public and no longer behind the `axum` feature.
- The integer-width policy (ADR 0010): every integer of a pushed document is an `i64`, so `InvoiceInfo::id` and `Party::location` are `i64` (they were `i32`), and `InvoiceAck::accept` / `for_document` take an `i64`. `InvoiceAppearance::is_e_invoice` is the `Electronic` variant, whatever code it carries.
- The English names of two wire elements follow the workspace's vocabulary: `InvoiceInfo::kind` is `document_type` and `InvoiceInfo::e_invoice` is `appearance` (a code, not a flag); `BankTransaction::kind` is `transaction_type`, `ReceiptInfo::kind` is `document_type`. The archived JSON carries the new keys.
- Invoice `RecordedPayment` is `RecordedCreditEntry`, `InvoiceDocument::payments` is `credit_entries`, and the entry's `method` is `title` (`jogcim`, the payment method's wire token, kept as `Option<String>`). This follows the shared vocabulary of ADR 0010. Receipt payments remain `payments` / `ReceiptPayment::method`: they describe how the buyer paid. XML element names are unchanged. Newly archived invoice JSON uses `credit_entries` and `title`; there are no compatibility aliases or archive migrations, and historical JSON is not rewritten.

## Archiving

With `opendal`, `Archiver` can store the exact pushed XML, an embedded invoice PDF, and typed JSON independently; all three default to enabled, and JSON omits the PDF bytes. Paths default to `{type}/{YYYY}/{MM}/{name}`, relative to the OpenDAL operator root and dated from the document. Invoices and receipts use their Adatkapcsolat record id, avoiding collisions between business numbers. Receipt-batch XML is stored once as `batch-{first-id}-{last-id}.xml`, while receipts retain individual JSON files. Existing business-number receipt artifacts are not automatically migrated; consumers should switch to id paths and retain historical artifacts as needed.

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
