# Current Számla Agent receipt audit

**Reviewed baseline:** `382cf7615aca1d64a05c7c3f77110248dde51950`  
**Sources fetched:** 2026-09-10; official documentation reports build `v202608271632`.  
**Scope:** `crates/szamlazz-agent`, all four receipt operations, receipt types/templates, receipt item restrictions, responses, PDFs, tenders, email, call identities and receipt recovery guidance.

## Conclusion

**No defect in emitting the documented receipt requests or decoding a conforming receipt response was established.** All documented request fields and response data fields are represented. Two small, current **documentation gaps** remain: the documented refusal of a repeated receipt storno is presented only as an unestablished successful-repeat guarantee, and receipt order-number guidance omits the separate receipt repetition setting.

There is also one **optional malformed-response hardening opportunity**: an explicitly requested PDF can be missing or empty without any diagnostic beyond `Receipt::pdf == None`. This is not a missing PDF-download capability or evidence of a live vendor incident. Its remedy should preserve a successfully issued document's identity.

| Category | Result |
|---|---|
| Conforming request/response implementation defects | None established |
| Documentation gaps | D1, D2 — P3, high confidence |
| Unsupported useful vendor capability | None established |
| Malformed-response hardening | H1 — P3 candidate; behavior reproduced, remedy is a policy choice |
| Vendor/documentation uncertainty | Listed separately below; not promoted to bugs |

No production code, fixtures, configuration or recovery documentation was edited. No live account requests ran. Temporary downloads, binaries and reproductions were confined to `/tmp/opencode`. This report is the only repository file created by this review.

### Baseline and evidence discipline

HEAD initially matched the requested SHA. During the review another actor committed `7da23b44cd006783eb47e60aa54ab8969bdd1c2c` (`chore: use published restate-e2e-harness crate`). A comparison against the requested SHA confirmed **no changes** to `crates/szamlazz-agent`, `fixtures/SOURCES.md`, `docs/szamlazz-hu-behaviour.md` or `docs/research/2026-09-10-agent-vendor-questions.md`. The workspace manifest/lock changes concern the externalized harness; the agent dependency versions in the workspace lock are unchanged. Locations below refer to the requested agent baseline.

Read the relevant receipt, line-item, open-token and recovery vocabulary in `CONTEXT.md`; the complete `docs/szamlazz-hu-behaviour.md`; `fixtures/SOURCES.md`; and `docs/research/2026-09-10-agent-vendor-questions.md`. The latter currently asks about invoice preview ordering and invoice layouts, not receipt semantics. The untracked `docs/review/2026-09-09-agent-api/FINAL.md` was used only to identify earlier fixes to verify, never as evidence that a finding is still present.

The behavior record contains invoice/proforma probes, **not receipt account observations**. In particular, invoice storno replay, invoice order-number behavior, invoice payment removal and invoice rounding are not receipt guarantees.

## Current documentation findings

### D1 — State the documented repeated-storno refusal, rather than only an absence of a replay guarantee

**Severity:** P3 / low. **Confidence:** high for the documentation gap and documentation-only remedy. No live receipt-storno observation.

**Current locations:**

- `crates/szamlazz-agent/src/ops/receipt.rs:299–302`: “A storno-specific 338 guarantee or invoice-style successful repeat has not been established”.
- `crates/szamlazz-agent/src/recovery.md:16`: “Neither invoice-style successful-repeat behavior nor a storno-specific 338 guarantee is established.”
- `crates/szamlazz-agent/README.md:194`: “Neither storno-specific 338 behavior nor invoice-style successful repeats are promised.”

**Official evidence:**

- <https://docs.szamlazz.hu/agent/reversing_receipt/response>, **Error handling**: “The receipt has already been reversed” → “Missing data: receipt to reverse (this receipt has already been reversed: RECEIPT_NUMBER)”. It also documents refusal when the target is itself a storno receipt.
- Confirmed in Hungarian: <https://docs.szamlazz.hu/hu/agent/reversing_receipt/response>: `Hiányzó adat: sztornózandó nyugta (ezt a nyugtát már sztornózták: NYUGTASZAM)` and `Hiányzó adat: sztornózandó nyugta (ez a nyugta egy sztornónyugta)`.

