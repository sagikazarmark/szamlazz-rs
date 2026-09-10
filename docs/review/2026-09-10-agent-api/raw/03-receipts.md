# Independent receipt documentation-conformance review

**Date:** 2026-09-10. **Scope:** `szamlazz-agent` receipt create, reverse, query and send; receipt-specific shared item, value-type, verdict, PDF and transport behavior.

## Conclusion

All four operations expose the current documented request fields and response payloads. Their actions, roots, namespaces, required containers, ordinary field ordering, PDF options, tender rows and email resend serialization match the current receipt documentation. **One remaining implementation defect was independently reproduced:** whitespace-padded numeric VAT rates are returned as an unknown token by the typed response helpers, despite being valid numeric values under the response schema. This can become zero VAT if the helper's result is passed to the shared calculator.

The principal receipt defects and documentation corrections in the historical `2026-09-09-agent-api/FINAL.md` are now implemented. They are recorded below as verified closures, not repeated findings. Source disagreements remain, including two operational documentation conflicts concerning the order-number toggle and receipt reporting rollout. Neither establishes another Rust implementation defect.

No live account calls, delegation, production changes or edits to historical reports were made. The uniquely named offline probe was removed after use. This report is the only lasting file added by this review.

### Baseline and provenance

The inspected working tree initially reported HEAD `f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3`. Other work was concurrent; the parent review README's baseline is not substituted for this inspection. Git blob hashes of the inspected files pin the evidence more precisely:

| File under `crates/szamlazz-agent/src/` | Inspected Git blob hash |
|---|---|
| `ops/receipt.rs` | `01007fe775bdd949b4cad1122a1b0a22dddd3510` |
| `item.rs` | `a14bcf0686912613e306cf025faf64ed266bf761` |
| `types.rs` | `4176b23cea254b808b5614f17d93a14cdbfade02` |
| `xml.rs` | `ffffa2052da6626b951b277060ad1ae9a102fc3c` |
| `error.rs` | `3771f19d577482481ce22d21d2e31a6ba92c10c5` |

Code citations below use those files and their current line numbers, not historical report lines. `src/` means `crates/szamlazz-agent/src/`; `tests/` means that crate's tests. The documentation pages were fetched afresh using unauthenticated GETs. Their displayed site build was `v202608271632`; that is not a claim that every paragraph was updated on that date.

## 1. Finding

### R-01 — P3: valid numeric VAT whitespace is misclassified by receipt response helpers

**Category:** implementation / numeric interpretation. **Confidence:** high in the code behavior and schema mismatch; high in the bounded remedy; no evidence of live vendor emission of this spelling.

**Current code:**

- `src/ops/receipt.rs:614–616,637–643,809–817`: `afakulcs` is preserved as a raw `String`; `ReceiptItem::vat_rate()` passes it directly to `VatRate::from` when no special `afatipus` is present.
- `src/xml.rs:530–546` and `src/types.rs:1064–1085`: the shared VAT subtotal follows the same path through `VatTotal::vat_rate()`.
- `src/types.rs:288–315`: numeric conversion calls `other.parse::<Decimal>()` without numeric XML-whitespace handling, then falls back to `Other`.
- `src/item.rs:191–198`: the calculator calculates percentage VAT only for `VatRate::Percent`; every other variant yields zero.

**Official evidence:** both the current [inline receipt response schema][C-response] and [downloaded receipt response schema][X-response] define the item and subtotal `afakulcs` using `<restriction base="double">` with `<minInclusive value="0">`. [XML Schema Datatypes §4.3.6][W-space] says, “For all atomic datatypes other than string … the value of whiteSpace is collapse”; its definition removes “leading and trailing” spaces after replacement of XML tabs/newlines. Thus `<afakulcs> 27 </afakulcs>` and a tab/newline-padded 27 denote the same percentage as `27`.

**Offline reproduction:** parse the synthetic receipt response after replacing the second row's invoice-style monetary aliases with canonical receipt names. Substitute the first item's `<afakulcs>27</afakulcs>` and call `items[0].vat_rate()`:

| Wire text | Actual typed result |
|---|---|
| `27` | `Percent(27)` |
| `27.0` | `Percent(27.0)` |
| ` 27 ` | `Other(" 27 ")` |
| tab + `27` + newline | `Other("\t27\n")` |
| `2.7E1` | `Percent(27)` |
| `+27` | `Percent(27)` |
| `TEHK` | `Other("TEHK")`, correctly preserved as an open token |

