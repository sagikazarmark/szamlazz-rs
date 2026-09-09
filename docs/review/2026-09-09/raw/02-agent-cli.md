# Architecture review: `szamlazz-agent` + `szamlazz-cli` (research only, 2026-09-09)

Workspace at `8355ff5`. Mechanical baseline: `cargo clippy --all-targets --all-features` (workspace lints: `clippy::all` + `pedantic` + `unwrap_used`, `missing_docs`): **clean**; `cargo doc --no-deps --all-features`: **no warnings**; `cargo test -p szamlazz-agent --all-features`: 174 unit + 6 + 1 + 2 + 9 + 6 pass, 4 live ignored. Nothing mechanical to report.

**Ticket staleness check** (all verified against HEAD, none obsolete): #145 envelope still in `ops/invoice.rs:1110-1403`; #139/#129 `document_type: String` at `query_xml.rs:209`; #131 `From<quick_xml::DeError>` at `error.rs:574`, `ApiError` without `non_exhaustive` at `error.rs:461`, `rust_decimal/macros` still a normal dep (`Cargo.toml:34`), four `#[allow]` sites; #146/#126 unchanged; #150 CLI has zero `#[test]`; #76 rows all still present; #111/#112 unchanged. Several findings below *extend* #145 and #131 and are marked so.

---

## Axis 1: SEAMS

### S-1 · The `xmlszamlavalasz`-style verdict (`sikeres`/`hibakod`/`hibauzenet` → `ApiError`) is hand-rolled in five response structs; beyond #145's scope
**Strong.** Extends #145.
- `ops/invoice.rs:1315-1334` (`InvoiceResponse`), `:1337-1344` (`MinimalInvoiceResponse`), `ops/proforma.rs:95-102` (`DeleteResponse`), `ops/receipt.rs:690-701` (`ReceiptResponse`), `:726-733` (`ReceiptSendResponse`); the `into_success`/`api_error` conversion written at `invoice.rs:1357-1365, 1380-1392, 1394-1402`, `receipt.rs:680-686, 715-721, 747-753`, `proforma.rs:79-89`; the fabricated `ErrorCode::Unknown("0")` sentinel at six sites (`receipt.rs:682`, `invoice.rs:1362,1387,1399`, `taxpayer.rs:359`, `proforma.rs:85`).
- **What is wrong.** The per-op `parse` implementations share four lines of ritual (`check()?` → `response_text(root, ns)` → `quick_xml::de::from_str` → `if !sikeres { ApiError }`) and each declares its own copy of the verdict triple. `xml.rs` is the shared *writer* module and already hosts `de` helpers and the `totals` block, but not the envelope verdict, so the sharing layer stops one step short. #145 moves the *invoice* envelope out of `invoice.rs`; it does not unify the verdict across proforma/receipt/receipt-send.
- **Why.** Shallow implementation repeated across the per-op adapters; the deletion test on any one copy fails only because the others are copies. Locality is right (per op), depth is not.
- **Direction.** `xml::Valasz<T> { sikeres, hibakod, hibauzenet, #[serde(flatten)] payload: T }` plus one `pub(crate) fn parse_valasz<T>(response, root, ns) -> Result<T, ResponseError>`; the `"0"` sentinel becomes one honest representation (see R-3). Fold into #145 or file as its sibling.

### S-2 · `ops/invoice.rs` is the de-facto shared-types module; sibling ops import request-side types from it
**Worth exploring.** Extends #145 (which moves the envelope only).
- `ops/storno.rs:8` imports `CreatedInvoice, InvoiceTemplate, SellerEmail, parse_issued`; `ops/receipt.rs:11` imports `ExchangeRate`; `ops/query_xml.rs:13` imports `InvoiceSelector` from `ops/query_pdf.rs:20`.
- **What is wrong.** `types.rs:1` describes itself as "Domain value types shared across operations", yet `ExchangeRate`, `InvoiceTemplate`, `SellerEmail` and `InvoiceSelector`, each used by two or more operations, live inside one operation's module. After #145 lands, `storno` still depends on `invoice` for `CreatedInvoice`, `InvoiceTemplate`, `SellerEmail`.
- **Why.** Inverted dependency between sibling adapters; the `ops/` split is a clean per-endpoint mirror (appropriate: each endpoint has its own root/namespace) *except* where shared vocabulary leaks into the first module that needed it.
- **Direction.** Move the four types to `types.rs` (or `ops/shared.rs`); `CreatedInvoice` goes with the envelope in #145.

