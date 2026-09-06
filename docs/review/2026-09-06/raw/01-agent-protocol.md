# Review 01 — `szamlazz-agent`: Számla Agent protocol & Hungarian invoicing correctness

Reviewer scope: `crates/szamlazz-agent` (src, tests, README), compared against `fixtures/upstream/agent/**`
(szamlazz.hu docs examples + XSDs, provenance in `fixtures/SOURCES.md`), `docs/szamlazz-hu-behaviour.md`,
`CONTEXT.md`, and the crate's use in `crates/szamlazz-cli`. Read-only; no cargo commands were run.

## Summary

The wire layer is in good shape. I cross-checked every hand-written serializer against the sequence order
of the corresponding upstream XSD (`xmlszamla`, `xmlszamlast`, `xmlszamlakifiz`, `xmlszamlapdf`,
`xmlszamlaxml`, `xmlszamladbkdel`, `xmltaxpayer`, and the four `xmlnyugta*` schemas) and found **no
element-order, namespace, field-name or multipart-action mismatches**; the request side covers every element
the (docs-normative) XSDs define, and the `szamla` response model is complete against `szamla.xsd`. Error
handling reads both `szlahu_*` headers and the body `<hibakod>`, which matches the observed per-operation
header presence. The findings below are therefore mostly about **semantics that sit above the wire**: a
retryability flag that is unsafe for document-issuing operations, an `InvoiceKind` enum that cannot express
a proforma reference on prepayment/final invoices (a common Hungarian díjbekérő → előlegszámla flow),
currency rounding that behaves inversely to its name for non-HUF documents, an unnormalised VAT-rate
wire token, and a handful of parser robustness / documentation gaps. Nothing I found would make the crate
emit a schema-invalid document or misread a documented success/error response.

Confidence legend: *high* = verified from code + an upstream fixture/XSD or the behaviour doc; *medium* =
verified in code, server behaviour inferred; *low* = server behaviour not verifiable from this repo.

---

## Findings

### 1. `ErrorCode::is_retryable()` marks codes 1 and 55 retryable regardless of operation — unsafe for creates

- **Severity:** medium
- **Confidence:** high — code is unambiguous; `CONTEXT.md` (*Unconfirmed*) itself classifies 1 and 55 on a
  create as "answer not known" (the document may or may not have been issued).
- **Location:** `crates/szamlazz-agent/src/error.rs:192-201` (`is_retryable`), `error.rs:24-25`, `error.rs:51-53`.
- **Evidence:**
  ```rust
  pub fn is_retryable(&self) -> bool {
      matches!(self, Self::Maintenance | Self::EInvoiceSigningFailed)
  }
  ```
  The doc comment above it says "Only retry on errors the server itself reported", implying that a
  server-reported 1/55 is safe to re-send.