Removing the subtotal's special `afatipus` and changing its numeric value to ` 27 ` also returned `Other(" 27 ")`. Passing the first padded result to `LineItem::try_calculated("copy", 1, "db", 100, rate, Rounding::Scale(0))` produced **VAT 0**, independently asserted in the probe. The incoming stored amounts themselves remain correct; only interpretation/recalculation is affected.

**Concrete impact:** a caller grouping receipt rows by typed VAT rate gets an unexpected category. A caller constructing a later document from the helper can derive zero VAT from an actual 27% row. Server acceptance of that derived request was not tested and is not claimed. This is a conditional downstream impact, not a demonstrated production accounting incident.

**Bounded recommendation:** preserve `vat_rate_code` exactly, while parsing the *numeric response branch* after trimming only XML whitespace (`SP`, `HT`, `CR`, `LF`). Retain special-code precedence and unknown strings verbatim; do not reintroduce universal trimming of business text or require a new closed VAT catalogue. Share the numeric interpretation between `ReceiptItem::vat_rate` and `VatTotal::vat_rate` (and inspect the invoice helper using the same type). Add source-derived tests for padded percentages, exponent/leading-plus controls, and unchanged special/unknown tokens. This need not change response fields or the calculator's explicit non-percentage policy.

## 2. Complete source acquisition inventory

All links in the following tables were fetched in this session unless expressly marked as failed. XML examples and inline schemas were read from the current combined `/xml` and `/response` pages, rather than assumed from the July fixture corpus. All 12 receipt request/XML/response pages and all five rule pages were additionally fetched in the Hungarian locale (`/hu/agent/` plus the same suffix); no additional request or response field was found there. The English and Hungarian example labels/content differ in places, so they are not byte-identical fixtures.

### Receipt operation pages and examples

| Surface | Current official sources | Coverage |
|---|---|---|
| Create | [Request][C-request], [XML + XSD][C-xml], [Response + XSD][C-response] | Multipart HTML form; complete request example; request schema; receipt success example and envelope/document schema; 336–340 supplement; PDF and call-ID semantics |
| Reverse | [Request][R-request], [XML + XSD][R-xml], [Response][R-response] | Multipart HTML form; complete reverse request example and sequence schema; shared reply, SN identity, PDF; missing/already-reversed/SN-target error descriptions |
| Query | [Request][Q-request], [XML + XSD][Q-xml], [Response][Q-response] | Multipart HTML form; number/order selector example, optional call ID/template, inline schema; shared receipt reply |
| Send | [Request][S-request], [XML + XSD][S-xml], [Response + XSD][S-response] | Multipart HTML form; complete email request and sequence schema; success/error acknowledgement examples and schema |
| Receipt rules index | [Settings and rules][rules] | Checked the full linked receipt rule list, including the current reporting notice |
| Rule pages | [Order number][order], [PDF template][template], [Erasure codes][erasure], [Amounts][amounts], [NAV reporting][nav] | Included the order header/alap examples, amount examples, all template codes, account rules and published capabilities |

Reverse and query response pages do **not** supply independent document examples or distinct response XSDs. Reverse now contains substantial response/error prose; query refers to create. The receipt-create example is illustrative, not a capture: it has an invalid base64 placeholder, contradictory NY/reference metadata, inconsistent totals/tenders, and mixed monetary element names.

### Independently fetched downloadable schemas

| Schema | URL | Current comparison |
|---|---|---|
| Create | [xmlnyugtacreate.xsd][X-create] | Same ordinary fields; **omits `torloKod`** present in EN/HU inline schema and feature docs |
| Reverse | [xmlnyugtast.xsd][X-reverse] | Same fields/order; header sequence is number, template, call ID |
| Query | [xmlnyugtaget.xsd][X-query] | **Omits `rendelesSzam`** present in current EN/HU inline schema and selector prose |
| Send | [xmlnyugtasend.xsd][X-send] | Same sequence/optionality; email block optional |
| Create/reverse/query reply | [xmlnyugtavalasz.xsd][X-response] | Same modeled fields; enum includes `TEHK`, absent from current inline VAT enum |
| Send reply | [xmlnyugtasendvalasz.xsd][X-send-response] | Three-field verdict; no receipt or PDF payload |

The four legacy URLs embedded in example `schemaLocation` values were also attempted, with HTTPS transport: `/docs/xsds/nyugtast/xmlnyugtast.xsd`, `/docs/xsds/nyugtaget/xmlnyugtaget.xsd`, `/docs/xsds/nyugtasend/xmlnyugtasend.xsd`, and `/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd` at `www.szamlazz.hu` all returned **404**. The working downloads above include `/szamla/`. An initial guessed `/agent/generating_receipt` URL returned 403; the actual navigation pages above were available. No required operation page was left unread because of that guess.

