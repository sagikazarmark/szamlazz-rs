# szamlazz-agent

[![crates.io](https://img.shields.io/crates/v/szamlazz-agent?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-agent)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-agent?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-agent)

**Rust client for the [szamlazz.hu Számla Agent](https://docs.szamlazz.hu/agent/basics/what-is).**

The core performs no I/O: request types serialize into a ready-to-send `WireRequest`, and typed responses parse from raw headers and body bytes. Any HTTP client can drive it on native Rust or `wasm32-unknown-unknown`, including Cloudflare Workers.

## Quick Start

Enable `client-reqwest` to use the ready-made async client. The happy path of an integration: issue an invoice under an order number and an external id, find it again by that id, and fetch its PDF.

```rust
use szamlazz_agent::ops::invoice::{Buyer, CreateInvoice, CreationOutcome, InvoiceHeader, InvoiceKind};
use szamlazz_agent::ops::query_pdf::QueryInvoicePdf;
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::{
    Client, Credentials, Currency, Date, InvoiceSelector, Language, LineItem, PaymentMethod,
    Rounding, VatRate,
};

async fn issue_invoice() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new(Credentials::agent_key("your-agent-key"))?;
    let item = LineItem::try_calculated(
        "Development",
        1.into(),
        "hour",
        10_000.into(),
        VatRate::percent(27),
        Rounding::minor_unit(&Currency::HUF),
    )?;
    // Constructors take the required fields; set the rest with functional update.
    // The order number and the external id are how the document is found later:
    // szamlazz.hu answers a query by either, so keep both with the order.
    let header = InvoiceHeader {
        order_number: Some("ORD-1".to_owned()),
        ..InvoiceHeader::new(
            "2026-07-04".parse::<Date>()?,
            "2026-07-12".parse::<Date>()?,
            PaymentMethod::Transfer,
            Currency::HUF,
            Language::Hungarian,
        )
    };
    let request = CreateInvoice {
        external_id: Some("shop:ORD-1:invoice".to_owned()),
        ..CreateInvoice::new(
            InvoiceKind::invoice(),
            header,
            Buyer::new("Example Kft.", "1111", "Budapest", "Example utca 1."),
            vec![item],
        )
    };

    // The outcome is the issued document, or the preview PDF when the header
    // asked for one (`preview_pdf`) and nothing was issued.
    let CreationOutcome::Issued(created) = client.send(&request).await? else {
        unreachable!("no preview was requested");
    };
    println!("issued: {}", created.invoice_number);

    // Query by the external id: the newest document carrying it, in full.
    let query = QueryInvoiceXml::new(InvoiceSelector::ExternalId("shop:ORD-1:invoice".to_owned()));
    let document = client.send(&query).await?;
    assert_eq!(document.info.order_number.as_deref(), Some("ORD-1"));
    println!("gross total: {}", document.totals.total.gross);

    // Fetch the PDF, by invoice number here; the order number or the external id work too.
    let fetch = QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber(document.info.invoice_number));
    let fetched = client.send(&fetch).await?;
    fetched.pdf.save_to("ORD-1.pdf")?; // or `fetched.pdf.as_bytes()` for the raw bytes
    Ok(())
}
```

## When the Call Fails

Every failure is a `ClientError` variant, and each says something different about the document you asked for. Invoice creation has no idempotency key, so a failed create settles to one of three outcomes, and only one of them permits sending the same request again: `outcome_class()` says whether a document may already exist, and when it may, a query by the external id says whether one does.

```rust
use szamlazz_agent::ops::invoice::{CreateInvoice, CreationOutcome};
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::{Client, ClientError, InvoiceNumber, InvoiceSelector, OutcomeClass, Pdf};

/// What one create settled to.
enum Outcome {
    /// Issued, now, or by an earlier attempt whose reply was lost.
    Issued(InvoiceNumber),
    /// A preview was rendered (`header.preview_pdf`); no document exists.
    Preview(Pdf),
    /// Nothing was issued: szamlazz.hu refused, or confirmed that nothing
    /// carries the external id. The one outcome after which the same
    /// request may be sent again, once its cause is fixed.
    NotIssued(ClientError),
    /// A document may exist: neither the create nor the reconciling query
    /// answered. Never send again from here, query by the external id
    /// until szamlazz.hu answers.
    Unknown(ClientError),
}

async fn issue_once(client: &Client, request: &CreateInvoice) -> Outcome {
    let error = match client.send(request).await {
        Ok(CreationOutcome::Issued(created)) => return Outcome::Issued(created.invoice_number),
        Ok(CreationOutcome::Preview(preview)) => return Outcome::Preview(preview.pdf),
        // An arm a later release adds is not an issued document either.
        Ok(_) => return Outcome::NotIssued(ClientError::from(
            szamlazz_agent::ParseError::Missing("szamlaszam"),
        )),
        Err(error) => error,
    };

    match &error {
        // Refused by this crate before anything was sent: fix the request.
        ClientError::Request(refusal) => eprintln!("invalid request: {refusal}"),
        // szamlazz.hu answered with an error code and its Hungarian message.
        ClientError::Api(api) => eprintln!("szamlazz.hu error {}: {}", api.code, api.message),
        // No answer to conclude from: the request may have been acted on.
        ClientError::Transport(cause) => eprintln!("transport: {cause}"),
        ClientError::ServiceUnavailable(message) => eprintln!("szlahu_down: {message}"),
        ClientError::HttpStatus { status, .. } => eprintln!("the endpoint answered {status}, not szamlazz.hu"),
        ClientError::Parse(cause) => eprintln!("unreadable response: {cause}"),
        _ => eprintln!("{error}"),
    }

    match error.outcome_class() {
        // Nothing was issued: the request, the account or the order number is the problem.
        OutcomeClass::Rejected | OutcomeClass::NotFound | OutcomeClass::DuplicateOrderNumber => {
            Outcome::NotIssued(error)
        }
        // `Unknown` (and any class a later version adds): a document may exist.
        // Ask szamlazz.hu what carries the external id before anything is sent again.
        _ => {
            let Some(external_id) = &request.external_id else {
                return Outcome::Unknown(error); // nothing to reconcile by
            };
            let query = QueryInvoiceXml::new(InvoiceSelector::ExternalId(external_id.clone()));
            match client.send(&query).await {
                // An earlier attempt issued it; only the reply was lost.
                Ok(document) => Outcome::Issued(document.info.invoice_number),
                // Code 7: szamlazz.hu confirms nothing carries the id, the create did not land.
                Err(answer) if answer.outcome_class() == OutcomeClass::NotFound => {
                    Outcome::NotIssued(error)
                }
                // The query did not answer either: still open.
                Err(_) => Outcome::Unknown(error),
            }
        }
    }
}
```

`Outcome::Unknown` is answered by querying again, never by re-sending: the create may have landed, and a second one would be a second legal document. `ClientError::Api` carries the typed `ErrorCode` with the verbatim message; `OutcomeClass::DuplicateOrderNumber` (71/152) means another document already carries the order number; query by `InvoiceSelector::OrderNumber` to find it.

## Bring Your Own HTTP Client

Without `client-reqwest`, `AgentRequest::to_wire` builds the request body and `RawResponse` takes whatever your HTTP client returns. The transport is yours: `POST` to `wire::ENDPOINT` with the given `Content-Type`, and hand every response to `parse`; szamlazz.hu signals errors in-band. Here with the blocking [`ureq`](https://crates.io/crates/ureq):

```rust
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, ENDPOINT, RawResponse};
use szamlazz_agent::Credentials;

fn look_up_taxpayer() -> Result<(), Box<dyn std::error::Error>> {
    let request = QueryTaxpayer::new("12345678")?;
    let wire = request.to_wire(&Credentials::agent_key("your-agent-key"))?;

    let mut response = ureq::post(ENDPOINT)
        .content_type(&wire.content_type)
        .send(&wire.body[..])?;
    let status = response.status().as_u16();
    let body = response.body_mut().read_to_vec()?;
    let headers = response
        .headers()
        .iter()
        .map(|(name, value)| (name.as_str(), String::from_utf8_lossy(value.as_bytes())));

    let taxpayer = request.parse(&RawResponse::new(headers, body).with_status(status))?;
    println!("valid: {}", taxpayer.valid);
    Ok(())
}
```

To skip re-authentication on consecutive calls, replay `RawResponse::session_cookie()` as the `Cookie` header of the next request; the reqwest client does this through its cookie store.

The HTTP status is optional but worth passing: szamlazz.hu answers in-band (HTTP 200 with `szlahu_*` headers and a `<hibakod>` body), so the parsers read those first, and the status only decides the case where neither carries an answer; a non-2xx there is `ResponseError::HttpStatus` (`ClientError::HttpStatus` through the client), a proxy or CDN speaking instead of szamlazz.hu, rather than a puzzling `UnexpectedBody`. Without the status that case is still an `UnexpectedBody` parse error; both are `OutcomeClass::Unknown`.

## Feature Flags

No features are enabled by default. The [crate documentation](https://docs.rs/szamlazz-agent/latest/szamlazz_agent/#features) is authoritative for feature semantics and platform constraints.

- **`client-reqwest`** provides the ready-made async `Client` on native Rust and browser wasm, and re-exports `reqwest`, so a caller supplying its own HTTP client (`ClientBuilder::http_client`: a proxy, a custom TLS setup) names the one version this crate is built against.

## Operations

| Operation | Type |
|---|---|
| Invoice, proforma, prepayment invoice, final invoice, corrective invoice, or delivery note | `ops::invoice::CreateInvoice`, with the kind selected by `InvoiceKind`; answers a `CreationOutcome` (`Issued(CreatedInvoice)` or `Preview`) |
| Storno an invoice | `ops::storno::StornoInvoice`; answers the storno `CreatedInvoice` (check `reverses`) |
| Register credit entries | `ops::credit_entry::RegisterCreditEntry`; answers the `InvoiceBalance` |
| Query invoice PDF or full XML | `ops::query_pdf::QueryInvoicePdf`, `ops::query_xml::QueryInvoiceXml`, both by an `InvoiceSelector` |
| Delete a proforma | `ops::proforma::DeleteProforma` |
| Create, storno, query, or send receipts | `ops::receipt::*`; the first three answer a `Receipt` |
| Look up a taxpayer through NAV | `ops::taxpayer::QueryTaxpayer` |

The shared request vocabulary (`ExchangeRate`, `InvoiceTemplate`, `SellerEmail`, `InvoiceSelector`) and the document codes (`DocumentType` for a queried invoice's `tipus`, `ReceiptType` for a receipt's) live in `types` and are re-exported at the crate root. Every code set is open: a token the crate does not know is kept in an `Other(String)` variant, a numeric code in an `Unknown(n)`.

## Line Items

szamlazz.hu verifies every row's arithmetic server-side (net = unit price × quantity, VAT = net × rate / 100, gross = net + VAT; error codes 259–264), and the crate does not duplicate that check. It offers two ways to fill the values:

- **`LineItem::try_calculated(…, rounding)`** derives them and returns `ArithmeticError` instead of panicking when a value does not fit a `Decimal`. The rounding is an explicit choice:
  - `Rounding::minor_unit(&currency)`: the currency's minor unit, whole forints for HUF (`Currency::minor_unit_digits` returns 0 for HUF although ISO 4217 says 2: the fillér is out of circulation and szamlazz.hu works in whole forints), cents for EUR, thousandths for KWD, 2 for a code the table does not know. This is what the invoice can state and what NAV reporting takes, so it is the choice for a document that must reconcile to the caller's ledger.
  - `Rounding::Scale(n)`: a fixed number of decimal places.
  - `Rounding::Exact`: no rounding; a `100.005 EUR` net goes on the wire with a five-decimal VAT. szamlazz.hu then rounds each value to two decimals on its own and does not recompute the gross: `100.004 / 27.00108 / 127.00508` is stored as `100 / 27 / 127.01`, a document whose gross is not net + VAT (observed on the test account). Ask for this only when your business rule requires it and you accept that.

  Rounding is half away from zero and applied at each step (the net is rounded before the VAT is derived from it), so gross = net + VAT holds exactly on the wire, and what is sent is what szamlazz.hu stores. The rounded net can differ from unit price × quantity by up to half a minor unit (`2 × 1234.25 HUF = 2468.5 → 2469`); szamlazz.hu's `net = price × qty` check (259) tolerated discrepancies of 0.5, 1 and 2 HUF on the test account and rejected 5 and 10, so the half unit is safely inside: hand-computed values passed to `LineItem::new` are the ones that can hit it.
- **`LineItem::new(…)`** takes net, VAT and gross as your system computed them and sends them as-is.

`LineItem` is plain data like every request type: set the optional fields with functional update (`LineItem { comment: Some(..), ..item }`). A receipt row carries fewer fields than an invoice row; a `CreateReceipt` whose item sets `margin_vat_base` or the ledger's economic-event or settlement fields is refused before the wire (`RequestError::UnsupportedOnReceipt`) rather than sent without them.

`VatRate::Percent` renders its wire token normalised: `27.00`, `27.0` and `27` all go out as `27`, `5.50` as `5.5`. szamlazz.hu accepts `27.00` and `27.0` as well (test account), so this is hygiene: the integer form is the one every fixture shows, and a queried rate comes back as a double (`27.0`) that round-trips to `27` this way.

## Protocol Notes

- Identifiers are English; Rustdoc search also finds types by Hungarian names such as `díjbekérő` and `kintlévőség` through doc aliases.
- Errors are typed as `ErrorCode` values while preserving the verbatim Hungarian message. A failure szamlazz.hu reports without any code (`sikeres=false` and no `hibakod`) is `ErrorCode::Absent`, never an invented `0`.
- Two different questions are answered per error. `ErrorCode::is_retryable()` says whether the same *query* can succeed later (codes 1 and 55). `outcome_class()` (on `ErrorCode`, `ResponseError` and `ClientError`) says whether a *document may exist* despite the error: `Rejected` (nothing was created), `Unknown` (1, 55, 56, `szlahu_down`, a transport or parse failure, any code the crate does not know; query by external id before re-sending), `DuplicateOrderNumber` (71/152) or `NotFound` (7). Re-sending a create because `is_retryable()` is true can issue a duplicate legal document; act on `outcome_class()` instead.
- Agent code 56 means issuance succeeded but notification delivery failed. It sets `notification_delivery_failed = true`; do not retry that issued document.
- An invoice, a prepayment invoice and a final invoice can each name the proforma they consume (the `proforma_number` field of `InvoiceKind::Invoice`, `InvoiceKind::Prepayment` and `InvoiceKind::Final`, written as `dijbekeroSzamlaszam`; `InvoiceKind::proforma_number()` reads it on any kind). szamlazz.hu also consumes a proforma that shares the document's order number when the reference is absent (verified for an invoice and a prepayment invoice); the reference makes the link explicit rather than leaving it to the order number. A reference to a deleted or already consumed proforma is not refused (it is silently ignored), so read the issued document's `hivdijbekszam` to see which link landed.
- **A final invoice (`végszámla`) is not netted by szamlazz.hu.** The server links the prepayment invoice (by `elolegSzamlaszam` or by the shared order number), but issues the final invoice for exactly the lines it is sent: a final invoice listing only the full performance bills the buyer the prepayment twice. List the full performance and deduct the prepayment as a **negative line item at the same VAT rate**; the crate does not add that line. Verified on the test account.
- Response version 2 carries requested PDFs as base64 inside XML. The crate decodes them and exposes raw bytes through `Pdf`.
- Invoice creation has no idempotency key. Receipt call IDs prevent duplicate issuance by returning error 338 when reused, but do not replay the original success. The client never retries automatically.
- A replacing credit-entry request (`RegisterCreditEntry` with `additive: false`, the default) with no entries is refused before the wire (`RequestError::EmptyCreditEntryReplace`): the schema allows it and it would clear the invoice's payments. Clearing is not offered as an operation until the server's behaviour on it is verified.
- Error displays quote at most a bounded excerpt of an upstream body (`error::BODY_EXCERPT_LEN`, with the total length noted), and `RawResponse`'s `Debug` names its `Set-Cookie` header without the cookie value and prints the body as its length: a parse failure can be logged as is.
- A queried document's `test` flag (`teszt`) is an `Option<bool>`: the schema has the element mandatory, so a document without one reports `None` rather than an invented "live".
- The vocabulary follows the domain: a `kifizetes` registered against an invoice is a *credit entry* (`CreditEntry` out, `RecordedCreditEntry` back, `InvoiceDocument::credit_entries`; its `jogcim` is the `title`, a `PaymentMethod` on both sides), a `stornozott` receipt is *reversed*, and a queried document's `eszamla` is its `appearance` (a code), while the `e_invoice` of a create or storno request is a flag.
- Every integer of a queried document (`alap/id`, `gazdEsemAzon`, `forras`, the parties' `id` and `lokacio`, `sztetordering`, `afalevon`, `banktranzid`, the `eszamla` code) is an `i64`, and so is the `szlahu_id` header of a create reply: one width, whatever the schema declares, shared with `szamlazz-adatkapcsolat`, which models the same `<szamla>` (ADR 0010). `InvoiceAppearance` serialises as its integer code.

## Breaking Changes in 0.4

One release, so a consumer pays the migration once. The naming and shape changes of the 2026-09-09 review are listed in [PR #191](https://github.com/sagikazarmark/szamlazz-rs/pull/191) (the verdict envelope, `CreationOutcome`, `try_calculated`, credit entry / reversed / `title` / `appearance`, the typed `DocumentType`, the Rust-convention batch). On top of them, from the integer-width policy (ADR 0010):

- `InvoiceInfo::id`, `Supplier::id`, `BuyerInfo::id`, `InvoiceInfo::economic_event_id`, `InvoiceInfo::source`, `DocumentItem::ordering`, `FinancialItem::deductible_vat`, `RecordedCreditEntry::bank_transaction_id`, `CreatedInvoice::document_id` and `Receipt`'s `id` are `i64` (they were `u64`, `u32` or `i32`).
- `InvoiceAppearance` carries and returns an `i64` (`Electronic(i64)`, `Unknown(i64)`, `code() -> i64`, `From<i64>`) and serialises as the integer code (`1`), not the string `"1"`; the CLI's `--json` output of a queried invoice changes with it. A JSON string is no longer read back.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