- **Why it matters:** For `xmlszamla`/`xmlszamlast`/`xmlnyugtacreate`, a 1 ("system maintenance or internal
  error") or 55 ("e-invoice signing failed") does not prove the document was *not* stored. A naive caller
  that loops on `is_retryable()` can issue a duplicate legal document. The worker crate avoids this by
  query-first re-execution, but the agent crate is a standalone published API and the CLI/other users will
  read this flag literally.
- **Recommendation:** Either (a) document that the flag is safe only for read-only operations (queries,
  taxpayer, PDF) and that on creates it means "outcome unknown — query by external id / order number before
  re-sending", or (b) replace it with `retry_class() -> {Safe, OutcomeUnknown, Terminal}` so the
  create/read distinction is in the type.

### 2. `InvoiceKind` cannot express `dijbekeroSzamlaszam` on a prepayment or final invoice

- **Severity:** medium
- **Confidence:** high that the API cannot express it (code); medium on how often callers need it (the
  server auto-links by order number, behaviour doc C1-3, which mitigates when order numbers are used).
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:22-51` (enum), `invoice.rs:829-850` (writer).
- **Evidence:** Only `InvoiceKind::Invoice { proforma_number }` writes `dijbekeroSzamlaszam`; `Prepayment`
  and `Final { prepayment_number }` cannot carry one. `xmlszamla.xsd:121-124` lists `dijbekeroSzamlaszam`,
  `elolegszamla`, `vegszamla`, `elolegSzamlaszam` as independent optional elements.
- **Why it matters:** "Díjbekérő → (payment) → előlegszámla" is a standard Hungarian deposit flow; the
  proforma should be consumed by the prepayment invoice. The behaviour doc shows this works implicitly via a
  shared order number (`C1-3`: an `ES` issued without `dijbekeroSzamlaszam` under the `D`'s order got
  `<hivdijbekszam>`), but a caller that does not use order numbers, or has several proformas under one order,
  has no way to link explicitly. The doc comment ("makes the meaningless combinations unrepresentable")
  over-reaches: this combination is meaningful.
- **Recommendation:** Add `proforma_number: Option<InvoiceNumber>` to `Prepayment` (and probably `Final`),
  written as `dijbekeroSzamlaszam` before the kind flag, preserving XSD order. Non-breaking under
  `#[non_exhaustive]` only if done as a new variant or with a constructor; consider a struct-style
  `Prepayment { proforma_number }`.

### 3. `LineItem::calculated_for_currency` performs no rounding for non-HUF currencies

- **Severity:** medium
- **Confidence:** medium — behaviour verified in code and unit tests; whether szamlazz.hu accepts or
  silently rounds 5-decimal EUR VAT values is not verifiable here.
- **Location:** `crates/szamlazz-agent/src/item.rs:129-163` (esp. `139-146`), tests `item.rs:253-281`.
- **Evidence:**
  ```rust
  } else {
      let net_value = unit_price * quantity;
      let vat_value = match &vat_rate { VatRate::Percent(rate) => net_value * *rate / Decimal::ONE_HUNDRED, ... };
  ```
  Test `foreign_currency_calculation_preserves_exact_decimal_values` asserts `vat_value == 27.00135` EUR.
  Meanwhile the plainer `LineItem::calculated` *does* round to 2 dp (`item.rs:115-123`).
- **Why it matters:** The name promises currency-aware arithmetic but for every non-HUF currency it emits
  sub-minor-unit values (`<afaErtek>27.00135</afaErtek>`). szamlazz.hu will print two decimals and compute
  its own per-VAT-rate and grand totals from the values as sent; the caller's bookkeeping (which will round
  to cents) can drift from the printed document, and the reversed expectation (the *less* currency-aware
  helper rounds, the currency-aware one does not) is a trap. Under Hungarian law amounts on the invoice
  must be stated in the invoice currency, in practice to the currency's minor unit.
- **Recommendation:** Round to the ISO 4217 minor unit (2 dp by default; a tiny table for 0-dp — HUF, JPY,
  ISK — and 3-dp currencies such as KWD/BHD), keeping `LineItem::new` as the escape hatch. If the intent
  really is "exact", rename to make that explicit and warn in docs.

### 4. `VatRate::Percent` wire token is not normalised (`27.00`, `27.0` are emitted verbatim)

- **Severity:** low–medium
- **Confidence:** medium — code verified; the upstream request fixtures and XSD comments only ever show
  integer tokens (`<afakulcs>27</afakulcs>`; receipt docs list `0, 5, 10, 27, AAM, …`), acceptance of
  `27.00` is unverified.
- **Location:** `crates/szamlazz-agent/src/types.rs:239-243` (`as_wire`), `types.rs:294-297` (`From<&str>`).
- **Evidence:** `Self::Percent(rate) => Cow::Owned(rate.to_string())` — `rust_decimal` preserves scale, so a
  rate loaded from a `DECIMAL(5,2)` column or `dec!(27.00)` serialises as `27.00`.
- **Why it matters:** `afakulcs` is an `xsd:string` on the request side (`xmlszamla.xsd:75`), i.e. szamlazz.hu
  string-matches it against a known set. If `27.00` is not in that set the item is rejected (or worse,
  treated as an unknown code). Round-trip from the query response is also asymmetric: the response
  `afakulcs` is a `double` and may come back as `27` or `27.0`.