### Supporting official sources

| Source | Why consulted |
|---|---|
| [Sending requests][requests], [Authentication][auth], [Session cookies][cookies], [Error catalogue][errors] | All four action names, one XML/document, multipart file requirement, auth alternatives, cookie handling, general refusals and five-total-send limit |
| [Supported currencies][currencies], [VAT rates][vat] | Receipt-linked currency list and exchange fields; shared open VAT vocabulary. Invoice-only `eusAfa` is not a receipt field |
| [PHP receipt generation][P-create], [reverse][P-reverse], [send][P-send], [data query][P-query], [PDF query][P-pdf] | Every receipt PHP capability table and inline example; number/order “last matching document”; convenience defaults versus wire requirements |
| [Official PHP 2.12.4 ZIP][P-zip] | Independently fetched and inspected all seven `examples/document/receipt/*.php` files and `src/szamlaagent/Header/ReceiptHeader.php`; automatic MNB provenance, erasure count, writer sequences |
| [Erasure knowledge base][K-erasure] | Count rather than identifier, 400/item, account setting, stock/vendor sourcing; `SzlaMost` instruction concerns invoices |
| [Order-number knowledge base][K-order] | Account toggle and subscription/UI context; conflict with developer docs recorded below |
| [Receipt-reporting knowledge base][K-nav] | Computer-generated receipt versus ePG e-receipt; rollout statements conflict with developer docs |
| [W3C date][W-date], [double][W-double], [whitespace][W-space] | Normative meaning of the types the vendor schema uses, especially R-01 and already-fixed civil-date support |

The PHP ZIP acquired bytes have SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. The seven examples are `create_receipt_with_custom_data.php`, `create_receipt_with_data_deletion_code.php`, `create_receipt_with_default_data.php`, `create_reverse_receipt.php`, `get_receipt_data.php`, `get_receipt_pdf.php`, and `send_receipt.php`. No PHP example was executed. The ZIP was read in memory, not substituted into the fixture corpus.

## 3. Request coverage: fields, order, defaults and constraints

`?` below means optional on the wire; `*` means repeated. Rust constructors use explicit caller choices for required business values; PHP convenience defaults are not server defaults.

### Common envelope and settings

| Concern | Implementation | Verdict |
|---|---|---|
| Endpoint/POST/file part | `src/wire.rs:7–14,66–99`; `src/client.rs:300–326` | Matches [request rules][requests]: `https://www.szamlazz.hu/szamla/`, POST, multipart XML file. Built-in receipt requests contribute no attachment parts |
| Four actions | `receipt.rs:185,333,412,494` | Exact `action-szamla_agent_nyugta_create`, `_storno`, `_get`, `_send` |
| Four request roots/namespaces | `receipt.rs:218–226,336–344,415–423,497–503` | `xmlnyugtacreate`, `xmlnyugtast`, `xmlnyugtaget`, `xmlnyugtasend`; each namespace `http://www.szamlazz.hu/` + root |
| XML declaration/text | `xml.rs:21–40,306–334` | UTF-8 XML 1.0, escaped text, decimal dot notation; no dependency on broken `xsi:schemaLocation` links |
| Auth | `xml.rs:348–357`; all four `beallitasok` blocks | Agent key or username/password supported in settings, correct sequence for reverse/send; legacy key-in-both-fields is representable with username/password |
| PDF request flag | `receipt.rs:145–147,177,309–311,325,387–389,404` and writers | Required `pdfLetoltes` always emitted on create/reverse/query, default false. Send has no such field. No receipt `valaszVerzio` field is documented or emitted |

Create/query schemas mostly use `xs:all`, despite page prose saying order cannot be interchanged. Writers preserve example/inline ordering anyway; reverse/send have actual `xs:sequence` constraints. This distinction matters when comparing PHP query order, not just the order of declarations in an `all` group.

### Create (`src/ops/receipt.rs:117–290`)