**Impact:** The current advice is conservative and does not authorize a duplicate send, but it unnecessarily presents the repeat result as unknown. An integrator distinguishing invoice reversal replay from receipt reversal needs the positive fact that the receipt documentation describes an error, not a replayed `SN` success. An already-reversed error after a lost answer does not recover the storno number/PDF or identify which actor performed the reversal.

**Minimal remedy:** Add the documented already-reversed and storno-target refusals to the receipt storno rustdoc and recovery row; align the README sentence. Keep the uncertainty about storno call-ID/338 behavior. Do not infer an exact numeric code from the error text, introduce a successful-repeat implementation, or change shared error classification based on this page: the table supplies messages, not numeric codes.

### D2 — Receipt order-number guidance omits its independent repetition setting

**Severity:** P3 / low. **Confidence:** high for omission and documentation-only remedy.

**Current locations:**

- `crates/szamlazz-agent/src/ops/receipt.rs:142–144`: `CreateReceipt::order_number` is documented only as the number shown on the receipt.
- `crates/szamlazz-agent/src/ops/receipt.rs:368–371`: order lookup correctly describes last-match behavior but does not link the receipt repetition setting.
- `crates/szamlazz-agent/README.md:157–194`: receipt recovery uses a “deliberately managed order” without explaining the receipt-specific account switch.

**Official evidence:**

- <https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number>: “Receipts have their **own toggle**, separate from the invoice setting.”
- Same page: “If the restriction is **on**, you cannot create a new receipt with a previously used order number.” It locates the switch in **Settings → Account settings → Invoicing settings → receipt editor settings** and says a receipt and an invoice may carry the same order number.
- Hungarian corroboration: <https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number>: “A nyugtákra **külön kapcsoló** vonatkozik, a számlák beállításától függetlenül.”

**Impact:** A caller configuring receipt order-based recovery or intentionally issuing several receipts per order can configure only the invoice toggle and get unexpected receipt duplicates or refusals. Last-match lookup alone does not explain how multiple matches become possible. This is a missing operational prerequisite, not a request writer bug; stable creation call IDs remain the primary documented duplicate guard.

**Minimal remedy:** Add a short note/link on `CreateReceipt::order_number`, referenced by the receipt README: the receipt switch is separate, enabling repetition permits multiple receipt matches, and order queries return the last match. Retain identity/type/reversal checks. Do not claim that this setting supplies call-ID idempotency, define its default, or import the invoice-specific two-day replay/post-storno rules; those receipt details were not established.

## Malformed-response hardening, separate from conformance

### H1 — A requested but missing PDF has no explicit diagnostic

**Severity:** P3 / optional hardening. **Confidence:** high in current behavior; medium in the preferred API policy. No evidence the vendor currently emits this response.

**Exact locations:** `crates/szamlazz-agent/src/ops/receipt.rs:288–290,356–358,440–442` call the same parser without passing `download_pdf`. At `681–687`, an absent or empty `nyugtaPdf` becomes `None`; `689–692` returns success. The documented public PDF field is at `586–588`.

**Official evidence:**

- <https://docs.szamlazz.hu/agent/reversing_receipt/response>: “If `<pdfLetoltes>true</pdfLetoltes>`, the response includes the storno receipt PDF as well”.
- <https://docs.szamlazz.hu/agent/querying_receipt/xml>: the `pdfLetoltes` annotation says “if true, the receipt PDF is included in the response”.
- <https://docs.szamlazz.hu/agent/generating_receipt/response> describes base64 `nyugtaPdf` conditional on the request. Its XSD deliberately makes the element optional because it covers both flag values and failures.