- **Recommendation:** `rate.normalize().to_string()` in `as_wire` (and in `From<&str>` after parse), so
  `27.00`, `27.0`, `27` all serialise as `27`, while `5.5` stays `5.5`.

### 5. HUF line net is rounded to whole forints before the server's `net = unit price × quantity` check

- **Severity:** low
- **Confidence:** low–medium — the only evidence that szamlazz.hu tolerates the discrepancy is an
  `#[ignore]`d live test (`tests/live.rs:82-84`) that I cannot run; the tolerance itself is undocumented in
  the repo.
- **Location:** `crates/szamlazz-agent/src/item.rs:137-138`, `item.rs:173-176`.
- **Evidence:** `calculated_for_currency(HUF)` → `calculated_with_precision(…, 0)`;
  `net_value = round(unit_price * quantity)`; `unit_price` itself is sent unrounded (`1234.56 × 2 = 2469.12 → 2469`).
- **Why it matters:** szamlazz.hu documents codes 259–264 for exactly this arithmetic (`error.rs:111-125`).
  A half-forint discrepancy per line is normal Hungarian practice and the live test suggests it is accepted,
  but the tolerance is an assumption baked into a default helper.
- **Recommendation:** Record the tolerance (once measured) in `item.rs` docs and in
  `docs/szamlazz-hu-behaviour.md`; consider offering `calculated_for_currency` a documented note that
  `unit_price` should itself be a whole-forint value if the caller wants zero discrepancy.

### 6. Delivery note silently overrides the caller's template with `SzlaFuvarlevelesAlap`

- **Severity:** low
- **Confidence:** low–medium — code verified; whether szamlazz.hu *requires* this template for a
  `szallitolevel` is unverified (a delivery note was created successfully through the crate per the
  behaviour doc, but that only proves the override works, not that it is needed).
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:865-872`.
- **Evidence:**
  ```rust
  let template = match self.kind {
      InvoiceKind::DeliveryNote => Some(&InvoiceTemplate::DeliveryNote),
      _ => h.template.as_ref(),
  };
  ```
- **Why it matters:** `SzlaFuvarlevelesAlap` is the "invoice with *fuvarlevél* (carrier waybill)" layout; a
  *szállítólevél* (delivery note, `szallitolevel=true`) is a different document. Conflating the two is at
  best a naming confusion and at worst prints a waybill block on a delivery note. Also any
  `header.template` set by the caller is discarded without error.
- **Recommendation:** Do not force the template; if a live probe shows it is required, keep the default but
  respect an explicit `header.template`, and rename the variant to `WithWaybill` with a doc note.

### 7. Taxpayer parser does not recognise a szamlazz.hu `xmlszamlavalasz` error envelope

- **Severity:** low–medium
- **Confidence:** low — the code path is certain; whether szamlazz.hu ever answers `xmltaxpayer` with an
  `xmlszamlavalasz` body (e.g. for credential codes 3/135/136/164, or 7) is not verifiable from the fixtures
  (`taxpayer_error.xml` shows szamlazz.hu's own code 57 *relayed inside* `QueryTaxpayerResponse`).
- **Location:** `crates/szamlazz-agent/src/ops/taxpayer.rs:207-220`; contrast `ops/query_xml.rs:534-545`,
  `query_xml.rs:580-626` which dispatch on both `szamla` and `xmlszamlavalasz` roots.
- **Evidence:** `from_body` accepts only `QueryTaxpayerResponse` in the OSA 2.0/3.0 namespaces; anything else
  is `ParseError::UnexpectedBody`.
- **Why it matters:** A body-only API error (the pattern the crate itself documents for queries, code 7) would
  surface as a *parse* error, which the worker maps to `Unanswered` and retries under the read policy — a
  permanent rejection retried until exhaustion. The XML query already solves this; the taxpayer op is
  inconsistent with it.
- **Recommendation:** Reuse the dual-root dispatch: if the root is `xmlszamlavalasz`, parse it with
  `InvoiceResponse::from_body(...)?.into_success()` and return the `ApiError`.

### 8. Documented-but-untyped codes: 339 (receipt not found) and VAT code `TEHK`

- **Severity:** low
- **Confidence:** high that they are untyped; low on exact server semantics of `TEHK`.
- **Location:** `crates/szamlazz-agent/src/error.rs:28-39` (doc says "Receipt operations report an unknown
  receipt number as code 339, which parses as `ErrorCode::Unknown`"); `src/types.rs:176-231` (no `Tehk`
  variant) vs `fixtures/upstream/agent/xsd/szamla.xsd:35` and `xmlnyugtavalasz.xsd:13` which enumerate
  `TEHK` beside `TAHK`.
- **Why it matters:** Both are documented in the crate's own sources; leaving them in `Unknown`/`Other` forces
  every caller (e.g. the worker's `not_found` mapping for receipts) to string-match.
- **Recommendation:** Add `ErrorCode::ReceiptNotFound` (339) and `VatRate::Tehk`. While there, verify the
  `Tahk` doc text: in NAV terms the two are the two legal bases of objective exemption (Áfa tv. 85. § "a
  tevékenység közérdekű jellegére tekintettel" vs 86. § "egyéb sajátos jellegére tekintettel"); the current
  `Tahk` comment merges both.

### 9. Final-invoice docs omit that the caller must supply the negative prepayment line

- **Severity:** low
- **Confidence:** medium — behaviour doc C6-2 ("The server does **not** net the prepayment into the final:
  `VS` gross 1270, `kintlevoseg` 1270") is explicit; the crate docs are silent.
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:38-44` (`InvoiceKind::Final`), `error.rs:72-83`.
- **Why it matters:** A Hungarian végszámla must list the full performance and deduct the előleg as a
  negative line at the same VAT rate; otherwise the buyer is invoiced twice for the deposit and the NAV
  report is wrong. This is exactly the kind of protocol/tax fact a crate consumer will not guess.
