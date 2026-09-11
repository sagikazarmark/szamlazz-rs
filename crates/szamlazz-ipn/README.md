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
    println!("{} reports paid {:?}", status.document_number, status.paid_gross);
    // Durably enqueue a query of this document through the Számla Agent
    // on the account configured for this receiver before financial actions.
    Ok(())
}
# receive_ipn(b"szlahu_szamlaszam=E-2026-123&szlahu_kifizdat=unknown")?;
# Ok::<(), szamlazz_ipn::IpnParseError>(())
```

With the `axum` feature, `PaymentNotification` can instead be used directly as a request extractor. Return HTTP 200 only after the notification has been durably accepted (for example, queued for a query); szamlazz.hu retries every three minutes, up to ten times, after any other status. Successful extraction alone does not persist the notification.

## Shape vs Content

Parsing refuses **shape**: malformed form encoding (invalid percent escapes or decoded text that is not UTF-8), or an absent, empty or whitespace-only `szlahu_szamlaszam`. The axum extractor answers these with HTTP 400; body-extraction failures retain their HTTP status, such as 413.

Everything else is **content**. `gross_total`, `paid_gross`, `payment_date` and `payment_method` are optional. Missing or empty values become `None`, as do unreadable dates and amounts. Dates are read only as `%Y-%m-%d`; a datetime remains available as raw text. Amounts accept a dot or a lone comma as the decimal separator and use `Decimal::from_str_exact`: a value that cannot be represented exactly becomes `None`, never silently rounded. Unknown non-empty payment methods remain strings.

`raw_gross_total`, `raw_paid_gross`, `raw_payment_date` and `raw_payment_method` retain the **decoded, untrimmed** strings, including empty and unreadable text. `None` means the parameter was omitted; `Some("")` means it was present but empty. They are not the original body bytes: percent escapes and `+` have been decoded. Repeated parameters use the last value for both the typed reading and raw text; unknown parameters are ignored. `from_pairs` applies the same content policy, but its caller must validate form encoding before decoding it.

`is_fully_paid()` returns `Option<bool>`: `None` when either amount is unknown. `Some(true)` is only a comparison of unverified reported amounts, **not evidence of payment**.

## Feature Flags

No features are enabled by default. The [crate documentation](https://docs.rs/szamlazz-ipn/latest/szamlazz_ipn/#features) is authoritative for feature semantics and platform constraints.

- **`axum`** implements the axum request extractor for `PaymentNotification`.
- **`serde`** implements Serde serialization and deserialization for `PaymentNotification`.

### JSON Monetary Fields (`serde`)

`gross_total` and `paid_gross` serialize as decimal strings or `null`. JSON input accepts numbers or strings containing JSON-number syntax, including exponent notation. Both the significand and the resulting amount must fit `Decimal` exactly; excess precision and range are rejected without rounding. Whitespace inside strings, comma decimals, grouping, empty strings, booleans and objects are rejected. Omitted fields and `null` mean unknown. This is the typed JSON representation, distinct from the lenient IPN form parser above.

The `serde` feature enables serde_json's `arbitrary_precision` and `raw_value` modes itself, so another crate enabling them cannot change IPN numeric input behavior. The small monetary adapter targets JSON deserializers; arbitrary other Serde formats are not promised. Deserialize untrusted JSON directly with `serde_json::from_str` or `from_slice`: converting it through `Value` or other buffering first can interpret serde_json private adapter-shaped objects before the monetary boundary can reject them. Ordinary JSON values and the crate's serialized string/null representation round-trip exactly; digits already lost to a float upstream cannot be recovered.

## Delivery Semantics

Each payload is an absolute payment-status snapshot, not a new credit entry or a delta. **Never add `paid_gross` to a previously stored amount.** Deliveries can repeat, retries can interleave, and changes can be coalesced. There is no sequence number or ordering timestamp; the optional payment date does not order notifications. Treat each IPN as **“query now”**, not an ordered event. Do not let a late delivery overwrite verified status with an older reported amount. Coordinate query results in your application so concurrent refreshes do not overwrite newer observations with older ones.

Scope queries and stored status by `(configured account or endpoint context, document_number)`. Document numbers alone are not unique across szamlazz.hu accounts, and IPN carries no account identifier. Select the Számla Agent credentials from the receiver's configuration, not an assertion in the body.

IPN covers invoices and proformas but has no reliable document-kind discriminator. The optional payment date is sent only after szamlazz.hu customer service enables it for the account.

## Trust Model

IPN is **unauthenticated by design**. Treat the body as a hint to query the document: **confirm `paid_gross` and the document's payment status through the Számla Agent on the configured account before financial actions**, such as fulfillment, refunds or issuing another document. Successful parsing and `is_fully_paid() == Some(true)` do not authenticate a sender or establish freshness.

Register a URL containing an unguessable path segment. That URL and the optional `SOURCE_IPS` allowlist are defense in depth, **not authentication**. The list is dated **2025-08-01**; check szamlazz.hu's current published addresses before deploying or updating it. Addresses can change, and an obsolete allowlist can lose legitimate notifications when retries run out.

Behind a reverse proxy, the connection's peer address is the **proxy's** address. Prefer enforcing the allowlist at the internet-facing edge. If using `Forwarded` or `X-Forwarded-For`, accept them only from explicitly trusted proxies that strip and replace client-supplied forwarding headers, and configure the trusted proxy chain correctly. Never allow arbitrary client headers to choose the address being checked. The crate does not enforce an allowlist or configure proxy trust.

## Migration to 0.4

The upcoming minor release is breaking for this pre-1.0 crate. See [CHANGELOG.md](CHANGELOG.md) for the field and error changes. Handle optional amounts and methods explicitly; do not default an unknown amount to zero or treat unknown full-payment status as a reported `false`. `PaymentNotification::new` still accepts known amounts and a method for fixtures; its raw fields are `None` because it has no wire text.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
