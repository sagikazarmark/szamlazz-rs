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

Use the [operation recovery table](https://docs.rs/szamlazz-agent/latest/szamlazz_agent/error/index.html#recovery) for invoice creation/storno, receipt creation/storno, reads, credit entries, proforma deletion and receipt email. `outcome_class()` describes this exchange, not an earlier lost send of the logical operation. Invoice creation has no idempotency key; an immediate empty query cannot prove a create failed while it may still be in flight.

This example sends once and examines one reconciliation result. It checks identity before adopting a found invoice and leaves an empty or inconclusive query unresolved. The caller must serialize logical issuance and retain the request's order and external id.

```rust
use szamlazz_agent::ops::invoice::{CreateInvoice, CreationOutcome};
use szamlazz_agent::ops::query_xml::QueryInvoiceXml;
use szamlazz_agent::{Client, ClientError, DocumentType, InvoiceNumber, InvoiceSelector, OutcomeClass, Pdf};

/// What one create settled to.
enum Outcome {
    /// Issued, now, or by an earlier attempt whose reply was lost.
    Issued(InvoiceNumber),
    /// A preview was rendered (`header.preview_pdf`); no document exists.
    Preview(Pdf),
    /// This exchange was refused; an earlier lost send remains a separate question.
    Refused(ClientError),
    /// Recovery is unresolved, including an immediate empty query or wrong identity.
    Unknown(ClientError),
}

async fn issue_once(client: &Client, request: &CreateInvoice, expected_type: DocumentType) -> Outcome {
    let error = match client.send(request).await {
        Ok(CreationOutcome::Issued(created)) => return Outcome::Issued(created.invoice_number),
        Ok(CreationOutcome::Preview(preview)) => return Outcome::Preview(preview.pdf),
        // An arm a later release adds is not an issued document either.
        Ok(_) => return Outcome::Unknown(ClientError::from(
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
        ClientError::HttpStatus { status, .. } => eprintln!("HTTP {status} before body interpretation"),
        ClientError::Parse(cause) => eprintln!("unreadable response: {cause}"),
        _ => eprintln!("{error}"),
    }

    match error.outcome_class() {
        // Refused now; this says nothing about an earlier send.
        OutcomeClass::Rejected | OutcomeClass::NotFound | OutcomeClass::DuplicateOrderNumber => {
            Outcome::Refused(error)
        }
        // `Unknown` (and any class a later version adds): a document may exist.
        // Ask szamlazz.hu what carries the external id before anything is sent again.
        _ => {
            let (Some(external_id), Some(order)) = (&request.external_id, &request.header.order_number) else {
                return Outcome::Unknown(error); // missing reconciliation identity
            };
            let query = QueryInvoiceXml::new(InvoiceSelector::ExternalId(external_id.clone()));
            match client.send(&query).await {
                Ok(document)
                    if document.info.order_number.as_ref() == Some(order)
                        && document.info.document_type == expected_type
                        && document.info.reversed != Some(true) => Outcome::Issued(document.info.invoice_number),
                // Wrong identity, reversed, absent (7), or unanswered: no automatic resend.
                _ => Outcome::Unknown(error),
            }
        }
    }
}
```

`Outcome::Unknown` requires reconciliation and time for earlier sends to finish before another create is considered. An external id returns its newest holder and is not unique server-side; a collision must be resolved rather than adopted. `ClientError::Api` carries the typed `ErrorCode` with the verbatim message. For 71/152, `InvoiceSelector::OrderNumber` can find the existing document, which also needs identity checks.

The vendor allows at most **five total sends of the same request, including the initial send**, then operator intervention; never a tight retry loop. This wording does not specify exact combined accounting for a write and its reconciliation queries. Code 55 means signing failed, not proven issuance: timestamp access may recover, certificate expiry requires remediation. It remains `Unknown`. Code 56 is known issuance only **with a number**, corroborated by first-party PHP 2.12.4, not observed in the account probes. Without a number it remains `Unknown`.

The detailed A4d stalled-send observation found no issuance; a broader project assertion of delayed issuance lacks a linked probe. [Recovery evidence](https://docs.rs/szamlazz-agent/latest/szamlazz_agent/error/index.html#retry-limit-and-evidence) qualifies both. A timeout does not cancel server work, and the default remains 60 seconds.

## Receipts

**NAV reporting setup (2026-09-11):** the [current vendor guidance](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg)
requires a NAV connection and the technical-user permission **“Hozzáférés a
nyugtaadat-szolgáltatási interfészhez”** for automatic receipt reporting; see
[connection guide, step 13](https://www.szamlazz.hu/nav-online-szamlazas-regisztracios-segedlet/#lepesek).
The vendor describes reporting from September 10, retrospectively for receipts
issued after September 1; the Agent settings page still carries older rollout
wording. Follow the current setup instructions. Successful receipt issuance is
not confirmation of NAV reporting: the receipt response exposes no reporting status.

Persist a unique creation call id **before the first send** and keep it for the logical issuance. Error 338 prevents another receipt but does not return the original number or PDF. Query a known number or a deliberately managed order, then verify call id, order, number, document type and reversal data. Unresolved recovery does not justify a fresh call id.

```rust
use rust_decimal::dec;
use szamlazz_agent::ops::receipt::{CreateReceipt, QueryReceipt, ReceiptSelector};
use szamlazz_agent::{Client, Currency, LineItem, PaymentMethod, ReceiptType, VatRate};

// `persisted_call_id` and `order` come from durable caller storage, not a new UUID per retry.
fn creation(persisted_call_id: &str, order: &str) -> CreateReceipt {
    CreateReceipt {
        call_id: Some(persisted_call_id.to_owned()),
        order_number: Some(order.to_owned()),
        ..CreateReceipt::new("NYGTA", PaymentMethod::Cash, Currency::HUF, vec![
            // Documented HUF receipt example: fractional net/VAT, whole gross.
            LineItem::new("Item", dec!(1), "db", dec!(787.40), VatRate::percent(27),
                dec!(787.40), dec!(212.60), dec!(1000)),
        ])
    }
}

async fn inspect_order(client: &Client, persisted_call_id: &str, order: &str)
    -> Result<bool, szamlazz_agent::ClientError>
{
    // If the receipt number is known, prefer ReceiptSelector::ReceiptNumber(number).
    // Order lookup returns the last match; this caller manages the order's uniqueness.
    let query = QueryReceipt::new(ReceiptSelector::OrderNumber(order.to_owned()));
    // query.call_id stays None: it is not a supported call-ID-only lookup.
    let receipt = client.send(&query).await?;
    println!("inspect returned number: {}", receipt.receipt_number);
    Ok(receipt.call_id.as_deref() == Some(persisted_call_id)
        && receipt.order_number.as_deref() == Some(order)
        && receipt.document_type == ReceiptType::Receipt
        && !receipt.reversed
        && receipt.reversed_receipt_number.is_none())
}
```

`false` or an error leaves recovery unresolved. A normal number/order query omits `QueryReceipt::call_id`: the XML field is optional and its query behavior unspecified. First-party PHP docs say order queries return the **last matching document**; the exact “last” criterion and `SN` selection remain unresolved. Receipts have their [own order-number repetition toggle](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number), independent of invoices: restriction refuses a previously used receipt order number, while allowing repetition permits multiple matches.

For receipt storno, keep the logical call identity and query the known original's reversal state; that alone does not recover the `SN` number/PDF. The [vendor documents refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response) for already reversed receipts and targets that are themselves storno receipts, rather than invoice-style successful replay. That refusal does not identify who reversed it. Storno-specific call-ID/338 behavior remains unestablished.

### Receipt email

For a first send supply all details and one recipient:

```rust
use szamlazz_agent::ops::receipt::{ReceiptEmail, SendReceipt};

let first_send = SendReceipt {
    email: Some(ReceiptEmail {
        to: Some("buyer@example.com".into()),
        reply_to: Some("seller@example.com".into()),
        subject: Some("Your receipt".into()),
        body: Some("Thank you for your purchase.".into()),
    }),
    ..SendReceipt::new("NYGTA-2026-1")
};
let resend = SendReceipt::new("NYGTA-2026-1");
```

The resend writes a **present empty `emailKuldes` block**, requesting the previous email details. Within `ReceiptEmail`, `None` omits a child while `Some("")` emits an empty child; independent partial-field merging and comma-separated recipients are not established by the source. After a lost acknowledgement, receipt existence does not prove email delivery; another send may duplicate the email.

## Response parsing

Invoice and receipt dates retain the printed **civil date**, without UTC conversion.
Alongside the existing finite date-domain spellings, hyphenated dates accept XML
padding and `Z` or `±hh:mm` timezone suffixes (up to `±14:00`). Invalid calendar
dates, offsets and suffix junk are parse failures. Optional empty dates remain absent.
This is a civil-date reader, not strict XSD lexical validation or a new CE-year restriction.

Monetary **headers** accept ungrouped decimals with a dot or comma, an optional sign
and scientific exponent, with outer HTTP space/tab padding. `1,234` means `1.234`;
grouping intent cannot be inferred. Mixed/repeated separators, underscores and
embedded spaces are refused, as is outer whitespace other than HTTP space/tab
(including forms the former Decimal-based header reader accepted). Conversion uses
Decimal directly. XML amounts keep their separate grammar and take precedence:
a malformed nonblank XML amount fails rather than falling back to a header.
A missing header is absent; a present blank header is malformed. Numbered code-56
replies retain readable metadata and drop malformed optional metadata. A unique
body-only invoice number survives a structural failure in optional totals/PDF;
malformed or duplicate body identity is refused even when a number header is
present; a genuinely absent body number may still fall back to the header.

Receipt reversal state requires `true`, `false`, `1` or `0`: an empty
`stornozott` is refused rather than interpreted as an unreversed receipt. Missing,
empty or whitespace-only receipt PDFs are `None`, even when requested. Keep the
returned receipt number and query it to recover the artifact; do not create again.

Optional business text in queried invoices, receipts and taxpayer records preserves
decoded characters, including padding and non-breaking spaces. Absent, empty or
XML-space/tab/CR/LF-only text is `None`; NBSP-only text is `Some`. This affects
identifiers, comments, bank/ledger values and optional open string tokens. XML
entities and line endings are decoded, so this is character fidelity rather than
raw-byte preservation. Issuance-envelope numbers, numeric/verdict parsing, URLs
and base64 retain their own policies; the Restate worker still normalizes order
numbers at its projection boundary.

Structured replies must contain one completed expected XML root with matching
closes and a legal prolog/epilog through EOF. Truncation, extra roots, outside
text/CDATA/references and malformed tails are refused. A zero-allocation XML
tokenizer complements the structural reader: illegal XML characters, malformed
names/attributes, forbidden character-data delimiters and undefined references
are refused even in ignored extensions. This is XML checking, not XSD business
validation; sparse content and well-formed unknown extensions remain supported.
Namespace declarations are normalized before checking reserved bindings and
attribute expanded-name uniqueness (Namespaces in XML 1.0). Structured replies
reject the reserved `xmlns` element prefix and colons in processing-instruction
targets. Repeated invoice and receipt rows may use different prefixes for the
same namespace or have ignored
extensions between them; row order is retained. Duplicate singleton fields and
children inside scalar values remain refused.
Taxpayer extraction follows
NAV 2.0/3.0 expanded names and recognized parent paths: foreign or unknown subtrees
cannot supply a verdict or business data. Duplicate recognized singleton fields
or containers, children inside scalar values and undefined entities are refused.
Sparse records, unknown tokens and `taxpayerValidity=false` remain data; an `OK`
verdict still requires validity. Header/down/status precedence remains in effect.
Numbered-header-56 fallback permits empty or plain-text notification bodies only;
malformed XML must not conceal a refusal and become issued success.
An unusable optional `hibauzenet` (nested content or duplicate elements) is read
as an absent diagnostic, independently of the unique scalar verdict and code.
It cannot erase a readable refusal or numbered-56 evidence; malformed verdicts,
codes and ambiguous body identity remain subject to the existing checks.

## Bring Your Own HTTP Client

Without `client-reqwest`, `AgentRequest::to_wire` builds the request body and `RawResponse` takes whatever your HTTP client returns. The transport is yours: `POST` to `wire::ENDPOINT` with the given `Content-Type`, and hand every response to `parse`; szamlazz.hu signals errors in-band. Here with the blocking [`ureq`](https://crates.io/crates/ureq):

```rust
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, ENDPOINT, RawResponse};
use szamlazz_agent::Credentials;

fn look_up_taxpayer() -> Result<(), Box<dyn std::error::Error>> {
    let request = QueryTaxpayer::new("12345678")?;
    let wire = request.to_wire(&Credentials::agent_key("your-agent-key"))?;

    let http = ureq::Agent::config_builder().http_status_as_error(false).build().new_agent();
    let mut response = http.post(ENDPOINT)
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

`RawResponse::session_cookie()` extracts the first exact, case-sensitive `JSESSIONID` pair from repeated `Set-Cookie` headers, skipping malformed/nonmatching entries. It accepts empty values and preserves later `=` signs. HTTP header names are case-insensitive; cookie names are not. This helper discards attributes: the transport jar owns lifetime, path, domain and expiry handling. The native reqwest client uses its jar directly.

The HTTP status is optional but worth passing. Before reading the body, every parser checks, in order: a nonblank `szlahu_down` (`ServiceUnavailable`), a nonblank `szlahu_error_code` (the operation judges it; invoice issuance/storno can tolerate **numbered** 56), then a known non-2xx status (`ResponseError::HttpStatus`, `ClientError::HttpStatus` through the client). Only after that does the body decide: a body-only `<hibakod>` is an API error at 200 and `HttpStatus` at 500. A success-number or unrelated `szlahu_*` header does not bypass status. The status does not identify whether szamlazz.hu or an intermediary answered. Without it, the body is left to the operation's parser after the header checks. HTTP-status and parse failures both have `OutcomeClass::Unknown`. The example disables ureq's status-as-error behavior to preserve this interpretation.

`RawResponse::szlahu()` is a utility for **encoded textual** headers (`szlahu_szamlaszam`, `szlahu_error`, `szlahu_down`, `szlahu_vevoifiokurl`): it decodes once, turning `+` into space and `%2B` into `+`. Numeric headers (totals, `szlahu_id`) and error codes use raw `header()` instead. URLs in XML receive XML entity decoding only: literal `+` and `%2B` remain intact, and are never decoded again as headers.

### Sessions and refresh

On native targets, reuse a client within one account. Independently authenticated accounts need **distinct cookie jars**. `Client::clone()` and `reqwest::Client::clone()` share the underlying jar; cloning does not refresh a session. Building a new default `Client` creates a fresh in-memory jar. With `ClientBuilder::http_client`, use `.cookie_store(true)` or `.cookie_provider(Arc<Jar>)`; a fresh HTTP client over an existing provider still shares that provider's sessions.

[Vendor guidance](https://docs.szamlazz.hu/agent/basics/session-cookie) advises a fresh session after company-data or email edits. Build a new default client, or inject a fresh client **and fresh jar**. Do the same on credential/account changes as caller ownership policy, not as a proven vendor invalidation mechanism: vendor key-versus-cookie precedence has not been established. Sessions expire after **90 minutes of inactivity**. Without cookie persistence, requests reauthenticate; disk storage and an automatic refresh timer are not required.

## Feature Flags

No features are enabled by default. The [crate documentation](https://docs.rs/szamlazz-agent/latest/szamlazz_agent/#features) is authoritative for feature semantics and platform constraints.

- **`client-reqwest`** provides the ready-made async `Client` with native and browser wasm transport compilation, and re-exports `reqwest`, so a caller supplying its own HTTP client (`ClientBuilder::http_client`: a proxy, a custom TLS setup) names the one version this crate is built against.

Browser transport compilation does not prove direct Számla Agent access. Vendor CORS must permit the request and expose the `szlahu_*` response headers; browsers manage cookies and hide `Set-Cookie` from application code. Reqwest Fetch defaults to **same-origin credentials** unless set per request. `Client::send` keeps that default; injecting another client does not enable cross-origin credential inclusion. XML authentication may work without cookies. Direct browser feasibility remains an explicit vendor/platform question; native loopback checks establish neither CORS nor browser cookie access.

The [vendor authentication guidance](https://docs.szamlazz.hu/agent/basics/authentication) explicitly says not to include agent keys in client-side code. Keep account keys on a trusted server, including server-side wasm; browser applications call that server rather than receiving the key.

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
  - `Rounding::minor_unit(&currency)`: the crate's minor-unit policy, whole forints for HUF (`Currency::minor_unit_digits` returns 0 although ISO 4217 says 2), cents for EUR, thousandths for KWD, 2 for a code the table does not know. This is a local arithmetic choice for ledger reconciliation, not a promise of server precision or acceptance for every currency.
  - `Rounding::Scale(n)`: a fixed number of decimal places.
  - `Rounding::Exact`: no rounding. On the test account, a EUR **invoice** sent with `100.004 / 27.00108 / 127.00508` was stored as `100 / 27 / 127.01`: independent two-decimal rounding without recomputing gross. This is invoice evidence, not receipt evidence or a rule for KWD.

  Rounding is half away from zero and applied at each step (net before VAT), so gross = net + VAT holds exactly on the wire. Intermediates that cannot fit exactly are refused, including precision loss and underflow, even if later rounding would make them fit. This calculator invariant does not establish server acceptance or storage precision. HUF **invoice** probes tolerated net discrepancies of 0.5, 1 and 2 HUF and rejected 5 and 10; those observations are not receipt probes.
- **`LineItem::new(…)`** takes net, VAT and gross as your system computed them and sends them as-is.

`LineItem` is plain data like every request type: set the optional fields with functional update (`LineItem { comment: Some(..), ..item }`). A receipt row carries fewer fields than an invoice row; a `CreateReceipt` whose item sets `margin_vat_base` or the ledger's economic-event or settlement fields is refused before the wire (`RequestError::UnsupportedOnReceipt`) rather than sent without them.

For HUF/Ft **receipts**, [documented item rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) require whole gross, net/VAT with at most two decimals, and exact net + VAT = gross. The `787.40 / 212.60 / 1000` example above is valid under those rules. `Scale(2)` alone does not ensure whole gross; minor-unit HUF rounding produces whole net and VAT as a stricter local choice. `LineItem::new` and `Exact` remain available; the caller supplies amounts appropriate to the document. These receipt rules were not live-probed here.

Foreign receipts retain `ExchangeRate::automatic_mnb()` (bank `MNB`, omitted numeric rate). General XML pages ask for bank and rate; the receipt-specific `ReceiptHeader` and custom-data receipt example comments in official PHP **2.12.4** document automatic MNB lookup. The example supplies an explicit rate, so this is documentation evidence, not an omitted-rate execution; local tests prove emission only.

`VatRate::Percent` renders its wire token normalised: `27.00`, `27.0` and `27` all go out as `27`, `5.50` as `5.5`. szamlazz.hu accepts `27.00` and `27.0` as well (test account), so this is hygiene: the integer form is the one every fixture shows, and a queried rate comes back as a double (`27.0`) that round-trips to `27` this way.

## Protocol Notes

- Identifiers are English; Rustdoc search also finds types by Hungarian names such as `díjbekérő` and `kintlévőség` through doc aliases.
- Errors are typed as `ErrorCode` values while preserving the verbatim Hungarian message. A failure szamlazz.hu reports without any code (`sikeres=false` and no `hibakod`) is `ErrorCode::Absent`, never an invented `0`.
- `ErrorCode::is_retryable()` is a potentially transient hint (1 and 55), never permission to repeat a write. `outcome_class()` (also on `ResponseError` and `ClientError`) answers `Rejected` (this exchange refused), `Unknown` (1, 55, unnumbered 56, unanswered exchanges and open codes), `DuplicateOrderNumber` (71/152) or `NotFound` (7, operation-dependent missing data; 339, receipt not found). Use the operation recovery table above.
- Agent code 56 **with a number** means issuance succeeded but notification failed. It sets `notification_delivery_failed = true`; do not retry that issued document. Without a number it remains unknown.
- An invoice, a prepayment invoice and a final invoice can each name the proforma they consume (the `proforma_number` field of `InvoiceKind::Invoice`, `InvoiceKind::Prepayment` and `InvoiceKind::Final`, written as `dijbekeroSzamlaszam`; `InvoiceKind::proforma_number()` reads it on any kind). szamlazz.hu also consumes a proforma that shares the document's order number when the reference is absent (verified for an invoice and a prepayment invoice); the reference makes the link explicit rather than leaving it to the order number. A reference to a deleted or already consumed proforma is not refused (it is silently ignored), so read the issued document's `hivdijbekszam` to see which link landed.
- **A final invoice (`végszámla`) is not netted by szamlazz.hu.** The server links the prepayment invoice (by `elolegSzamlaszam` or by the shared order number), but issues the final invoice for exactly the lines it is sent: a final invoice listing only the full performance bills the buyer the prepayment twice. List the full performance and deduct the prepayment as a **negative line item at the same VAT rate**; the crate does not add that line. Verified on the test account.
- Response version 2 carries requested PDFs as base64 inside XML. The crate decodes them and exposes raw bytes through `Pdf`.
- A body download that fails after headers arrive returns `ClientError::IncompleteResponse` with `client::IncompleteResponse { status, headers, source }`. Raw headers preserve repeated values and may contain session cookies; its `Debug` lists header names only. This error remains `OutcomeClass::Unknown`, even with number/error headers: the unread body could contradict them. Use the evidence to reconcile, never turn it into a completed response with an invented empty body.
- Invoice creation has no idempotency key. Receipt call IDs prevent duplicate issuance by returning error 338 when reused, but do not replay the original success. The client has no application-level retry/recovery loop. Supplied HTTP clients retain their retry policies; one `send` need not mean one POST, and transport retries do not perform reconciliation.
- A replacing credit-entry request (`RegisterCreditEntry` with `additive: false`, the default) with no entries is refused before the wire (`RequestError::EmptyCreditEntryReplace`): the schema allows it and it would clear the invoice's payments. Clearing is not offered as an operation until the server's behaviour on it is verified.
- `HttpStatus` and `UnexpectedBody` diagnostics quote bounded body excerpts (`error::BODY_EXCERPT_LEN`, with the total length noted). Other API/parser messages may contain full upstream text; the verbatim `ApiError.message` is not truncated. `RawResponse`'s `Debug` redacts `Set-Cookie` and prints the body length, but other headers remain visible. Apply your application's logging policy to these messages and headers.
- A queried document's `test` flag (`teszt`) is an `Option<bool>`: the schema has the element mandatory, so a document without one reports `None` rather than an invented "live".
- The vocabulary follows the domain: a `kifizetes` registered against an invoice is a *credit entry* (`CreditEntry` out, `RecordedCreditEntry` back, `InvoiceDocument::credit_entries`; its `jogcim` is the `title`, a `PaymentMethod` on both sides), a `stornozott` receipt is *reversed*, and a queried document's `eszamla` is its `appearance` (a code), while the `e_invoice` of a create or storno request is a flag.
- Every integer of a queried document (`alap/id`, `gazdEsemAzon`, `forras`, the parties' `id` and `lokacio`, `sztetordering`, `afalevon`, `banktranzid`, the `eszamla` code) is an `i64`, and so is the `szlahu_id` header of a create reply: one width, whatever the schema declares, shared with `szamlazz-adatkapcsolat`, which models the same `<szamla>` (ADR 0010). `InvoiceAppearance` serialises as its integer code.

## Breaking Changes in 0.4

Interrupted body downloads now return `ClientError::IncompleteResponse` instead
of `Transport`, retaining received evidence and the reqwest source. Receipt
reversal booleans refuse empty values. Structured XML rejects the lexical defects
listed above, including malformed content inside previously ignored extensions.

Derived line-item arithmetic now refuses lossy intermediates instead of silently
rounding or underflowing before the chosen rounding policy. Arithmetic-error
messages describe exact representability, not just overflow. Invalid-endpoint
errors carry a URL with userinfo removed, or `[invalid URL]` when parsing or userinfo removal fails,
rather than echoing the original input.

Response numbers now require exact finite Decimal representation: excess precision,
underflow and overflow are refused rather than implicitly rounded. Equivalent
plain/exponent spellings are handled consistently; XML numeric underscores are
no longer accepted. Numeric VAT tokens (including XML-whitespace padding) are
interpreted as percentages without changing the raw response fields. Derived
items reject unrepresentable numeric VAT tokens with
`ArithmeticError::UnrepresentableVatRate`. Response elements from foreign namespaces
no longer supply protocol fields; undeclared prefixes are refused.

One release, so a consumer pays the migration once. The naming and shape changes of the 2026-09-09 review are listed in [PR #191](https://github.com/sagikazarmark/szamlazz-rs/pull/191) (the verdict envelope, `CreationOutcome`, `try_calculated`, credit entry / reversed / `title` / `appearance`, the typed `DocumentType`, the Rust-convention batch). On top of them, from the integer-width policy (ADR 0010):

- `InvoiceInfo::id`, `Supplier::id`, `BuyerInfo::id`, `InvoiceInfo::economic_event_id`, `InvoiceInfo::source`, `DocumentItem::ordering`, `FinancialItem::deductible_vat`, `RecordedCreditEntry::bank_transaction_id`, `CreatedInvoice::document_id` and `Receipt`'s `id` are `i64` (they were `u64`, `u32` or `i32`).
- `InvoiceAppearance` carries and returns an `i64` (`Electronic(i64)`, `Unknown(i64)`, `code() -> i64`, `From<i64>`) and serialises as the integer code (`1`), not the string `"1"`; the CLI's `--json` output of a queried invoice changes with it. A JSON string is no longer read back.
- #195 intentionally changes thirteen documented codes from `Unknown`: 339 becomes named `ReceiptNotFound` / `NotFound`; 336, 337, 340, 363–365 and 551–556 become named `Rejected` variants. All are non-retryable and not credential errors. Future numeric, NAV text and absent codes remain unknown. These are source-derived classifications, not newly observed account behavior.
- #199 adds `InvoiceHeader::simple_items: Option<bool>` in this breaking 0.x release: exhaustive literals must add it (or use functional update over `InvoiceHeader::new`). Missing/null JSON and the constructor default to `None`, omitting XML; explicit false/true is emitted. This selects the per-document tour-operator invoice image while retaining full monetary item data. See the field's rustdoc for inheritance and server content rules. The deterministic header tail is `szamlaSablon` → `elonezetpdf` → `simpleItems`, following official PHP 2.12.4 and the downloadable XSD. Current EN/HU inline XSDs reverse the last two; acceptance of the combined-preview order and preservation of non-issuing preview remain unverified live.
- Response JSON grows with optional `InvoicePdf::{outstanding, customer_account_url}`, `CreatedInvoice::payment_method` (create/storno header), and `TaxpayerInfo::{short_name, county_code, vat_group_membership, incorporation, info_date}`. Each new field defaults to `None` when reading old JSON. `Incorporation` is an open wire-string enum in `ops::taxpayer`. The root-child `infoDate` is retained as nonblank decoded **source text**, not a validated datetime, lookup time or cache expiry; no timezone or precision is invented. The dated official NAV 2.0 example supplies it; synthetic NAV 3.0 tests cover all five fields, whose current forwarding by szamlazz.hu has not been captured live. Worker contracts/journals and CLI-specific projections require independent exposure decisions.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