- **Recommendation:** One sentence on `InvoiceKind::Final` and in the README protocol notes; optionally a
  `LineItem::prepayment_deduction(...)` helper that negates a prepayment amount at a given rate.

### 10. `header_error()` treats a present-but-empty `szlahu_error_code` as an error

- **Severity:** low
- **Confidence:** low — the behaviour doc says success responses do not carry `szlahu_error_code` at all
  (A6, D1), so this may never fire; noted because the parser is otherwise deliberately lenient about empty
  elements (`<hibakod></hibakod>` → `None`).
- **Location:** `crates/szamlazz-agent/src/wire.rs:205-211`.
- **Evidence:** `let code = self.header("szlahu_error_code")?; let code = ErrorCode::from(code);` — `""` →
  `ErrorCode::Unknown("")`, which `check()` returns as `Err`.
- **Recommendation:** `.filter(|c| !c.trim().is_empty())` on the header, mirroring `check_available()`.

### 11. `RawResponse` carries no HTTP status; intermediary errors are indistinguishable from bad XML

- **Severity:** low (design)
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/wire.rs:136-162`; `src/client.rs:191-205`.
- **Evidence:** `RawResponse { headers, body }` only; the reqwest client discards `response.status()`.
- **Why it matters:** szamlazz.hu does signal in-band, but a 502/503 HTML page from a proxy/CDN becomes
  `ParseError::UnexpectedBody("<html>…")`. Callers (and the worker's `Unanswered::Transport` mapping) cannot
  tell a transient upstream failure from a genuine protocol surprise, and the HTML lands in error messages.
- **Recommendation:** Add `status: Option<u16>` to `RawResponse` (builder-optional to stay sans-IO), and have
  parsers short-circuit `>= 500` (without `szlahu_*` headers) into a dedicated `ResponseError::HttpStatus`.

### 12. `RegisterCreditEntry` with zero entries and `additive = false` wipes an invoice's payments, no guard

- **Severity:** low
- **Confidence:** high — XSD allows `kifizetes` `minOccurs="0"`, the crate allows an empty `CreditEntries`,
  and behaviour doc D7 confirms replace semantics.
- **Location:** `crates/szamlazz-agent/src/ops/credit_entry.rs:44-93`, `credit_entry.rs:172-195`.
- **Why it matters:** "Clear all credit entries" is a legitimate operation, but it is one field default away
  from an accidental wipe (`RegisterCreditEntry::new(n)` defaults to `additive: false` and no entries).
- **Recommendation:** Either require ≥1 entry unless an explicit `clear: true` / `CreditEntries::CLEAR` is
  used, or document the wipe prominently on `RegisterCreditEntry` and `new()`.

### 13. `InvoiceAppearance` follows the docs (`1` = paper), but `docs/szamlazz-hu-behaviour.md` reads `eszamla=1` as "e-invoicing enabled"

- **Severity:** low for the crate; medium for the design docs that depend on it
- **Confidence:** medium — `fixtures/upstream/adatkapcsolat/szamla_example.xml:32` states verbatim
  "0: not an invoice, 1: paper invoice, 2: e-invoice, 3: e-invoice"; the behaviour doc (lines 5, 137) says
  the probe account had e-invoicing enabled *because* documents showed `<eszamla>1</eszamla>`.
- **Location:** `crates/szamlazz-agent/src/ops/query_xml.rs:121-163`; `docs/szamlazz-hu-behaviour.md:5,137`;
  consumer `crates/restate-szamlazz/src/gateway.rs:324-326`.
- **Why it matters:** One of the two is wrong. If the crate is right, the probe documents were *paper*
  invoices, the "352 may be an e-invoice rule" caveat is unfounded, and the worker's storno `e_invoice`
  derivation from `eszamla` (design doc §334) has never been exercised on an e-invoice. If the doc is right,
  `InvoiceAppearance::Paper` misclassifies e-invoices and `is_e_invoice()` returns `false` for them.
- **Recommendation:** Resolve on the go-live account (query one document issued with `<eszamla>true</eszamla>`
  and one with `false`; record both codes). Until then flag the behaviour doc line as unverified.

### 14. `RawResponse`'s derived `Debug` prints `Set-Cookie` (`JSESSIONID`)

- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/wire.rs:141-145` (`#[derive(Debug, Clone)] pub struct RawResponse`).
- **Why it matters:** The crate is careful to redact `AgentKey`, `Credentials` and `WireRequest` bodies
  (`credentials.rs:27-31, 86-96`; `wire.rs:39-49`), but a `{:?}` of a `RawResponse` — the natural thing to
  log on a parse failure — leaks a live session cookie, which authenticates as the account for 90 minutes.