**Reproduction:** `/tmp/opencode/receipts-audit-382cf76/src/main.rs` invokes the three public parsers with `download_pdf=true`. Both absent `nyugtaPdf` and `<nyugtaPdf/>` return `Ok` and `pdf=None`; valid `JVBERi0=` decodes to bytes; `...` produces a base64 error. A success envelope with no `nyugta` is correctly refused. For storno, this probe uses a synthetic receipt payload to exercise the shared PDF branch; it is not an `SN` execution trace.

**Impact:** An integration that equates `send(...).await?` with fulfillment of its PDF request can proceed without the expected artifact. The missing artifact is observable through `Option`; the library does not fabricate PDF bytes or lose the receipt number in this case.

**Minimal remedy:** At minimum document `pdf=None` after an explicit request as an anomalous/missing artifact and recommend retrieving the PDF through `QueryReceipt` by the known number. If an explicit diagnostic is desired, preserve the `Receipt` and its number alongside that diagnostic. Do not turn a confirmed create into a generic issuance failure that encourages another create. Retaining the existing optional field with explicit caller checking is a valid policy; this is not a mandatory parser change.

### Other response checks and boundaries

- Storno's `parse` does not enforce `SN` or the original reference: it returns the reported `ReceiptType` and reference through the common parser. A synthetic `NY` payload is accepted. This is not counted as another defect: the model exposes the evidence, recovery explicitly requires verification, and the official response example itself is semantically inconsistent (`NY`, false reversal, yet an original-reference field). A future verification helper could compare known types and references without rejecting unknown future tokens; do not silently reinterpret `NY` as `SN`.
- Empty `tetelek`, absent per-rate totals, and empty/missing `teszt` are deliberately tolerated despite XSD cardinalities; missing `teszt` stays `None`, never “live”. They are not current findings merely because an XSD validator would refuse them.
- Invalid base64 is currently an error, even when valid receipt data accompanies it. That is an observable, existing strict policy; the abbreviated `...` in the vendor example does not establish that invalid base64 is a real server format. Shared PDF decode/lexical policy belongs with the shared-helper reviewer.

## Unsupported capability assessment

No useful receipt capability documented on these pages is absent from the built-in operations:

- All creation header/item/tender fields, storno call ID/template, number/order query with template/PDF, and email fields are supported.
- `SendReceipt` always includes `emailKuldes`; it cannot request the schema's omitted-block **no-send**. That is an intentional restriction appropriate to a send operation, not a missing email capability. `None` correctly means present-empty resend, and is documented that way.
- A call-ID-only query, receipt external-ID query, PDF attached to the send acknowledgement, caller-set issue date, buyer/seller postal blocks, and a receipt delete operation are not established by the four operation contracts. Do not invent them from invoice features, a PHP example comment, or the receipt error supplement's loose “query, send or delete” wording for 339.
- Returned item `megjegyzes` and erasure-code values are not declared by either current receipt response XSD or response example. Their absence from `ReceiptItem` is not a proven response capability gap; both creation fields are written.
- The NAV receipt-reporting page describes forthcoming automation and no extra request/response field. It establishes no missing wire capability in this crate.

## Systematic field/default/order/type coverage

Paths in this section are relative to `crates/szamlazz-agent/src/`; `receipt.rs` means `ops/receipt.rs`. `?` means an optional wire child. All string fields are written as XML text, not raw markup. Business-value validation remains the vendor's responsibility except for the explicit local checks listed here.

### Requests