| Block/fields, in writer order | Mapping/defaults and assessment |
|---|---|
| `fejlec`: `hivasAzonosito?`, `elotag`, `fizmod`, `penznem` | `call_id=None`, required prefix/payment method/currency. Call ID emitted verbatim when present; no generated ephemeral ID. Prefix alphabet/uniqueness is server-owned, with typed 336/337. Explicit cash/Ft defaults in PHP are not compulsory Rust defaults |
| `devizabank?`, `devizaarf?` | `exchange_rate=None` for HUF; foreign currency requires a nonblank, unpadded bank and explicit rate, except documented PHP MNB omission policy. Zero numeric rate is representable. Correct receipt spelling, not invoice `arfolyam*` |
| `megjegyzes?`, `pdfSablon?`, `fokonyvVevo?`, `rendelesSzam?` | All present as optional public fields, default None, emitted in example order. Order is at the tail. Template A/J/L/N and arbitrary future token supported |
| `tetelek/tetel*`: `megnevezes`, `azonosito?`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs`, `netto`, `afa`, `brutto` | Full item payload; at least one item checked before `to_wire`. Correct receipt amount names. Decimal quantities/prices and arbitrary units/names supported |
| Item `fokonyv?`: `arbevetel?`, `afa?`; then `megjegyzes?`, `torloKod?` | Both supported ledger fields, row comment and erasure count retained. `0..=400` accepted; >400 refused; unsigned type excludes negative counts. Invoice-only margin base/economic-event/settlement fields refused by name, not silently dropped (`649–677`) |
| `kifizetesek?/kifizetes*`: `fizetoeszkoz`, `osszeg`, `leiras?` | Arbitrary number of tender rows. Empty Vec omits the whole block rather than emitting an invalid empty repeated container. Free-text tender and description correctly modeled as strings; source comment calling `leiras` a double is contradicted by its string XSD and example |

**Arithmetic:** `src/item.rs:9–49,74–87,133–213` distinguishes asserted amounts from calculated values. HUF receipt rules are whole gross, net/VAT ≤2 decimals, exact sum; current [amount rules][amounts] also document 2 HUF tolerance for price×quantity/net and net×rate/VAT. `Scale(2)` alone is insufficient for whole gross, and docs say so. Minor-unit HUF rounding is a stricter local policy, not the only valid representation: `787.40/212.60/1000` is preserved by explicit construction and a focused test. No claimed automatic server repair, no floating-point money, no unsolicited arithmetic correction. Tender-sum validation remains explicitly server-owned; 340 is typed. No calculator bug is inferred merely because callers can choose unsuitable raw amounts.

### Reverse (`receipt.rs:293–358`)

- Root sequence: settings, header. Header sequence **`nyugtaszam`, `pdfSablon?`, `hivasAzonosito?`** matches both schemas and PHP writer.
- Number required in the public constructor; PDF false, template and call ID absent by default.
- Response is the newly issued **SN**, with `stornozottNyugtaszam` referring to the original; it is not the original's document data. The shared parser retains both identities. Synthetic SN/PDF probe passed.
- Current [reverse response][R-response] explicitly describes errors for missing original, already reversed original and an SN target. It supplies no numeric codes for those three messages. The library preserves the returned error instead of inventing receipt-specific named codes or invoice-style repeated-storno success. Call-ID reuse scope/retention for reversal remains unestablished.

### Query (`receipt.rs:361–441`)

- Header: exactly one `nyugtaszam` or `rendelesSzam`, followed by optional `hivasAzonosito`, optional `pdfSablon`. Public selector prevents both/neither; fields themselves remain unvalidated strings, consistent with this low-level crate's number types.
- PDF false; template/call ID None. Query's call ID is exposed as an optional wire field, explicitly **not** a lookup selector. Neither docs nor the PHP query writer establishes call-ID-only lookup.
- [PHP data/PDF query pages][P-query] document last matching document for an order. Code documentation correctly warns that precise ordering and SN selection are not established. Order-number PDF retrieval is fully supported by `QueryReceipt { download_pdf: true, .. }`; a separate operation/type is unnecessary.

### Send (`receipt.rs:445–524`)

- Sequence: settings, header with `nyugtaszam`, `emailKuldes` with optional **`email`, `emailReplyto`, `emailTargy`, `emailSzoveg`** in that order.
- `SendReceipt::new` has `email=None` but still emits **one present empty block**, matching documented resend using previous details. `Some(ReceiptEmail::default())` has the same wire shape. `None` child omits it; `Some("")` emits it empty.
- Full first-send details are representable, including reply-to and multiline body. Partial child defaults, clearing previously stored fields, multi-recipient separators and body markup are not established; current rustdoc avoids promising them.
- Already-issued receipt is the documented prerequisite. No create-and-email atomic operation, send-time PDF template override, PDF download option, or delivery-status query is documented. Success parses as `()`, not a receipt/PDF.

## 4. Response coverage: every published field and response surface

Source: [create response example/inline schema][C-response], [downloaded schema][X-response], and reverse/query references to this envelope.

| Wire path | Current public representation / code | Assessment |
|---|---|---|
| Envelope `sikeres`, `hibakod?`, `hibauzenet?` | `xml.rs:207–284` | Verdict before document parsing; true/false/1/0; body-only failures preserved. Absent/empty code is `ErrorCode::Absent`, not fabricated success |
| Envelope `nyugtaPdf?` | `Receipt::pdf`; `receipt.rs:681–701`, `types.rs:96–117` | Standard base64 → raw bytes; whitespace wrapping accepted. Absent/empty → None, invalid nonempty payload → parse error. Parses a supplied PDF independently of request flag; requested-but-absent PDF remains None |
| Envelope `nyugta` | `receipt.rs:681–692,704–711` | Required on successful create/reverse/query by prose; missing payload refused even though failure-capable XSD makes it optional |
| `alap/id`, `hivasAzonosito?`, `nyugtaszam` | `id: i64`, `call_id`, `receipt_number`; `744–754` | Identity retained; i64 accommodates XSD int. Optional business text preserved except empty/XML-whitespace-only → None |
| `alap/tipus`, `stornozott`, `stornozottNyugtaszam?` | `ReceiptType`, `reversed`, `reversed_receipt_number`; `754–764` | NY/SN typed; unknown type retained. Reverse-original reference is independent of reversed flag, not confused with the SN's own number |
| `alap/kelt` | `issue_date: Date`; `763–764`, `xml.rs:367–415` | Full valid timezone suffix checked, printed civil date preserved. Invalid date refused; legacy finite Jiff domain retained |
| `alap/fizmod`, `penznem` | `PaymentMethod`, `Currency`; `765–766,725–726` | Free-text/unknown values preserved. English example `cash` remains `Other("cash")`; no unsupported translation |
| `alap/devizabank?`, `devizaarf?` | Optional bank text/Decimal; `767–770` | Blank-to-None numeric parsing, positive/negative/exponent finite Decimal vocabulary; no inferred rate |
| `alap/megjegyzes?`, `fokonyvVevo?`, `rendelesSzam?` | Comment, ledger customer, order; `771–786` | Present and fidelity-tested; returned order not globally trimmed |
| `alap/teszt` | `test: Option<bool>`; `779–780` | More lenient than mandatory XSD: missing/empty unknown, never silently false/live |
| `tetelek/tetel*`: `megnevezes`, `azonosito?`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `ReceiptItem` fields; `795–808,830–848` | All carried; numerical values parsed from text without f64 conversion |
| Item `afatipus?`, `afakulcs` | `vat_type`, raw `vat_rate_code`, `vat_rate()`; `809–817,637–643` | Special-code precedence and open tokens supported; **R-01** for padded numeric fallback |
| Item `netto`, `afa`, `brutto` | Three Decimal amounts; `812–817` | Canonical names plus `nettoErtek/afaErtek/bruttoErtek` aliases from the actual current published example |
| Item `fokonyv?/arbevetel?`, `afa?` | `ReceiptItemLedger`; `818–827,843–846` | Both fields covered, empty container distinguishable from absent |
| `kifizetesek?/kifizetes*/fizetoeszkoz`, `osszeg`, `leiras?` | `ReceiptPayment`; `734–737,851–873` | All fields, free-text tenders, arbitrary count, absent block → empty Vec; no invoice five-credit-entry limit applied |
| `osszegek/afakulcsossz*`: `afatipus?`, `afakulcs`, `netto`, `afa`, `brutto` | `Totals::by_vat_rate`; `xml.rs:519–546,563–580`, `types.rs:1064–1085` | All fields; no subtotal → empty Vec accepted as leniency; **R-01** applies to helper |
| `osszegek/totalossz/netto`, `afa`, `brutto` | `Totals::total`; `xml.rs:549–560` | All required amounts retained; reported totals not recomputed from inconsistent example rows |
| Send `xmlnyugtasendvalasz`: `sikeres`, `hibakod?`, `hibauzenet?` | `receipt.rs:518–524`; shared verdict | Entire documented payload covered; code 7's missing-email-subject example correctly remains `Api(MissingData)` with its message |

**Request-only fields:** item `megjegyzes` and `torloKod`, and header `pdfSablon`, are absent from both current receipt **response** schemas and examples. Their absence from `ReceiptItem`/`Receipt` is not established response data loss. No known declared response field is discarded.

**Headers/status:** all four parsers apply nonblank `szlahu_down`, then nonblank error header, then known non-2xx HTTP status, then the XML verdict (`wire.rs:258–306`, `xml.rs:249–257`). Receipt pages do not document invoice-style success-number/totals headers as substitute success payloads, raw PDF bodies, text-version responses, or numbered-56 soft success. The parser appropriately requires the receipt XML; no receipt success-header capability gap is established. Native client collects the raw status/headers/body before parsing (`client.rs:311–326`); callers of the Sans-I/O API retain `RawResponse` for diagnosis.

**Error coverage:** 336/337 → named prefix refusals; 338 → duplicate call refusal; 339 → receipt not found; 340 → tender mismatch; 363/364/365 → HUF receipt gross/net/VAT precision refusals. General 259–264 arithmetic and 537–539 erasure codes are named. 7 remains operation-dependent missing data, not proof of receipt absence; reverse's three textual cases do not justify invented codes. Unknown numeric/text codes and absent code remain uncertain. See `error.rs:42–54,134–186,266–312,349–449` and focused source-derived tests.

**Structure/leniency:** root local name and namespace, UTF-8 and complete single-document boundary are checked (`xml.rs:63–141`). This is not full namespace-aware validation of every child or complete XSD validation. Required strings may be empty, required bool helper accepts empty as false, missing `teszt` is unknown, and empty row/subtotal lists are tolerated. These are broader-input compatibility choices, not inability to parse conforming responses. No receipt live exception is claimed for them. Finite Decimal/date domains do not implement every possible XSD double/date value; NaN/infinity/extreme historical years are not raised as useful missing receipt capabilities.

## 5. Source conflicts, optional capabilities and justified choices

These entries are **not additional confirmed implementation bugs**. Each identifies the current code boundary so later work can avoid changing a supported feature merely to fit another source.

| ID / classification | Evidence and concrete effect | Current code; bounded recommendation; confidence |
|---|---|---|
| S-01 — conflicting create/query schemas | [Erasure page][erasure]: “schema includes the `torloKod` element”; [query page][Q-xml]: identify by “receipt number … or … order number”. The respective downloads omit those fields. Offline validation against the downloads alone would incorrectly reject supported crate output | `receipt.rs:267–269,425–434`. Preserve both fields and independent source versions. `fixtures/SOURCES.md:182–190` already qualifies cached create-schema provenance; never call the cached `torloKod` an authenticated historical download revision. High conflict confidence; server schema selection unobserved |
| S-02 — order prose versus schema/PHP order | [Create/query XML pages][C-xml] say order “cannot be interchanged”, but use `all`; PHP query header orders `nyugtaszam,pdfSablon,rendelesSzam`, while Rust puts selector before template | `receipt.rs:424–434`. Current Rust ordering matches inline declaration order and normal example. Do not invent an ordering failure from different `all` declaration order. High source observation confidence; no live combined query-option observation |
| S-03 — automatic MNB, supported first-party deviation | [Currencies][currencies]: rate and bank “must also be provided”. PHP `ReceiptHeader::$exchangeBank`: “Ha 'MNB' és nincs megadva az árfolyam … az 'MNB' aktuális árfolyamát használjuk” (MNB plus omitted rate uses current MNB rate). Its custom-data example repeats the omission comment but supplies 300.0 | `receipt.rs:203–213,232–237`, `types.rs:935–971`. Keep `automatic_mnb`; preserve the present provenance qualification. Verify vendor execution separately before stronger guarantees. High documentation-support confidence; no receipt account test |
| S-04 — VAT list/example inconsistencies | Response download includes `TEHK`; inline enum omits it. Request example lists only selected numeric rates; response example uses invoice aliases and inconsistent monetary data; PDF is `...` | `types.rs:178–315`, `receipt.rs:812–817,681–687`. Keep open tokens and documented aliases; do not require consistency of illustrative fixture totals or accept dots as base64. High source confidence; no missing named-TEHK capability because `Other` preserves it |
| S-05 — account toggle conflict | [Developer order page][order]: receipts have “their own toggle, separate from the invoice setting”. Linked [knowledge base][K-order]: “bizonylattípusonként nem állítható be külön” (cannot be configured separately per document type), after describing the receipt-editor toggle. Selecting an account uniqueness policy from these pages alone is ambiguous | `receipt.rs:142–144,368–373`; recovery `src/recovery.md:12,24–27`. No local toggle assumption is enforced. Keep deliberate order management/call-ID checks; ask vendor to reconcile account/UI documentation before promising a shared or separate toggle. High conflict confidence; low confidence in actual account configuration behavior |
| S-06 — reporting rollout conflict / vendor-owned capability | [Receipt reporting page][nav]: “We are working on automating the data reporting”. Linked [English knowledge base][K-nav]: “From 1 September 2026, Számlázz.hu will automatically provide mandatory data reporting”. Both discuss a transition period, but neither gives a receipt XML reporting option/status field | `receipt.rs:117–156,534–589`. No missing request flag can be inferred; no crate guarantee of current reporting completion exists. Track vendor clarification and schema changes; do not add invented NAV fields or describe these receipts as ePG e-receipts. High source-conflict confidence; rollout unverified |
| S-07 — PHP send-example PDF comment | PHP `send_receipt.php` introductory comment says the successful answer contains PDF, but [send response][S-response] and both send-response schemas contain only `sikeres/hibakod/hibauzenet` | `receipt.rs:467–468,494–524`. Keep `Response=()`; use documented query PDF option when needed. High conflict confidence; the comment is insufficient evidence for a send-PDF capability |
| O-01 — omitted email block is a wire no-op, intentionally not exposed | [Send XSD][S-xml]: “If omitted, no e-mail is sent”. Default Rust send uses present-empty for the separate documented previous-email behavior | `receipt.rs:477–489,506–513`. A send operation with no send has no established useful capability; do not change `email=None` to omit the container. High confidence in current documented intent and tested serialization; delivery untested |
| O-02 — no receipt call-ID query or automatic recovery engine | [Query XML][Q-xml] identifies number/order and calls `hivasAzonosito` an “optional unique call identifier”; [create response][C-response] promises duplicate prevention, not original-success replay | `receipt.rs:94–102,299–302,361–395`, `src/recovery.md:12–27`. Keep all four operations explicit. Missing call-ID-only recovery is a vendor question, not demonstrated omitted capability. High confidence in available selectors; unresolved scope/retention and SN lookup semantics |

Prefix format/receipt-only prefix and account eligibility are documented server checks, not new local validation requirements. The knowledge base's subscription statements concern product/UI eligibility; this review did not infer a new API account gate. The amount page's long-decimal example also illustrates binary precision loosely (its displayed decimal pair sums to 1000 in exact decimal arithmetic); retain the explicit precision/sum rules without treating that arithmetic explanation as an observed execution.

## 6. Historical checklist disposition and live-exception boundary

| Historical receipt-related concern | Current independent disposition |
|---|---|
| Required receipt date rejects valid XSD suffix/whitespace | Fixed: `receipt.rs:763–764`, `xml.rs:367–415`; 27 receipt unit tests include all three receipt-payload parsers and valid/invalid date table |
| Receipt error codes missing/classified Unknown | Fixed: current typed map/classification verified against freshly fetched supplement/catalogue and source-derived tests |
| Optional identifiers/comments trimmed | Fixed: dedicated business-text adapters, explicit padded and NBSP checks. R-01 is the distinct numeric interpretation path, not a request to undo this fix |
| Invoice-external-ID recovery advice applied to receipts | Fixed: stable create call ID, 338 refusal semantics, number/order queries, SN/email uncertainty documented in receipt rustdoc, README and recovery table |
| Invoice rounding observations generalized to receipts | Fixed: `item.rs:9–29,74–87,163–173`; README and create docs distinguish local arithmetic, HUF receipt rules and invoice observations |
| Query call-ID meaning asserted; last match missing | Fixed: optional unspecified call field, normal lookup omission and vendor-documented last match now explicit |
| Partial email merge/multiple-recipient promises | Fixed: current docs only establish child omission and whole-details-absent resend |
| Empty-email semantics hidden by lossy outline test | Fixed evidence boundary: `tests/receipt_wire.rs:12–100` tests actual one-block presence and omitted versus empty children; `tests/upstream.rs:1225–1234` now disclaims universal empty=omitted equivalence |
| Automatic MNB questioned for lack of receipt evidence | Independently confirmed receipt-specific PHP documentation, with current code's precise no-live-test qualification; S-03 |
| Cached receipt-create schema provenance overclaimed | Qualified in `fixtures/SOURCES.md:182–190`; no new claim about how historical bytes were acquired |

`docs/szamlazz-hu-behaviour.md:3–24,155–172,174–278` records invoice-family test-account observations, not a receipt lifecycle. In particular its EUR invoice storage rounding, invoice order whitespace/replay, invoice storno successful repeats, and invoice body/header observations **do not establish corresponding receipt behavior**. No documented receipt-specific live exception was found to override the current receipt sources. Existing leniency and source-backed extensions remain distinguished from live-proven deviations.

## 7. Verification performed and limits

Fresh commands (no whole suite, no ignored tests):

```text
cargo test --locked --offline -p szamlazz-agent --lib ops::receipt::tests -- --nocapture
  27 passed