- **Recommendation:** Hand-write `Debug` to elide `set-cookie` values (and perhaps truncate the body).

### 15. `MinimalInvoiceResponse.hibakod` lacks `empty_as_none`; missing `hibakod` becomes `Unknown("0")`

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:1268-1297`; also `invoice.rs:1316-1321`,
  `proforma.rs:83-89`, `receipt.rs:672-678`, `taxpayer.rs:357-360`.
- **Evidence:** `#[serde(default)] hibakod: Option<String>` → `<hibakod></hibakod>` is `Some("")` →
  `ErrorCode::Unknown("")`; a `sikeres=false` without any `hibakod` is reported as code `"0"`.
- **Why it matters:** `"0"` is a fabricated wire code that could be confused with a real one and is not
  distinguishable by callers from a genuine `Unknown("0")`.
- **Recommendation:** Use `empty_as_none` consistently and add an `ErrorCode::Unspecified` (or make
  `ApiError.code` `Option<ErrorCode>`) for "server said failure without a code".

### 16. Exchange-rate requirement is stricter than the schema and applies to VAT-free / non-invoice documents

- **Severity:** low
- **Confidence:** medium — XSD has `arfolyamBank`/`arfolyam` optional (`xmlszamla.xsd:118-119`); the docs'
  comment ties them to VAT display, so a proforma/delivery note or an AAM/EUFAD37 invoice in EUR does not
  need a rate for VAT purposes. Whether szamlazz.hu itself refuses without one is unverified.
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:776-787`; `ops/receipt.rs:165-175`; `types.rs:379-381`.
- **Evidence:** `if !self.header.currency.is_huf() { … ok_or(RequestError::MissingExchangeRate) }`, and
  `is_huf` matches only `"HUF"`/`"Ft"` (so `"huf"` is treated as foreign).
- **Why it matters:** Over-strict client validation refuses documents the server would accept; the
  `ExchangeRate::automatic_mnb()` escape hatch mitigates but callers must know about it.
- **Recommendation:** Keep the check but make it case-insensitive and document the MNB shortcut on
  `RequestError::MissingExchangeRate`; consider relaxing for `Proforma`/`DeliveryNote`.

### 17. CLI `invoice storno` does not call `CreatedInvoice::reverses()`

- **Severity:** low (consumer, not the crate)
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/src/commands/invoice.rs:209-222`; the crate's own instruction is at
  `src/ops/storno.rs:20-24` ("check `CreatedInvoice::reverses` after every call").
- **Why it matters:** `szamlazz invoice storno D-…` on a proforma or delivery note prints the echoed document
  as a successful reversal (behaviour doc B5). The crate exposes the guard; its first-party consumer skips it.
- **Recommendation:** In the CLI, `if !created.reverses(&number) { bail!("szamlazz.hu did not reverse …") }`.
  In the crate, consider making `StornoInvoice::parse` itself return a `StornoOutcome { Reversed | Echoed }`
  so the check cannot be forgotten.

### 18. `+` in `szlahu_*` header values is decoded as a space, including `szlahu_vevoifiokurl`

- **Severity:** info
- **Confidence:** medium — correct *if* szamlazz.hu encodes with Java `URLEncoder` (space → `+`, literal `+` →
  `%2B`), which the observed `Sikertelen+bejelentkez%C3%A9s` shape suggests; unverified for URLs.
- **Location:** `crates/szamlazz-agent/src/wire.rs:249-257`.
- **Recommendation:** Document the assumption; if a URL with a literal `+` is ever observed, decode
  `szlahu_vevoifiokurl` without the `+` substitution.

### 19. `TÉTELÁFA` / explicit-VAT items have no representation and `calculated*` would zero their VAT

- **Severity:** info
- **Confidence:** low — no fixture or XSD in the repo mentions `TÉTELÁFA`; I recall it from szamlazz.hu's
  `afakulcs` documentation as "take the VAT amount from `afaErtek` as given" but cannot verify here.
- **Location:** `crates/szamlazz-agent/src/types.rs:176-231`; `src/item.rs:141-144, 177-180`.
- **Why it matters:** If the code exists, `VatRate::Other("TÉTELÁFA")` is expressible, but every `calculated*`
  helper computes `vat_value = 0` for any non-`Percent` rate, which is wrong for a code whose whole point is
  an explicit VAT amount.
- **Recommendation:** Confirm on the docs page; if real, add a variant and make `calculated*` refuse it (or
  take an explicit VAT amount).

### 20. `Buyer.send_email` default when omitted is asserted in docs but not verified

- **Severity:** info
- **Confidence:** low
- **Location:** `crates/szamlazz-agent/src/ops/invoice.rs:390-398`.
- **Evidence:** Field doc says "notification is sent when present unless `send_email` is `Some(false)`";
  the behaviour doc probes (D6) always set `sendEmail=true` explicitly.
- **Recommendation:** Verify on the go-live checklist; until then soften the doc to "server default".

---

## What is done well

- **Element order is correct everywhere.** I walked each `write_xml` against the XSD `<sequence>` for
  `beallitasok`, `fejlec`, `elado`, `vevo`, `vevoFokonyv`, `fuvarlevel` (+ `tof`/`ppp`/`sprinter`/`mpl`),
  `tetel`, `tetelFokonyv` (`ops/invoice.rs:794-1016`), the storno/credit/query/delete/taxpayer documents, and
  the receipt documents (which use `<all>` anyway). Notably the two different positions of `valaszVerzio`
  (`xmlszamla`: before `aggregator`; `xmlszamlast`: after `guardian`) are both right (`invoice.rs:803`,
  `storno.rs:158`).
