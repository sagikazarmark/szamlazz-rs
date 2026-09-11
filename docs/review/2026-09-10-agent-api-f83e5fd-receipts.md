# Independent receipt review — szamlazz-agent at f83e5fd

**Reviewed revision:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9` (HEAD).

**Fresh source acquisition and verification:** 2026-09-10.

**Scope:** receipt create, reverse/storno, query and send; `ops/receipt.rs`, receipt uses of item/types/errors/PDF, shared response handling and operation-specific recovery guidance.

## Conclusions

**No confirmed omitted, misencoded or misread useful receipt API capability was established at this revision.** All fields in the freshly fetched EN/HU operation contracts, current inline schemas, downloadable schemas and reviewed first-party receipt supplements have a representation or an explicitly explained disposition below. This is a documentation/code comparison, not evidence of a successful account lifecycle.

- **The older empty-reversal defect is closed:** missing, empty, whitespace-only and non-boolean `stornozott` are refused; `false/0/true/1` work.
- **The older whitespace-PDF defect is closed:** omitted, empty and whitespace-only `nyugtaPdf` yield `None`, preserving receipt identity. The README now explicitly describes this even when PDF was requested.
- **One low-priority hardening tradeoff remains:** nonblank invalid base64 fails the entire typed receipt parse. It is classified as uncertain, not refused issuance. This is not a demonstrated vendor occurrence or an omitted API field; see P1.
- **Vendor disagreements remain material evidence limits:** downloadable create/query schemas omit supported fields; the linked knowledge base contradicts the Agent pages about independent order-number toggles; NAV rollout statements differ between the Agent pages and EN/HU knowledge base.
- **66 existing tests passed**, plus independent offline receipt matrices and freshly fetched EN/HU response-example checks. No live account calls, PHP execution, XSD-validator execution or PDF-renderer execution occurred.

## Method and boundaries

This is an independent whole-surface audit of the pinned receipt implementation against the official contract, rather than a review limited to the latest commit's diff. No delegation was used.

`receipt.rs` below means `crates/szamlazz-agent/src/ops/receipt.rs`; other source basenames are under `crates/szamlazz-agent/src/`. Test paths are under `crates/szamlazz-agent/tests/`. All source locations refer to the pinned revision.

The older untracked `docs/review/2026-09-10-agent-api-current-receipts.md` was read as a lead list. Its revision is `fbda137…`, not this HEAD. Its findings, acquisition assertions and test results were not reused as current evidence. Each cited vendor source below was fetched anew in this session. The example/XSD tabs were both inspected. Python retrieval used only unauthenticated documentation GETs; official PHP ZIP members were read in memory, never executed or extracted into the repository.

`docs/szamlazz-hu-behaviour.md:1–308` was read. Its observed documents are invoices, proformas and related invoice operations on one TEST account, not receipt executions. In particular:

| Invoice evidence | Not established for receipts |
|---|---|
| Order trimming/case/exact query, two-day/identical replay discussion, order reuse after reversal (`33–56,185–191`) | Receipt order normalization, replay fingerprints/window, reuse after storno, latest-match selection criterion |
| External-id behavior and visibility (`59–71`) | Receipt call-ID scope/retention/queryability; `hivasAzonosito` is not invoice `szamlaKulsoAzon` |
| Successful repeat invoice storno, proforma no-op, fulfillment date and appearance (`73–98`) | Receipt storno replay success, receipt dates/appearance flags; receipt docs instead describe repeat-storno refusals |
| Credit-entry removal/replacement and five-entry limit (`80,129–135`) | Receipt tender mutation, deletion of tenders on reversal, five-tender limit |
| P60 invoice rounding/storage and tolerance (`155–162`) | Receipt stored precision or foreign-currency rounding; the receipt amount page supplies its own HUF rules |
| Invoice error headers, latency and email probes (`137–153`) | Actual receipt header distribution, receipt latency, synchronous email failure/delivery behavior |

Source/fixture/lock/behavior-note diffs against the pin were empty when checked during the review. Concurrent unrelated Restate/CONTEXT/ADR edits were visible and left alone. The only repository file authored by this review is this report; authored scratch files are under `/tmp/opencode/`.

## Confirmed implementation findings

**None.** No P0–P3 implementation defect against an established receipt feature is asserted. Potential malformed-response handling and unresolved vendor semantics are recorded separately, rather than promoted into confirmed API defects.

## Rechecked older findings and remaining policy

### C1 — Empty reversal is no longer interpreted as false (closed)

**Official rule:** [create response][C-response] / [HU response][C-response-HU] / [response download][X-response] declare:

> `<element name="stornozott" type="boolean" maxOccurs="1" minOccurs="1">`

The HU example explains `true, ha a nyugta sztornózva van; false egyébként` (true if reversed, false otherwise).

**Current code:** `receipt.rs:764–765` uses `xml::de::required_bool`, not `flexible_bool`. `xml.rs:615–622,651–655` trims XML whitespace and accepts exactly `true/1/false/0`; it has no empty-to-false branch. Public `Receipt::reversed` remains `bool` (`555–558`).

**Executed reproduction:** `receipt_wire.rs:12–42` passed. Independent scratch replaced `stornozott` in a complete synthetic receipt and called all three public create/storno/query parsers: four valid tokens decoded correctly; omission, self-closing, paired-empty, whitespace-only, `unknown` and a foreign-namespace replacement all failed. No negative reversal fact was manufactured. No vendor empty-boolean occurrence is claimed.

### C2 — Whitespace PDF no longer becomes Some(zero bytes) (closed)

**Official rule:** [storno response][S-response] says:

> “If `<pdfLetoltes>true</pdfLetoltes>`, the response includes the storno receipt PDF as well (`<nyugtaPdf>`, base64-encoded), alongside the `<nyugta>` block.”

Create and query document the same conditional PDF facility. The shared response XSD makes the field optional across the operation's possible responses.

**Current code:** `receipt.rs:690–701`, specifically `body.nyugta_pdf.filter(|s| !s.trim().is_empty())` at `693`, filters whitespace before `Pdf::from_base64`. `README.md:239–242` explicitly documents absent/empty/whitespace PDFs as `None`, even when requested, and advises querying the retained receipt number rather than creating again.

**Executed reproduction:** `receipt_wire.rs:44–64` passed. Independent scratch used `download_pdf=true` for create, storno and query: omitted/self-closing/paired-empty/whitespace-only PDFs all returned `Ok(Receipt { pdf: None, … })` with the number retained. Wrapped `JVBE\nRi0=` decoded to the five bytes `%PDF-`. Those bytes test decoding, not a valid renderable PDF.

**Disposition:** the requested-but-missing-artifact result is now a documented local policy, not an undocumented success guarantee. Callers must inspect `pdf`; no automatic artifact recovery is offered. `Pdf::from_base64("")` itself still permits empty bytes, but the receipt parser no longer reaches it for blank input.

### P1 — Invalid nonblank PDF still prevents a typed receipt result

**Category:** optional hardening/API-policy tradeoff; **priority P3 (low)**. **Confidence:** high in current behavior; no evidence that szamlazz.hu emits invalid nonblank receipt base64 in real responses.

**Location:** `receipt.rs:691–701`, `types.rs:104–117`, `client.rs:403–405`; `ResponseError::outcome_class` at `error.rs:744–761` correctly returns `Unknown` for the parse error.

**Reproduction actually executed:** in the same complete successful synthetic receipt, insert `<nyugtaPdf>...</nyugtaPdf>`. All three parsers return `invalid base64 payload: Invalid symbol 46, offset 0.` with `OutcomeClass::Unknown`, rather than a typed receipt. The fresh EN/HU official examples contain exactly that placeholder and fail the same way; substituting `JVBERi0=` explicitly makes their remaining fields parse. A placeholder is not a real vendor PDF failure.

**Consequence and distinction from the HEAD fixes:** valid document fields are not returned alongside an invalid artifact. A direct `AgentRequest::parse(&RawResponse)` caller retains its raw body; `Client::send` does not return the completed raw body with a parse failure. HEAD's `IncompleteResponse` retains status/headers after a body-transfer failure (`client.rs:66–90,385–393`), not a completed receipt alongside invalid base64. Invoice-specific numbered-response salvage is not receipt salvage.

**Possible improvement:** retain the parsed receipt identity with a separate artifact diagnostic, or expose completed-response evidence. Such an interface change would need a deliberate policy for malformed/duplicate identity; blindly treating every malformed receipt as issued would be wrong. Current strict failure is conservative and the documented recovery guidance forbids treating it as permission to issue again. This report does not count the policy as a missing API feature or demonstrated duplicate-issuance bug.

## Complete request inventory

Notation: `?` = optional XML child; `1+` = one or more. Unmarked scalar children have cardinality exactly one. `double → Decimal` means the crate deliberately uses exact finite decimal money rather than the full XSD double domain. A required `xs:string` does not itself prohibit empty text. Optional Rust strings can represent omission (`None`) and a present empty element (`Some("")`). No receipt schema exposes an invoice-style `valaszVerzio`.

### Operation identity, containers and common settings

| Operation | Multipart file field | Root / namespace | Containers and code |
|---|---|---|---|
| Create | `action-szamla_agent_nyugta_create` | `xmlnyugtacreate`, `http://www.szamlazz.hu/xmlnyugtacreate` | `beallitasok`, `fejlec`, `tetelek`, `kifizetesek?`; `receipt.rs:189–191,223–290` |
| Storno | `action-szamla_agent_nyugta_storno` | `xmlnyugtast`, `http://www.szamlazz.hu/xmlnyugtast` | sequence `beallitasok`, `fejlec`; `340–362` |
| Query | `action-szamla_agent_nyugta_get` | `xmlnyugtaget`, `http://www.szamlazz.hu/xmlnyugtaget` | `beallitasok`, `fejlec`; `420–447` |
| Send | `action-szamla_agent_nyugta_send` | `xmlnyugtasend`, `http://www.szamlazz.hu/xmlnyugtasend` | sequence `beallitasok`, `fejlec`, `emailKuldes?`; `502–525` |

