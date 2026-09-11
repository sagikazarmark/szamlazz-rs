# Számla Agent receipt compliance review — 2026-09-11

## Scope and verdict

Reviewed **`2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`**, independently against freshly fetched first-party documentation. Ownership: `crates/szamlazz-agent/src/ops/receipt.rs`, receipt-specific behavior of `item.rs`/`types.rs`, and their response-plumbing dependencies and public guidance. All line references below refer to that commit. `git diff --exit-code <commit> -- crates/szamlazz-agent README.md docs/szamlazz-hu-behaviour.md` was empty.

**No confirmed receipt wire-implementation defect found. One P2 integration-documentation gap: current NAV receipt-reporting setup requirements are absent from the receipt guidance.** All four operations and every field in the current inline request/response schemas are represented. Two failures against downloadable schemas are justified by newer inline schemas and official PHP, rather than crate defects. Important unresolved source questions remain, especially reporting rollout, foreign-currency execution, call-ID scope, and email partial-field behavior.

This was read-only apart from this report and scratch files under `/tmp/opencode/receipt-2ba5fb86`. No account calls, production-code edits, or repository test edits. Existing review reports were not used as evidence. The lead owns the full crate suite; the focused checks here are additional offline wire/schema/parser checks, not a claim that the full suite ran.

## Fresh source register

All sources below were fetched on **2026-09-11**. Documentation pages identify build **`v202608271632`**. These are first-party protocol assertions, not observations of a particular billing account.