- **Wire envelope:** correct namespaces for all 11 operations, correct multipart action names
  (`action-xmlagentxmlfile`, `action-szamla_agent_st`, `_kifiz`, `_pdf`, `_xml`, `_dijbekero_torlese`,
  `_nyugta_create/storno/get/send`, `_taxpayer`), XML sent as a *file* part (avoids code 53), UTF-8
  declaration, XML 1.0 character validation, deterministic boundary with collision avoidance, credentials
  written in XSD order for both key and user/password (`xml.rs:151-159`). `xsi:schemaLocation` is omitted
  and the behaviour doc's ~75 live creates prove it is not required.
- **Error channel handling matches observed behaviour:** `szlahu_down` first, then header
  `szlahu_error_code`, then body `<hibakod>` on every parser (`wire.rs:213-232`, `invoice.rs:1045-1130`),
  with code 56 correctly treated as "issued, notification failed" including the text-body fallback and
  recovery of the number from `szlahu_szamlaszam`. Observed codes 14/73/221/352/463 are typed with verbatim
  Hungarian messages in the docs.
- **Open sets where the protocol is open:** `ErrorCode::Unknown`, `VatRate::Other`, `Currency`,
  `PaymentMethod::Other`, `InvoiceTemplate::Other`, `document_type: String`. `Language` and
  `TaxpayerStatus` are closed but match the XSD enumeration / documented codes exactly (`cz` not `cs`; `7,6,1,0,-1`).
- **Response model completeness:** `InvoiceDocument` covers every element of `szamla.xsd` including
  `qutetek`, `cimkek`, `forras`, `iktatoszam`, `afatipus`, `sztetordering`, `banktranzid`; `sztornozott` is
  `Option<bool>` mirroring the wire (absent ≠ false), which is the load-bearing reversal signal for the worker.
- **Storno semantics encoded, not just documented:** `CreatedInvoice::reverses()` implements the observed
  "success-shaped echo" case with `gross ≤ 0` (zero-total stornos allowed), and `issue_date` defaults to
  `None` to avoid 352.
- **Money is `rust_decimal` throughout**, half-away-from-zero rounding for HUF, no `f64` anywhere on the wire.
- **Credential hygiene:** `AgentKey`/`Credentials`/`WireRequest` have redacting `Debug`, no `Display`, no
  serde on `Credentials`; the key only appears in the request body.
- **Sans-IO split holds:** `AgentRequest::{validate, write_xml, parse, multipart_files}` and
  `RawResponse` are the only surfaces; the reqwest client is a 60-line shell behind a feature flag, with
  redirects disabled (a 30x would turn the POST into a GET) and cookie store on. `Pdf::save_to` is the only
  I/O in the core and is gated to non-wasm.

## Questions I could not resolve

1. What tolerance does szamlazz.hu apply in the 259/260/261 arithmetic checks (±1 HUF? relative?) — decides
   how safe finding 5 is and whether finding 3's sub-cent values are accepted.
2. Is `<afakulcs>27.0</afakulcs>` / `27.00` accepted as equivalent to `27`? (finding 4)
3. Does `TÉTELÁFA` exist as an `afakulcs` value and what exactly does it do? (finding 19)
4. Exact NAV meanings of `TAHK` vs `TEHK` (finding 8).
5. Is `SzlaFuvarlevelesAlap` required for `szallitolevel=true`, or merely one layout option? (finding 6)
6. For `xmltaxpayer`, how do szamlazz.hu-side errors 3/135/136/164/7 arrive — headers, `xmlszamlavalasz`
   body, or relayed inside `QueryTaxpayerResponse` like 57 is? (finding 7)
7. What is `<eszamla>` for a document created with `<eszamla>true</eszamla>` on the probe account — `1`,
   `2` or `3`? (finding 13)
8. Server default for `sendEmail` when omitted but `email` is present (finding 20).
9. Are `szlahu_*` header values always Java-`URLEncoder`-encoded (so `+` ⇒ space is right for
   `szlahu_vevoifiokurl` too)? (finding 18)
10. Does szamlazz.hu accept `penznem` in lower case (`huf`, `eur`)? Affects `Currency::is_huf` (finding 16).