cargo test --locked --offline -p szamlazz-agent --test receipt_wire --test business_text --test error_classification --test response_completion
  4 + 2 + 3 + 1 passed
cargo test --locked --offline -p szamlazz-agent --lib item::tests
  6 passed
cargo test --locked --offline -p szamlazz-agent --test review_receipts_20260910_independent -- --nocapture
  1 observational probe passed (run twice, second run added subtotal/calculator assertions)
```

The **43 existing focused tests** passed. The separate probe asserted current behavior; its green result confirms the misclassification rather than demonstrating a fix. It additionally confirmed SN/original-reference preservation, numeric `sikeres=1`, and whitespace-wrapped PDF decoding. It used synthetic data and was deleted with `apply_patch`. Reproduce R-01 by the substitutions and calls in §1; no current production code changed.

Read but not rerun as a broad corpus: `tests/upstream.rs`, its receipt fixtures, `fixtures/SOURCES.md`; the corpus's response check at `858–945` explicitly replaces only the invalid PDF placeholder before field assertions, and checks the unmodified placeholder is rejected. Request outline comparison is lossy, as documented above. `tests/client.rs` was inspected for the generic HTTP shell; it is not a four-operation receipt transport/lifecycle test. `tests/live.rs` defines taxpayer, invoice, proforma and appearance cases, **no receipt lifecycle**.

An attempted independent `lxml` validation could not run because Python reported `ModuleNotFoundError: No module named 'lxml'`. Therefore R-01's schema validity follows direct schema/W3C inspection plus Rust behavior, **not** an automated XSD-validator result. Request schema coverage was field/order/type review, not automatic validation of all combinations. No dependency was installed to change that limit.

No evidence here establishes real PDF rendering/layout, email arrival/partial defaults, receipt call-ID lifetime or cross-operation scope, exact last-order selection after SN, server-side MNB execution, receipt rounding acceptance, erasure allocation, or current NAV rollout. The published examples and PHP comments are first-party documentation evidence, not account exchanges. There is no claimed missing capability beyond the confirmed helper defect.

[C-request]: https://docs.szamlazz.hu/agent/generating_receipt/request
[C-xml]: https://docs.szamlazz.hu/agent/generating_receipt/xml
[C-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[R-request]: https://docs.szamlazz.hu/agent/reversing_receipt/request
[R-xml]: https://docs.szamlazz.hu/agent/reversing_receipt/xml
[R-response]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[Q-request]: https://docs.szamlazz.hu/agent/querying_receipt/request
[Q-xml]: https://docs.szamlazz.hu/agent/querying_receipt/xml
[Q-response]: https://docs.szamlazz.hu/agent/querying_receipt/response
[S-request]: https://docs.szamlazz.hu/agent/sending_receipt/request
[S-xml]: https://docs.szamlazz.hu/agent/sending_receipt/xml
[S-response]: https://docs.szamlazz.hu/agent/sending_receipt/response
[rules]: https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules
[order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[template]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template
[erasure]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code
[amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[nav]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting
[X-create]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd
[X-reverse]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd
[X-query]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd
[X-send]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd
[X-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[X-send-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd
[requests]: https://docs.szamlazz.hu/agent/basics/sending-requests
[auth]: https://docs.szamlazz.hu/agent/basics/authentication
[cookies]: https://docs.szamlazz.hu/agent/basics/session-cookie
[errors]: https://docs.szamlazz.hu/agent/basics/error-handling
[currencies]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[vat]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates
[P-create]: https://docs.szamlazz.hu/php/nyugta-generalas
[P-reverse]: https://docs.szamlazz.hu/php/sztorno-nyugta-generalas
[P-send]: https://docs.szamlazz.hu/php/nyugta-kuldes
[P-query]: https://docs.szamlazz.hu/php/nyugta-lekerdezes
[P-pdf]: https://docs.szamlazz.hu/php/nyugta-pdf
[P-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
[K-erasure]: https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor
[K-order]: https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan
[K-nav]: https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting
[W-date]: https://www.w3.org/TR/xmlschema-2/#date
[W-double]: https://www.w3.org/TR/xmlschema-2/#double
[W-space]: https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace
