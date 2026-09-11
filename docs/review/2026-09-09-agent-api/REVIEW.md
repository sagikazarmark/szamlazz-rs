# szamlazz-agent against the current Számla Agent documentation

> **Superseded by the [round-two final report](FINAL.md) and [independent judge](round-2/JUDGE.md).** This first-round report is retained as the review history; the second round rechecks every finding, adds confidence ratings, and resolves proposed-solution disagreements.

**Reviewed:** 2026-09-09 · **Revision:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`

## Verdict

**The crate substantially implements the documented Számla Agent surface, but is not complete.** All eleven operations have the correct endpoint/action, request roots, namespaces and supported-field ordering. The queried invoice and receipt models cover their published document fields. The review found:

- **Three functional defects:** valid XSD date spellings rejected; thirteen documented error codes left inconclusive; comma-decimal monetary-header fallback broken.
- **Three capability gaps:** invoice `simpleItems`, PDF-query balance/customer URL, and five taxpayer business-data fields.
- **Two lower-priority parser concerns:** malformed taxpayer-response acceptance and optional-string whitespace loss.
- **Nine public-documentation corrections**, including incorrect TAHK meaning and incomplete `eusAfa` semantics.

No critical/high-severity defect was established. The functional failures were reproduced offline; their live occurrence is qualified below. Existing tests pass, but do not cover these cases. **Intentional behavior supported by the recorded live tests is not counted as a defect.**

### Read this report first

This is the deduplicated, adjudicated result. The five specialist reports contain the field inventories, source quotations and reproduction details; their initial severity/classification judgments are superseded here where they differ. [Independent adjudication](ADJUDICATION.md) explains the functional decisions and counter-checks.

## Scope and evidence

Five parallel subagents reviewed invoice requests; XML/PDF queries; receipts; storno/credit entries/proforma deletion/taxpayer lookup; and transport/envelopes/errors. A sixth independently adjudicated the functional findings. The coordinating review checked the principal implementation paths and re-fetched the disputed invoice-feature/VAT documentation.

Sources were fetched from **docs.szamlazz.hu during this review**, including current inline XSDs, linked downloadable XSDs, Hungarian originals where meanings were disputed, the linked NAV specification/schema, and first-party PHP source for code 56. Current pages generally report build `v202608271632`; older still-accessible pages report older builds. Each specialist report records the actual URLs and source conflicts.

Repository evidence includes `docs/szamlazz-hu-behaviour.md`, relevant decisions in `CONTEXT.md`/ADRs, source, tests and `fixtures/SOURCES.md`. Cached fixtures were navigation/regression aids, not proof that current official definitions were unchanged. Live observations are bounded to the account/dates documented in that behavior record; no new live Számla Agent operation ran.

**Priority:** P2 = normal-priority functional/capability work or consequential documentation; P3 = narrower coverage, robustness, fidelity or documentation work. These are fix priorities, not assertions about incident frequency. Code line references below are at the reviewed revision, under `crates/szamlazz-agent/src/` unless otherwise stated.

## 1. Functional defects

### F1 · P2 — Valid XSD dates reject otherwise usable responses

**Locations:** `xml.rs:269–283`; `ops/query_xml.rs:703–708,810–825,957–960,994–997,1034–1039`; `ops/receipt.rs:735`.

The [queried-invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) and [receipt response schema](https://docs.szamlazz.hu/agent/generating_receipt/response) use `xs:date`. [Its lexical definition](https://www.w3.org/TR/xmlschema-2/#date) permits an optional timezone. The parser instead delegates to Jiff's narrower civil-date reader.

- An otherwise valid invoice response with `<kelt>2026-09-09Z</kelt>` or `2026-09-09+02:00` fails in full. The credit-entry date fails similarly.
- Receipt `kelt` fails for those forms and for schema-valid surrounding XML whitespace, such as ` 2026-01-01 `.
- Invoice query loses a read result; receipt create/storno can lose a successful issuance answer and become an unknown outcome.

**Confidence:** high, independently reproduced. **Live limit:** the examples and recorded queries use plain dates; no live suffixed-date failure was observed. This is not a criticism of accepting absent optional dates or of the separate Adatkapcsolat date policy.

**Fix:** explicit required/optional XSD-date adapters that validate the whole lexical form and document projecting to the printed civil date. Cover all eleven invoice date positions and the shared receipt parser. Test offsets, XML whitespace, malformed suffixes and multibyte text; never truncate blindly at byte 10.

Details: [query QR-01](raw/02-query-responses.md#qr-01--p2-timezone-qualified-schema-dates-reject-the-document), [receipt F-01](raw/03-receipts.md#f-01--valid-xsdate-receipt-replies-are-refused).

### F2 · P2 — Thirteen documented errors are classified as unknown outcomes

**Locations:** `error.rs:220–253,314–348`.

The current [general catalogue](https://docs.szamlazz.hu/agent/basics/error-handling) and [receipt supplement](https://docs.szamlazz.hu/agent/generating_receipt/response) document these codes, but all become `ErrorCode::Unknown` / `OutcomeClass::Unknown`:

| Codes | Meaning | Expected handling |
|---|---|---|
| 336, 337 | Receipt prefix already used for invoices / invalid prefix | `Rejected` |
| 339 | Receipt does not exist | `NotFound`, with class documentation extended to receipts, or equivalent explicit handling |
| 340 | Receipt tender total differs from gross | `Rejected` |
| 363, 364, 365 | HUF receipt gross not whole / net or VAT exceeds two decimals | `Rejected` |
| 551–556 | Simplified-image account, item-count, VAT, correction and document-type restrictions | `Rejected` |

This is more than missing enum conveniences: callers using the outcome helper cannot distinguish known refusals from possible issuance. **The behavior is safely conservative**, preserves code/message, and causes no automatic resend; it can nevertheless cause unnecessary reconciliation and delayed reporting of bad input.

**Reach:** 554 is relevant today even without crate support for `simpleItems`: the docs explicitly prohibit correcting a simplified-image original even when the corrective request omits that flag. Other simplified-image codes concern feature completion and inherited settings; not thirteen independent observed incidents.

**Fix:** complete the source-derived mapping and test actual invoice/receipt body errors, representative header errors, and a genuinely future code that must remain unknown. Preserve the distinction between a duplicate receipt call (338) and recovery of the original result.

Details: [complete catalogue and W05-01](raw/05-wire-errors.md#explicit-catalogue-to-codeclass-comparison). The seven receipt codes in the receipt report are included in these thirteen, not additional findings.

### F3 · P2 — Monetary-header fallback cannot read an observed live format

**Locations:** `ops/envelope.rs:146–155,205–225,285–303`; `ops/credit_entry.rs:253–274`.

The live P60 evidence records **`szlahu_nettovegosszeg: 100,01`** (`docs/szamlazz-hu-behaviour.md:160`). The shared fallback passes this directly to Decimal, which rejects it. The [structured reply schema](https://docs.szamlazz.hu/agent/generating_invoice/response) allows optional body totals, and the crate explicitly supports header fallback.

**Complete trigger:** a numbered success, a comma-decimal monetary header, and its corresponding body amount absent, empty or blank. A valid body amount takes precedence and masks the issue. Offline probes reproduced failure on PDF net and storno/credit gross; creation shares the same helper. The code-56 warning path drops an unreadable optional amount instead of failing issuance.

**Impact:** a completed write can become a parse failure; a PDF query can fail on auxiliary metadata, including outstanding amount that its public result then discards.

**Confidence:** high for the code failure and recorded create-header format. **Live limit:** no recorded call combines the missing body amount with the comma header; the format was observed on create net, not independently on every operation/header. Ordinary fractional replies with valid XML body totals are unaffected.

**Fix:** a monetary-header-specific parser accepting dot and single decimal comma forms, with explicit rejection of ambiguous grouping/mixed separators. Keep XML `xs:double` parsing separate. Test body precedence, signed values, all three monetary headers, and the warning path.

Details: [other operations O4-02](raw/04-other-operations.md#o4-02--header-totals-reject-szamlazzhus-observed-comma-decimal-separator), [adjudication §3](ADJUDICATION.md#3-comma-monetary-headers-eligible-with-the-full-trigger). This finding respects the live-testing exception: it asks the parser to handle the observed behavior, not to remove an intentional deviation.

## 2. Documented capabilities not exposed

| ID / priority | Gap and evidence | Code / recommended direction |
|---|---|---|
| **C1 · P2** | **`simpleItems` is missing.** The [current tour-operator documentation](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) requires this per-document boolean to enable the simplified invoice image. Account UI configuration is not a substitute. No live-backed exclusion was found. | `ops/invoice.rs:138–187,758–768`. Add an optional header field and describe per-kind rules/inheritance. Resolve the simultaneous-preview ordering conflict below before claiming schema conformance for both options. [IR-01](raw/01-invoice-requests.md#ir-01--missing-simpleitems-request-support). |
| **C2 · P3** | **PDF result drops outstanding amount and customer account URL.** Both are optional elements in the [PDF operation's own reply schema](https://docs.szamlazz.hu/agent/querying_pdf/response), already read by the shared envelope. | `ops/query_pdf.rs:36–48,75–83`. Expose optional `outstanding` / `customer_account_url` and carry them through the projection. Test body/header/absent cases. Fetching a valid PDF otherwise works; actual emission of these optional fields on this operation is unverified. [QR-02](raw/02-query-responses.md#qr-02--p2-invoicepdf-loses-two-fields-explicitly-documented-for-pdf-queries). |
| **C3 · P3** | **Taxpayer result exposes only a subset:** omits `countyCode`, `vatGroupMembership`, `incorporation`, `taxpayerShortName`, `infoDate`. The [Számla Agent response page](https://docs.szamlazz.hu/agent/querying_taxpayer/response) links the NAV model; `infoDate` is already in its 2.0 example. | `ops/taxpayer.rs:110–127,187–199,317–354`. Add optional business fields or explicitly accept/document the subset. County/VAT-group/economic type have the strongest utility. The other four fields' current forwarding through Számla Agent remains unverified. NAV 3.0 namespaces and multiple addresses already work. [O4-03](raw/04-other-operations.md#o4-03--taxpayer-business-data-coverage-stops-short-of-the-linked-nav-model). |

C2/C3 are **coverage gaps, not failures to parse the currently exposed fields**. NAV does not require every client to consume every field. The general client and the worker's intentionally narrow journal projection are separate scope decisions. A custom `AgentRequest` can retain raw metadata through the bundled client, but that does not fill the built-in result types.

## 3. Lower-priority parser work

### R1 · P3 — Taxpayer parsing does not enforce one complete, structurally scoped document

**Locations:** `ops/taxpayer.rs:202–285,287–338`; shared trailing-content boundary at `xml.rs:63–108,152–176` and `ops/envelope.rs:259–264`.

Independently reproduced:

- A body ending after complete `OK`, `taxpayerValidity=true` and taxpayer-name elements, but before closing the document, returns success.
- A complete body-only `ERROR/57` followed by a second XML root containing `OK/true` becomes success.
- An unknown container containing a recognized leaf can overwrite the real name; descendant namespaces are not checked.
- Undefined `&bogus;` is silently removed: `A&bogus;B` becomes `AB`.

The [documented NAV structure](https://docs.szamlazz.hu/agent/querying_taxpayer/response) does not allow these shapes. **These are robustness defects, not evidence of normal taxpayer replies being misread.** A header error still wins. HTTP-detected truncation returns a transport error before parsing; the incomplete XML must arrive as a successfully collected body or directly as `RawResponse`.

Shared serde parsers also ignore a trailing second root, but do not share the taxpayer overwrite algorithm; truncated deletion XML correctly fails. Do not generalize failure-to-success behavior to every operation.

**Fix:** complete-document checking, path/expanded-name-aware NAV extraction, atomic skipping of unknown subtrees, and explicit undefined-entity refusal. Preserve actual NAV 2.0/3.0 namespace layouts, sparse content, standard entities, CDATA and legal surrounding comments. [Reproductions and adjudication](ADJUDICATION.md#4-taxpayer-malformed-acceptance-robustness-not-demonstrated-core-conformance-failure).

### R2 · P3 — Generic optional-scalar parsing loses nonempty string whitespace

**Locations:** `xml.rs:269–283`; e.g. receipt identifiers at `ops/receipt.rs:719–757`.

`<hivasAzonosito> CALL-1 </hivasAzonosito>` becomes `CALL-1`, and the same trimming affects optional order numbers, comments and other strings. These are `xs:string`, not whitespace-collapsing tokens; the [receipt order-number docs](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) also promise the sent value back. The writer preserves the input text.

**Narrow conclusion:** demonstrated loss of response fidelity, not proof that the server stores distinct padded receipt identities. Invoice order-number live normalization does not justify receipt call-id/free-text normalization. Preserve nonempty string text separately from blank-to-`None` detection and numeric whitespace parsing, or make the normalization a deliberate documented interface contract. [Receipt F-03](raw/03-receipts.md#f-03--nonempty-receipt-identifiers-and-optional-text-are-silently-trimmed).

## 4. Public documentation corrections

These affect guidance, not emitted XML token mappings. No new local business-state validation is implied.

| ID / priority | Correction | Code and evidence |
|---|---|---|
| **D1 · P2** | `VatRate::Tahk` describes TAM's exemption. TAHK means **outside VAT subject-matter scope**, not exemption due to public-interest/special activity. A caller can choose the wrong zero-VAT category from the current description. | `types.rs:194–196`; [official VAT list](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), corroborating Hungarian text/vendor PDF in [IR-02](raw/01-invoice-requests.md#ir-02--vatratetahk-documents-the-wrong-vat-category). Keep the TAHK wire token. |
| **D2 · P2** | `eu_vat` does more than label another member state's VAT: accepted `eusAfa=true` suppresses NAV Online Invoice submission, has seller prerequisites, and does not replace item VAT codes. | `ops/invoice.rs:181–182`; [official field semantics](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates#the-eusafa-field). Expand the field docs; no unverified local OSS check. |
| **D3 · P3** | Receipt recovery cannot use invoice-only `szamlaKulsoAzon`. Describe stable receipt call IDs, 338 duplicate prevention, and the supported receipt/order-number queries. Do not promise call-id-only lookup or recovery of the original success. | `error.rs:265–270,368–370`; `client.rs:69–72`; [receipt query](https://docs.szamlazz.hu/agent/querying_receipt/request), [receipt result](https://docs.szamlazz.hu/agent/generating_receipt/response). [Adjudication §7](ADJUDICATION.md#7-documentation-decisions). |
| **D4 · P3** | Shared `Rounding::Exact` guidance generalizes observed invoice rounding to receipts. HUF receipt gross must be whole, net/VAT at most two decimals, with exact sum; do not promise server repair. | `item.rs:20–26`; [receipt amount rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts). Raw/exact construction remains intentional; fractional receipt net/VAT can be valid. |
| **D5 · P3** | Credit-entry bank account is the **sender's** when known, otherwise the account printed on the invoice; not always the receiving account. | `ops/query_xml.rs:511–512`; [official Hungarian annotation of the shared document](https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak). |
| **D6 · P3** | “Unpaid proforma” implies a restriction absent from the operation. A paid proforma can be deleted. | `ops/proforma.rs:1–2`; [deletion request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), live D3 at `docs/szamlazz-hu-behaviour.md:110`. Change prose, not the low-level operation. |
| **D7 · P3** | Explain `Buyer::id` identity reuse: the vendor documents partner-data updates and access to that partner's documents through the customer account link. | `ops/invoice.rs:288–289`; [warning following the official XML example](https://docs.szamlazz.hu/agent/generating_invoice/xml), [IR-04](raw/01-invoice-requests.md#ir-04--buyerid-hides-the-documented-partner-identity-consequences). The current sentence is not an explicit isolation guarantee, but omits consequential semantics. |
| **D8 · P3** | Remove stale “37” currency count; current list has 46 currencies / 47 tokens including `Ft`. The open type already represents all of them. | `types.rs:361–364`; [current currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies). |
| **D9 · P3** | Erasure-code guidance bundles the wrong conditions: 537 is the count limit, 538 forbids demo/test accounts, 539 is disabled account configuration. No cited source assigns that range to wrong template selection. | `item.rs:107–111`; [error catalogue](https://docs.szamlazz.hu/agent/basics/error-handling), [IR-06](raw/01-invoice-requests.md#ir-06--erasure-code-rustdoc-misattributes-538-and-omits-test-account-restriction). |

Smaller wording/provenance notes remain in the specialist reports: the exact five-send ceiling rather than “~5”; code 55's uncertain outcome versus an unproven “issued, signing failed” assertion; statements suggesting every named error was observed live; and raw-response prose claiming body-before-status precedence when the actual implementation checks only error/down headers before status. No observed protocol failure was established from these alone.

## 5. What conforms, and why apparent deviations were excluded

| Area | Result |
|---|---|
| All eleven operations | Correct common endpoint, POST multipart action names, operation roots/namespaces, credential placement and supported response-version selection. |
| Invoice requests | All supported settings, six creation kinds, references, seller/buyer/ledger fields, items and carrier blocks checked. **Only missing current XSD field identified: `simpleItems`.** Full generated requests for all six kinds validated against both current inline language schemas. |
| XML/PDF queries | All three selectors, including their operation-specific ordering, conform. Queried `szamla` model covers every current XSD element, including financial items, totals, party variants and all seven credit-entry fields. |
| Receipts | All four request operations and all declared receipt response fields covered. Correct distinct amount names, tender payments, selectors and send-email block semantics. Ordinary PDF decoding and NY/SN metadata work. |
| Storno, credit entries, deletion | Current documented request fields/order covered. Correct lowercase deletion `rendelesszam`, five-entry credit limit, additive/replacing flag and body-only errors. |
| Taxpayer | Eight-digit prefix gate correct. NAV 2.0/3.0 namespace layouts, validity=false as data, symbolic errors and multiple detailed addresses work. Selected result subset noted in C3. |
| Error 56 | Number-dependent successful issuance agrees with current first-party PHP 2.12.4 source. Missing number remains unknown. This is first-party corroboration, not a newly observed live response. |

Excluded after comparison with the live record and explicit design decisions:

- External IDs are nonunique, resolve to the newest holder and are not echoed. A storno's external ID belongs to its SS. Repeat invoice storno can echo the existing SS; proforma/delivery-note storno can be a success-shaped no-op.
- Storno request appearance and fulfillment date follow the documented/probed wire behavior; the worker's stronger derivation from the original is a separate layer. Queried appearance is an integer code, not a boolean.
- Body-only query/credit errors, header-free deletion success, sparse optional response fields, and missing reversal markers before reversal are accepted correctly.
- Invoice rounding/tolerance and final-invoice negative prepayment lines are supported by P60/C6 evidence. No automatic server netting was invented.
- Version 2 only, open wire-token sets, finite Decimal money, wider signed IDs, and leaving arithmetic/account rules to the server are deliberate boundaries. Ordinary scientific notation already parses.
- Refusing an empty replacing credit-entry request and refusing invoice-only fields on receipt items are explicit restrictions, not silent data loss.

The full evidence reconciliation is in the raw reports and [adjudication](ADJUDICATION.md#exclusions-and-remaining-questions).

## 6. Vendor-source conflicts and unresolved questions

### Do not refresh schemas blindly

| Source | Invoice `csoportazonosito` / `torloKod` | Invoice header tail |
|---|---|---|
| Current English/Hungarian inline XSD | Both present | `szamlaSablon`, **`simpleItems`**, `elonezetpdf` |
| Current downloadable XSD | Both absent | `szamlaSablon`, `elonezetpdf`, **`simpleItems`** |
| Cached workspace XSD / legacy inline | Both present | `szamlaSablon`, `elonezetpdf` |

The download rejects existing group-id/erasure-code fields supported by current inline definitions; those rejections are **not serializer bugs**. `simpleItems` plus preview ordering cannot be settled by comparing freshness alone. Seek vendor clarification or separately authorized evidence for that combination.

Receipt downloads likewise omit current inline `torloKod` / query `rendelesSzam`, and VAT lists disagree about `TEHK`. Several example schemaLocation URLs return 404. Current storno/credit response pages now include structured examples absent from the older corpus; the taxpayer page now links NAV's specification. Refresh fixture provenance selectively, preserving known corrections. [Invoice source comparison](raw/01-invoice-requests.md#d-01--there-is-no-single-mutually-consistent-current-invoice-xsd), [receipt sources](raw/03-receipts.md#downloaded-schemas-and-broken-links), [other-operation drift](raw/04-other-operations.md#drift-from-the-cached-source-map).

### Questions that remain unverified, rather than confirmed defects

1. **Automatic MNB rate on foreign receipts:** implemented/tested locally, but only invoice omission behavior is explicitly documented; no receipt live evidence establishes it. Qualify the shared promise or obtain confirmation.
2. **Receipt recovery semantics:** call-id-only lookup, query call-ID meaning, order lookup selection with duplicates/after reversal, and send-email partial defaults/multiple recipients are not established. Invoice observations cannot answer them.
3. **Positive-gross storno of a negative original:** `CreatedInvoice::reverses` uses a sign heuristic based on positive-original probes. The wider case remains unverified, not proven impossible or broken here.
4. **Currency storage beyond HUF/EUR**, explicit false versus omitted `fizetve`, and obscure account-dependent fields remain unprobed. Do not generalize the one-account observations.
5. **Direct NAV error roots or non-2xx body-only Agent errors:** no evidence establishes their forwarding by Számla Agent. Current status/header precedence is not judged against hypothetical forwarding.

## 7. Verification and recommended sequence

The subagents executed these checks successfully; overlapping runs are not summed into an inflated test total:

| Check | Result |
|---|---|
| `cargo test -p szamlazz-agent --lib --test upstream` (also locked/offline in one review) | **178 library + 9 corpus tests passed** |
| `cargo test -p szamlazz-agent --features client-reqwest --lib --test client` | **179 library + 6 client tests passed** |
| Focused invoice/item/type/request-corpus tests | Passed; overlap the suites above |
| Six full creation-kind requests against freshly fetched English/Hungarian inline XSDs | Passed; downloadable-XSD disagreement reproduced |
| Independent scratch parser probes and counter-cases | Confirmed date/code/header failures, metadata omission, whitespace loss and malformed taxpayer behavior; dot/exponent, ordinary dates, valid entities/CDATA, body precedence and error-header controls passed |

These are targeted unit/integration/corpus checks, not a complete workspace/CI run. Receipt and other-operation fresh-schema comparisons were field-by-field review, not automated XSD validation. No live/ignored tests ran. Temporary probes live under `/tmp/opencode`; the reports preserve triggering values and commands. This review added only review documents. Concurrent changes elsewhere in the workspace appeared during the session; the final diff check showed no changes under `crates/szamlazz-agent`, and this report remains tied to the revision above.

**Recommended order:**

1. Fix F1–F3 and add the source-derived regression cases; correct D1/D2/D3/D4/D6 alongside them where practical.
2. Add C1 with the schema-order conflict explicitly resolved; complete related error coverage from F2.
3. Decide whether to expose C2/C3 fully; keep the worker's narrower projection separate.
4. Address R1/R2, finish remaining documentation corrections and refresh current-source fixture coverage/provenance.
5. Resolve the specific unverified receipt/source questions before advertising stronger support. Existing passing goldens cannot answer them.

## Specialist reports

- [01 — Invoice requests, all fields/kinds/types/carriers](raw/01-invoice-requests.md)
- [02 — XML/PDF queries and complete queried-invoice model](raw/02-query-responses.md)
- [03 — Receipt create/storno/query/send](raw/03-receipts.md)
- [04 — Storno, credit entries, proforma deletion, NAV taxpayer lookup](raw/04-other-operations.md)
- [05 — Transport, authentication, sessions, envelopes and full error catalogue](raw/05-wire-errors.md)
- [Independent functional adjudication](ADJUDICATION.md)