| Surface | Every field/block checked | Current result and locations |
|---|---|---|
| Four operation selectors | POST multipart file fields `action-szamla_agent_nyugta_create`, `_storno`, `_get`, `_send`; roots `xmlnyugtacreate`, `xmlnyugtast`, `xmlnyugtaget`, `xmlnyugtasend`; corresponding `http://www.szamlazz.hu/<root>` namespaces | Match request pages and inline/download schemas: `receipt.rs:184–186,218–225,332–343,411–422,493–502`. No receipt response-version element exists. Shared transport behavior is outside this review. |
| Credentials/settings | `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?`; required boolean `pdfLetoltes` for create/storno/query, absent from send | Common credential writer invoked in each settings block; explicit true/false emitted. Constructors and serde default PDF to false: `146–147,168–179,323–327,402–406`. PHP choosing other convenience defaults is not an XML contract difference. |
| Create header | `hivasAzonosito?`, `elotag`, `fizmod`, `penznem`, `devizabank?`, `devizaarf?`, `megjegyzes?`, `pdfSablon?`, `fokonyvVevo?`, `rendelesSzam?` | All present in API and written in example order: `117–144,227–244`. Optional fields default absent; prefix/method/currency are explicit constructor inputs. Rate is `Decimal` for XSD double; strings remain open. |
| Create items | Required `tetelek`, one or more `tetel`; `megnevezes`, `azonosito?`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs`, `netto`, `afa`, `brutto`, `fokonyv?`, `megjegyzes?`, `torloKod?` | Complete mapping at `245–271`; required numbers use Decimal, VAT is a string token, erasure is a nonnegative count. Empty items refused at `188–194`; counts above 400 refused at `195–202`. `torloKod` is supported by current EN/HU inline schema and PHP despite missing from the direct download. |
| Receipt ledger restriction | `fokonyv/arbevetel?`, `fokonyv/afa?`; invoice-only margin base, economic event, VAT economic event, settlement-from/to | The supported strings are written at `260–265`. All five unsupported fields are refused, not dropped: `649–676`; `item.rs:52–71,103–126`. No invoice ledger wrapper or invoice value spelling leaks into the receipt request. |
| Create tenders | `kifizetesek?`, one or more `kifizetes`; `fizetoeszkoz`, `osszeg`, `leiras?` | `Vec<ReceiptPayment>` defaults empty and omits the entire optional block; nonempty writes method/string, amount/Decimal, optional description/string at `273–282`. XSD, not the example's erroneous “double” comment on `leiras`, establishes its string type. Sum rule documented at `151–153`, left to server; 340 supported. No invoice five-credit-entry limit is imposed on receipt tenders. |
| Foreign currency | Receipt `devizabank` and `devizaarf`, not invoice spellings | `203–213,232–237`; HUF/Ft recognized case-insensitively. Non-HUF requires bank plus explicit rate or automatic MNB. Blank/padded bank refused for foreign currency; absent numeric rate permitted only for exact `MNB`. Zero/negative monetary values are not silently rewritten. MNB provenance is correctly qualified in `types.rs:966–973`. |
| Storno | `beallitasok` then `fejlec`; `nyugtaszam`, `pdfSablon?`, `hivasAzonosito?` | Exact `xs:sequence` order at `336–354`. Required number supplied by constructor; optional template/call ID absent, PDF false. No unsupported storno date, amounts, payment override or caller-selected resulting number. |
| Query | Required settings/header; `nyugtaszam?`, `rendelesSzam?`, `hivasAzonosito?`, `pdfSablon?` | Selector enum emits exactly one number/order at `425–430`, then call ID and template at `431–434`. This satisfies the prose's either/or rule even though `xs:all` itself permits zero or both. No default call ID. Order selector retained over stale download. |
| Send | `beallitasok`, `fejlec/nyugtaszam`, `emailKuldes?`; `email?`, `emailReplyto?`, `emailTargy?`, `emailSzoveg?` | `497–515` follows the exact sequence. Present-empty block is intentional resend. Each `None` omits its child; `Some("")` writes empty. Details are strings. No unproven recipient splitting or partial-field merging in the writer or docs. |
| Template tokens | `A` A4, `N` 80 mm, `J` ticket, `L` ticket with logo; empty/omitted/invalid fallback to A | `25–59`; all four named variants and `Other(String)` serialize to the correct XML token. `Option::None` omits the field. Rust/JSON enum representation is an SDK representation, not an Agent XML token contract. |

**Order nuance:** Create and query XSDs use `xs:all` for their singleton fields, notwithstanding the pages' general “order … cannot be interchanged” warning. Rust follows the published example/inline field order. PHP emits item comment before ledger and query template before order, while Rust follows the page; those groups are `xs:all`, so this is not a schema-order defect. Storno and send use actual `xs:sequence`; their Rust order matches both direct download and inline schemas.

### Responses

| Surface | Every field/block checked | Current result and locations |
|---|---|---|
| Shared create/storno/query envelope | `sikeres`, `hibakod?`, `hibauzenet?`, `nyugtaPdf?`, `nyugta?` | Correct root/namespace constants at `20–23`; parser at `679–702`. Shared envelope supplies verdict/error handling; a successful envelope requires `nyugta`. PDF is base64-decoded, optional; H1 covers absent requested artifact. |
| Basic receipt data | `id`, `hivasAzonosito?`, `nyugtaszam`, `tipus`, `stornozott`, `stornozottNyugtaszam?`, `kelt`, `fizmod`, `penznem`, `devizabank?`, `devizaarf?`, `megjegyzes?`, `fokonyvVevo?`, `teszt`, `rendelesSzam?` | Every field mapped at `714–786` to public fields `534–575`. ID is signed i64 (wider than XSD int); required issue date is civil Date, reversal bool supports XSD lexical forms; test is deliberately optional. Optional business text preserves meaningful padding; optional decimal rate supports blank absence. |
| Receipt type and reversal | `NY`, `SN`; original reference belongs to `SN`, reversal flag meaningful for `NY` | `types.rs:861–940`, `receipt.rs:543–552`. Known tokens correct, unknown strings preserved as `Other`; JSON uses exact type token. No invoice `SZ/SS` or `sztornozott` spelling substituted for receipt `NY/SN` and `stornozott`. |
| Items | `megnevezes`, `azonosito?`, `nettoEgysegar`, `mennyiseg`, `mennyisegiEgyseg`, `netto`, `afatipus?`, `afakulcs`, `afa`, `brutto`, `fokonyv?` with `arbevetel?`, `afa?` | Complete public/private mapping at `599–643,789–848`. Decimal quantities/amounts, raw VAT rate string plus optional category. `vat_rate()` prefers category. Both official example's invoice-style value aliases and canonical receipt names parse (`812–817`). |
| Tenders | `kifizetesek?`, repeated `kifizetes/fizetoeszkoz`, `osszeg`, `leiras?` | `851–873` maps all fields, returns empty list if absent (`734–737`), preserves tender descriptions. No invoice credit-entry date/title/bank fields invented. |
| Totals | `osszegek/afakulcsossz*`: `afatipus?`, `afakulcs`, `netto`, `afa`, `brutto`; `totalossz`: `netto`, `afa`, `brutto` | All carried through `xml.rs:647–719` into `Receipt::totals`. Per-rate collection deliberately may be empty; grand totals remain required. No local recomputation from the vendor's inconsistent example. Shared decimal implementation remains with its owner. |
| Send verdict | `xmlnyugtasendvalasz`, `sikeres`, `hibakod?`, `hibauzenet?` | `518–524`, result `()`. XML acknowledgement, not raw PDF or a receipt. Official missing-subject failure remains `Api` with code/message, not success. “Plain acknowledgement” at `467` means no document payload; could be worded “XML acknowledgement” for clarity, but no functional mismatch. |
| Receipt errors/recovery | 336 prefix collision, 337 prefix format, 338 repeated call ID, 339 unknown receipt, 340 tender sum, 363/364/365 amount constraints; 537/538/539 erasure constraints; send code 7 | Named codes and source-derived tests are present. Receipt 339 is NotFound; send 7 is documented as missing data, not proof a receipt is absent. Create recovery persists call ID, checks result identity, and forbids fresh-ID escape while unresolved. Storno and email recovery remain operation-specific; D1 fills in the documented storno refusal. |

## Preserved deviations and documentation uncertainty

### Preserve these supported behaviors

1. **Receipt `torloKod`:** keep it. Direct download omits it, current EN/HU inline schemas and erasure page include it, and PHP `ReceiptItem::buildXmlData` writes it. `fixtures/SOURCES.md:182–191` already records the cached-download acquisition uncertainty. This review does not relabel a cached schema as an unmodified current download.
2. **Query `rendelesSzam`:** keep it. Current EN/HU query request/inline schema and first-party PHP query/PDF pages explicitly support it; direct download still omits it. The fixture provenance already explains the earlier inline refresh.
3. **Automatic MNB on receipts:** keep it. Fresh PHP 2.12.4 download, `Header/ReceiptHeader.php:62–75`, explicitly documents omitted-rate MNB lookup; `examples/document/receipt/create_receipt_with_custom_data.php:41–44` repeats the comment while supplying rate 300. The general currency page asks for both bank and rate. Rust correctly calls this documentation evidence, not a live omitted-rate result; do not impose an explicit-rate-only gate.
4. **Receipt amount policy:** preserve exact caller-supplied `787.40 / 212.60 / 1000`, optional explicit rounding, and server-owned arithmetic checks. The current rule additionally specifies 2-HUF tolerances for price×quantity and rate-derived VAT, but exact net+VAT=gross. Whole net/VAT is a stricter local HUF rounding choice, not the vendor's requirement. Invoice storage observations remain invoice-only.
5. **Open response tokens and lenient content:** retain unknown receipt types, VAT tokens and payment-method strings; `TEHK`, present in the downloaded response VAT enumeration but absent from the EN inline enumeration, is not lost by the open model. Preserve missing test marker as unknown and receipt amount aliases justified by the actual example.
6. **Email presence:** preserve present-empty resend. Removing `emailKuldes` when `SendReceipt::email` is `None` would change the documented operation into no-send.

### Unresolved vendor questions — not current implementation findings

| Topic | What is established | What remains unknown / bounded action |
|---|---|---|
| Creation call identity | Unique `hivasAzonosito` prevents duplicate creation; duplicate produces 338, not a success document | Account/prefix/operation scope, retention, byte normalization, concurrency and behavior after reversal are not specified. Retain the same nonempty logical ID; do not claim unlimited idempotency. |
| Storno call ID | Optional field exists after template; repeat of an already reversed target is documented as an error | Storno-specific 338 behavior, collision scope with create and interaction of duplicate ID versus already-reversed checks. No automatic retry rule inferred. |
| Query call ID | Optional wire field; request contract identifies by number or order | Whether it filters, labels the call or has another effect. No call-ID-only selector or reuse promise. |
| Last order match | First-party PHP query and PDF pages explicitly say last matching document | Exact ordering criterion, `SN` versus `NY` selection after reversal and receipt post-storno order-reuse details. Verify returned identity; invoice observations cannot settle these. |
| Email details | Whole-details-absent resend; individually optional string fields; subject omission can yield code 7 | Partial overrides, empty strings, several recipients, and delivery timing/receipt evidence. No independent merge or comma-recipient promise. |
| PDF template after creation | Template can be sent on create/query/storno | Persistence/override scope and byte-for-byte PDF reproducibility across later queries. No immutable-PDF or changed-template guarantee invented. |
| Current schemas/examples | Current inline/create supports erasure; inline/query supports order; real downloads fetched separately | Actual deployed validation schema is not established by either source alone. The response example's abbreviated base64, inconsistent totals/tenders and NY/reference combination are illustrative defects, not production captures. |
| NAV reporting | Current receipt page says automation is being developed and gives a grace-period notice | No new XML field or client action defined by that page. Track vendor updates; no speculative new capability required here. |

The existing vendor-question draft was read and preserved. If expanded later, the uncertainty rows above provide receipt questions, whereas D1 and D2 already have official answers and need only documentation updates.

## Prior fixes verified at this baseline

These are **closed prior findings**, not counted again:

- **F1 / required receipt date parsing:** `AlapXml::kelt` now uses the shared civil-date adapter (`receipt.rs:763–764`); the receipt test exercises all three operations, timezone suffixes, whitespace and malformed dates (`920–971`), and passed.
- **F2 / receipt error codes:** 336/337/339/340 and 363–365 have named meanings; source-derived `tests/error_classification.rs` passes. 338 and unknown-code conservatism remain intact.
- **R2 / business text fidelity:** receipt identifiers, bank/ledger data and tender descriptions use business-text adapters; the receipt-specific `tests/business_text.rs` case passed, including padded and NBSP strings.
- **R1 / complete XML and namespaces, shared scope:** current completion and namespace tests pass, including receipt query/send entry points. This verifies existing fixes, not a replacement audit of shared helpers.
- **D3 / operation-specific recovery:** `src/recovery.md` and README no longer recommend invoice external-ID recovery for receipts; stable call ID, identity verification, unresolved-write caution and uncertain email delivery are explicit.
- **D4 / receipt rounding:** receipt-specific rules and `Scale(2)` limitations are documented; the independent fractional-net/VAT, whole-gross wire test passes.
- **B.E03 / query call ID and order match:** no unsupported interpretation of query `call_id`; normal queries omit it, order lookup says last matching document.
- **B.E04 / receipt email semantics:** removed independent-merge and comma-recipient promises; current wire tests distinguish omitted children, empty children and a present-empty block.
- **B.E05 / MNB provenance:** supported feature retained with receipt-specific PHP attribution and an explicit no-live-execution qualification.
- **E.E6 / fixture evidence:** current `fixtures/SOURCES.md` distinguishes historical acquisition, source conflicts, project transformations, synthetic data and the outline comparator's limitations; it records the receipt-create discrepancy instead of treating the download as universally authoritative.
- **Receipt item restriction fix:** every invoice-only margin/ledger field is rejected before wire construction; supported receipt ledger strings still pass.

## Verification performed

Existing tests, no live tests:

```text
cargo test --locked -p szamlazz-agent --lib ops::receipt::tests --target-dir /tmp/opencode/receipts-audit-382cf76-target
  27 passed