| ID | Source and material read |
|---|---|
| C1 | Create: [request](https://docs.szamlazz.hu/agent/generating_receipt/request), [XML example and inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/xml), [response example and inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/response) |
| S1 | Storno: [request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [XML example and inline XSD](https://docs.szamlazz.hu/agent/reversing_receipt/xml), [response and refusal cases](https://docs.szamlazz.hu/agent/reversing_receipt/response) |
| Q1 | Query: [request](https://docs.szamlazz.hu/agent/querying_receipt/request), [XML example and inline XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml), [response](https://docs.szamlazz.hu/agent/querying_receipt/response) |
| E1 | Email: [request](https://docs.szamlazz.hu/agent/sending_receipt/request), [XML example and inline XSD](https://docs.szamlazz.hu/agent/sending_receipt/xml), [success/failure examples and inline response XSD](https://docs.szamlazz.hu/agent/sending_receipt/response) |
| R1 | Complete [receipt settings index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules); all five children: [amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts), [NAV reporting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting), [order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number), [PDF template](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template), [erasure codes](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code) |
| R2 | Receipt-linked [supported currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies); [erasure-code knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) |
| H1 | HU conflict checks: [create XML](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml), [query XML](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml), [send XML](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml), [storno response](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response), [amounts](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts), [NAV reporting](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/nav-data-reporting) |
| P1 | Official PHP [download/version page](https://docs.szamlazz.hu/php/), [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [query](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [PDF](https://docs.szamlazz.hu/php/nyugta-pdf), [send](https://docs.szamlazz.hu/php/nyugta-kuldes) |
| P2 | Fresh [PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), version dated 2026-08-12; SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Read `Header/ReceiptHeader.php`, `Header/ReverseReceiptHeader.php`, `Item/ReceiptItem.php`, `examples/document/receipt/create_receipt_with_custom_data.php`. Source paths below are relative to `PHPApiAgent-2.12.4/szamlaagent/`, with classes under `src/szamlaagent/`. ZIP was inspected in memory; no PHP example executed. |
| N1 | Linked reporting knowledge base: [EN](https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting), [HU](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg); linked first-party [NAV connection guide, step 13](https://www.szamlazz.hu/nav-online-szamlazas-regisztracios-segedlet/#lepesek) |
| B1 | Shared requirements: [sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests), [authentication](https://docs.szamlazz.hu/agent/basics/authentication), [errors/retry limit](https://docs.szamlazz.hu/agent/basics/error-handling), [sessions](https://docs.szamlazz.hu/agent/basics/session-cookie) |

### Downloadable XSDs, distinct from inline XSDs

| Schema | Fresh result |
|---|---|
| [Create](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd) | HTTP 200; SHA-256 `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f`; lacks `torloKod` |
| [Storno](https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd) | HTTP 200; SHA-256 `59c7e8564875da3c675a4ef8d1eba543d19b766551c7b3e2959bcea08d3aebcb`; matches current sequence |
| [Query](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd) | HTTP 200; SHA-256 `b7c04396435b5da9bfaa4193834c13136e3a412821f3cc7373cf66b0774b69ec`; lacks `rendelesSzam` |
| [Send](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd) | HTTP 200; SHA-256 `6ba13dda3b59b4fbfd472dd8b19e2a0827c9dc5adf7bbf3a2abdc35ae03a2a07`; matches current sequence |
| [Receipt response](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd) | HTTP 200; read completely. Includes `TEHK` beyond the inline VAT enumeration; the crate's open VAT model accommodates it. |
| [Send-response URL printed in the example](https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd) | HTTP 404; `/szamla/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd` and `/szamla/docs/xsds/nyugtasendvalasz/xmlnyugtasendvalasz.xsd` also 404. Inline XSD and both actual examples are available and were checked. |

The storno/query/send example `schemaLocation` URLs omit `/szamla` and return 404 as printed; their `/szamla/docs/xsds/...` counterparts above work. These are documentation link failures, not missing XML declarations in the crate. `xsi:schemaLocation` is not required in a request instance.

## Prioritized confirmed finding

### R-NAV-1 — P2: receipt integration guidance omits the now-actionable NAV connection permission

**Locations:** `crates/szamlazz-agent/README.md:155–217` (receipt integration guidance); `crates/szamlazz-agent/src/ops/receipt.rs:91–113` (creation rustdoc). These sections describe receipt issuance/recovery/email but contain no receipt-reporting prerequisite or link to the reporting instructions. This is an integration-documentation completeness finding, **not a missing request XML field** or evidence that an account's reports failed.

**Fresh requirement:** the HU article in N1 expressly says Számlázz.hu automatically forwards computer-generated receipt data **from 2026-09-10**, retrospectively for receipts issued after September 1, provided the account is connected to NAV and the technical user has **“Hozzáférés a nyugtaadat-szolgáltatási interfészhez”**. The connection guide's step 13 independently lists that permission and says it is needed for computer-generated receipts. This is material to an integrator with an existing invoice-reporting connection: an Agent key and invoice permissions do not themselves document that the additional permission is set.

**Source conflict:** both EN/HU API settings pages still say automation is being developed and “nothing you need to do for now”; the EN knowledge-base article instead promises automatic reporting from September 1. The freshly read HU article has the more specific September 10 rollout and concrete setup action, corroborated by the connection guide. Do not copy the stale “nothing to do” text into the README. The four-month grace period concerns penalties; it is not proof that reporting occurred or that the reporting obligation starts in January.

**Reproduction:** read the two code/documentation ranges above alongside N1's section “Mi a teendő az automatikus nyugta-adatszolgáltatáshoz” and connection-guide step 13. No runtime request is required to reproduce the documentation omission. `Receipt` at `receipt.rs:543–598` has no NAV submission/acceptance status, and the published receipt response schema has none either: a successful `CreateReceipt` cannot be interpreted as proof of completed NAV reporting.

**Recommended correction:** add a dated receipt-reporting note linking the current HU knowledge base and connection guide, identifying the separate technical-user permission and distinguishing issuance success from reporting status. Account setup remains an operator responsibility. No invented NAV XML flag or client-side reporting implementation is warranted by the reviewed schemas.

## Request coverage: all fields and sequences

`receipt.rs` below means `crates/szamlazz-agent/src/ops/receipt.rs`. **Pass** means representable/emitted according to the reconciled sources, not that every caller-provided string or amount is accepted by the server.

| Surface / complete field group | Current implementation | Assessment / source |
|---|---|---|
| All four HTTP actions | `receipt.rs:190,341,421,503`; `wire.rs:66–100`, endpoint `wire.rs:14` | Pass: single XML file, `multipart/form-data`, documented action names and HTTPS target (C1/S1/Q1/E1/B1). No receipt operation invents a response-version field. |
| Root namespaces | `receipt.rs:224–227,345–348,425–428,507–510` | Pass: `xmlnyugtacreate`, `xmlnyugtast`, `xmlnyugtaget`, `xmlnyugtasend`, each under its corresponding `http://www.szamlazz.hu/…` namespace. |
| Every `beallitasok` | `receipt.rs:228–231,349–352,429–432,511`; `xml.rs:603–612` | Pass: agent key or `felhasznalo` then `jelszo`; mandatory boolean `pdfLetoltes` on create/storno/query, absent on send. Authentication alternatives match B1. |
| Create header: `hivasAzonosito`, `elotag`, `fizmod`, `penznem` | Model `receipt.rs:118–129`; writer `233–236` | Pass: optional call ID; mandatory prefix/payment/currency elements. Number itself is allocated by szamlazz.hu, not caller-supplied. |
| Create header: `devizabank`, `devizaarf` | `receipt.rs:130–134,208–218,237–242`; `types.rs:943–980` | Pass with documented source qualification: explicit pair or MNB automatic lookup. Missing foreign-currency exchange info rejected locally. |
| Create header: `megjegyzes`, `pdfSablon`, `fokonyvVevo`, `rendelesSzam` | `receipt.rs:135–149,243–248` | Pass: exact spelling and example order; all optional. |
| Create root order | `receipt.rs:228–288` | Pass: settings → header → items → optional payments, as sample. Actual create XSD uses `all`; see order ambiguity below. |
| Each item: `megnevezes`, `azonosito`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs`, `netto`, `afa`, `brutto` | `receipt.rs:250–264`; `item.rs:90–110` | Pass: all required amount/quantity fields and optional ID. Uses receipt amount names, not invoice `…Ertek`. Exact decimal values emitted without floating-point conversion. |
| Each item: `fokonyv/arbevetel`, `fokonyv/afa`, `megjegyzes`, `torloKod` | `receipt.rs:265–274`; `item.rs:111–131` | Pass against current inline XSD/PHP. Two ledger strings, optional comment and nonnegative count; local cap 400. Older downloadable XSD lacks the count. |
| Item minimum and invoice-only fields | `receipt.rs:193–207,655–686` | Pass: empty items rejected; invoice-only margin base/economic-event/settlement data rejected rather than silently discarded. No such elements exist in receipt schema. |
| Payment block: `kifizetesek/kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | `receipt.rs:61–88,156–160,278–288` | Pass: free-text tender and optional string description (despite one example's erroneous “double” annotation), Decimal amount; arbitrary number of tenders; empty vector omits block. No invoice five-credit-entry cap. |
| Storno header sequence: `nyugtaszam` → `pdfSablon` → `hivasAzonosito` | `receipt.rs:314–325,353–359` | Pass against inline and downloadable `sequence`, and PHP `ReceiptHeader.php:188–190`. No order selector, prefix, caller date, tender edit or invoice `eszamla` flag is offered. |
| Query header: `nyugtaszam` OR `rendelesSzam`, optional `hivasAzonosito`, `pdfSablon` | `receipt.rs:369–405,433–444` | Pass against current EN/HU docs and PHP 2.12.4. `ReceiptSelector` makes number/order exclusive; call ID is not a selector. Old downloadable query XSD is stale. |
| Send header and email sequence | `receipt.rs:454–489,506–525` | Pass: required `nyugtaszam`; `emailKuldes` containing `email`, `emailReplyto`, `emailTargy`, `emailSzoveg` in that exact sequence. No PDF/template/call-ID input exists on this operation. |
| Empty resend | `receipt.rs:491–498,515–522`; `tests/receipt_wire.rs:118–154` | Pass: `None` emits a **present empty container**; `None` child is omitted, `Some("")` child is present-empty. First send can carry all four details. |
| XML text/number encoding | `xml.rs:561–589`; `wire.rs` request validation | Pass for reviewed receipt inputs: escaped text, UTF-8 document, lower-case boolean, plain decimal notation. No silent numeric rounding in writer. |

### Numbering, payment, deduplication and money semantics

| Requirement | Assessment and exact locations |
|---|---|
| Receipt-only prefix, uppercase letters/digits | C1 documents 336 (prefix used for invoices), 337 (invalid format). `CreateReceipt::prefix` is a string (`receipt.rs:123–125`); emission preserves it and error parsing retains both typed codes (`error.rs:154–162,295–299`). Server validation is intentional. The crate does not claim to know prefix stock or allocate numbers. PHP `ReceiptHeader.php:345–349` says numbering starts at 1 without gaps. |
| Header payment method vs tender details | `PaymentMethod` is open (`types.rs:588–687`); all UI values, including those without named variants, are representable through `Other`. `ReceiptPayment.method` is independently free text. No assumption that header method must equal each tender. The optional tender sum must equal receipt total (C1; code 340); `receipt.rs:156–158` explicitly delegates this check to server. |
| Call-ID duplicate prevention | C1 says repeated `hivasAzonosito` fails (338), not replayed success. `receipt.rs:94–102,118–122`, README `155–194`, `recovery.md:15,24–34` correctly tell callers to persist it and keep it during recovery. Constructor does not invent a fresh ID. Absence remains allowed by XSD. |
| Order repetition | R1 has a distinct receipt toggle, per document type; receipt and invoice may share order. `receipt.rs:142–149` and README `194` match. No import of invoice byte-identical replay, two-day window, trimming behavior, or post-storno order reuse as receipt facts. |
| Order query / PDF | Q1/P1 allow number or order, returning last match. `receipt.rs:373–383,385–404` explicitly leaves exact “last” and `SN` selection unresolved. No external-ID or call-ID-only lookup is invented. |
| Receipt storno repeat | S1/H1 list refusals for absent/already reversed/storno targets. `receipt.rs:298–310` and README `196` match. Invoice repeat-storno success is not imported. Storno call-ID dedup guarantee remains unestablished. |
| HUF/Ft precision | R1/H1: gross whole, net/VAT ≤2 places, exact sum; net/unit and VAT calculations have documented 2-HUF tolerance. `receipt.rs:104–108`, `item.rs:9–18,74–87`, README `344–360` correctly distinguish these from invoice probes. Explicit `787.40/212.60/1000` is preserved. |
| Calculator | `item.rs:183–222`: exact representability checks, explicit rounding at net then VAT, exact addition; no float. `Scale(2)` can give fractional gross; `Exact` can exceed receipt precision. Both are consciously allowed, with no acceptance guarantee. `minor_unit(HUF)` is a stricter whole-net/VAT local policy. |
| Foreign currencies and precision | R2 expressly covers receipts; `Currency` accepts every listed token, including `Ft` and vendor `KSH`, without a closed whitelist (`types.rs:370–478`). ISO-like minor-unit table is a local arithmetic policy, not storage evidence. HUF receipt-specific limits do not establish two-place rounding for EUR or three-place storage for KWD. |
| Automatic MNB | PHP `Header/ReceiptHeader.php:61–79` documents MNB with absent/zero rate; example `create_receipt_with_custom_data.php:41–44` comments on omission but sends 300 explicitly. `types.rs:966–973`/README `360` accurately qualify evidence. No account execution was established. |
| Erasure-code settings | R1 plus B1: enable account feature (539), not demo/test (538), cap 400 (537). `item.rs:115–131`, `receipt.rs:200–207` match. Count, not literal erasure-code text. `SzlaMost` restriction is documented for invoices; no receipt-template restriction is invented. |
| Email delivery | E1 is a separate operation for an already issued receipt; `receipt.rs:473–479`/README `198–217` match. Receipt existence is not delivery evidence; lost acknowledgment and deliberate resend can duplicate email. One recipient and full details are recommended; partial merge/multiple recipients not asserted. |
| NAV reporting | R-NAV-1. No current XML schema exposes a reporting setting or accepted-report status. No invoice NAV success inference and no conflation with ePénztárgép e-receipts. |
| Retry/session guidance | `recovery.md:4–9,36–42`, README `151,308–318,374`: caller handles recovery and five-total-send limit; cookie refresh after account edits; neither automatic write retry nor persistent dedup machinery is introduced by receipt types. |

## Response coverage: complete schema mapping

Create, storno and query all use `parse_receipt` (`receipt.rs:293–295,364–366,449–451,688–711`) and the documented `xmlnyugtavalasz` namespace. Send uses its distinct `xmlnyugtasendvalasz` verdict (`527–532`).

| Wire fields / behavior | Implementation and assessment |
|---|---|
| `sikeres`, `hibakod`, `hibauzenet` | `xml.rs:453–539` validates envelope/verdict before business payload; errors need no `nyugta`. Header/down/status precedence is shared and documented. Typed receipt errors 336–340 and 363–365 are present; unknown codes and absent code remain distinguishable. Send success returns `()`. |
| `nyugtaPdf` | `receipt.rs:693–696`, `types.rs:104–116`: optional base64, whitespace wrapping decoded, absent/empty/whitespace-only becomes `None`. Number survives a missing/blank artifact even when requested. Nonblank invalid base64 fails; see robustness boundary below. No raw-PDF body is required by the receipt operations. |
| `nyugta/alap/id`, `hivasAzonosito`, `nyugtaszam`, `tipus` | `receipt.rs:544–554,727–730,753–763`: i64 ID (intentional wider range than XSD int), optional call ID, number, open `ReceiptType` (`types.rs:861–940`, NY/SN/Other). |
| `stornozott`, `stornozottNyugtaszam` | `receipt.rs:555–561,731–732,764–771`: required boolean accepts `true/false/1/0`; no blank→false fabrication; optional original number for SN. No derivation from gross sign or invoice-style `sztornozott` spelling. |
| `kelt`, `fizmod`, `penznem` | `receipt.rs:562–567,772–775`: required civil date, open method, open currency. Date timezone suffixes do not convert printed day to UTC (`xml.rs:642–690`). |
| `devizabank`, `devizaarf`, `megjegyzes`, `fokonyvVevo`, `teszt`, `rendelesSzam` | `receipt.rs:568–584,776–795`: all represented, optional empty business text→None without trimming nonblank identity; optional numeric rate; missing/blank `teszt` stays unknown rather than live. |
| `tetelek/tetel/megnevezes`, `azonosito`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `receipt.rs:608–618,804–817,839–857`: full mapping, row multiplicity preserved. |
| Item `afatipus`, `afakulcs`, `netto`, `afa`, `brutto` | `receipt.rs:619–631,646–652,818–826`: retain category and raw rate separately; category takes precedence in convenience method. All three `…Ertek` aliases accommodate official response example's second row. Exact Decimal failure rather than silent truncation. |
| Item `fokonyv/arbevetel`, `fokonyv/afa` | `receipt.rs:632–644,827–837,852–855`: complete ledger projection. Neither inline nor downloaded response XSD carries item `megjegyzes` or `torloKod`; their absence from response model is not a dropped documented field. |
| `kifizetesek/kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | `receipt.rs:588–591,718–719,743–746,860–883`: all mapped; optional block, repeated tenders, description text. No five-entry limit. |
| `osszegek/afakulcsossz/afatipus`, `afakulcs`, `netto`, `afa`, `brutto`; `totalossz/netto`, `afa`, `brutto` | `xml.rs:799–881`; `receipt.rs:592–594,720,747`: complete shared projection. Allows absent per-rate rows, requires grand total; no computed totals replace returned figures. |
| Namespaces, repetitions, malformed structure | Shared `xml.rs:176–350,453–539` checks root/namespace/full XML; foreign subtrees cannot supply fields. Unknown extensions are tolerated. Open VAT/type tokens allow newer values; duplicate recognized scalar fields are refused by serde. |

### Deliberate response leniency and robustness limits

- **Not an XSD validator:** absent `teszt`, empty item lists, or absent VAT subtotals can be represented although schema requires them. Returning uncertainty/sparse content is justified; required reversal state is stricter because a false fact could mislead recovery. Exact Decimal's finite domain is narrower than `xs:double` (including NaN/infinity), deliberately preventing financial precision loss.
- **Malformed nonblank PDF loses the typed receipt:** `parse_receipt` returns `Err(Parse(Base64(...)))` at `receipt.rs:694` despite readable identity. Reproduced below. This is a robustness opportunity, not a confirmed first-party compliance defect: the response contract promises base64, and no receipt observation establishes corrupt PDFs. Caller retaining `RawResponse` still has evidence. A future design could separate the receipt fact from artifact-decoding errors, but must not import the invoice numbered-56 fallback without receipt-specific evidence.
- **Semantic consistency remains caller/server responsibility:** parsing does not assert that SN references the requested number or that the sum of tenders equals totals. Indeed the official response example is internally inconsistent. Adopting a recovered receipt requires checks documented in README `157–194`; `parse` alone is not reconciliation.

## Source ambiguities and justified deviations

1. **Downloaded create XSD omits `torloKod`.** Current EN/HU inline XSD and receipt rule page include it; official PHP `Item/ReceiptItem.php:73–75` emits it. Keeping the field is justified. A downstream validator using the downloadable file will incorrectly reject this supported feature.
2. **Downloaded query XSD omits `rendelesSzam`.** Current EN/HU request/XML docs and official PHP query/PDF docs explicitly offer order selection. PHP `Header/ReceiptHeader.php:191–194,238` emits it. Keeping the selector is justified. This is not proof of newest-SN selection or receipt order normalization.
3. **“Fixed order” prose vs `xs:all`.** Create/query header/root groups and receipt response groups use `all`; storno/send use true `sequence`. Crate matches current examples and actual sequences. PHP emits item comment before ledger (`ReceiptItem.php:65–71`) while sample/crate emit ledger first; both validate under `all`. PHP query emits template before order, current inline listing/crate order before template; again `all`. Neither difference is a confirmed request rejection.
4. **Rate required vs automatic MNB.** General receipt/currency prose says supply bank and rate, while receipt-specific PHP comments permit omitted/zero rate. The crate documents the discrepancy and confines omission to exact `MNB`. Support for a currency in the Agent list does not establish that MNB publishes it. Case-insensitive HUF detection (`types.rs:408–410`) is local leniency, not a documented lower-case currency acceptance result.
5. **Example arithmetic is not server evidence.** The create example uses ÁKK with nonzero VAT; the response's rows total 50,800 while its reported gross is 254 and tenders total 4,000; NY also carries `stornozottNyugtaszam`. These examples test parsing shape, not valid accounting state. The unrounded HUF example's printed decimals actually add to exactly 1000 in decimal arithmetic, although prose says `999.999999…`; excess precision still violates the separate rule. Do not turn that binary-arithmetic explanation into a Rust Decimal invariant or assume exact vendor tolerance boundaries from it.
6. **Send omitted container vs empty container.** EN/HU inline XSD comments say absent `emailKuldes` sends no email, while example says missing details resends previous email. Crate chooses present-empty, consistent with both. Independent partial-field inheritance, clearing via empty values, multi-recipient delimiters, and whether a resend changes artifact template remain unestablished. No send-template field exists.
7. **Call-ID semantics are creation-specific.** XSD includes call ID for storno/query but only creation docs establish 338 deduplication. Retention duration, uniqueness scope across operation kinds, concurrency atomicity, treatment after reversal, and query meaning are not specified. Current docs avoid guaranteeing these. Do not add a call-ID-only selector from an optional XSD field.
8. **NAV documents are at different rollout stages.** R-NAV-1 records the September 10 HU update and step-13 permission. This review does not claim to observe actual NAV delivery. Three-calendar-day reporting, daily aggregation by VAT rate and number range, penalty grace through December 31, and distinction from ePénztárgép are first-party knowledge-base requirements. The receipt API's response has no reporting confirmation.
9. **Storno refusal codes are not given by the storno page.** The page gives the three Hungarian messages, not a numeric mapping. Do not invent a dedicated code or infer invoice storno idempotence. The general receipt supplement's 339 covers unknown receipt numbers; body code/text remain exposed.

## Focused offline verification

Scratch reproduction sources: `/tmp/opencode/receipt-2ba5fb86/{Cargo.toml,main.rs,check.py}`. `main.rs` uses public `AgentRequest` interfaces and the current crate path. Python fetches official documents with GET, extracts inline XML/XSD code blocks, validates emitted requests using system libxml2 through ctypes, and passes response examples to the local Rust parser. It performs no Számla Agent POST and reads no credentials. The placeholder PDF is removed only where stated; fixtures are not silently repaired into alleged live evidence.

Commands executed successfully:

```sh
cargo build --offline --manifest-path /tmp/opencode/receipt-2ba5fb86/Cargo.toml
python3 /tmp/opencode/receipt-2ba5fb86/check.py
```

The scratch build resolves its own offline lockfile (not the repository's locked suite): quick-xml 0.42.0, rust_decimal 1.43.0, serde 1.0.229, jiff 0.2.35. Source code is the pinned crate; whole-crate verification with the tracked lockfile is the lead's separate check.

| Local case | Result |
|---|---|
| Full create: optional metadata, fractional HUF net/VAT, two tenders | Inline and downloadable XSD **pass** |
| Create with `torloKod=400` | Inline **pass**, downloadable **fails** exactly on unsupported `torloKod`; 401 refused by crate before serialization to wire |
| Foreign create, MNB with absent numeric rate | Both XSDs **pass**; establishes emission only |
| Storno with template and call ID | Both XSDs **pass**, exact `nyugtaszam/pdfSablon/hivasAzonosito` sequence |
| Number query with PDF/template/call ID | Both XSDs **pass** |
| Order query with PDF/template/call ID | Inline **pass**, downloadable **fails** exactly on unsupported `rendelesSzam` |
| Send full details and present-empty resend | Both XSDs **pass** |
| Fresh receipt response example, `nyugtaPdf` placeholder removed | **Parses** both rows including `…Ertek` aliases, tenders, VAT category, grand total, test/reversal/reference fields |
| Same example with short base64 PDF content; blank PDF | **Parses** `Some(Pdf(9 bytes))`; **parses** `None` respectively (base64 test bytes, not a rendering validation) |
| Same example with `nyugtaPdf=!!!` | `Err(Parse(Base64("Invalid symbol 33, offset 0.")))` — robustness boundary above |
| Same example with empty required reversal boolean | **Refused** as invalid bool, not interpreted as false |
| Same example changed to `tipus=SN` | **Parses** Storno and original-number reference; synthetic shape control, not evidence of storno amount behavior |
| Both freshly fetched send response examples | Success **`Ok(())`**; failure **MissingData**, exact message `Hiányzó adat: emailtargy elem.` |
| Body-only receipt codes 336,337,338,339,340,363,364,365,537,538,539,7 | All **typed API errors**, Hungarian entity-decoded text retained |

The two expected libxml2 errors reproduce vendor schema drift, not current crate defects. No receipt live test was performed, and no new acceptance/rounding/idempotence claim is inferred from these checks.

## Evidence boundary and remaining work

`docs/szamlazz-hu-behaviour.md:3–24,155–162,164–172,249–254` describes invoice-family probes on one TEST account, not receipt probes. Its invoice `eszamla`, successful repeated storno, fingerprint replay, independent two-decimal storage and external-ID behavior cannot justify receipts. README `351–360` and item rustdoc already make the money distinction explicit. The workspace README's broad surface claim is consistent with the four implemented receipt operations; it is not proof of completed NAV reporting.

Remaining vendor/account questions: actual omitted-rate foreign receipt execution and storage precision by currency; call-ID retention/scope and storno/query semantics; exact order-query newest selection and behavior after receipt reversal; email partial fields and multi-recipient syntax; reporting rollout/status observability. These are source ambiguities or unobserved behaviors, not newly confirmed implementation defects. The concrete integration-documentation correction is R-NAV-1.
