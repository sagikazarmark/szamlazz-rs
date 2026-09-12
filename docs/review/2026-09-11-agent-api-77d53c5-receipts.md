# Számla Agent receipt API review — `77d53c5`

## Verdict

**No confirmed runtime defect in the current receipt API. One low-severity documentation defect: shared error guidance still denies receipt-code observations that the September 11 execution record now establishes.** Every request field in the fresh receipt schemas is representable, and every specified response data field is projected. The downloadable schemas disagree with the inline schemas on two supported request fields; these are vendor-documentation conflicts, not reasons to remove the fields.

Verification: **248 existing offline tests passed**, plus the request-matrix exporter. **226 generated receipt requests** were checked against freshly fetched English inline, Hungarian inline and downloadable schemas: **678 validations, 632 valid and 46 expected download conflicts**, with every declared element path covered by a valid request. Five negative validation controls passed. Fresh response examples were also executed through the public parsers, with explicit treatment of the example's invalid PDF placeholder.

### Scope and execution provenance

- Date: **2026-09-11**. Starting and reviewed HEAD: **`77d53c553c9ecdc86d5fa72ca932c636256ae807`**, resolved and verified locally.
- Whole-current audit, not a diff review: `crates/szamlazz-agent/src/ops/receipt.rs` in full, its use of `item.rs`, `types.rs`, `xml.rs`, `wire.rs`, `error.rs`, `recovery.md`, receipt README guidance and relevant tests.
- The initial working tree already contained unrelated Restate worker/design/operations edits. The agent crate had no initial modifications. This review adds only this report within the repository.
- Concurrent workspace activity advanced HEAD to `370ff2e5398e9ff4a3aff3b6008ab82e38b9d058` before final verification. `git diff --quiet 77d53c553c9ecdc86d5fa72ca932c636256ae807 HEAD -- crates/szamlazz-agent docs/research/2026-09-11-receipts-live.md docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock` succeeded: reviewed code, evidence and dependency inputs are unchanged from the pinned starting commit. Other newly appearing review reports belong to concurrent work.
- Read the mandatory evidence first: [`docs/research/2026-09-11-receipts-live.md`](../research/2026-09-11-receipts-live.md) and [`docs/szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md). Their receipt observations, not historical invoice observations, govern the execution-evidence assessment below.
- This is the receipt specialist subagent's report, assigned by the coordinating reviewer. This specialist performed its scope directly, without further subdelegation.
- No `.env` access, vendor operation, live/probe execution, source edit or fix. Public documentation GETs and local offline checks only. Scratch artifacts use `/tmp/opencode/receipts-77d53c5-*`.
- The prior `837dad0` receipt report was consulted **after** the independent implementation/docs comparison, as leads for remaining source conflicts and a working response-XSD URL. Its “not executed” assertions were not adopted. Referenced conflicts were freshly fetched and revalidated.

## Fresh source register

All sources listed here were fetched in this review. Agent pages displayed **`v202608271632`**; this identifies the site build, not the date of every paragraph. “XML” includes the example and the inline XSD tab. Storno/query response pages refer to the common receipt response rather than defining another schema.

| ID | Source URLs |
|---|---|
| C | [Generating category](https://docs.szamlazz.hu/agent/category/generating-a-receipt), [request](https://docs.szamlazz.hu/agent/generating_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/xml), [response example + inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/response) |
| S | [Reversing category](https://docs.szamlazz.hu/agent/category/reversing-a-receipt), [request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/reversing_receipt/xml), [response/refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response) |
| Q | [Querying category](https://docs.szamlazz.hu/agent/category/querying-a-receipt), [request](https://docs.szamlazz.hu/agent/querying_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml), [response](https://docs.szamlazz.hu/agent/querying_receipt/response) |
| E | [Sending category](https://docs.szamlazz.hu/agent/category/sending-a-receipt), [request](https://docs.szamlazz.hu/agent/sending_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/sending_receipt/xml), [success/error examples + inline XSD](https://docs.szamlazz.hu/agent/sending_receipt/response) |
| R | [Settings index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules), [NAV reporting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting), [order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number), [template](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template), [erasure count](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code), [amount rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) |
| B | Receipt-linked [currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies), [general error handling and retry limit](https://docs.szamlazz.hu/agent/basics/error-handling) |
| HU | [Create XML](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml), [create response](https://docs.szamlazz.hu/hu/agent/generating_receipt/response), [storno XML](https://docs.szamlazz.hu/hu/agent/reversing_receipt/xml), [storno response](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response), [query XML](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml), [send XML](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml), [amount rules](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts) |
| P | Official PHP [query](https://docs.szamlazz.hu/php/nyugta-lekerdezes), HU [create](https://docs.szamlazz.hu/hu/php/nyugta-generalas), HU [send](https://docs.szamlazz.hu/hu/php/nyugta-kuldes) |
| K | Linked knowledge base [receipt order number](https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan), reporting [HU](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg) / [EN](https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting) |
| X-C | [Downloaded create XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd) |
| X-S | [Downloaded storno XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd) |
| X-Q | [Downloaded query XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd) |
| X-E | [Downloaded send XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd) |
| X-R | [Downloaded receipt response XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd) |
| X-ER | [Downloaded send response XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd) |

The four example URLs under `https://www.szamlazz.hu/docs/xsds/` for storno, query, send and `nyugta/xmlnyugtasendvalasz.xsd` returned **404**. Adding `/szamla` fixes the first three. For the send response, `/szamla/docs/xsds/nyugta/…` and `/szamla/docs/xsds/nyugtasendvalasz/…` also returned 404; **X-ER above worked**. A guessed PHP slug `/hu/php/nyugta-keszites` returned 403; the linked `/hu/php/nyugta-generalas` worked. No failure was silently counted as a read schema.

### Request-schema fingerprints

Fresh HTML-decoded inline code blocks, without formatting edits; downloaded bytes hashed directly:

| Root | English inline SHA-256 | Hungarian inline SHA-256 | Download SHA-256 |
|---|---|---|---|
| `xmlnyugtacreate` | `eeda3911ba443f2938925a86da037348bdd6b4849af22497197f76293dc31f6b` | `1d5fe9ae12f2dd2e1ecdf1b92f9675ba3f2539bc2e07419dd693f69fbc1e24eb` | `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f` |
| `xmlnyugtast` | `98c6764084bb68f21c5f68199893b520721ff9246fb84c743e982de75eeafb21` | `432bd39c40f033fa04612c689299ec49418448d9e824a0c88be6d6d43c124b64` | `59c7e8564875da3c675a4ef8d1eba543d19b766551c7b3e2959bcea08d3aebcb` |
| `xmlnyugtaget` | `2785b4d32a5b8959f27007e55b282ebbd9964950bb897fc29071d16b67b815ec` | `0e72f74f8ac1b3efa06bc0efde78ef8c9bf41a83a6241864fc257b6a89dbdab0` | `b7c04396435b5da9bfaa4193834c13136e3a412821f3cc7373cf66b0774b69ec` |
| `xmlnyugtasend` | `3caf37114072dcb49783bf5034171db4556560cba4263de267200c8d62cd71a4` | `7f37534006d4ec1b1de8c6cbdb55342850a7bf7827a0a2c2e130919b06a3db98` | `6ba13dda3b59b4fbfd472dd8b19e2a0827c9dc5adf7bbf3a2abdc35ae03a2a07` |

Fresh response-page HTML hashes: C `a60d1f598d6f91dd02557f0ddb5bed037c65d16285ae3049b4d2b244a86182ea`; E `a1161fe936eef55293f53a4b0a40b7f694dae5bcc0ea79685bf1fc14eab56fa5`.

## Confirmed finding

### D1 — Low / P3: shared evidence summary still says receipt additions were unobserved

**Locations:** `crates/szamlazz-agent/src/error.rs:369–374`, especially “Neither 55 nor the thirteen receipt/simplified-image additions were observed on the account.” Related stale grouping: `src/recovery.md:57–62`, describing the additions as documentation-sourced, “not live-account observations.”

**Contradicting evidence:** `docs/research/2026-09-11-receipts-live.md:20–37,44–59` records a direct **337** prefix refusal and a deliberately repeated completed create returning **338**, followed by an order query still returning the same original. `docs/szamlazz-hu-behaviour.md:35–43` points to the same executed receipt evidence. Even `error.rs:157–159` now records the observed five-character restriction.

**Vendor contract:** C response says `337` is an invalid receipt prefix and `338` means the call ID already exists; HU says “A hívásazonosító (`hivasAzonosito`) már létezik a szerveren.” The execution record corroborates those two codes on the test account. It does not establish all thirteen additions or live-production behavior.

**Reproducer:** compare the literal no-observations sentence at `error.rs:374` with the dated table and quoted 337 result, then the “repeating the completed, verified create returned 338” bullet. No network or new test is needed; this is a reproducible documentation inconsistency, not a fabricated runtime failure.

**Impact:** readers following the public error/recovery documentation receive stale evidence provenance and may repeat already completed probes or discount established receipt-specific behavior. Classification and wire behavior are unaffected. The receipt-specific API docs themselves already qualify the fresh prefix/MNB evidence correctly.

**Suggested disposition:** qualify the summary by operation, account/date and exact observed codes. Retain the uncertainty on the other codes, concurrent deduplication and retention. No fix made.

## Coverage matrix: requests

In code references below, `receipt.rs` is `crates/szamlazz-agent/src/ops/receipt.rs`; other `.rs`/`.md` names without directories are under `crates/szamlazz-agent/src/`. `1` means exactly one; `?` means 0–1; `+` means 1–unbounded. Optional fields have no XSD default value unless stated; the Rust constructor's omission is distinguished from vendor business behavior.

### Envelope and authentication — all four operations

| Operation | Multipart field; XML root | Location | Result |
|---|---|---|---|
| Create | `action-szamla_agent_nyugta_create`; `xmlnyugtacreate` | receipt.rs:193–195,227–235 | Matches C |
| Storno | `action-szamla_agent_nyugta_storno`; `xmlnyugtast` | :344–355 | Matches S |
| Query | `action-szamla_agent_nyugta_get`; `xmlnyugtaget` | :424–435 | Matches Q |
| Send | `action-szamla_agent_nyugta_send`; `xmlnyugtasend` | :506–518 | Matches E |

Each request uses exactly the `http://www.szamlazz.hu/<root>` namespace. HTTP target is separately **HTTPS** `https://www.szamlazz.hu/szamla/` (`wire.rs:7–14`). `wire.rs:66–100` emits the single XML file part, filename and multipart content type required by each request page. `xml.rs:157–179` writes XML 1.0/UTF-8; schema-location hints are not necessary business fields.

`beallitasok` is required once everywhere. Its optional credential fields are `felhasznalo`, `jelszo`, `szamlaagentkulcs`; the crate emits either the key or username then password (`xml.rs:628–637`). This matches both actual sequence schemas. Create/storno/query additionally require one boolean `pdfLetoltes`, always emitted; constructors default **false**. Send has no PDF setting. **No receipt schema contains `valaszVerzio`**: invoice response-version logic must not be added here.

### Create — complete field inventory

| XML path / cardinality / type | Model, writer, defaults | Assessment |
|---|---|---|
| `fejlec` 1 | receipt.rs:236–253 | Present |
| `hivasAzonosito` ? string | `call_id`, :118–122,237; default None | Caller-controlled, serialized unchanged; no automatic per-send ID generation |
| `elotag` 1 string | `prefix`, :123–129,238 | Required constructor input. Receipt-only prefix (336), uppercase/digits (337), observed max 5; no invoice registration requirement imposed |
| `fizmod` 1 string | `payment_method`, :130–131,239 | Required input; free-text-compatible `PaymentMethod` |
| `penznem` 1 string | `currency`, :132–133,240 | Required input; open Currency, including HUF/Ft and every listed code |
| `devizabank` ? string, `devizaarf` ? double | `exchange_rate`, :134–138,241–246; default None | Correct receipt spellings. Foreign currency requires a rate object, nonblank unpadded bank, and numeric rate unless exact MNB; :212–222 |
| `megjegyzes` ? string | `comment`, :139–140,247; None | Free text, preserved/escaped |
| `pdfSablon` ? string | `template`, :141–142,248–250; None | A/J/L/N or Other; absent/empty defaults A4, invalid fallback documented in C XML |
| `fokonyvVevo` ? string | `ledger_customer`, :143–145,251; None | Complete |
| `rendelesSzam` ? string | `order_number`, :146–153,252; None | Exact case and header-tail placement; separate from call ID |
| `tetelek` 1 / `tetel` + | `items`, :157–159,198–203,254–281 | Empty input rejected; no upper item count invented |
| `megnevezes` 1 string / `azonosito` ? string | item name/id; :260–261 | Item identity, not receipt identity (HU clarifies misleading EN comments) |
| `mennyiseg` 1 double / `mennyisegiEgyseg` 1 string / `nettoEgysegar` 1 double | quantity/unit/unit price; :262–264 | Exact decimal emission, no f64 intermediate |
| `afakulcs` 1 string | `vat_rate`, :265 | Numeric normalization or exact special/unknown token; no closed-code gate |
| `netto`, `afa`, `brutto` each 1 double | net/VAT/gross values; :266–268 | Correct receipt names, caller figures unchanged |
| `fokonyv` ? / `arbevetel`, `afa` each ? string | `LineItemLedger` receipt fields; :269–274 | Empty ledger representable; both children independently optional |
| Item `megjegyzes` ? string | `LineItem.comment`, :275 | Supported |
| Item `torloKod` ? nonnegative int | `erasure_code_count`, :276–278; validation :204–210 | 0–400 enforced at checked boundary; inline schema/rules support, download omits |
| `kifizetesek` ? / `kifizetes` + | `payments`, :160–164,282–292; empty Vec | Empty Vec omits container; nonempty yields unlimited repeated tenders |
| `fizetoeszkoz` 1 string / `osszeg` 1 double / `leiras` ? string | `ReceiptPayment.method/amount/description`, :61–88,285–289 | Correct types; example's `leiras` “double” comment is wrong, XSD says string |

Root order: settings → header → items → payments. Item order: name → ID → quantity → unit → price → VAT token → net/VAT/gross → ledger → comment → erasure count. Both follow C's example. **Create's XSD uses `all`** for these scalar groups; its declaration of rate before bank is not a sequence requirement. `xml.rs:586–614` escapes text, omits None, writes Some("") as an empty element, and emits plain decimal text.

Shared item fields that receipts cannot carry are refused, not dropped: `margin_vat_base`, ledger `economic_event`, `vat_economic_event`, `settlement_from`, `settlement_to` (`receipt.rs:659–690`; `item.rs:52–71,103–114`). This covers every field of the shared `LineItem`/ledger model. No invoice-only template restriction is applied to receipt erasure counts.

### Storno, query and send — complete inventory

| Surface | Fields, order, defaults | Location / assessment |
|---|---|---|
| Storno root | `beallitasok` 1 → `fejlec` 1 | receipt.rs:348–366; true XSD sequence respected |
| Storno header | `nyugtaszam` 1 string → `pdfSablon` ? string → `hivasAzonosito` ? string | :318–340,357–363; target required, template/call ID None, PDF false |
| Query root | `beallitasok` 1, `fejlec` 1 | :428–451; sample order respected, XSD `all` |
| Query header | `nyugtaszam` ? string OR `rendelesSzam` ? string; `hivasAzonosito` ? string; `pdfSablon` ? string | :373–420,437–448; selector enum enforces one number/order selector, stronger than XSD but matches Q/P prose; call ID/template None, PDF false |
| Send root/header | `beallitasok` 1 → `fejlec` 1 containing `nyugtaszam` 1 string → `emailKuldes` ? | :487–528; true XSD sequence respected; API deliberately always emits email container |
| Send email | `email` ? string → `emailReplyto` ? string → `emailTargy` ? string → `emailSzoveg` ? string | :458–475,519–526; four independent options, exact case/order |
| Default send | `SendReceipt::new` has email None, writes `<emailKuldes></emailKuldes>` | :495–503,519–526; documented resend, not absent-container no-op |

No receipt request supports invoice-kind flags, caller dates, `eszamla`, external ID, a full buyer/seller block, email attachments, send template, or order-based storno/send. PHP's Buyer/Seller objects in its send example map to the four email children, not missing party XML. No call-ID-only query is promised by Q or P; optional `hivasAzonosito` is not promoted into an identity selector.

## Coverage matrix: responses and parsing

| Operation | Documented response | Code / assessment |
|---|---|---|
| Create | `xmlnyugtavalasz`, success verdict + `nyugta`; optional base64 `nyugtaPdf` when requested | receipt.rs:297–299,692–715; complete data parser |
| Storno | Same envelope, **new SN**, not original NY; optional SN PDF | :368–370; S: “The response contains the data of the storno receipt, not the original receipt.” |
| Query | Same as creation response | :453–455; Q explicitly reuses C |
| Send | `xmlnyugtasendvalasz`, boolean verdict and optional error fields; no document/PDF payload | :531–537; returns `()` only after verdict |

### Every common receipt response field

| XML path / schema cardinality | Projection / location | Behavior |
|---|---|---|
| `sikeres` 1 boolean; `hibakod` ? int; `hibauzenet` ? string | `xml.rs:461–565` | Required unique boolean fact; true/false/1/0; failure before payload; absent/unknown code preserved; malformed optional diagnostic cannot hide readable failure |
| `nyugta` ? in envelope | receipt.rs:695–696,710–724 | Required on successful create/storno/query by C/S prose; not required on failure |
| `nyugtaPdf` ? string | :697–700,711–712; types.rs:104–116 | Missing/blank → None; standard base64 with whitespace → bytes; corrupt nonblank → parse error |
| `nyugta/alap` 1 | :720,758–800 | Required block |
| `id` 1 int | :549,759,731 | Required i64, wider than XSD int |
| `hivasAzonosito` ? string | :552,760–765,732 | Optional returned creation call ID |
| `nyugtaszam` 1 string | :555,766,733 | Required ReceiptNumber |
| `tipus` 1 NY/SN enum | :558,767,734 | Open ReceiptType retains new tokens; no false known classification |
| `stornozott` 1 boolean | :559–562,768–769,735 | Required boolean; missing/blank/unknown refused. Correct receipt spelling, not invoice `sztornozott` |
| `stornozottNyugtaszam` ? string | :563–565,770–775,736 | Original's number on SN, not SN number on original |
| `kelt` 1 date | :567,776–777,737 | Civil Date, valid timezone suffix discarded without UTC shift |
| `fizmod`, `penznem` each 1 string | :569–571,778–779,738–739 | Open method/currency; EN `cash` retained as Other rather than translated |
| `devizabank` ? string, `devizaarf` ? double | :573–575,780–783,740–741 | Optional bank/Decimal |
| `megjegyzes`, `fokonyvVevo` each ? string | :577–580,784–791,742–743 | Complete |
| `teszt` 1 boolean | :581–585,792–793,744 | Deliberate optional model: absent/empty means unknown, not production |
| `rendelesSzam` ? string | :588,794–799,745 | Nonblank decoded text preserved, not trimmed |
| `tetelek` 1 / `tetel` + | :721,746,802–806 | Container required; empty list deliberately tolerated |
| Row `megnevezes` 1, `azonosito` ? strings | :614–616,810–812,846–847 | Complete |
| Row `mennyiseg` 1 double, `mennyisegiEgyseg` 1 string, `nettoEgysegar` 1 double | :618–622,813–821,848–850 | Required exact Decimal/string |
| Row `afatipus` ? enum, `afakulcs` 1 nonnegative double | :623–629,822–824,851–852 | Both raw texts retained; category takes precedence in `vat_rate()` (:650–657); open read domain |
| Row `netto`, `afa`, `brutto` each 1 double | :630–635,825–830,853–855 | Required Decimal; three `…Ertek` aliases accommodate C's second example item |
| Row `fokonyv` ? / `arbevetel`, `afa` each ? string | :636–648,831–841,856–859 | All response ledger fields represented |
| `kifizetesek` ? / `kifizetes` + | :722–723,747–750,864–868 | Absent or empty → empty Vec; no invoice five-entry cap |
| Tender `fizetoeszkoz` 1 string, `osszeg` 1 double, `leiras` ? string | :870–886 | Same ReceiptPayment projection as request; negative SN tender preserved |
| `osszegek` 1 / `afakulcsossz` + | :724,751; xml.rs:820–848,864–882 | Repeated per-rate subtotals; deliberate tolerance of no subtotals |
| Subtotal `afatipus` ? enum, `afakulcs` 1 nonnegative double, `netto`, `afa`, `brutto` each 1 double | xml.rs:833–848 | Category/raw rate plus exact Decimal triple; types.rs:1088–1094 helper |
| `totalossz` 1 / `netto`, `afa`, `brutto` each 1 double | xml.rs:827–828,850–862,885–892 | Required grand total; no recomputation |

Neither inline nor downloaded response XSD promises row `megjegyzes`, requested erasure count, allocated erasure codes, prefix as a separate field, selected template, stored email details or NAV reporting status. Their absence from `Receipt` is not a proven missing projection. Unknown extension data are not generally exposed as raw XML in this result.

### Shared parser boundaries

`RawResponse::check` (`wire.rs:278–310`) checks nonblank down header, error header, then known non-2xx status before the body. Both body-only receipt errors and header errors are handled; there is no dependency on undocumented receipt success headers. Receipt parses do **not** apply invoice numbered-56 promotion.

`xml.rs:201–301` checks complete XML/UTF-8, correct root and namespace, declarations and lexical validity; `:303–378` filters foreign namespaces before serde. Correct aliases and interleaved repeated rows remain readable; foreign elements cannot provide protocol identity/verdicts. Required scalar and duplicate-singleton tests execute through the receipt paths. DTDs are refused. The parser does not execute XSD validation.

`xml.rs:647–665` reads exact finite numbers without silent loss; `:667–715` reads civil dates without unchecked UTF-8 indexing; `:745–787` distinguishes absent business text and required/optional booleans. This is deliberately not the entire `xs:double`/`xs:date` domain. Empty mandatory string elements are not semantic nonblank identity validation. These are projection/hardening boundaries, not evidence of a conforming vendor response failing in ordinary receipt use.

## Rules, evidence-backed deviations and vendor contradictions

### 1. Prefix, call ID and order are three separate concepts

C response: “if the same XML is posted multiple times, it will not duplicate an existing receipt”; reused ID is **338**, not replayed success. `receipt.rs:94–102,118–122` and `recovery.md:15` correctly require caller persistence and retention through uncertain recovery. Call ID remains optional because the schema permits omission. The client does not generate or persist it.

The executed 337 response adds “**hossza legfeljebb 5 karakter lehet**” (at most five characters), absent from the general prefix table. A new `RSPRB` prefix was accepted without UI registration. `receipt.rs:123–129` correctly describes that as test-account evidence; applying invoice code 202's pre-registration rule would be wrong. Prefix validation is currently delegated to szamlazz.hu; malformed prefix passthrough is not a serialization defect.

Q request says use “either `nyugtaszam` … or `rendelesSzam`”; HU confirms “nyugtaszám … vagy … rendelésszám.” P says exactly one and “last matching document.” The enum and ordinary omission of query call ID comply (`receipt.rs:377–408`). “Last,” post-storno NY/SN selection, query-call-ID meaning, ID retention/scope and concurrent deduplication remain unresolved. The completed-duplicate probe does not settle them.

R says “Receipts have their **own toggle**, separate from the invoice setting.” K's current order article points to receipt-editor settings yet says “**bizonylattípusonként nem állítható be külön**” (cannot be configured separately by document type). This source conflict remains after fresh fetching. Current code/docs follow the Agent-specific source. No settings were inspected. Invoice fingerprint replay, two-day windows, whitespace normalization, external-ID behavior and post-storno order reuse were not imported into receipts.

### 2. Inline and downloaded XSDs disagree

- **Erasure:** C EN/HU inline includes `torloKod`, nonnegative int; R explicitly permits it and caps the count at 400. X-C omits it. The writer at `receipt.rs:276–278` matches the positive documentation.
- **Order query:** Q EN/HU inline and prose include `rendelesSzam`; X-Q omits it. Number/order queries resolving the same receipt are also live-tested. The writer at `:442` is supported.
- **VAT categories:** X-R includes **TEHK**, absent from the inline enumeration. `VatRate::Other` preserves it. Neither source family is universally authoritative/newer.
- **Order:** C/Q say fields “cannot be interchanged,” but use `all`; S/E use actual `sequence`. The code follows sample order and both actual sequences. Rate-before-bank in create's declaration is not grounds for an ordering finding.

Fresh executable reproduction: 6 erasure-enabled generated requests fail X-C and 40 order queries fail X-Q, while the same inputs pass both inline schemas. Removing only erasure counts, or changing only the query selector element to receipt number, makes each download control valid. No unexpected schema failure remained.

### 3. Currency, exchange rates, amounts and tenders

B explicitly applies to receipts and says foreign documents must provide rate and bank, naming `devizaarf`/`devizabank`. `Currency` preserves every string, including HUF/Ft, KSH and KWD (`types.rs:369–477`). Case-insensitive HUF detection is a local convenience, not fresh evidence the server accepts every case variant. Constructor-required payment method/currency are appropriate: PHP cash/Ft defaults are wrapper defaults, not XSD defaults the Rust API must implement.

**Automatic MNB is now executed receipt evidence**, not merely a PHP comment: the EUR receipt omitted numeric rate and returned `MNB`, **363.9**, with unchanged gross 1000. `receipt.rs:212–222,241–246` and `types.rs:966–980` implement and accurately qualify that exception to the general prose. No claim follows for every currency/date or missing bank, zero/negative rate, or generic omitted foreign exchange data.

R/HU amount rule: “**Nincs tűrés**” for net + VAT = gross; HUF/Ft gross whole, net/VAT at most two decimals; product and VAT equations within 2 HUF. Foreign-currency receipts are expressly excluded from those HUF rules. `LineItem::new` preserves explicit `787.40 / 212.60 / 1000`; this exact pattern was accepted in the receipt lifecycle. The writer does not round caller values.

`LineItem::try_calculated` (`item.rs:183–223`) uses exact intermediate arithmetic, explicit half-away-from-zero rounding, then exact net + VAT. Numeric `VatRate::Other` is calculated numerically; named/non-numeric specials get zero derived VAT. `Scale(2)` can yield fractional gross; HUF minor-unit rounding is a stricter local choice, not a universal acceptance guarantee. Current rustdoc says so. ISO-like KWD/JPY precision is local policy, not invoice P60 evidence rebranded as receipt evidence.

R's rejected decimal example is itself misleading: `787.40157480315 + 212.59842519685` equals **exactly 1000** in decimal arithmetic, not `999.999999…`. It still exceeds the documented precision limits. No vendor floating-point explanation is treated as a Rust invariant.

C: the optional `kifizetesek` sum must equal the receipt total; **340** names the refusal. No requirement says each tender must equal `fizmod`. `ReceiptPayment` deliberately uses free text and Decimal, with unbounded count. `receipt.rs:160–162` explicitly delegates sum validation; no hidden five-entry invoice-credit limit or recalculation is present.

### 4. Templates and erasure codes

R template table: **A** normal A4, **N** 80 mm, **J** ticket, **L** ticket with logo. `ReceiptTemplate::as_wire` (`receipt.rs:47–58`) matches all four and passes Other verbatim. None omits, Other("") emits empty, both permit documented A4 default; C's XML comment additionally describes invalid-token fallback. Request JSON's snake-case enum representation is distinct from XML `as_wire`, as with the shared invoice template; no receipt response parses a template token through serde.

Erasure is a **count**, not an arbitrary code string. `item.rs:115–131` and checked receipt validation enforce 400, fit XSD int, and document account enablement (539) and demo/test prohibition (538). General error table and R corroborate the limit. Receipt template rendering and erasure allocation were not executed; invoice-only SzlaMost guidance is not enforced on receipts.

### 5. Email presence, first send and resend

E/HU example: “**e-mail adatok; ha nincs megadva, az előző e-mail kerül újraküldésre**” (when details are not given, the previous email is resent). The inline schema separately says omitted email block means no email. The implementation correctly distinguishes **present-empty container** from **absent container**: default resend always emits the former (`receipt.rs:519–526`). `None` versus `Some("")` remains distinct on every child.

For a first send, supplying all four details is supported by E/P and the executed probe; the send failure example explicitly shows **7**, `Hiányzó adat: emailtargy elem.` Missing-subject 7 is not evidence a receipt does not exist. The API cannot know whether previous details exist, so locally allowing partial/empty details is not inherently erroneous. Partial inheritance, clearing individual fields with empty strings, recipient delimiters, exact message-body/attachment equality remain unverified.

**First-send and delayed present-empty resend both reached the inbox.** Immediate resend instead received **153**, explicitly requesting 15-second spacing. Keeping 153 as `ErrorCode::Unknown` is conservative: one operation-specific observation does not establish a universal code meaning. The 16-second adjustment is test-only pacing, not client retry. Receipt existence alone is not delivery evidence; current send/recovery docs preserve uncertainty.

### 6. Response examples, projection and optional hardening

C's response example includes a literal **`<nyugtaPdf>...</nyugtaPdf>`**, which is not base64. Its second item uses invoice-style `…Ertek`, while response XSDs specify the receipt names. It shows unreversed NY together with an original-number reference; two item gross values sum to **50,800**, tenders sum to **4,000**, grand total is **254**. Empty `hibakod` also contradicts its optional-int schema. These are illustrative source defects, not consistent business fixtures.

Fresh parsing reproduced base64 failure on the exact example. Removing only the placeholder allows all data to parse; substituting synthetic `JVBERi0=` yields `%PDF-` bytes. Existing aliases are justified by the example, not a claim that actual receipt responses normally use invoice spellings. Reported totals are retained rather than fabricated from the inconsistent sample.

**Hardening, not a confirmed conforming-response bug:** nonblank corrupt PDF causes the complete receipt parse to fail (`receipt.rs:697–700`), so typed number/data are unavailable in that result. Missing/blank PDF instead retains identity even if requested. An independently reported artifact failure could improve recovery ergonomics. Current behavior is deliberate and consistent with the base64 contract; no corrupt live response was observed. Base64 validity also does not prove PDF structure/rendering.

Similarly, `parse_receipt` does not assert a query result matches its selector, that a storno response's type/reference matches the request, or that tenders/items/totals reconcile. Callers are directed to verify adoption/recovery identity (`receipt.rs:99–102,380–383`; README:180–219). Unknown tokens, missing test marker and empty lists are projection choices. Missing required reversal is correctly refused. None of these should be reported as a newly observed server problem.

### 7. Receipt reversal differs from invoice reversal

S/HU documents failures for missing original, already reversed original (“**ezt a nyugtát már sztornózták**”), and a storno-receipt target. It assigns no numeric code to each message. The library preserves the returned code/message and describes the distinction (`receipt.rs:302–314`). Synthetic 339 propagation tests do not establish the code for all three cases.

The observed SN has its own number and original reference; the original is reversed. In exact-number recovery, the SN itself also had `reversed=true`, negative quantity/totals/tender, while the original retained positive tenders/totals. This agrees with `Receipt.reversed` being meaningful for NY and must not be replaced with invoice B8's cleared-credit behavior or invoice B4's successful repeated-storno replay. Receipt storno-specific duplicate-call semantics remain open.

### 8. NAV rollout source conflict is already handled by public guidance

R still says “nothing you need to do for now,” automation under development. K EN promises automation from September 1; K HU says September **10**, retrospective reporting, requiring NAV connection and receipt-interface permission. README:171–178 already records the current HU setup and says issuance is not reporting confirmation. None of the four request/response schemas provides a reporting-status field. No additional XML implementation omission was established. Reporting was not tested in the receipt runs.

## Actual receipt execution evidence — not rerun here

Source: [`2026-09-11-receipts-live.md`](../research/2026-09-11-receipts-live.md), especially lines 3–16,20–25,44–118. The record describes observed console/CLI output on an operator-confirmed test account, not archived raw HTTP/PDF files. Account continuity with earlier invoice probes is expressly unestablished.

| Executed case | What it establishes | What it does not establish |
|---|---|---|
| `RSPROBE` prefix refusal | Direct 337; uppercase/digits, maximum five characters | Every account's future rule or prefix collision 336 |
| Lifecycle `b166192e-4de5-4411-80f1-147956119015` | `RSPRB-2026-1` id 87214636; number/order identity, call ID, NY, test, HUF amounts/tender; PDF signatures; completed repeat 338; SN `…-2`, original reversed | Concurrent deduplication, call-ID lifetime, unknown-answer recovery |
| Automatic MNB `4894ec4b-fa77-46da-abee-33454e08179f` | EUR `…-3`, bank MNB, rate 363.9 after omitted numeric rate; SN `…-4` verified | Every currency/date, foreign storage precision |
| Email `9e99201e-65bf-4e80-8613-09e37d1e48b9` | `…-5`: first-send ack; immediate resend refusal 153; later exact-number empty-block send ack; operator confirms both delivered | Independent partial overrides, exact attachments/body equality, automatic retry safety |
| Exact-number cleanup of `…-5` | SN `…-6` id 87214810, original reference, test/order, original reversed; SN negative values/tender, original positive values retained | Invoice-style repeated-storno success, who performed any unknown reversal |

**All three issued originals were verified reversed; no unresolved write/email remains from these runs.** The initial email probe failed at throttling, then recovery succeeded; the newly 16-second-spaced full probe was **not rerun**. The report does not relabel that original probe as a passing end-to-end run.

The old `837dad0` report's “Receipt-specific live execution evidence was not established” and its automatic-MNB “not execution” heading are superseded for these exact observations. NAV, template rendering, erasure allocation, partial email behavior and identity lifetime remain genuinely open. The old invoice behavior tables cannot fill those gaps.

## Executed offline checks

### Existing suite

```sh
cargo test -p szamlazz-agent --all-features --locked --offline --lib --test receipt_wire
cargo test -p szamlazz-agent --all-features --locked --offline --test response_booleans --test response_headers --test response_namespaces --test response_completion --test business_text --test numeric_fidelity --test error_classification --test upstream -- --nocapture
```

Results: **188 library + 6 receipt-wire + 3 boolean + 14 header + 11 namespace + 4 completion + 2 business-text + 6 numeric + 3 error-classification + 11 upstream = 248 passed**, zero failed/ignored. The two printed upstream panics are deliberate passing `should_panic` controls. Upstream tests actually read the workspace corpus (including all receipt examples); these cached tests are not the fresh-fetch audit or vendor execution. Shared/invoice tests ran because the plumbing is shared, not as evidence of receipt business behavior.

### Fresh-schema checks

The existing exporter at `tests/schema_requests.rs:572–707` was inspected: it creates pure dummy-credential requests, no client. It exports exact first-part XML from the checked `to_wire` boundary, both key and password credentials. Executed:

```sh
SZAMLAZZ_SCHEMA_OUTPUT=/tmp/opencode/receipts-77d53c5-audit/requests.json cargo test -p szamlazz-agent --locked --offline --test schema_requests -- --ignored --exact emit_request_matrix
python3 /tmp/opencode/receipts-77d53c5-audit.py
```

Exporter: **1 passed**. Scratch script freshly fetched the 12 request schemas, compiled them with system **libxml2** through Python ctypes, and validated only the receipt rows. It did not substitute the cached fixture schemas. `python` was unavailable, Python 3 lacked `lxml`, and `xmllint` was not on PATH; therefore the workspace `check-agent-schemas.py` command itself was **not run**. The direct libxml2 validator supplied actual XSD validation, not a structural-outline substitute.

| Operation | Generated requests (including both credentials) | EN valid | HU valid | Download valid / expected conflict | Declared paths covered in valid requests, EN/HU/download |
|---|---:|---:|---:|---:|---|
| Create | 40 | 40 | 40 | 34 / 6 | 38/38, 38/38, 37/37 |
| Storno | 40 | 40 | 40 | 40 / 0 | 10/10, 10/10, 10/10 |
| Query | 80 | 80 | 80 | 40 / 40 | 11/11, 11/11, 10/10 |
| Send | 66 | 66 | 66 | 66 / 0 | 12/12, 12/12, 12/12 |

Cases cover all populated create fields, isolated optional header fields, empty ledger, multiple items/tenders, erasure 0/400, explicit/automatic EUR rate; all four known templates plus omission, PDF false/true and call-ID absent/present on storno/query, both query selectors; every combination of four email-child presences with nonempty and empty strings, plus default resend.

**Five negative controls** independently rejected: nonboolean PDF setting, nonnumeric net, missing send target, duplicate send target, reversed email child sequence. Every expected download conflict was additionally checked with only the unsupported field removed/replaced to obtain a valid control. These tests prove representability/schema shape, not arbitrary field values passing vendor business checks or delivery behavior.

### Fresh response examples through public parsers

```sh
cargo build -p szamlazz-agent --locked --offline
rustc --edition=2024 /tmp/opencode/receipts-77d53c5-response.rs --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -L dependency=/home/laborant/szamlazz-rs/target/debug/deps -o /tmp/opencode/receipts-77d53c5-response
/tmp/opencode/receipts-77d53c5-response
```

| Input | Actual result |
|---|---|
| Fresh C response verbatim | `invalid base64 payload: Invalid symbol 46, offset 0.` |
| Remove only `<nyugtaPdf>...</nyugtaPdf>` | Parsed `NYGT-2017-123`, 2 items, 2 tenders, grand gross 254 |
| Replace placeholder with `JVBERi0=` | Create parser returns `%PDF-` bytes; synthetic signature only |
| Same, change only NY to SN | Storno parser retains SN and original reference `NYGT-2017-100`; synthetic projection control |
| Fresh E success verbatim | `Ok(())` |
| Fresh E failure verbatim | `Api(MissingData)`, exact `Hiányzó adat: emailtargy elem.` |

Both downloadable response schemas were read in full. Response examples were not represented as schema-valid financial records; their placeholders, empty int and alias fields prevent that claim. No live/probe invocation was made by any check above.

## Final disposition

- **Runtime/API conformance:** no confirmed defect found across the four current receipt operations and relevant shared behavior.
- **Documentation:** one low-severity stale evidence summary (D1), with exact contradictory execution evidence.
- **Vendor contradictions:** missing download fields, mixed `all`/fixed-order prose, differing VAT enumeration, inconsistent examples, order-toggle wording and NAV rollout wording are explicitly separated from implementation faults.
- **Evidence-backed deviations:** automatic MNB, query by order despite old download, five-character prefix observation and delayed empty-block resend are supported at their stated evidence level.
- **Remaining hardening/unknowns:** corrupt-artifact identity retention, semantic adoption checks, partial email overrides, call-ID lifetime/concurrency, post-storno order selection, template rendering, erasure allocation and NAV execution. None is falsely reported as settled by invoice-only probes.
