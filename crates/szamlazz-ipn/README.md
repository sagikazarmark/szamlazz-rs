# szamlazz-ipn

[![crates.io](https://img.shields.io/crates/v/szamlazz-ipn?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-ipn)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-ipn?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-ipn)

**Receiver types for szamlazz.hu IPN (instant payment notification) status snapshots.**

IPN is the form-urlencoded POST szamlazz.hu sends when an invoice's or proforma's paid amount changes.

## Quick Start

Parse a raw IPN request body without a web framework:

```rust
use szamlazz_ipn::PaymentNotification;

fn receive_ipn(body: &[u8]) -> Result<(), szamlazz_ipn::IpnParseError> {
    let status = PaymentNotification::from_form_bytes(body)?;
    println!("{} has paid {}", status.document_number, status.paid_gross);
    Ok(())
}
```

With the `axum` feature, `PaymentNotification` can instead be used directly as a request extractor. Return HTTP 200 only after the status has been durably accepted; szamlazz.hu retries every three minutes, up to ten times, after any other status.

## Feature Flags

No features are enabled by default. The [crate documentation](https://docs.rs/szamlazz-ipn/latest/szamlazz_ipn/#features) is authoritative for feature semantics and platform constraints.

- **`axum`** implements the axum request extractor for `PaymentNotification`.
- **`serde`** implements Serde serialization and deserialization for `PaymentNotification`.

## Delivery Semantics

Each payload is an absolute snapshot of the document's current payment status, not a new credit entry or a delta. Deliveries can be retried and changes can be coalesced, so replace or upsert the stored status by `(configured account or endpoint context, document_number)`. Document numbers alone are not unique across szamlazz.hu accounts, and IPN carries no account identifier. **Never add `paid_gross` to a previously stored amount.**

IPN covers invoices and proformas but has no reliable document-kind discriminator. The optional payment date is sent only after szamlazz.hu customer service enables it for the account.

IPN is unauthenticated by design. Register a URL containing an unguessable path segment, and optionally treat `SOURCE_IPS` as a defense-in-depth signal rather than authentication.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