cargo test --locked -p szamlazz-agent --test receipt_wire --test business_text --test response_completion --test response_namespaces --test error_classification --target-dir /tmp/opencode/receipts-audit-382cf76-target
  receipt_wire: 4 passed
  business_text: 2 passed
  response_completion: 1 passed
  response_namespaces: 6 passed
  error_classification: 3 passed

cargo run --manifest-path /tmp/opencode/receipts-audit-382cf76/Cargo.toml --target-dir /tmp/opencode/receipts-audit-382cf76-target --offline
  absent/empty/valid/invalid PDF, unknown type, missing receipt controls observed
```

**43 existing tests passed.** Some shared-suite cases also exercise invoices; their results are recorded honestly rather than counted as 43 receipt-only tests. The scratch crate's independently resolved lock used the same quick-xml 0.42.0, rust_decimal 1.43.0 and serde 1.0.229 as the tested workspace; some proc-macro/transitive patch versions differed. It changes no production code and performs no network operations.

All fields were compared directly with the fetched inline and downloadable schemas. No XSD-validation run is claimed: neither `xmllint` nor Python `lxml` was available. Existing golden/outline tests were not treated as schema validation, and no current live XML acceptance or PDF rendering was tested.

## Official source acquisition register

All URLs below were fetched during this review, not merely copied from an earlier report. The four `.../<operation>/` guessed index URLs returned 403; the actual request/response/XML routes below succeeded. The XML pages include both example and inline-XSD tabs in the fetched content. Storno and query response pages refer to the shared create response; they contain no independent response example/download.

### Operation pages: requests, responses, examples and inline XSDs

| Operation | Request | Response | Example + inline request XSD |
|---|---|---|---|
| Generate | <https://docs.szamlazz.hu/agent/generating_receipt/request> | <https://docs.szamlazz.hu/agent/generating_receipt/response> (also shared response XSD/example) | <https://docs.szamlazz.hu/agent/generating_receipt/xml> |
| Reverse | <https://docs.szamlazz.hu/agent/reversing_receipt/request> | <https://docs.szamlazz.hu/agent/reversing_receipt/response> | <https://docs.szamlazz.hu/agent/reversing_receipt/xml> |
| Query | <https://docs.szamlazz.hu/agent/querying_receipt/request> | <https://docs.szamlazz.hu/agent/querying_receipt/response> | <https://docs.szamlazz.hu/agent/querying_receipt/xml> |
| Send | <https://docs.szamlazz.hu/agent/sending_receipt/request> | <https://docs.szamlazz.hu/agent/sending_receipt/response> (success/failure examples and response XSD) | <https://docs.szamlazz.hu/agent/sending_receipt/xml> |

Hungarian corroboration fetched: [create XML](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml), [query XML](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml), [send XML](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml), [storno response](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response), [receipt order rule](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number).

### Actual schema downloads and broken locations

| Fetched URL | Result |
|---|---|
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd> | XML schema; missing `torloKod`, otherwise current request fields present |
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd> | XML schema; sequence includes number → template → call ID |
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd> | XML schema; missing `rendelesSzam` |
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd> | XML schema; optional email block, ordered optional children |
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd> | XML schema; shared receipt data/PDF; VAT enumeration additionally includes `TEHK` |
| <https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd> | XML schema; verdict/error fields only |
| <https://www.szamlazz.hu/docs/xsds/nyugtast/xmlnyugtast.xsd> | 404 — location embedded in storno example, fetched via HTTPS |
| <https://www.szamlazz.hu/docs/xsds/nyugtaget/xmlnyugtaget.xsd> | 404 — location embedded in query example, fetched via HTTPS |
| <https://www.szamlazz.hu/docs/xsds/nyugtasend/xmlnyugtasend.xsd> | 404 — location embedded in send example, fetched via HTTPS |
| <https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd> | 404 — location embedded in send response, fetched via HTTPS |

### Behavior pages and linked executable examples

- [Receipt order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number)
- [Receipt PDF template](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template)
- [Receipt erasure code](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code)
- [Receipt item amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts)
- [Receipt NAV reporting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting)
- [Currencies, including receipt exchange fields](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies)
- [Error handling/retry limit](https://docs.szamlazz.hu/agent/basics/error-handling)
- PHP [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [query](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [PDF](https://docs.szamlazz.hu/php/nyugta-pdf), [send](https://docs.szamlazz.hu/php/nyugta-kuldes)
- Actual [PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), downloaded to `/tmp/opencode/receipts-audit-382cf76-php.zip`, extracted separately under `/tmp/opencode/receipts-audit-382cf76-php/`. Read receipt header/writer, item writer, response projection, custom-data and send examples. The send example's introductory comment claims a PDF, but the dedicated current send response/schema defines only verdict/error fields; that PHP comment is not sufficient evidence to add a send PDF response.

## Handoff to the lead / shared-scope owners

- Shared error classification is deliberately not re-reviewed here. One receipt-specific constraint to preserve: code **7 on send means missing subject in the official example**, while receipt-not-found is documented as **339**. General `NotFound` classification must not be used as an operation-independent assertion of document absence. Current receipt recovery docs already make this distinction.
- Complete-document, namespace projection, scalar lexical, PDF/base64 and error-precedence policies belong to the shared XML/transport reviewers. Existing completion/namespace tests passed; no duplicate shared finding is raised here. If a shared diagnostic policy is added, H1 identifies the receipt-specific requested-PDF boundary and the need to preserve issued identity.
- Do not apply invoice storno echo/no-op, invoice credit-entry semantics or invoice order-replay observations to receipts. The freshly fetched receipt-storno error table and separate receipt order switch are the bounded facts behind D1/D2.