All four match their [request pages](#official-source-register) and [sending requests][Requests]. `wire.rs:7–14,66–99,402–408` packages one XML file in multipart/form-data for POST to `https://www.szamlazz.hu/szamla/`; receipt operations add no attachments. `xml.rs:19–41,442–470` supplies UTF-8, root namespace, XML-escaped text, true/false and plain decimal values. Example `xsi:schemaLocation` attributes are schema hints, not missing business inputs. The complete-wire path validates XML 1.0 character legality before sending.

| Settings fields | Wire type/default | Rust mapping / disposition |
|---|---|---|
| `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | strings | Every writer invokes `credentials`; username/password or agent key, ordered username then password when applicable (`xml.rs:484–494`). The XSD's optional fields do not define authentication success. |
| `pdfLetoltes` on create/storno/query | required boolean; no XSD default | `download_pdf: bool`, constructors and serde default false (`receipt.rs:150–152,173–184,317–319,328–337,396–398,410–415`). Explicitly written; absent from send as required by that schema. |

### Create header (`fejlec`)

| XML field | Wire type | Rust field; emission | Semantics / coverage |
|---|---|---|---|
| `hivasAzonosito?` | string | `call_id: Option<String>`; `118–122,233` | Caller-supplied creation identity; optional, not auto-generated. Reuse is error 338, not success replay. |
| `elotag` | string | `prefix: String`; `123–125,234` | Required explicit constructor argument. Receipt-only prefix; vendor codes 336/337. Empty prefix remains representable; PHP describes an empty default, but no stronger local rule is inferred. |
| `fizmod` | string | `payment_method: PaymentMethod`; `126–127,235` | Free text; named methods plus `Other(String)` preserve every listed value. |
| `penznem` | string | `currency: Currency`; `128–129,236` | Open wire code, including HUF/Ft and all listed foreign/legacy codes. |
| `devizabank?` | string | `exchange_rate.bank`; `130–134,237–238` | Correct receipt spelling, not invoice `arfolyamBank`. |
| `devizaarf?` | double | `exchange_rate.rate: Option<Decimal>`; `239–241` | Explicit numeric value or automatic MNB omission; see semantic inventory. Zero remains representable. |
| `megjegyzes?` | string | `comment`; `135–136,243` | Printed free text. |
| `pdfSablon?` | string | `template: Option<ReceiptTemplate>`; `137–138,244–246` | `A/N/J/L`, unknown and empty strings supported through `Other`. |
| `fokonyvVevo?` | string | `ledger_customer`; `139–141,247` | Customer's general-ledger identifier, not a buyer party block. |
| `rendelesSzam?` | string | `order_number`; `142–149,248` | Optional displayed/order-query identity, distinct from call ID. |

All optional header values start absent in `CreateReceipt::new`. Create groups use XSD `all`; despite the prose's blanket fixed-order warning, bank-before-rate is allowed and matches the example and PHP `ReceiptHeader.php:184–186`. It is not an XSD sequence violation.

### Create items and tenders

| XML path under `tetelek/tetel` (1+) | Wire type | `LineItem` mapping / current behavior |
|---|---|---|
| `megnevezes` | string | `name`; emitted `receipt.rs:256` |
| `azonosito?` | string | `id`; `257` |
| `mennyiseg` | double | `quantity: Decimal`; `258` |
| `mennyisegiEgyseg` | string | `unit`; `259` |
| `nettoEgysegar` | double | `unit_price: Decimal`; `260` |
| `afakulcs` | string | `vat_rate: VatRate`; `261`; open special/numeric tokens |
| `netto`, `afa`, `brutto` | double each | `net_value`, `vat_value`, `gross_value`; `262–264`; **not** invoice `*Ertek` request names |
| `fokonyv? / arbevetel?`, `afa?` | string each | `ledger.revenue_account`, `ledger.vat_account`; `265–270` |
| `megjegyzes?` | string | `comment`; `271` |
| `torloKod?` | int ≥ 0 | `erasure_code_count: Option<u32>`; `272–274`; validation caps at 400 (`200–207`) |

The complete item mapping is `receipt.rs:250–277`, public input `item.rs:90–126`. Missing all items is refused (`193–196`). Invoice-only `margin_vat_base`, economic event, VAT economic event and settlement-from/to are refused with `UnsupportedOnReceipt` (`655–685`), rather than silently dropped. PHP writes comment before ledger; Rust follows the XML example's ledger-before-comment; the group's `all` permits both. The downloadable create schema's missing `torloKod` is V1 below.

| XML path | Wire type/cardinality | Rust mapping / current behavior |
|---|---|---|
| `kifizetesek? / kifizetes` | optional container, 1+ children | `payments: Vec<ReceiptPayment>`, empty/default omits container; `receipt.rs:156–160,278–288` |
| `fizetoeszkoz` | string | `ReceiptPayment::method`; free-text tender |
| `osszeg` | double | `amount: Decimal` |
| `leiras?` | string | `description: Option<String>`; schema/PHP agree, despite the XML example's mistaken “double” comment |

All tender fields are read back too (`61–88,860–882`). The vendor requires the sum to match gross if the block is present (340); the crate documents server-owned validation. No five-entry cap, date, additive/replace mode or receipt-tender mutation endpoint is established.

### Storno/query/send headers and email

| Operation/path | Full field inventory | Mapping / result |
|---|---|---|
| Storno `fejlec` | sequence `nyugtaszam` string, `pdfSablon?` string, `hivasAzonosito?` string | `receipt_number`, `template`, `call_id`; exact order at `receipt.rs:353–359`. Constructor sets optional values absent and PDF false. No date/appearance/payment fields in the contract. |
| Query `fejlec` | `nyugtaszam?`, `rendelesSzam?`, `hivasAzonosito?`, `pdfSablon?`, all strings | `ReceiptSelector::{ReceiptNumber,OrderNumber}`, separate optional `call_id` and `template`; `369–443`. Exactly one number/order selector follows prose and PHP, although XSD `all` permits zero/both. No call-ID-only lookup. |
| Send `fejlec` | `nyugtaszam` string | `receipt_number`; `512–514`. Targets an already issued receipt. No order selector or call ID documented. |
| Send `emailKuldes?` | sequence `email?`, `emailReplyto?`, `emailTargy?`, `emailSzoveg?`, all strings | `ReceiptEmail::{to,reply_to,subject,body}`; `454–471,515–522`. All optional children preserve absence versus present-empty on write. |

**Presence matters:** [send XML][E-xml] says `If omitted, no e-mail is sent`, while its example says `e-mail details, if not defined, the previous e-mail will be sent`. HU independently says `Ha nincs megadva, nem kerül e-mail kiküldésre` for the block and `az előző e-mail kerül újraküldésre` for absent details. `SendReceipt::new`/`email=None` and `Some(ReceiptEmail::default())` both emit a **present empty block**. `None` for a child omits only that child; `Some("")` emits it empty. These distinctions are independently tested, not inferred from the lossy upstream-outline comparator.

The SDK does not expose an absent-block no-email operation through `SendReceipt`. That is a schema-permitted no-op, not an omitted useful sending feature. First-send complete details and a single recipient are guidance, not locally imposed validation. Partial overrides, clearing saved values with empty strings, comma-separated recipients and HTML/BBCode behavior remain undocumented for receipts.

## Complete response inventory

Create/storno/query share `xmlnyugtavalasz` in `http://www.szamlazz.hu/xmlnyugtavalasz`; send uses `xmlnyugtasendvalasz` in its same-named namespace. The response mappings below were compared to both fresh response locales and downloads.

| Envelope field | Official type/cardinality | Current parser |
|---|---|---|
| `sikeres` | required boolean | Shared `Verdict` (`xml.rs:347–378`), checked before payload. Missing cannot establish success. Empty currently means false via `flexible_bool`, a conservative leniency rather than receipt reversal behavior. |
| `hibakod?` | int | String-based open `ErrorCode`; blank/absent code on failure becomes `Absent`, large/unknown codes preserved (`error.rs:24–34,455–469`). |
| `hibauzenet?` | string | `ApiError.message`, decoded text preserved. |
| `nyugtaPdf?` | string, documented base64 | `Receipt.pdf: Option<Pdf>`; blank absent, nonblank decode or error; C2/P1. |
| `nyugta?` | record, required on documented success | `ReceiptBody.nyugta` → `Receipt`; missing on success is `ParseError::Missing("nyugta")` (`receipt.rs:690–721`). |

`RawResponse::check` checks nonblank down header, error header, then supplied non-2xx status before body (`wire.rs:273–310`); body-only receipt errors still work at HTTP 200. Receipt code does not apply invoice numbered-56 warning recovery, and no receipt-specific numbered-56 contract was found. The shared XML path checks UTF-8, expected root/namespace, completion through EOF and lexical well-formedness; foreign namespace subtrees cannot supply protocol data (`xml.rs:63–255,384–409`). This is not full XSD validation.

### `nyugta/alap`

| XML field | Official type/cardinality | Public field / mapping |
|---|---|---|
| `id` | int, required | `id: i64`; widens XSD int safely |
| `hivasAzonosito?` | string | `call_id: Option<String>` |
| `nyugtaszam` | string, required | `receipt_number: ReceiptNumber` |
| `tipus` | NY/SN enum, required | `document_type: ReceiptType`; unknown tokens preserved as `Other` |
| `stornozott` | boolean, required | `reversed: bool`; strict required token since this HEAD |
| `stornozottNyugtaszam?` | string | `reversed_receipt_number`; original's number on SN, not the SN number on NY |
| `kelt` | date, required | `issue_date: jiff::civil::Date`; printed date retained, timezone suffix discarded without UTC conversion |
| `fizmod` | string, required | `payment_method: PaymentMethod`; arbitrary text preserved |
| `penznem` | string, required | `currency: Currency`; original code retained, including an empty reported string |
| `devizabank?` | string | `exchange_bank: Option<String>` |
| `devizaarf?` | double | `exchange_rate: Option<Decimal>` |
| `megjegyzes?` | string | `comment` |
| `fokonyvVevo?` | string | `ledger_customer` |
| `teszt` | boolean, required by XSD | `test: Option<bool>` deliberately retains missing/empty as unknown, never assumes live |
| `rendelesSzam?` | string | `order_number` |

Public definitions `receipt.rs:543–584`; XML adapters `754–795`; conversion `723–741`. Optional business strings containing only XML whitespace become `None`; otherwise decoded characters, meaningful padding and NBSP are preserved (`xml.rs:601–613`). Receipt number and type remain required strings, without extra semantic nonempty/selector matching. This is parsing data, not proof that a response belongs to the intended logical call.

### `nyugta/tetelek`, `kifizetesek`, `osszegek`

| Block | Complete official field inventory | Public mapping / disposition |
|---|---|---|
| `tetelek/tetel` (1+) | required strings `megnevezes`, `mennyisegiEgyseg`; optional string `azonosito`; required doubles `nettoEgysegar`, `mennyiseg`, `netto`, `afa`, `brutto`; optional `afatipus` enum; required nonnegative double `afakulcs`; optional `fokonyv` | `ReceiptItem::{name,unit,id,unit_price,quantity,net_value,vat_value,gross_value,vat_type,vat_rate_code,ledger}` (`receipt.rs:600–652,798–858`). All supplied documented fields retained. |
| Item `fokonyv` | optional strings `arbevetel`, `afa` | `ReceiptItemLedger::{revenue_account,vat_account}`; empty block can be an all-None ledger. |
| `kifizetesek? / kifizetes` (1+) | `fizetoeszkoz` string, `osszeg` double, `leiras?` string | `payments`, same receipt tender shape as input; `743–746,860–882`. Absent/empty block becomes empty vector. |
| `osszegek/afakulcsossz` (1+) | `afatipus?` enum; `afakulcs` nonnegative double; required `netto`, `afa`, `brutto` doubles | `Totals.by_vat_rate: Vec<VatTotal>` with `vat_type`, raw `vat_rate_code`, `net`, `vat`, `gross`; `xml.rs:689–716,733–750`, `types.rs:1055–1093`. |
| `osszegek/totalossz` (one) | required doubles `netto`, `afa`, `brutto` | `Totals.total: GrandTotal`; required, mapped without recomputation (`xml.rs:719–730,754–759`; `types.rs:1096–1107`). |

Request item `megjegyzes` and `torloKod` are **not** returned fields in either fresh receipt response schema/example. Their absence from `ReceiptItem` is not a confirmed omission. No buyer/seller block, customer-account URL, external id, delivery status, NAV acknowledgement, PDF-template echo or receipt-language field is established by this response contract.

The parser accepts `nettoErtek/afaErtek/bruttoErtek` aliases at `receipt.rs:821–826` because the current official example's second row uses them. It still writes canonical receipt names. Required numeric values use exact finite Decimal conversion; exponent and XML-padded numbers are covered by existing tests. Nonfinite/out-of-range/precision-losing numbers fail instead of rounding silently. Raw `afakulcs` stays text; `vat_rate()` chooses special `afatipus` first, otherwise interprets numeric text. Unknown special tokens, including the download-only `TEHK`, remain representable.

Missing VAT subtotals and empty item collections are deliberate parser leniencies despite XSD `1+`. `teszt` leniency is explicitly documented/tested. Invalid required date fails, rather than adopting Adatkapcsolat's separate shape-versus-content policy. The finite civil-date domain and accepted legacy spellings are model policy, not full `xs:date` conformance.

**Send response:** only `sikeres`, `hibakod?`, `hibauzenet?` are defined. `SendReceipt::parse` returns `()` through the verdict reader (`527–533`). The dedicated response schema supports no PDF or delivery identifier. PHP's send-example comment promising a PDF does not establish a new response contract.

## Semantic inventory and evidence gaps

| Topic | Fresh official evidence and current support | What remains unresolved / policy |
|---|---|---|
| Creation call ID | [C-response]: “This ensures that if the same XML is posted multiple times, it will not duplicate an existing receipt”; 338 means ID already exists. `receipt.rs:94–102,118–122`, `recovery.md:15` correctly require durable caller identity and explain failure rather than success replay. | No retention bound, case/whitespace normalization, account/prefix/operation namespace or concurrent-call proof. Blank IDs are syntactically representable; documentation recommends a unique identity, not a new random value per retry. |
| Query call ID | [Q-xml]: “optional unique call identifier”; number/order identify the receipt. PHP query writer omits call ID (`ReceiptHeader.php:191–194`). Rust keeps it optional but not a selector. | No evidence it searches by creation ID or provides safe call-ID-only recovery. No missing selector is inferred from optional schema strings. |
| Order lookup | [PHP-query]: “last matching document”; [PHP-PDF] repeats this. Rust supports number/order queries with optional PDF/template and returns order/call ID/type/reversal data for checking. | Exact last criterion, SN inclusion, receipt order case/trim normalization, and reuse after storno remain unknown. No invoice two-day replay assumption. |
| Order restriction | [Order] EN/HU says separate receipt toggle, ON prevents previously used order; receipt and invoice can share an order. Rust docs accurately cite those pages (`142–149,376–380`). | V3: knowledge-base contradiction. Default setting and subscription availability are not locally enforced. |
| Receipt storno | [S-response]: result is the **new SN**, not original NY; its PDF is SN's. Explicit errors for missing original, already reversed original and SN target. `receipt.rs:298–325` and recovery docs state the distinction. | These refusal messages have no numerical mappings on that page. Synthetic 339 tests do not establish that all three refusal cases use 339. Optional storno call ID does not establish a storno-specific 338 guarantee. No receipt repeat-success evidence. |
| Original versus SN | `stornozott` describes reversal state; `stornozottNyugtaszam` on SN references the original. Both are preserved. Independent synthetic SN control preserved number/reference, negative totals/quantity and PDF. | Negative-value control establishes parser capability, not how the vendor actually reverses tenders/rows. Parsers do not enforce requested-number, call-ID or SN-reference equality; caller verification is documented. |
| Templates | [Template] lists `A` normal A4, `N` 80 mm, `J` ticket, `L` ticket with logo; omitted/empty default A4; XML example says invalid also defaults A. `ReceiptTemplate` exposes all plus `Other`. | No live rendering, logo, dimension, template persistence or byte-stability evidence. Send has no override. |
| Email | Present empty block requests prior details; first-send example includes recipient/reply-to/subject/body; send error example is 7 `Hiányzó adat: emailtargy elem.`. Writer preserves all four optional fields and presence. | First-send minimum beyond the example's subject failure, partial merge, empty clearing, multiple recipients, HTML/BBCode and inbox delivery unestablished. Querying receipt existence cannot settle lost email acknowledgement. |
| HUF/Ft item money | [Amounts]: whole gross; net/VAT at most 2 decimals; exact sum; price×quantity and VAT relation each tolerate 2 HUF. It explicitly says these rules do not apply to foreign currency. `receipt.rs:104–108`, `item.rs:9–18,74–87,163–222` distinguish local arithmetic from vendor acceptance. | `validate` leaves amount/tender arithmetic to the server. `Rounding::Scale(2)` alone may yield fractional gross; HUF minor-unit rounding is stricter all-whole policy. Explicit `787.40/212.60/1000` remains supported and was tested. Invoice P60 storage is not receipt evidence. |
| Foreign currency | [Currencies] explicitly covers receipt `devizabank/devizaarf`; PHP `ReceiptHeader.php:62–79` documents MNB omission and zero as current-rate cases. Rust accepts `ExchangeRate::automatic_mnb()` and explicit zero (`receipt.rs:208–218,237–242`; `types.rs:943–980`). | Automatic MNB has receipt-specific documentary provenance, not a live probe. PHP custom example's comment promises omission but sends 300. Currency support is open; lower-case HUF detection is local, not tested vendor acceptance. Bank padding/blank/non-MNB omission are deliberate validation restrictions; no supported useful value blocked was established. |
| VAT and payment methods | Receipt examples, response schemas and linked VAT list enumerate ordinary/special tokens; `VatRate` and `PaymentMethod` are open (`types.rs:178–365,588–644`). Every documented token can be sent/read. | Invoice `eusAfa`, OSS switches, margin-base field and NAV subtype assumptions do not become receipt header fields through a shared type. Numeric normalization observations in `VatRate::as_wire` came from invoices. |
| Erasure codes | [Erasure] names per-item nonnegative `torloKod`, enablement 539 and max 400/537; [Errors] gives test/demo refusal 538. PHP receipt example calls `setDataDeletionCode(1)` and labels it a count. Rust uses a count and supports 0–400. | The linked erasure KB's `SzlaMost` requirement is for invoice templates, not receipt `A/N/J/L`. No issued erasure PDF or returned-code XML evidence. |
| Refusals and uncertainty | Typed 336/337/338/339/340, 363/364/365, 537/538/539 match current sources (`error.rs:154–189,243–255,295–307,383–413`). 339 is `NotFound`; other listed receipt-specific refusals are `Rejected`; open/missing codes are `Unknown`. | 338 refuses **this** duplicate exchange, not evidence that the earlier issuance failed. Code 7 can mean missing email subject, not missing receipt. Invoice-only code 56 heuristics are not applied to receipts. |
| Retries/HTTP evidence | [Errors] says “at most five times” for the same request, then operator involvement; never loop until success. `recovery.md:4–9,36–42` explains per-exchange classification and supplied HTTP retries. HEAD retains status/headers on body interruption. | No application receipt retry loop, no imposed combined write/query budget. Timeout, empty query or interrupted body does not settle earlier writes. Partial body bytes are not retained by the convenience client. |
| NAV reporting | Receipt [NAV] pages refer to computer-generated receipts; linked HU knowledge base now describes automatic reporting from September 10 with account connection and technical-user permission. | V4: rollout docs disagree. No extra receipt XML request field or per-document NAV response verdict was found. Successful `Receipt` parsing is not proof of NAV acceptance. |

## Vendor-source disagreements and illustrative-data defects

### V1 — Downloadable schemas differ from the current documented fields

Fresh [create download][X-create] omits `torloKod`; both [EN][C-xml]/[HU][C-xml-HU] inline schemas, both erasure pages and PHP receipt writer `Item/ReceiptItem.php:73–75` include it. Fresh [query download][X-query] omits `rendelesSzam`; both query locales, their inline schemas and PHP query/PDF docs/source support it. **Keep both capabilities.** No universal precedence of inline over download is asserted.

The response download includes VAT type `TEHK`, absent from both inline response enums; the open model retains it. No additional response field appears in the download. Four schema locations embedded in the examples returned 404 over HTTPS; working `/szamla/docs/xsds/…` variants were fetched (register below).

Cached `fixtures/upstream/agent/xsd/xmlnyugtacreate.xsd` has `torloKod` with unresolved historical acquisition provenance (`fixtures/SOURCES.md:182–191`); cached query schema was refreshed from inline (`103–108,135–140`). Neither is evidence of the bytes served by today's download. No cache was edited or synthetic merged XSD used as conformance proof.

### V2 — Published examples are not account execution traces

- Both receipt-response locales contain `nyugtaPdf=...`, which is not base64; second row uses invoice amount names; item gross sum is 50,800, tender sum 4,000, grand gross 254; `NY`/false also carries an SN-style original reference. The parser correctly reports supplied values, not recalculated ones.
- Create's second row combines `ÁKK` with positive VAT. Example comments mislabel `leiras` as double and `devizaarf` as string; XSD/PHP establish string description and numeric rate.
- Both amount pages claim `787.40157480315 + 212.59842519685 = 999.999999...`; the printed exact decimal strings sum to 1000. Their decimal-place violation remains clear; that example does not prove floating-point behavior or actual refusal-check order.
- PHP `send_receipt.php:17–18` claims a returned PDF. Dedicated send response/XSD defines only the verdict/error fields; no missing send-PDF capability is established.
- The inline send-response XSD code blocks contain non-XML NBSP spacing between attributes. The downloadable schema has a readable verdict-only shape. This review did not execute either through an XSD validator.

### V3 — Receipt order-toggle independence has contradictory first-party prose

[Agent EN][Order]:

> “Receipts have their own toggle, separate from the invoice setting.”

[Agent HU][Order-HU]:

> “A nyugtákra külön kapcsoló vonatkozik, a számlák beállításától függetlenül.”

But the directly linked [knowledge-base article][KB-order], fetched anew, says:

> “A beállítás valamennyi érintett bizonylattípusra, így a számítógéppel előállított nyugtákra, a számlákra és a díjbekérőkre is érvényes; bizonylattípusonként nem állítható be külön.”

That says the setting applies to all affected types and cannot be set separately per type. The same article locates it in receipt-editor settings. **Disposition:** vendor clarification/account evidence needed, not a confirmed Rust bug. The crate follows the explicit Agent EN/HU rule; no local toggle implementation exists to misconfigure an account. The KB's #start/#digital/#profi order-entry statement and #free web receipt rollout do not establish API subscription gates.

### V4 — Current NAV supplement is newer than the Agent rollout statement

Both Agent [NAV] locales still say `nothing you need to do for now` / `egyelőre nincs teendőd`, automation under development. [EN knowledge base][KB-NAV-EN] says automatic reporting from September 1 and NAV connection required. [HU knowledge base][KB-NAV-HU] now says:

> “2026. szeptember 10-től automatikusan továbbítja ezeket az adatokat, visszamenőleg a szeptember 1. után kiállított nyugtákra is.”

It additionally requires a connected account and technical-user permission:

> “Hozzáférés az nyugtaadat-szolgáltatási interfészhez”

The HU article retains internally inconsistent rollout/product prose elsewhere. This is first-party publication evidence, not verified NAV operation. No request/response field is added by these supplements; neither a new Rust receipt field nor an e-cash-register API should be invented from the rollout.

## Verification actually executed

```text
cargo test --locked --offline -p szamlazz-agent --lib ops::receipt::tests
  27 passed

cargo test --locked --offline -p szamlazz-agent --test receipt_wire --test numeric_fidelity --test business_text --test response_completion --test response_namespaces --test error_classification --test upstream
  receipt_wire 6; numeric_fidelity 6; business_text 2
  response_completion 4; response_namespaces 6; error_classification 3; upstream 11
  all passed (38)

cargo test --locked --offline -p szamlazz-agent --features client-reqwest --test client interrupted_response_retains_headers_without_promoting_them_to_a_verdict
  1 passed (loopback HTTP only; existing invoice-shaped transport control)

cargo run --offline --manifest-path /tmp/opencode/receipt-f83e5fd/Cargo.toml
  three-operation PDF and reversal matrices passed
  invalid base64 classified Unknown; synthetic SN fields/PDF and unknown type retained

cargo build --offline --manifest-path /tmp/opencode/receipt-f83e5fd/Cargo.toml
python3 /tmp/opencode/receipt-f83e5fd-examples.py
  fresh EN/HU receipt response placeholders refused; explicit synthetic PDF replacements parsed
  fresh EN/HU send success -> Ok(()); both missing-subject examples -> code 7 with exact HU message

cargo tree --locked --offline -p szamlazz-agent --depth 1
  direct runtime versions compared with scratch compilation

git rev-parse HEAD
git diff --exit-code f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9 -- crates/szamlazz-agent fixtures Cargo.toml Cargo.lock docs/szamlazz-hu-behaviour.md
  HEAD matched; reviewed inputs had no diff
```

**66 existing tests in total**, including shared/invoice cases; not 66 receipt-only tests. The independent scratch matrices use a complete project-authored one-row receipt with consistent totals and tenders. Their common NY body through all three parsers isolates shared parser behavior, not storno-result correctness; a separate SN/original-reference/negative-value control handles that distinction. `%PDF-` is a decoding sentinel only.

Fresh response tests extract HTML-decoded `<pre>` blocks in memory directly from today's pages, not from existing fixture files. Only the literal PDF placeholder is replaced, explicitly and only for the second parse. The upstream suite separately exercises the historical corpus; its outline comparator discards empty containers and trims text (`upstream.rs` outline tests and `fixtures/SOURCES.md:230–240`), so its success is not proof of email-presence equivalence or XSD validity.

Scratch has its own Cargo lock resolution; direct runtime versions match the workspace (base64 0.23.1, jiff 0.2.35, percent-encoding 2.3.2, quick-xml 0.42.0, rust_decimal 1.43.0, serde 1.0.229, thiserror 2.0.20, xmlparser 0.13.6). Identical transitive features/resolution are not claimed. Scripts and scratch source were authored using `apply_patch`; normal Cargo build outputs were generated by Cargo.

**Not executed:** account receipt create/query/storno/send lifecycle, account-setting changes, real email delivery, automatic MNB resolution, PDF rendering/printing, NAV reporting, XSD validation, full workspace tests or a broad transport audit. `fixtures/SOURCES.md:251–256` also identifies no receipt live lifecycle in the existing live-suite coverage.

## Official source register

All URLs below were freshly fetched in this review. Docs footer was `v202608271632`; that is not the acquisition date or the revision of linked knowledge-base articles. Successful document GETs returned content; the explicit broken links below returned 404. Scratch acquisition helpers: `/tmp/opencode/receipt-f83e5fd-fetch.py`, `receipt-f83e5fd-php.py`, `receipt-f83e5fd-examples.py`.

### All four operations, both locales

| Operation | EN pages fetched | HU pages fetched |
|---|---|---|
| Create | [request][C-request], [response][C-response], [XML + inline XSD][C-xml] | [request](https://docs.szamlazz.hu/hu/agent/generating_receipt/request), [response][C-response-HU], [XML + inline XSD][C-xml-HU] |
| Storno | [request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [response][S-response], [XML + inline XSD](https://docs.szamlazz.hu/agent/reversing_receipt/xml) | [request](https://docs.szamlazz.hu/hu/agent/reversing_receipt/request), [response](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response), [XML + inline XSD](https://docs.szamlazz.hu/hu/agent/reversing_receipt/xml) |
| Query | [request](https://docs.szamlazz.hu/agent/querying_receipt/request), [response](https://docs.szamlazz.hu/agent/querying_receipt/response), [XML + inline XSD][Q-xml] | [request](https://docs.szamlazz.hu/hu/agent/querying_receipt/request), [response](https://docs.szamlazz.hu/hu/agent/querying_receipt/response), [XML + inline XSD](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml) |
| Send | [request](https://docs.szamlazz.hu/agent/sending_receipt/request), [response][E-response], [XML + inline XSD][E-xml] | [request](https://docs.szamlazz.hu/hu/agent/sending_receipt/request), [response](https://docs.szamlazz.hu/hu/agent/sending_receipt/response), [XML + inline XSD](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml) |

### Downloaded schemas (fresh-byte SHA-256)

| Schema | SHA-256 |
|---|---|
| [Create][X-create] | `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f` |
| [Storno][X-storno] | `59c7e8564875da3c675a4ef8d1eba543d19b766551c7b3e2959bcea08d3aebcb` |
| [Query][X-query] | `b7c04396435b5da9bfaa4193834c13136e3a412821f3cc7373cf66b0774b69ec` |
| [Send][X-send] | `6ba13dda3b59b4fbfd472dd8b19e2a0827c9dc5adf7bbf3a2abdc35ae03a2a07` |
| [Receipt response][X-response] | `73b105be7f718ebbc181c3beefdf8ad63cb4714c4eaa253435e6ca9689689126` |
| [Send response][X-send-response] | `ee7c893c29b37b94ff40003b57ba0a539f18acb4d7fcfe998ee4ba59490af323` |

Embedded schema locations checked over HTTPS and returning **404**:

- <https://www.szamlazz.hu/docs/xsds/nyugtast/xmlnyugtast.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugtaget/xmlnyugtaget.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugtasend/xmlnyugtasend.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd>

### Rules and first-party supplements fetched

- [Receipt rules index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules).
- All five receipt rules in both locales: [order][Order] / [HU][Order-HU]; [template][Template] / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/pdf-template); [erasure][Erasure] / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/data-erasure-code); [amounts][Amounts] / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts); [NAV] / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/nav-data-reporting).
- Linked shared [currencies][Currencies], [VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [sending requests][Requests], [error handling][Errors]. Invoice-only sections were not imported into the receipt contract.
- Linked knowledge base: [order][KB-order], [NAV HU][KB-NAV-HU], [NAV EN][KB-NAV-EN], [erasure codes](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor).
- PHP pages: [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [query][PHP-query], [PDF][PHP-PDF], [send](https://docs.szamlazz.hu/php/nyugta-kuldes).
- [Official PHP 2.12.4 ZIP][PHP-zip], SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`: read full `Header/ReceiptHeader.php`, `Header/ReverseReceiptHeader.php`, `Item/ReceiptItem.php`, and all seven `examples/document/receipt/*.php` files (default/custom/erasure create, reverse, data query, PDF query, send). Evidence is source/comments, not execution.

[C-request]: https://docs.szamlazz.hu/agent/generating_receipt/request
[C-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[C-response-HU]: https://docs.szamlazz.hu/hu/agent/generating_receipt/response
[C-xml]: https://docs.szamlazz.hu/agent/generating_receipt/xml
[C-xml-HU]: https://docs.szamlazz.hu/hu/agent/generating_receipt/xml
[S-response]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[Q-xml]: https://docs.szamlazz.hu/agent/querying_receipt/xml
[E-xml]: https://docs.szamlazz.hu/agent/sending_receipt/xml
[E-response]: https://docs.szamlazz.hu/agent/sending_receipt/response
[X-create]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd
[X-storno]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd
[X-query]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd
[X-send]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd
[X-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[X-send-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd
[Order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[Order-HU]: https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number
[Template]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template
[Erasure]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code
[Amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[NAV]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting
[Currencies]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[Requests]: https://docs.szamlazz.hu/agent/basics/sending-requests
[Errors]: https://docs.szamlazz.hu/agent/basics/error-handling
[KB-order]: https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan
[KB-NAV-EN]: https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting
[KB-NAV-HU]: https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg
[PHP-query]: https://docs.szamlazz.hu/php/nyugta-lekerdezes
[PHP-PDF]: https://docs.szamlazz.hu/php/nyugta-pdf
[PHP-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