### S-3 · `InvoiceCreationResult` and `CreatedInvoice` are the same struct with `Option<InvoiceNumber>` vs `InvoiceNumber`; the preview-vs-issued distinction is an enum hidden as an `Option`
**Worth exploring** (breaking).
- `ops/invoice.rs:703-733` vs `:735-764`; `parse_issued` copies eight fields (`:1295-1310`); the worker re-derives the missing case as `Unnumbered` (`restate-szamlazz/src/gateway/document.rs:273-279`).
- **What is wrong.** Seven `Option` fields on `InvoiceCreationResult` are semantically dependent on `invoice_number.is_some()`; a downstream consumer had to write the `TryFrom` that the library's type should have made unnecessary. The `Result` suffix on a non-`Result` type (also `CreditEntryResult`, `ReceiptResult`) is a Rust naming smell (N-9).
- **Direction.** `CreateInvoice::Response = CreationOutcome { Issued(CreatedInvoice), Preview { pdf: Pdf, … } }`; `parse_issued` becomes a `match`. Removes the eight-field copy and the worker's `Unnumbered`.

### S-4 · One wire `osszegek` block → two structurally identical public trees under different names
**Strong** (small; breaking on the receipt side).
- Wire type shared at `xml.rs:236-284`; adapters at `query_xml.rs:1081-1110` (`Totals`/`VatTotal`/`GrandTotal`) and `receipt.rs:926-949` (`ReceiptTotals`/`VatRateTotal`/`TotalAmounts`). `xml.rs:232-235` defends "one wire shape, two public targets … so the public API stays per document".
- **What is wrong.** The two targets differ in *names only* (`by_vat_rate`/`by_rate`, `vat_rate_code`/`vat_code`, `GrandTotal`/`TotalAmounts`), not in shape. Deletion test: delete the receipt trio and point `Receipt.totals` at `Totals`; nothing is lost.
- **Why.** Two adapters over one wire type producing the same shape is a duplicated adapter, not a real seam; the doc comment rationalises the duplication rather than justifying it.
- **Direction.** One public `Totals` tree in `types.rs`; both `From<OsszegekXml>` collapse to one.

### S-5 · Root/namespace dispatch re-implemented for the two multi-root parsers
**Speculative / low.**
- `query_xml.rs:599-639` (`response_root`) re-implements `xml::response_text` (`xml.rs:46-85`) for two roots; `taxpayer.rs:207-219` calls `response_text` twice to accept two NAV namespaces.
- **Direction.** `response_text` takes `&[(root, ns)]` and returns the matched index; both call sites shrink.

### S-6 · `LineItem` is a union of the invoice row and the receipt row; the receipt writer silently drops invoice-only fields
**Worth exploring.**
- `item.rs:48-51` documents "Receipt creation supports only `revenue_account` and `vat_account`"; `receipt.rs:214-237` writes neither `margin_vat_base` nor `ledger.economic_event`/`vat_economic_event`/`settlement_*`; `CreateReceipt::validate` (`receipt.rs:158-183`) does not refuse them.
- **Why.** A caller-set field that never reaches the wire is the "trap not affordance" ADR 0008 named for `WireRequest.url`. The interface is the test surface: nothing on the interface can observe the drop.
- **Direction.** Either `CreateReceipt::validate` returns `RequestError::UnsupportedOnReceipt(field)`, or a `ReceiptLineItem` with the receipt's fields only.

### S-7 · `LineItem::calculated` / `calculated_for_currency` survive with no production caller, a documented panic on caller data, and a documented footgun default
**Strong.** Conflicts with nothing in an ADR; `README.md:205` ("kept for compatibility with the pre-0.4 non-HUF behaviour") is the only justification, in a 0.x crate.
- `item.rs:161-224`; callers are tests/fixtures only: `tests/client.rs:51`, `tests/literals.rs:29`, `ops/receipt.rs:970`, `ops/invoice.rs:1433`, `restate-szamlazz/src/service/journal.rs:323`. The worker's production path uses `try_calculated`.
- **What is wrong.** `calculated_for_currency` picks `Rounding::Exact` for every non-HUF currency, which `Rounding::Exact`'s own doc (`item.rs:20-27`) says produces a stored document whose gross ≠ net + VAT. Both forms `panic!` on Decimal overflow of user-supplied money (`:222-223`). CONTEXT's *Avoid* list already warns against calling it "currency-aware".
- **Why.** Deletion test passes (five test call sites become `try_calculated(..).expect(..)`). The infallible-panicking pattern is idiomatic only for programmer errors, not for arithmetic on input.
- **Direction.** Remove both in the next minor (or `#[deprecated(note = "use try_calculated with Rounding::minor_unit")]` for one release). Update CONTEXT's *Line item* entry ("the infallible `calculated*` forms panic, documented").

### S-8 · CLI: the PDF-delivery sequence is copied three times; `output.rs` exposes two parallel APIs; a boolean where an enum reads better
**Low.**
- Sequence `warn_missing_pdf → pdf_on_stdout → write_pdf → report(..)` at `commands/invoice.rs:142-148`, `:216-223`, `commands/receipt.rs:84-90`. `output.rs:57-69` free fns `field`/`field_required`/`json` are `report(false).x(..)` wrappers (deletion test passes). `report(pdf_on_stdout: bool)` (`output.rs:21`).
- **Direction.** `output::deliver_pdf(requested: Option<&Path>, pdf: Option<&Pdf>) -> Result<Report>`; drop the free fns; `Report::stdout()`/`Report::stderr()`.

### S-9 · CLI command tree mirrors the agent's operations, not the user's documents
**Worth exploring** (UX; breaking for scripts).
- `main.rs:37-55`: `invoice create` issues proformas and delivery notes (via JSON `kind`) while `proforma` offers only `delete` (`commands/proforma.rs:10-13`); PDF retrieval is `invoice download` (`commands/invoice.rs:22`) but `receipt get --pdf` (`commands/receipt.rs:55-57`); `taxpayer` is a leaf beside subcommand groups.
- **Why.** The CLI is a thin adapter (correct: it re-implements no protocol logic), but "thin" here means the *API's* shape leaks into the *user's* interface. A per-document tree (`proforma create|delete`, `invoice pdf`, `receipt pdf`) is the same thinness with better leverage.

### S-10 · CLI `--json` is the library's serde layout, including an inline base64 PDF when `--pdf` is also given; `EmailAttachment.content` is a JSON byte array
**Worth exploring.** Extends #76 B-20 (which names `listen` only).
- `commands/invoice.rs:98-99, 118-119`, `commands/receipt.rs:92-93` serialize the whole response; `Pdf` serializes as base64 (`types.rs:133-146`), so `invoice create --json --pdf out.pdf` writes the file *and* dumps its base64 to stdout. `EmailAttachment.content: Vec<u8>` (`ops/invoice.rs:496-504`) has no `serde(with)`, so a CLI user attaching a file writes `[37,80,68,…]` while `Pdf` gets base64.
- **Direction.** Strip `pdf` from the JSON when it was written to a target; base64 `with` on `EmailAttachment.content` (consistent with `Pdf`).

---

## Axis 2: NAMING vs DOMAIN

### N-1 · "payment" where the glossary says *credit entry*
**Strong** (library type); **Worth exploring** (CLI verb, user-facing).
- `ops/query_xml.rs:506-526` `RecordedPayment` (doc: "A payment recorded against the invoice"), `:67-68` `InvoiceDocument.payments`; CLI `main.rs:41-43` `Payment(...)`, `commands/payment.rs` (`payment register`).
- CONTEXT *Credit entry*: "_Avoid_: payment (overloaded; reserve for the buyer's act)". The worker had to rename the projection to `RecordedCreditEntry` (`gateway/document.rs:110`).
- **Direction.** `RecordedCreditEntry` / `credit_entries` in the agent crate; CLI `credit-entry register` (or keep `payment` and note the deliberate user-facing exception in CONTEXT).

### N-2 · `jogcim` is `method: PaymentMethod` on the way out and `title: String` on the way back
**Strong.**
- `ops/credit_entry.rs:20-22` vs `ops/query_xml.rs:513-515`. Same element on the two sides of one surface; two names and two types. CONTEXT's *Found document* entry uses `title`.
- **Direction.** Pick one (`title: PaymentMethod` or `method`) on both; the type should be the same on both sides.

### N-3 · "cancel"/"cancelled" for a receipt storno
**Strong.**
- `ops/receipt.rs:261` ("cancels an issued receipt"), `:268`, `:281`, `:475`, `:501-510` `cancelled`, `cancelled_receipt_number`; CLI `commands/receipt.rs:101-102` labels "Cancelled"/"Cancels".
- CONTEXT *Storno*: "_Avoid_: cancellation, void". The invoice side already says `reversed` (`query_xml.rs:286`).
- **Direction.** `reversed`, `reversed_receipt_number` (`stornozottNyugtaszam`); "storno receipt (`SN`)" in prose.

### N-4 · Same wire element, different English names between the invoice and receipt surfaces
**Worth exploring.**

| wire | invoice side | receipt/storno side |
|---|---|---|
| `tipus` | `InvoiceInfo.document_type` (`query_xml.rs:209`) | `Receipt.kind` (`receipt.rs:503`); and `CreateInvoice.kind` (`invoice.rs:615`) is the *request* word |
| `szamlaLetoltesPld` | `CreateInvoice.download_copies` (`invoice.rs:631`) | `StornoInvoice.copies` (`storno.rs:77`) |
| `afakulcs` | `vat_rate_code` (`query_xml.rs:378,442,469`) | `vat_code` (`receipt.rs:575,629`) |
| `szamlaSablon`/`pdfSablon` | `InvoiceHeader.template` (`invoice.rs:340`) | `pdf_template` (`receipt.rs:109,274,346`) |
| `osszegek` tree | see S-4 | see S-4 |

- **Direction.** One name per wire element across the crate; #139/#129 will touch `tipus` anyway; align `Receipt.kind` in the same change.

### N-5 · Domain value types applied to the receipt response but not to the invoice response
**Worth exploring** (breaking).
- `Receipt.payment_method: PaymentMethod`, `currency: Currency` (`receipt.rs:514-516`) vs `InvoiceInfo.payment_method: Option<String>`, `currency: Option<String>`, `language: Option<String>` (`query_xml.rs:226-238`), though `PaymentMethod`/`Currency` are open types that never fail to parse.
- **Direction.** `Option<PaymentMethod>`, `Option<Currency>`; `language` stays `String` only if `Language` stays closed (see N-8).

### N-6 · `InvoiceInfo.e_invoice: InvoiceAppearance`: the field name says flag, the type says code
**Strong** (small, breaking).
- `query_xml.rs:210-212`. CONTEXT *Invoice appearance*: "_Avoid_: `eszamla` as a flag in a queried document (it is a code)"; the worker names its field `appearance` (`gateway/document.rs:77`). The doc alias on `CreateInvoice.e_invoice` (a real flag) and on this field are the same `e-számla`, so rustdoc search lands on both without telling them apart.
- **Direction.** Rename to `appearance`; keep `e_invoice` for the two request booleans.

### N-7 · `Seller` (request `<elado>`) vs `Supplier` (response `<szallito>`)
**Speculative.** CONTEXT's *Seller block* entry already records `Supplier` as the code name, so this is a known trade-off; noting it because the glossary's own headword is "Seller block" and the crate uses both English words for one party (`invoice.rs:377-390`, `query_xml.rs:97-119`).

### N-8 · Open-set treatment is inconsistent across the enums
**Low.**
- `ReceiptTemplate` (`receipt.rs:17-34`) is `#[non_exhaustive]` with *no* `Other`, while its doc says an unknown wire value falls back: a value the type cannot hold; `InvoiceTemplate` (`invoice.rs:145-160`) has `Other(String)`. `Language`/`TaxpayerStatus` are closed with error types (fine for request-only closed sets, but they are never used on the response side, which is why N-5 keeps strings).

### N-9 · `*Result` suffix on non-`Result` types
**Low**; bundle with S-3. `InvoiceCreationResult` (`invoice.rs:707`), `CreditEntryResult` (`credit_entry.rs:153`), `ReceiptResult` (`receipt.rs:480`).

### N-10 · `lib.rs:9-10` overstates the doc-alias coverage
**Low.** "every type documents its Hungarian wire name and is findable in rustdoc search by that name via doc aliases": 38 public structs/enums carry no `#[doc(alias)]` (among them `InvoiceKind`, `InvoiceTemplate`, `Waybill`/carriers, `BuyerLedger`, `InvoiceCreationResult`, `CreatedInvoice`, `InvoiceSelector`, `VatTotal`, `GrandTotal`, `FinancialItem`, `TaxpayerPrefix`, `Rounding`). Most name the wire element in prose, which does not feed rustdoc search.

### N-11 · Stale "account mode pin" rustdoc, also in `receipt.rs`
**Low.** Extends #131 (which names `query_xml.rs` only): `receipt.rs:526-534` "A reader that pins the account mode treats `None` as a mismatch"; `query_xml.rs:257-268` likewise.

