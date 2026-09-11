# Independent adjudication — Számla Agent functional findings

> **First-round adjudication.** See the [round-two final report](FINAL.md) and [final independent judgment](round-2/JUDGE.md) for the superseding decisions and complete finding coverage.

**2026-09-09 · code:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`.

**Verdict:** retain the valid-date failures, incomplete known-error classification, and conditional comma-header failure as functional defects. The taxpayer malformed-input cases are reproducible **robustness defects**, not demonstrated failures on normal Számla Agent responses. PDF/NAV omissions are **field-exposure coverage gaps**, not invalid parsers. No P0/P1 finding is established.

This adjudicates [02](raw/02-query-responses.md), [03](raw/03-receipts.md), [04](raw/04-other-operations.md), and [05](raw/05-wire-errors.md), independently checking implementation, selected official sources, and offline reproductions. `F-*` below refers to report 03. Source basenames in code citations are relative to `crates/szamlazz-agent/src/`; `behaviour` means `docs/szamlazz-hu-behaviour.md`. P2 = normal functional-fix priority; P3 = lower-priority hardening, fidelity, documentation, or coverage work. Confidence in reproduction is distinct from evidence of live occurrence.

## Decisions

| Findings | Decision | Priority / confidence |
|---|---|---|
| QR-01, F-01 — valid XSD dates refused | **Confirmed functional defect**, one date-adapter family with required/optional paths | P2 / high; live emission unknown |
| W05-01, F-02 — missing error codes | **Confirmed classification coverage defect**; seven receipt codes are part of the thirteen, not additional findings | P2 / high; downstream impact conditional |
| QR-04, O4-02; 05's excluded comma note | **Confirmed conditional interoperability defect**; overturn 05's exclusion | P2 shared path, P3 PDF-only impact / high locally; compound live trigger unobserved |
| O4-01 — truncated/multiple-root/path/entity taxpayer handling | **Confirmed robustness defect**, downgraded from a core valid-response compatibility finding | P3 / high locally; normal-service trigger unestablished |
| QR-02 — PDF balance/URL dropped | **Confirmed exposure gap**, not failure to fetch/decode a valid PDF | P3 coverage / high; actual emission unknown |
| O4-03 — NAV business fields omitted | **Confirmed selected-subset coverage gap**, not invalid NAV 3.0 parsing | P3 coverage / high for omissions; delivery evidence differs by field |
| F-03 — optional strings trimmed | **Retain narrowly as fidelity defect**, not proof of live identifier collision | P3 / high for mutation; medium for practical harm |
| F-04, W05-02 — receipt external-id recovery advice | **Confirmed documentation defect**, deduplicated | P3 / high |
| QR-03 — credit-entry bank account meaning | **Confirmed documentation defect** | P3 / high |
| F-05 — receipt rounding guidance | **Confirmed documentation overgeneralization**, not an arithmetic/validation defect | P3 / high |
| O4-04 — “unpaid” proforma deletion | **Confirmed misleading scope wording**, not missing paid-state validation | P3 / high |

### 1. Dates: a supported value with a different legal spelling

The current [queried-invoice XSD][invoice-xsd] declares the eleven reported date positions as `date`; the [receipt response schema][receipt-response] declares `alap/kelt` likewise. [XSD §3.2.9.1][xsd-date] permits the timezone suffix; [§4.3.6][xsd-space] fixes date whitespace to `collapse`.

`query_xml.rs:703–708,810–825,957–960,994–997` uses `empty_as_none`, which trims then calls `Date::from_str` (`xml.rs:269–283`). Credit-entry `datum` (`query_xml.rs:1034–1039`) and receipt `kelt` (`receipt.rs:735`) use direct Jiff deserialization. Reproduced: ordinary dates succeed; `2026-09-09Z`, `2026-09-09+02:00`, and credit-entry `…Z` fail the entire invoice query. Receipt `Z`, `+02:00`, `-05:00`, `+14:00`, and space-padded dates fail; plain dates succeed. Receipt create/storno/query share `parse_receipt` (`receipt.rs:651–664`).

This is not the unavoidable limitation of a civil-date model: the calendar portion fits, and an adapter can validate the suffix and deliberately retain the printed date. It is also not the accepted absence/empty-date policy, nor the separate Adatkapcsolat invalid-date policy. Receipt create/storno can consequently report an unknown outcome after successful issuance; the invoice XML operation is read-only. None of the recorded live evidence establishes suffixed dates, so do not describe an observed outage.

**Fix boundary:** validate the complete lexical form, handle required dates as well as optional ones, and explicitly document discarding the timezone. Retaining the printed date is a business projection, not full XSD timezoned-value fidelity (§3.2.9 also describes normalization that can change the calendar day). Do not merely truncate at ten bytes, shift through UTC, or expand this finding into arbitrary ancient/out-of-range date support.

### 2. Thirteen known codes: real missing semantics, conservative failure mode

Fresh [general catalogue][errors] plus [receipt supplement][receipt-response], compared directly with `error.rs:220–253,314–348`:

| Codes | Documented meaning | Appropriate class |
|---|---|---|
| 336, 337 | Receipt prefix used for invoices / invalid format | `Rejected` |
| 339 | Receipt number does not exist | `NotFound` (extend class docs), or explicit receipt-not-found handling |
| 340 | Receipt paid amount differs from gross | `Rejected` |
| 363, 364, 365 | HUF receipt gross not whole / net or VAT exceeds two decimals | `Rejected` |
| 551, 552, 553 | Simplified image incompatible with account / too many items / invalid VAT | `Rejected` |
| 554 | Cannot correct an invoice using the simplified image | `Rejected` |
| 555, 556 | Simplified final/prepayment VAT mismatch / forbidden document type | `Rejected` |

All thirteen currently become `Unknown(code)`, class `Unknown`, `is_retryable=false`; numeric tokens and messages survive. The defect is not absence of enum names alone: the public outcome helper cannot supply documented settled semantics. The general catalogue contributes nine; the receipt supplement contributes four. The seven in F-02 overlap W05-01.

**Reach matters:** 551–553/555–556 depend on a simplified-image request feature the built-in invoice writer does not expose. They are catalogue completion rather than five independently demonstrated reachable failures. **554 remains reachable:** the official text expressly says it applies even without `simpleItems` in the corrective request, when the original used that image. Receipt queries can receive 339, and receipt writes can receive the other receipt refusals.

**Do not overclaim:** `Unknown` is safely conservative, not false success or permission to retry; the client has no resend loop. Twelve `Rejected` mappings describe this request, never prove an earlier lost request did nothing. Keep 338 as duplicate prevention, not recovered success; keep 335 as a settled delete refusal and 7's operation-dependent missing-data meaning. Unknown NAV textual codes must stay open. Sources justify preserving uncertainty for 55; they do not establish the stronger rustdoc phrase “issued, signing failed” (`error.rs:261–264`).

### 3. Comma monetary headers: eligible, with the full trigger

`envelope.rs:146–155,285–303` sends fallback header text straight to Decimal. `behaviour:160` records **`szlahu_nettovegosszeg 100,01`**, from EUR invoice creation (P60-E1/E3). That live spelling is evidence for supporting the header, not an intentional implementation deviation to exclude. Report 05's exclusion at `05-wire-errors.md:256` misapplies the user's rule.

**Compound trigger:** an otherwise acceptable numbered success, a comma-form monetary header, and its corresponding body element **absent, empty, or whitespace-only**, on a path that reads that amount. A valid body amount wins and masks the header. Reproduced `100,01` failure on PDF net and `-127,50` failure on storno/credit gross; dot/exponent controls succeed. Creation also shares this helper. The special numbered code-56 path drops an unreadable optional amount rather than failing (`envelope.rs:201–227`).

The [PDF schema][pdf-response] allows optional body totals; the code explicitly offers fallback. Thus this is a real compatibility bug in that fallback, although **no recorded live call combines missing body total and comma header**, and the observed comma is specifically the create net header, not proof of every header/operation using it. Storno/credit parse failure can obscure a completed write; PDF failure loses a read result. P2 is justified for the shared write path, with the lower PDF-only impact stated separately.

**Missing consideration:** PDF parsing also reads `outstanding` before dropping it. A header-only comma outstanding value can reject a PDF even though that metadata is not exposed. I reproduced this interaction; its particular live spelling remains unverified. Normalize documented/observed monetary **headers** deliberately; do not accept commas in XML `xs:double`, remove grouping punctuation indiscriminately, or call every fractional response broken.

### 4. Taxpayer malformed acceptance: robustness, not demonstrated core conformance failure

All five taxpayer behaviours in O4-01 reproduce. `taxpayer.rs:226–284` reads until EOF without checking completed root/depth; `set` (`287–337`) assigns by local name anywhere outside an address; `245–258` drops unknown entities. Therefore:

- A response truncated after completed `funcCode=OK`, `taxpayerValidity=true`, and `taxpayerName=ACME` returns success.
- A **body-only** complete `ERROR/57` followed by a second root with `OK/true` becomes success. An error header still wins before this parser (`181–183`); confirmed separately.
- An unknown wrapper containing `taxpayerName` overwrites the real name; recognized children in a foreign namespace are read as normal fields.
- `A&bogus;B` becomes `AB`. Standard entities, character references, and concatenated CDATA work. A supplementary probe shows `O&bogus;K` even becomes a successful `OK` verdict.

These are concrete implementation weaknesses, especially because “unknown elements are skipped” (`202–205`) actually means their recognized descendants are consumed. However, the examples are malformed XML or outside the documented NAV structure. The [official taxpayer response][taxpayer-response] and NAV schema do not demonstrate these shapes being emitted. Real 3.0 common/base namespace mixing and two sequential addresses **work**. No authentication bypass, ordinary NAV failure-to-success incident, or general “taxpayer API is broken” conclusion follows.

**Transport qualification missing from the raw finding:** `client.rs:284` awaits the whole HTTP body before parsing. If the HTTP stack detects incomplete framing/body delivery, it returns a transport error first. The truncation defect requires the shortened XML to arrive as a successfully collected HTTP body (or through `RawResponse` directly), with enough completed fields to satisfy `into_info`. Truncation before validity is still refused. No HTTP-level reproduction was performed.

**Adjudication:** actionable P3 hardening, not a core valid-response interoperability defect. Complete-document checking and path-aware reads should precede any project-wide strict-XSD policy. Namespace checks must account for actual 2.0/3.0 namespaces, not prefix spelling. Preserve sparse-content tolerance and legitimate comments/processing instructions. Undefined-entity deletion is corruption, but not demonstrated XXE/external resource access.

The shared serde paths also accept trailing roots (`xml.rs:63–108,152–176`; `envelope.rs:259–264`), but **do not share the taxpayer's overwrite algorithm**: deletion ignores a second root, while a truncated deletion envelope fails. Keep those claims separate rather than extrapolating the taxpayer failure reversal to all operations.

### 5. Missing exposure: PDF and NAV are selected projections

**QR-02:** confirmed: `InvoicePdf` (`query_pdf.rs:36–48,75–83`) drops `outstanding` and `customer_account_url` already read by the shared envelope. Both appear in the PDF operation's own [schema][pdf-response]. A successful probe carrying both serializes only number/net/gross/PDF. This limits useful access but does not make a valid PDF unparsable. P3 API-coverage work; add optional fields if full response exposure is intended. Dropped id/payment-method/notification metadata is a separate scope choice, not another proven PDF failure.

**O4-03:** confirmed: `TaxpayerInfo` and its private parser explicitly expose a reduced subset (`taxpayer.rs:110–127,187–199,317–354`). The [NAV 3.0 schema][nav-api] (`QueryTaxpayerResponseType`, lines 1552–1581; `TaxpayerDataType`, 1846–1889), and [linked specification][nav-spec] §1.8.9, pp.66–69, define the five omissions:

- `countyCode`: prevents reconstructing the full returned tax number;
- `vatGroupMembership`: prevents discovering the returned VAT-group id;
- `incorporation`: economic type;
- `taxpayerShortName`: short name;
- `infoDate`: last change of taxpayer data, **not** lookup time or a cache expiry.

The first three have stronger business utility; none corrupts the fields currently exposed. The NAV specification expressly leaves the extent of client-side use discretionary (§1.8.9.2, point 5). Public crate docs promise registered name/addresses, not every NAV field (`taxpayer.rs:74–80`). **Verdict: missing capability/coverage, not a mandatory parser fix.** The agent projection and the worker's deliberate journal projection are separate decisions.

**Correction to the evidence limit:** `infoDate` already appears in the Számla Agent page's current 2.0 success example and `fixtures/upstream/agent/responses/taxpayer.xml:19`. Its exposure gap is not contingent on forwarding new 3.0 fields. Current live forwarding of the other four remains unverified. `tax_number` intentionally means the eight-digit prefix (`119–121`), so its name is not evidence of a wrongly parsed full number.

`Client::send` returns only the typed response (`client.rs:262–288`), so built-in responses cannot recover discarded fields. The raw reports overstate the workaround slightly: a custom `AgentRequest` wrapper can retain body/header data through the same client, as well as a custom transport. Neither supplies the missing built-in typed fields.

### 6. Optional-string trim: keep the narrow fidelity finding

`xml.rs:279–281` trims nonempty strings as well as numbers; receipt call id/order/comment probes return `CALL-1`, `ORD-1`, `note` from padded text (`receipt.rs:719–757`). [Receipt order-number documentation][receipt-order] says the response returns the same sent value; unrestricted XSD strings preserve whitespace. No public per-field normalization contract or receipt live probe establishes this reduction as necessary.

Retain P3 loss of nonempty text, strongest for identifiers, and distinguish it from deliberate blank → `None`. This is not proof the service stores two distinct padded receipt identities. `behaviour:40–42` establishes **invoice** order trimming/exact query semantics, not receipt call-id or free-text normalization. It does not justify trimming every optional string. Conversely, do not “fix” the worker's deliberate trimmed order projection under this finding.

Scope is wider than receipts (`query_xml.rs:715–726,1040–1043`), but not universal: required names are preserved, and `empty_invoice_number` already retains nonblank original text (`1064–1070`). Separate string fidelity from numeric whitespace handling rather than changing the generic helper indiscriminately. Unicode `str::trim` also removes more than XML's four whitespace characters; an exact-text policy should account for that.

### 7. Documentation decisions

- **F-04/W05-02:** `error.rs:265–270,368–370` and `client.rs:69–72` prescribe invoice external-id recovery for receipts. [Receipt query][receipt-query] supports number/order; the create [call id][receipt-response] prevents duplicates but reuse returns 338, not the original success. Correct the guidance, keep a stable original call id, and do not invent call-id-only lookup or invoice-style receipt storno replay. Receipt order lookup with duplicates is not documented as deterministic. P3, not an implemented retry bug. The same overbroad advice also needs operation-specific treatment for deletion/credit-entry mutations. The official ceiling is **five**, not “~5”.
- **QR-03:** `query_xml.rs:511` says the amount arrived **on** the account. The official [Hungarian annotation][bank-meaning] says it arrived **from** that account, or falls back to the account printed on the invoice if the sender's is unknown. Correct the description with the fallback; neither unconditional payer nor unconditional recipient is accurate. No mapping change.
- **F-05:** `item.rs:20–26` and README:215 generalize observed **EUR invoice** independent rounding to shared line items. [HUF receipt rules][receipt-amounts] require whole gross, net/VAT at most two decimals, and exact sum. The `Scale(2)` → `1/0.27/1.27` wire reproduction shows no local guard, as deliberately documented (`item.rs:70–79`); it does not show a broken calculator. Scope the prose; retain explicit raw/exact amounts and permitted fractional net/VAT. `behaviour:249–254` even leaves fractional HUF invoice rounding unverified.
- **O4-04:** `proforma.rs:1–2` says “unpaid”; `behaviour:110` directly records paid deletion. Remove the misleading qualifier. It is not an explicit guard guarantee, so the raw report's hypothetical caller reliance should not inflate severity. Adding a balance check to the low-level operation is unwarranted.

## Exclusions and remaining questions

- **Excluded intentional/live-backed behaviour:** body-only errors and header-free delete success (`behaviour:109,135,141–145`); storno repeat/no-op/external-id semantics (`63–70,86–98`); appearance codes; observed invoice rounding; lenient absent reversal/date/test fields. Their implementation deviations are not bugs. The paid-delete and rounding documentation findings correct prose to respect that evidence, not reverse it.
- **Chosen scope:** v2-only operations, open response tokens, finite Decimal money, wider signed ids, no duplicated arithmetic validation, refusal of unsupported receipt item fields, and no empty replacing-credit operation. None becomes a defect merely because an XSD permits more shapes. Ordinary exponent notation works; NaN/extreme-domain arguments do not establish useful compatibility failures.
- **Receipt MNB automatic rate (03 Q-A):** unverified extension, not demonstrated failure and not live-backed exclusion. The receipt response page asks for bank **and rate** for foreign currency; emitted omission alone proves neither server support nor rejection. Qualify the promise or seek separate evidence before changing behaviour.
- **03 Q-B–D:** call-id-only query, send-email partial defaults/recipient syntax, and order-lookup selection with duplicates remain unestablished. No missing feature or wrong runtime result proved. Source/XSD download drift is a provenance issue; do not remove fields supported by current inline schemas.
- **Other boundaries:** no established Számla Agent forwarding of direct NAV `GeneralErrorResponse`, or non-2xx body-only codes requiring different precedence. Negative-original/positive-storno sign handling remains an unverified extension of the live-tested heuristic (`behaviour:79,87,224–225`), not an adjudicated bug here.

## Verification record

No production edits, live account operations, or delegation. Read the relevant behaviour evidence directly. Fresh public GETs rechecked the sources linked below; read the existing downloaded NAV specification §1.8.9 at `/tmp/opencode/04-nav-spec.txt:2895–3022`. Reviewed scratch sources before executing them against this checkout:

```sh
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/04-agent-audit/Cargo.toml
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/query-response-audit-20260909/Cargo.toml --bin query-response-audit
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/receipt-conformance-20260909/Cargo.toml
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/wire-conformance-20260909/Cargo.toml
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/query-response-audit-20260909/Cargo.toml --bin adjudication
```

All ran successfully; the last is a new scratch assertion program checking counter-cases (header-error precedence, insufficient truncated content, valid entities/CDATA, absent/blank/body-present amount precedence), the unexposed-outstanding interaction, and the thirteen body-error classifications. Query scratch resolves Jiff 0.2.35, quick-xml 0.42.0, Decimal 1.43.0.

These are parser probes, not live-service or full-XSD validation. The receipt fixture has invoice-style amount aliases; the supplementary run replaces them with canonical receipt tags and still reproduces the date failures. I did not rerun the reports' full test suites or claim their previously reported counts as new verification.

[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[xsd-date]: https://www.w3.org/TR/xmlschema-2/#date-lexical-representation
[xsd-space]: https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace
[errors]: https://docs.szamlazz.hu/agent/basics/error-handling#error-codes-and-messages
[receipt-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[pdf-response]: https://docs.szamlazz.hu/agent/querying_pdf/response
[taxpayer-response]: https://docs.szamlazz.hu/agent/querying_taxpayer/response
[nav-api]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1552-L1581
[nav-spec]: https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf
[receipt-order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number#where-the-order-number-appears-in-the-response
[receipt-query]: https://docs.szamlazz.hu/agent/querying_receipt/request#requirements
[receipt-amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[bank-meaning]: https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak#kimenő-számla-feladás-xml