### N-12 · Both directions of the glossary check
- **Glossary terms absent from code:** *Response version* is the literal `"2"` in four writers (`credit_entry.rs:194`, `query_pdf.rs:86`, `invoice.rs:867`, `storno.rs:178`); correct by design (the crate pins v2); one `const RESPONSE_VERSION` would make the pin greppable. Everything else in *Documents* / *Agent concepts* has a code counterpart.
- **Code terms absent from the glossary:** `FinancialItem` (`qutet`, `query_xml.rs:460-484`): no English domain meaning is documented anywhere in the repo; `Waybill`/`TransOFlex`/`PickPackPoint`/`Sprinter`/`Mpl`; `InvoiceTemplate`/`ReceiptTemplate`; `ExchangeRate`; `BuyerLedger`/`LineItemLedger`; `EmailAttachment`/`InvoiceAttachments`; `customer_account_url` (`vevoifiokurl`); `economic_event_id` (`gazdEsemAzon`). Whether they warrant entries is the lead's call; `qutet` is the one a reader cannot resolve from the code alone.

---

## Axis 3: RUST CONVENTIONS

### R-1 · `Mpl` derives `Default` with three XSD-required `String` fields
**Strong** (tiny). **ADR 0008 conflict**: "`Default` is derived only where every field has a sensible absence."
- `ops/invoice.rs:239-251`; `Mpl::new` (`:253-268`) says "the three XSD-required fields". `Mpl::default()` yields an MPL block with empty `vevokod`/`vonalkod`/`tomeg`.
- **Direction.** Drop `Default` on `Mpl`.

### R-2 · `LineItem::with_comment` is the one builder method left on a plain-data request struct
**Speculative.** `item.rs:275-280`. ADR 0008 tension: `LineItem { comment: Some(..), ..item }` is the sanctioned form; this survivor invites the pattern to spread.

### R-3 · `ErrorCode::Unknown("0")` is a fabricated wire code
**Speculative**; fold into S-1. Six sites (listed under S-1) invent a code szamlazz.hu never sent when `sikeres=false` arrives without `hibakod`. `outcome_class()` → `Unknown` is the right conservative answer, but `ApiError.code.code() == "0"` in a log claims a code was received. An `ErrorCode::Absent` variant (or `ApiError.code: Option<ErrorCode>`, breaking) says what happened.

### R-4 · `ErrorCode` parses via `From<&str>`/`From<String>`/`From<u16>` and has no `FromStr`; `VatRate`/`PaymentMethod` have both
**Low.** `error.rs:331-381` vs `types.rs:278-328, 603-637`. `From<u16>` allocates a `String` to run the string match (`:377-381`).

### R-5 · `InvoiceAppearance::Electronic(i32)` leaks its invariant
**Low.** `query_xml.rs:139, 157-159`: `is_e_invoice` matches `Electronic(2 | 3)`, so a caller-built `Electronic(4)` is not an e-invoice. Either `Electronic(_)` in the match or a payload-less variant with `code()` reading the stored integer.

### R-6 · `VatRate` (and `Currency`, `PaymentMethod`) lack `Hash`
**Low.** `types.rs:174`; `Decimal: Hash`, and grouping items by rate for per-rate totals is the natural use of the type.

### R-7 · Bounded newtypes without the collection idioms
**Low.** `InvoiceAttachments` (`invoice.rs:530-564`): `as_slice`, `is_empty`, no `len`, no `Deref<Target=[_]>`/`IntoIterator`; `CreditEntries` (`credit_entry.rs:48-73`): `as_slice` only.

### R-8 · `ClientBuilder::endpoint` accepts any string
**Speculative.** `client.rs:105-108, 137`: a malformed endpoint surfaces on every `send` as `ClientError::Transport`, not at `build()` as `BuildError`. The worker validates with its own `Endpoint` type, so the library's builder is the weaker of the two.

### R-9 · `RequestError::InvalidXmlCharacter(u32)` cannot name the field
**Low.** `wire.rs:398-409` scans the serialized document after the fact (one deep place, good) but the error carries only the code point.

### R-10 · Visibility cosmetics
**Cosmetic.** `pub(crate) struct InvoiceResponse` mixes `pub(crate) sikeres` with `pub hibakod …` (`invoice.rs:1315-1334`); `xml::Element` methods are `pub` on a `pub(crate)` type (`xml.rs:92-158`).

### R-11 · Wire booleans in three styles with no stated rule
**Low.** Always-written `bool` (`e_invoice`, `download_pdf`); `bool` written only when `true` (`paid`, `invoice.rs:923-925`); tri-state `Option<bool>` (`margin_vat`, `eu_vat`, `guardian`, `send_email`, `preview_pdf`, `continuous_fulfillment`, `item_identifiers_on_invoice`). A one-line rule in `ops.rs` would stop the drift.

### R-12 · `ParseError::HttpStatus` classifies "the endpoint answered, not szamlazz.hu" as a parse error
**Speculative.** `error.rs:518-533`. Semantically a transport verdict; placed under `ParseError` to avoid a new `ResponseError` variant (which `#[non_exhaustive]` would permit).

### R-13 · Test locality
**Cosmetic.** `types.rs:846-850` (`agent_key_debug_is_redacted`) tests `credentials.rs`.

### R-14 · ADR 0008 cites a file that was renamed
**Doc drift.** `docs/adr/0008…md:59` says `tests/sans_io.rs`; the file is `crates/szamlazz-agent/tests/custom_http_client.rs`. (Fixed with the review.)

### R-15 · Limits hardcoded in `Display`
**Cosmetic.** `error.rs:446` "the maximum is 400" beside `MAX_ERASURE_CODE_COUNT`; `:449` "2147483647".

---

## Checked and found FINE (do not re-check)

- **`AgentRequest` is a real seam**, not a single-adapter trait: 12 implementations; `to_wire` is deep (validate → write → XML-1.0 scan → multipart, `wire.rs:388-395`); `Client::send<R>` is the one generic entry (`client.rs:218-245`). No other trait exists in the crate.
- **`http_client` hook has two adapters** (reqwest default `client.rs:172-185`; `ureq` in `tests/custom_http_client.rs`; the worker's `Gateway::open_with_http`) → real seam. `REQUEST_TIMEOUT` exported and consumed downstream as documented; wasm `cfg` split correct.
- **ADR 0008 holds**: `WireRequest` carries no transport fields (`wire.rs:34-41`); no request *struct* is `#[non_exhaustive]` except the documented `ReceiptPayment`; request enums (`InvoiceKind`, selectors, templates, `Rounding`, `Credentials`) keep it as decided; `Default` derives are correct everywhere but `Mpl` (R-1); `tests/literals.rs` exists.
- **Header verdict is centralised** (`RawResponse::header_verdict` `wire.rs:286-307`, order `szlahu_down` → `szlahu_error_code` → status); six ops call `check()`, invoice/storno/query_pdf go through `parse_issued`'s 56-tolerance. `Debug` on `RawResponse` redacts cookies and body; on `WireRequest` redacts the body.
- **`ErrorCode`**: `outcome_class` is a wildcard-free match (`error.rs:264-299`); `is_credential_error` present (#128 landed); `#[non_exhaustive]` + `Unknown(String)` is the right pair; `OutcomeClass` is `Copy + Hash + non_exhaustive`; `is_retryable` vs `outcome_class` distinction documented and tested.
- **`VatRate`** open set with `Other`, token normalisation, infallible parse, serde as token; matches CONTEXT exactly. **`Rounding`** half-away-from-zero per step, `try_calculated` fully checked (`item.rs:239-273`).
- **XML writing is one shared module** (`xml::document`/`Element`, all 12 writers); no op concatenates XML strings. `xml::de` helpers and `xml::totals` are shared on the read side.
- **Builders** consume `self` consistently (`ClientBuilder`, `RawResponse::with_status`). **Getters**: no `get_`; `as_str`/`as_bytes`/`as_wire`/`as_slice` used consistently. `# Errors`/`# Panics` sections present (pedantic `missing_*_doc` passes). `#[must_use]` broadly applied.
- **Secrets**: `AgentKey`/`Credentials` redacting `Debug`, no serde on `Credentials`; `Pdf` custom serde base64 with byte-length `Debug`.
- **Error types**: `thiserror` throughout, `XmlError` erases the backend, `body_excerpt` bounds every quoted body. (Third-party types in the public API: #131.)
- **CLI as adapter**: re-implements no protocol logic; `selector()` (`commands/invoice.rs:80-91`) is the only mapping; `read_json_input`/`write_pdf` wrap IO with `anyhow` context; PDF-on-stdout routing to stderr is correct.
- **Serde attribute style** uniform (`rename(deserialize=…)` on wire structs, `default` + `deserialize_with` pairs, `transparent` on newtypes, `rename_all = "snake_case"` on tagged enums).
