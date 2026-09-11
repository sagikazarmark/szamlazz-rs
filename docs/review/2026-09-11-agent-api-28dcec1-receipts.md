# Számla Agent receipt operations — independent review at `28dcec1`

## Verdict

**No confirmed runtime conformance defect found in the four current receipt operations. One P3 documentation finding: the shared error/recovery evidence summary still denies observations of receipt codes that the September 11 execution record establishes.** All declared request fields are supported and all declared response data fields are projected. The intentional absence of an email-container no-op from `SendReceipt` is described below.

Fresh checks: **226 generated receipt requests × three freshly fetched schemas = 678 validations: 632 valid, 46 explained download-schema conflicts.** All 452 EN/HU inline validations passed. Four negative schema controls passed. Fresh response examples were exercised through the public parsers; the receipt example's literal `...` PDF is invalid base64, while its data parses when that placeholder alone is removed. These are offline checks, not proof of vendor execution.

### Scope and provenance

- Reviewed on **2026-09-11**, starting commit and current HEAD **`28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`**. `git diff 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2...HEAD` was empty: this is the requested **whole-current implementation audit**, not an empty-diff review.
- Primary code: `crates/szamlazz-agent/src/ops/receipt.rs`; related `item.rs`, `types.rs`, `xml.rs`, `wire.rs`, `client.rs`, `error.rs`, `recovery.md`, README and receipt/schema tests. Line references below are to this commit; bare source filenames mean `crates/szamlazz-agent/src/`, with `receipt.rs` meaning `src/ops/receipt.rs`.
- Public vendor documentation was fetched anew, including four operation request/response/XML pages, five receipt rules, all four EN/HU inline request schemas, all six downloadable request/response schemas, and relevant PHP/knowledge-base material. Agent pages displayed **`v202608271632`**; that is a site-build label, not proof of paragraph currency.
- Read the actual [receipt execution record](../research/2026-09-11-receipts-live.md) and [behavior record](../szamlazz-hu-behaviour.md). Their receipt evidence is distinguished from invoice observations and from test definitions throughout.
- No subagents, live requests, `.env` access, product edits or broad test run. The parent owns broad cargo testing. Scratch scripts, fetched schemas, exported requests and an offline parser executable use `/tmp/opencode/receipts-28dcec1-*`.
- The pre-existing untracked `77d53c5` reviews were preserved. Its receipt report was read **after** the initial source/implementation comparison, as a lead for the working send-response XSD URL and remaining evidence questions. Its test results were not adopted; all results claimed here were executed for this review.

## Prioritized finding

### R1 — P3 / low: stale shared evidence summary contradicts executed receipt observations

**Confidence: high.** Documentation defect; no change to runtime classification is implied.

**Locations:** [`error.rs:369–374`](../../crates/szamlazz-agent/src/error.rs#L369), especially “Neither 55 nor the thirteen receipt/simplified-image additions were observed on the account”; also [`recovery.md:57–62`](../../crates/szamlazz-agent/src/recovery.md#L57), which groups these additions as documentation-sourced, “not live-account observations.”

**Evidence:** `docs/research/2026-09-11-receipts-live.md:20–37,44–59` records direct **337** on the seven-character prefix and **338** after a deliberately repeated, completed and verified create. `docs/szamlazz-hu-behaviour.md:35–43` links those observations. `error.rs:157–159` itself already records the newly observed prefix-length rule. The freshly fetched [create response](https://docs.szamlazz.hu/agent/generating_receipt/response) independently documents 337 as invalid prefix and 338 as an existing call ID.

**Impact:** public error/recovery rustdoc misstates the evidence available for receipt recovery. Readers may discount receipt-specific evidence or repeat already completed experiments. It remains true that most added codes were not executed; this is not evidence of universal code behavior, concurrent deduplication or production-account behavior.

**Recommendation:** identify the exact observed codes and account/date, retaining the remaining unknowns. No fix made. Reproduction is the direct comparison of these source lines with the dated execution bullets; no live call is necessary.

No P0–P2 runtime finding was established. The optional hardening opportunities below are not promoted into defects of conforming vendor responses.

## Fresh authoritative source register

Each listed URL was fetched during this review; “XML” includes the example and inline schema tab.

| ID | Sources |
|---|---|
| C | Create [request](https://docs.szamlazz.hu/agent/generating_receipt/request), [response + example/XSD](https://docs.szamlazz.hu/agent/generating_receipt/response), [XML + XSD](https://docs.szamlazz.hu/agent/generating_receipt/xml) |
| S | Storno [request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [response/refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response), [XML + XSD](https://docs.szamlazz.hu/agent/reversing_receipt/xml), [HU response](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response) |
| Q | Query [request](https://docs.szamlazz.hu/agent/querying_receipt/request), [response](https://docs.szamlazz.hu/agent/querying_receipt/response), [XML + XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml) |
| E | Send [request](https://docs.szamlazz.hu/agent/sending_receipt/request), [response + examples/XSD](https://docs.szamlazz.hu/agent/sending_receipt/response), [XML + XSD](https://docs.szamlazz.hu/agent/sending_receipt/xml), [HU XML](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml) |
| H | HU inline request schemas: [create](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml), [storno](https://docs.szamlazz.hu/hu/agent/reversing_receipt/xml), [query](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml), [send](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml) |
| R | [Settings index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules), [NAV reporting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting), [order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number), [PDF template](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template), [erasure code](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code), [amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) / [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts) |
| B | Receipt-linked [currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies), [errors/retry limit](https://docs.szamlazz.hu/agent/basics/error-handling), [sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) |
| P | Official PHP [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [query](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [send](https://docs.szamlazz.hu/php/nyugta-kuldes), [PDF](https://docs.szamlazz.hu/php/nyugta-pdf); [2.12.4 source archive](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) |
| K | Knowledge base [order number](https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan), reporting [HU](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg) / [EN](https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting) |
| XC | [Create download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd) |
| XS | [Storno download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd) |
| XQ | [Query download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd) |
| XE | [Send download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd) |
| XR | [Receipt response download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd) |
| XER | [Send response download XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd) |

The send example's `https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd` returned **404**. A guessed `/szamla/docs/xsds/nyugtasendvalasz/xmlnyugtasendvalasz.xsd` also returned 404; XER succeeded. A guessed PHP `/php/nyugta-keszites` returned 403; the linked `/php/nyugta-generalas` succeeded. Failed URLs were not counted as read schemas. The PHP ZIP was fetched/read in memory with Python 3, not executed.

## Complete request coverage

Notation: **1** exactly one, **?** zero or one, **+** one or more without a schema upper bound. Unless stated otherwise, optional fields have no XSD default. Rust constructor defaults and vendor fallback behavior are separate facts.

### Transport, namespaces and settings

| Operation | Multipart field / root | Current code |
|---|---|---|
| Create | `action-szamla_agent_nyugta_create` / `xmlnyugtacreate` | `receipt.rs:193–195,227–235` |
| Storno | `action-szamla_agent_nyugta_storno` / `xmlnyugtast` | `receipt.rs:344–355` |
| Query | `action-szamla_agent_nyugta_get` / `xmlnyugtaget` | `receipt.rs:424–435` |
| Send | `action-szamla_agent_nyugta_send` / `xmlnyugtasend` | `receipt.rs:506–518` |

All match C/S/Q/E and B. Each root uses exactly **`http://www.szamlazz.hu/<root>`** as its namespace; that identifier is distinct from the HTTPS POST endpoint **`https://www.szamlazz.hu/szamla/`** (`wire.rs:7–14`). `wire.rs:66–100` writes the XML as one multipart **file** with content type `text/xml`, a filename and the correct action name; no bulk-receipt body is offered. `xml.rs:157–179` writes XML 1.0/UTF-8 with default namespace inherited by children. `xsi:schemaLocation` is an optional schema hint, not a missing business field.

All four requests require one `beallitasok`. The schema permits `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` (strings). `xml.rs:628–638` emits either agent key or username then password; authentication is not optional merely because these individual elements are. Create/storno/query additionally emit required boolean `pdfLetoltes`, default **false**. Send has no PDF flag. **None of the receipt request schemas contains `valaszVerzio`**; no invoice response-version field should be inserted.

`to_wire` performs validation before serialization and checks XML 1.0 characters (`wire.rs:405–411`); `write_xml` is explicitly unchecked (`:367–373`). Text is escaped, `None` omits a child and `Some("")` writes an empty element (`xml.rs:586–614`). No hidden automatic call-ID generation or receipt retry loop appears in this path.

### Create: every field

| XML path, cardinality and type | Public field / serialization lines | Assessment |
|---|---|---|
| `fejlec` 1 | `receipt.rs:236–253` | Present once |
| `hivasAzonosito` ? string | `call_id`, `:118–122,237` | None by default; caller controls persistent logical-call ID |
| `elotag` 1 string | `prefix`, `:123–129,238` | Constructor input; receipt-only prefix, no invoice registration gate |
| `fizmod` 1 string | `payment_method`, `:130–131,239` | `PaymentMethod` supports known Hungarian tokens and free text |
| `penznem` 1 string | `currency`, `:132–133,240` | Open `Currency` retains supplied code, including HUF/Ft and every listed foreign code |
| `devizabank` ? string | `exchange_rate.bank`, `:134–138,241–242` | Correct receipt spelling; None object omits both fields |
| `devizaarf` ? double | `exchange_rate.rate`, `:243–245` | Decimal emitted without f64 intermediate; omission supported for automatic MNB |
| `megjegyzes` ? string | `comment`, `:139–140,247` | Preserved and escaped |
| `pdfSablon` ? string | `template`, `:141–142,248–250` | A/J/L/N and Other; default absent |
| `fokonyvVevo` ? string | `ledger_customer`, `:143–145,251` | Complete |
| `rendelesSzam` ? string | `order_number`, `:146–153,252` | Exact name, case and example placement; distinct from call ID |
| `tetelek` 1 / `tetel` + | `items`, `:157–159,198–203,254–281` | Empty input refused; unlimited repeated rows representable |
| `tetel/megnevezes` 1 string | `LineItem.name`, `:260` | Name of item |
| `tetel/azonosito` ? string | `LineItem.id`, `:261` | Item identifier, not receipt identifier |
| `mennyiseg` 1 double | `quantity`, `:262` | Decimal |
| `mennyisegiEgyseg` 1 string | `unit`, `:263` | Free text |
| `nettoEgysegar` 1 double | `unit_price`, `:264` | Decimal |
| `afakulcs` 1 string | `vat_rate`, `:265` | Normalized numeric rate or known/unknown special token |
| `netto`, `afa`, `brutto` each 1 double | `net_value`, `vat_value`, `gross_value`, `:266–268` | Receipt element names, supplied amounts unchanged |
| `fokonyv` ? | `ledger`, `:269–274` | Empty block and partial block representable |
| `fokonyv/arbevetel`, `fokonyv/afa` each ? string | `revenue_account`, `vat_account`, `:271–272` | Both supported independently |
| Item `megjegyzes` ? string | `comment`, `:275` | Supported request-only field |
| Item `torloKod` ? nonnegative int | `erasure_code_count`, `:276–278` | Inline XSD/rules supported; checked range 0–400 (`:204–210`) |
| `kifizetesek` ? / `kifizetes` + | `payments`, `:160–164,282–292` | Empty Vec omits block; nonempty Vec emits all tenders |
| `fizetoeszkoz` 1 string | `ReceiptPayment.method`, `:71–73,286` | Tender free text |
| `osszeg` 1 double | `ReceiptPayment.amount`, `:74–75,287` | Decimal |
| `leiras` ? string | `ReceiptPayment.description`, `:76–77,288` | String despite erroneous “double” comment in example |

Root order is settings → header → items → tenders. Row order is name → ID → quantity → unit → unit price → VAT token → net/VAT/gross → ledger → comment → erasure count. This matches C's example. Create's scalar groups use **`xs:all`**, despite fixed-order prose; the XSD listing rate before bank does not impose a sequence. The code's bank-before-rate follows the example and PHP writer.

Every shared `LineItem`/ledger field is accounted for. Unsupported receipt fields (`margin_vat_base`, ledger `economic_event`, `vat_economic_event`, `settlement_from`, `settlement_to`) are refused, not silently dropped (`receipt.rs:659–690`; `item.rs:52–71,90–126`). No invoice-only margin/settlement element or invoice template restriction is added to receipts.

### Storno, query and send: every field

| Operation / block | Fields and cardinality | Code / conclusion |
|---|---|---|
| Storno root | `beallitasok` 1 → `fejlec` 1 | `receipt.rs:348–366`; real XSD sequence respected |
| Storno header | `nyugtaszam` 1 string → `pdfSablon` ? string → `hivasAzonosito` ? string | `:318–340,357–363`; target required, template and call ID default None |
| Query root | `beallitasok` 1, `fejlec` 1 | `:428–451`; sample order, XSD `all` |
| Query header | `nyugtaszam` ? string OR `rendelesSzam` ? string; `hivasAzonosito` ? string; `pdfSablon` ? string | `:373–420,437–448`; selector enforces exactly one number/order, matching Q/P prose; call ID/template default None |
| Send root/header | `beallitasok` 1 → `fejlec` 1 / `nyugtaszam` 1 string → `emailKuldes` ? | `:487–528`; true sequence respected; email block always emitted |
| Send email | `email` ? string → `emailReplyto` ? string → `emailTargy` ? string → `emailSzoveg` ? string | `:458–475,519–526`; all children independently optional, correct case and order |
| Default send | Present-empty `emailKuldes` | `:495–503,519–526`; None means documented resend, not omitted-block no-op |

Query's XSD technically permits neither/both identifiers; Q request and P query require one. The enum is a justified tighter interface. No source promises a call-ID-only query. Storno/send take receipt number only, not order. None of these operations has a caller date, invoice appearance, external ID, full seller/buyer block, attachment list or email-send template. PHP's Buyer/Seller objects populate email children, not additional missing XML party blocks.

The XSD permits omitting `emailKuldes`, with inline EN/HU commentary saying that omission sends no email. `SendReceipt` deliberately exposes the useful send/resend operation and cannot express that no-op. This is not a lost sending capability. Absence of individual children is still distinct from present empty strings.

## Complete response coverage

Create (`receipt.rs:297–299`), storno (`:368–370`) and query (`:453–455`) share **`xmlnyugtavalasz`**, namespace `http://www.szamlazz.hu/xmlnyugtavalasz`. S explicitly says the returned document is the **new storno receipt (`SN`)**, not the original. Q explicitly reuses C's response. Send (`:531–537`) checks **`xmlnyugtasendvalasz`**, namespace `http://www.szamlazz.hu/xmlnyugtasendvalasz`, then returns `()`.

| Response field / XSD cardinality and type | Projection / source lines | Assessment |
|---|---|---|
| `sikeres` 1 boolean | `xml.rs:467–499` | Required unique true/false/1/0; empty or malformed is not success |
| `hibakod` ? int | `xml.rs:481–485,502–517`; `ErrorCode` | Read as open token; absent/blank → Absent; unknown retained |
| `hibauzenet` ? string | `xml.rs:486–499,515` | Decoded diagnostic preserved; malformed/duplicate diagnostic cannot hide a readable refusal |
| `nyugta` ? in envelope | `receipt.rs:695–696,710–725` | Required on success by prose; failure is read before document payload |
| `nyugtaPdf` ? string | `:697–700,711–712`; `types.rs:104–116` | Missing/blank → None; standard base64 with whitespace decoded; corrupt nonblank → parse error |
| `nyugta/alap` 1 | `:720,758–800` | Required |
| `alap/id` 1 int | `Receipt.id`, `:549,759,731` | Required i64, wider than XSD int |
| `hivasAzonosito` ? string | `call_id`, `:552,760–765,732` | Optional returned call identity |
| `nyugtaszam` 1 string | `receipt_number`, `:555,766,733` | Required unvalidated wire-number newtype |
| `tipus` 1 enum NY/SN | `document_type`, `:558,767,734`; `types.rs:861–940` | Open enum preserves unknown token |
| `stornozott` 1 boolean | `reversed`, `:559–562,768–769,735` | Required receipt boolean, unlike optional invoice `sztornozott` |
| `stornozottNyugtaszam` ? string | `reversed_receipt_number`, `:563–565,770–775,736` | Original number on SN; not SN number on original |
| `kelt` 1 date | `issue_date`, `:567,776–777,737` | Civil date, complete timezone suffix discarded without shifting day |
| `fizmod` 1 string | `payment_method`, `:569,778,738` | Open; example's English `cash` remains Other |
| `penznem` 1 string | `currency`, `:571,779,739` | Supplied token retained |
| `devizabank` ? string / `devizaarf` ? double | `exchange_bank` / `exchange_rate`, `:573–575,780–783,740–741` | Optional text/Decimal |
| `megjegyzes`, `fokonyvVevo` each ? string | `comment`, `ledger_customer`, `:577–580,784–791,742–743` | Complete |
| `teszt` 1 boolean | `test`, `:581–585,792–793,744` | Deliberate optional read: absent/empty is unknown, never production |
| `rendelesSzam` ? string | `order_number`, `:588,794–799,745` | Nonblank decoded business text preserved, including surrounding whitespace |
| `tetelek` 1 / `tetel` + | `items`, `:721,746,802–806` | Container required; empty list intentionally tolerated |
| Row `megnevezes` 1 / `azonosito` ? strings | `name`, `id`, `:614–616,810–812,846–847` | Complete |
| Row `mennyiseg` 1 double / `mennyisegiEgyseg` 1 string / `nettoEgysegar` 1 double | `quantity`, `unit`, `unit_price`, `:618–622,813–821,848–850` | Exact finite numbers/string |
| Row `afatipus` ? enum / `afakulcs` 1 nonnegative double | `vat_type`, `vat_rate_code`, `:623–629,822–824,851–852` | Raw category/rate retained; category takes precedence in `vat_rate()` (`:650–657`) |
| Row `netto`, `afa`, `brutto` each 1 double | `net_value`, `vat_value`, `gross_value`, `:630–635,825–830,853–855` | Decimal; aliases accept C example's `nettoErtek`, `afaErtek`, `bruttoErtek` |
| Row `fokonyv` ? / `arbevetel`, `afa` each ? string | `ReceiptItemLedger`, `:636–648,831–841,856–859` | Every response ledger field covered |
| `kifizetesek` ? / `kifizetes` + | `payments`, `:722–723,747–750,864–868` | Absent/empty → empty Vec; no invoice credit-entry count limit |
| Tender `fizetoeszkoz` 1 string / `osszeg` 1 double / `leiras` ? string | `ReceiptPayment`, `:870–886` | All fields, including negative SN amount, retained |
| `osszegek` 1 / `afakulcsossz` + | `totals.by_vat_rate`, `:724,751`; `xml.rs:820–848,864–882` | Repeated subtotals; no subtotals tolerated |
| Subtotal `afatipus` ? enum / `afakulcs` 1 nonnegative double | `VatTotal.vat_type`, `vat_rate_code`; `xml.rs:833–838` | Both retained, open token reading |
| Subtotal `netto`, `afa`, `brutto` each 1 double | `VatTotal.net/vat/gross`; `xml.rs:839–847` | Required Decimal |
| `totalossz` 1 / `netto`, `afa`, `brutto` each 1 double | `totals.total`; `xml.rs:827–828,850–862,885–892` | Required grand total, no recomputation |

Send's entire response field set is the first three rows; neither document nor PDF is promised. Neither receipt response schema declares row comment, erasure count/allocated codes, separate prefix, selected template, stored email details or NAV reporting status. Their absence from `Receipt` is therefore not a proven missing projection.

### Parser boundaries, cardinality and error semantics

- `wire.rs:278–310`: nonblank `szlahu_down` → unavailable; else error-code header → API error; else known non-2xx → HTTP-status error; only then body. Header lookup is case-insensitive and textual headers are decoded once (`:227–270`). Receipt body-only errors work. No undocumented receipt success header is necessary.
- `xml.rs:201–301` checks complete XML, UTF-8, root namespace, declaration and lexical syntax, refusing DTDs. `:303–378` filters foreign-namespace subtrees before serde so foreign lookalikes do not supply protocol identity/verdict fields. Known scalar duplicates are refused by deserialization; unknown extensions are tolerated. This is not an XSD validator.
- Required booleans use XML lexical forms (`xml.rs:759–787`); optional business text treats XML-whitespace-only as absent and otherwise preserves decoded content (`:745–757`). Numeric reading is finite/exact (`:647–665`), not the entire `xs:double` range. Dates have a finite civil domain and some intentionally lenient legacy spellings (`:667–715`). Monetary infinities/NaN and precision loss are not silently accepted. No conforming ordinary receipt response failure was established from these domain choices.
- Response `all` groups permit varying field order; repeated item/tender/rate groups are vectors. Empty items/tenders/rate groups and missing test marker are explicitly tolerated beyond XSD minima. Mandatory strings can be empty: there is no separate nonblank-number or cross-field semantic validation.
- Fresh C supplement codes **336/337/338/340** map to named refusals; **339** maps to receipt not found (`error.rs:155–166,297–305,376–423`). 338 refuses this duplicate exchange, not the earlier issuance. Code **7** on send can mean missing email subject, as the exact E example demonstrates; its `NotFound` class is explicitly operation-dependent (`error.rs:43–55,451–455`).
- HUF amount codes **261/363/364/365**, erasure **537/538/539**, credentials **3/135/136/164**, malformed/missing-file **57/53** are represented. **71/152** retain duplicate-order classification. The known error table is open; absent codes and unknown codes, including observed email **153**, remain Unknown rather than invented refusals/retry permission. S/HU supplies messages but no numeric code for each of missing original, already reversed and storno-target refusal; do not infer all three are 339.
- There is no invoice-style numbered-56 success promotion in receipt parsing (`xml.rs:528–553`, `receipt.rs:694–706`). Receipt creation has no email-send fields; the invoice warning convention is not established for these operations. Unknown/parse/transport/unavailable errors remain uncertain (`client.rs:103–126`).
- `recovery.md:4–20,24–42` correctly distinguishes an exchange from earlier unresolved sends and email delivery from receipt existence. Vendor B allows at most five total sends of the same request, then operator intervention; `is_retryable` is not write permission. No automatic application-level retry is implemented; supplied HTTP-client retry policy remains active (`client.rs:193–207`).

## Business rules, source conflicts and evidence exemptions

### Identity and receipt reversal

C response says a unique `hivasAzonosito` makes repeated XML fault tolerant by preventing another receipt; it returns **338**, not the original success. Current guidance (`receipt.rs:94–102,118–122`) appropriately asks callers to persist it before sending and retain it during recovery. Optionality matches XSD. The record demonstrates a completed duplicate, not concurrency, scope, retention or a lost-answer recovery guarantee.

Prefix, call ID, order number and returned receipt number/id are separate. The observed 337 adds **maximum five uppercase letters/digits**; new `RSPRB` was accepted without UI registration. C documents receipt-prefix collision with an invoice prefix (336), which was not executed. PHP's EN example uses `RECEIPT` (seven letters), so copying that example would contradict the bounded live restriction; the Rust docs already qualify it correctly. Prefix validation remains server-side, not a serializer defect.

Q/P provide number or order lookup, with P's **last matching document** wording. `QueryReceipt.call_id` is optional wire metadata with unspecified behavior, not a third selector. The PHP `ReceiptHeader` query writer itself omits call ID. Exact last criterion, NY/SN selection after storno, order normalization and reuse after reversal remain unknown for receipts. Invoice two-day fingerprints, case/whitespace observations and external-ID behavior do not establish receipt rules.

S EN/HU expressly documents refusals for already reversed receipts and storno targets. The code's recovery documentation preserves this distinction. The actual SN has its own number and original reference; the original becomes reversed. The recorded SN itself also reports `reversed=true`, negative quantity/totals/tender, while the original retains positive tenders/totals. This supports `Receipt.reversed` being meaningful for NY and forbids importing invoice B8's cleared-credit behavior. A reversal marker alone neither recovers the SN PDF/number nor identifies who reversed it.

### Exchange rates, arithmetic and tender totals

- B explicitly covers **receipts**, with bank/rate required for foreign currency, using `devizabank`/`devizaarf`. `receipt.rs:212–222` requires a rate object, an unpadded nonblank bank, and a numeric rate unless the bank is exactly `MNB`. HUF/Ft may omit the object. `Currency` retains every code (`types.rs:369–477`); case-insensitive HUF recognition is a local convenience, not execution evidence for lowercase spellings.
- **Automatic MNB exception is supported.** Fresh PHP 2.12.4 `src/szamlaagent/Header/ReceiptHeader.php` says `MNB` plus absent rate uses the current rate; its separate rate comment also mentions absent/zero and a currency in MNB's database. `examples/document/receipt/create_receipt_with_custom_data.php` repeats that comment but actually supplies **300.0**, so the example is not an omitted-rate execution. The September 11 EUR receipt actually omitted the numeric field and queried back **MNB / 363.9**, with gross 1000. `types.rs:966–980` accurately qualifies this evidence. Zero rate is representable; no live conclusion is made for zero, negative rates, missing bank, non-MNB omission or every currency/date. Broad PHP omission wording does not override the more specific bank condition into a confirmed missing-bank feature.
- R EN/HU: HUF/Ft row gross must be whole; net/VAT at most two decimals; net + VAT exactly gross; quantity × unit price and net × VAT percentage within **2 HUF**. Foreign-currency receipts are expressly excluded from these HUF restrictions. `LineItem::new` preserves the accepted **787.40 / 212.60 / 1000** values. No five-forint rounding requirement is stated here.
- `item.rs:183–223` computes exact intermediates, then explicit half-away-from-zero rounding and exact addition. `Scale(2)` alone can produce fractional gross; HUF minor-unit rounding is stricter local arithmetic, not universal server acceptance. The rustdoc says so (`item.rs:9–29,74–87,163–180`; `receipt.rs:104–108`). Invoice P60 two-decimal storage observations and ISO-like KWD/JPY precision are not receipt storage evidence.
- `payments` may be omitted, but when present tender amounts must sum to the receipt total, else **340**. The writer supports all tenders; it does not recompute/validate that sum (`receipt.rs:160–162,282–292`). Delegation is explicit, consistent with the shared amount policy; absence of local validation is not claimed as a wire defect. No source requires every tender method to equal header `fizmod`, nor imposes the invoice five-credit-entry limit.
- R's numerical explanation is internally wrong in both locales: `787.40157480315 + 212.59842519685` equals **exactly 1000 in decimal arithmetic**, not `999.999999…`. It still violates the stated precision limits. Do not reproduce that source explanation as a Rust arithmetic invariant or claim to have executed its precise server error ordering.

### Templates, images, PDF and email resend

`ReceiptTemplate::as_wire` (`receipt.rs:25–58`) covers **A = A4, N = 80 mm, J = ticket, L = ticket with logo**, plus Other unchanged. Omission/empty uses A4; C's example comment additionally documents invalid-token fallback. The JSON snake-case enum encoding is a caller representation, not XML token parsing. No fetched receipt schema offers image bytes, logo URL/upload, bitmap rendering, invoice `simpleItems`, or language selection. L is a server-rendered template choice; lack of image-upload fields is not an omission from this contract.

Create/storno/query PDF requests use `pdfLetoltes`, with base64 `nyugtaPdf` beside the data. Query can request a template; send cannot select one. The crate decodes bytes but does not validate PDF structure or render images (`types.rs:104–116`). The live checks establish `%PDF-` signatures only; all four layouts, logo contents, printed erasure codes and stored-versus-regenerated image behavior were not inspected.

Erasure is a **count**, not an arbitrary code identifier. R and PHP agree on optional nonnegative integer; business limit is **400**, account enablement required (539), prohibited on test/demo (538). The Rust u32 plus maximum check fits XSD int. Invoice-specific `SzlaMost` conditions are not applied to receipt templates. No successful erasure allocation is inferred from a schema check or test-account receipt lifecycle.

E EN/HU example says missing email **details** resend the previous email; inline schema says missing **container** sends none. Default `SendReceipt` produces the present-empty container, correctly implementing the former. Four independent options preserve absent versus empty children (`receipt.rs:458–475,519–526`). A first send with all four values and one recipient is supported by P/E and the record. The client cannot know prior email state; allowing partial fields is not inherently wrong, but independent inheritance/clearing semantics and recipient delimiters are not established.

The executed first send was acknowledged; immediate empty-block resend received **153**, asking at least 15 seconds between notifications. Later deliberate exact-number resend was acknowledged, and the operator confirmed **both emails arrived**. This is stronger than existence/query evidence, but does not establish exact body/attachment equality. The subsequent 16-second test pacing change was **not rerun** as a complete probe. The library does not auto-retry or promise that fixed pacing avoids account-wide throttling. Lost send acknowledgement remains uncertain; another send may duplicate delivery.

### Conflicts that do not justify product changes

| Conflict | Fresh evidence and disposition |
|---|---|
| Inline create has `torloKod`, XC omits it | R explicitly documents it, PHP calls it a count. Keep support. Six generated cases fail only XC. No live allocation claimed. |
| Inline query has `rendelesSzam`, XQ omits it | Q/P explicitly support order queries; recorded number/order queries found the same receipt. Keep support. Forty cases fail only XQ. |
| XR adds `TEHK`, absent from inline response enum | Open VAT token handling preserves it. Download is not uniformly older/newer than inline. |
| Fixed-order prose versus `all` | Create/query scalar groups use `all`; storno/send use `sequence`. Current writers satisfy the actual sequences and follow examples elsewhere. |
| Receipt order toggle | R EN/HU say **separate receipt toggle**. K points to receipt-editor settings yet also says “bizonylattípusonként nem állítható be külön” (not separately configurable by document type), including invoices/proformas. Current crate follows Agent-specific R; no account-toggle experiment resolves the contradiction. |
| NAV rollout | R says automation is being developed and nothing to do yet. K EN promises September 1; K HU says **September 10**, retrospective reporting and required NAV connection/receipt-interface permission. README `:171–178` already reflects the HU setup and says issuance is not reporting proof. No reporting-status XML field was found or live reporting verified. |
| C response example | Literal PDF `...`; empty integer `hibakod`; second row uses invoice `…Ertek`; two gross rows total **50,800**, tenders **4,000**, grand total **254**; unreversed NY includes an original-reference field. It is not a financially consistent execution fixture. Existing aliases/read leniency are justified; synthetic totals must not drive reconciliation. |

### Optional hardening, not confirmed protocol defects

1. **Retain typed identity alongside a corrupt PDF error.** `receipt.rs:694–706` loses the typed receipt result when nonblank PDF decoding fails, although number/data were readable. Current strict-artifact policy and Unknown classification are conservative, and the invalid official placeholder is not a valid PDF. A separate artifact diagnostic could improve create/storno recovery; no corrupt live reply was observed.
2. **Optional semantic verification at adoption boundaries.** The shared parser does not compare query selector with returned number, assert SN/original-reference consistency against a storno request, reconcile sums, or require nonblank identity. The scratch control confirms a different number selector still parses the response. This is a data projection whose callers are already instructed to verify recovery identity, not evidence of incorrect vendor behavior. Missing requested PDF also yields `None` without losing the document.
3. **Evidence gaps remain real.** Call-ID lifetime/scope/concurrency, storno-specific 338 semantics, post-storno order selection, partial email inheritance, all template renderings, erasure allocation and reporting are unresolved. Neither old invoice probes nor compilation fills them.

## Actual receipt live evidence — not rerun here

The source is `docs/research/2026-09-11-receipts-live.md:3–16,20–25,44–118`, corroborated by `docs/szamlazz-hu-behaviour.md:35–43`. It transcribes observed console/CLI output; raw HTTP/PDF files were not archived. Account continuity with historical invoice probes is explicitly not established.

| Recorded execution | Established fact | Limit |
|---|---|---|
| Seven-character `RSPROBE` | Direct 337; at most five uppercase letters/digits | Does not execute prefix collision 336 |
| Lifecycle call/order `b166192e-4de5-4411-80f1-147956119015` | `RSPRB-2026-1`, id 87214636; number/order/call identity, NY, test=true, HUF amounts/tender, PDF signatures; completed repeat 338; verified SN `…-2` and original reversed | Not concurrency, retention or lost-answer recovery |
| MNB call/order `4894ec4b-fa77-46da-abee-33454e08179f` | EUR `…-3`, id 87214687; omitted numeric rate → MNB / 363.9; verified reversal `…-4` | One account/request/date/currency |
| Email call/order `9e99201e-65bf-4e80-8613-09e37d1e48b9` | `…-5`, id 87214712; first send ack, immediate resend 153, delayed exact-number resend ack; operator confirmed both deliveries | Original probe failed at throttling; adjusted full probe not rerun; no exact content equality |
| Exact-number cleanup | SN `…-6`, id 87214810; original/reference/order/test verified, negative SN tender/totals, positive original retained | Not invoice-style repeated-storno success |

All three issued originals were verified reversed; the record reports no unresolved create/storno/email from those executions. This review performed no additional operation.

## Executed checks and limitations

### Fresh request XSD validation

Inspected `tests/schema_requests.rs:32–91,572–707`: the exporter uses dummy credentials, calls `to_wire`, extracts the exact first multipart XML file and writes JSON. It does not construct an HTTP client. Executed:

```sh
SZAMLAZZ_SCHEMA_OUTPUT=/tmp/opencode/receipts-28dcec1-matrix.json cargo test -p szamlazz-agent --locked --offline --test schema_requests -- --ignored --exact emit_request_matrix
python3 /tmp/opencode/receipts-28dcec1-check.py
```

Exporter **1 passed**. The new scratch script fetches EN/HU HTML and downloads afresh, extracts the inline XSDs, compiles them with system **libxml2** via ctypes, then validates receipt rows only. Expectations are derived from the fresh source differences, not the exporter's recorded error list. First script execution completed all schema rows but failed selecting a negative-control case because the script used `False` instead of Rust's `false`; the corrected script reran successfully. `python` was unavailable; Python 3 worked. These setup failures were not product failures or counted as passing controls.

| Operation | Requests (both auth forms included) | EN valid | HU valid | Download valid / conflict |
|---|---:|---:|---:|---:|
| Create | 40 | 40 | 40 | 34 / 6 |
| Storno | 40 | 40 | 40 | 40 / 0 |
| Query | 80 | 80 | 80 | 40 / 40 |
| Send | 66 | 66 | 66 | 66 / 0 |

Cases include populated/isolated create options, multiple rows/tenders, empty ledger, erasure 0/400, explicit/automatic EUR rate; every known template and omission, PDF false/true, optional call ID, both query selectors; all 16 email-child presence combinations with populated and empty strings, plus default resend. The matrix is not every Cartesian product or a business-validity check: its multiple-item case, for example, does not recompute tender sum.

Each of the 46 download failures was made valid by removing only `torloKod`, or replacing the order selector element with receipt number. Four independent negative controls rejected **missing send number, duplicate send number, reversed email sequence, wrong root namespace**. Coverage tables above are manual field-by-field coverage; no automated declared-path count is claimed.

Fresh request-schema SHA-256 values (HTML-decoded inline blocks, download bytes unchanged):

| Root | EN inline | HU inline | Download |
|---|---|---|---|
| `xmlnyugtacreate` | `eeda3911ba443f2938925a86da037348bdd6b4849af22497197f76293dc31f6b` | `1d5fe9ae12f2dd2e1ecdf1b92f9675ba3f2539bc2e07419dd693f69fbc1e24eb` | `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f` |
| `xmlnyugtast` | `98c6764084bb68f21c5f68199893b520721ff9246fb84c743e982de75eeafb21` | `432bd39c40f033fa04612c689299ec49418448d9e824a0c88be6d6d43c124b64` | `59c7e8564875da3c675a4ef8d1eba543d19b766551c7b3e2959bcea08d3aebcb` |
| `xmlnyugtaget` | `2785b4d32a5b8959f27007e55b282ebbd9964950bb897fc29071d16b67b815ec` | `0e72f74f8ac1b3efa06bc0efde78ef8c9bf41a83a6241864fc257b6a89dbdab0` | `b7c04396435b5da9bfaa4193834c13136e3a412821f3cc7373cf66b0774b69ec` |
| `xmlnyugtasend` | `3caf37114072dcb49783bf5034171db4556560cba4263de267200c8d62cd71a4` | `7f37534006d4ec1b1de8c6cbdb55342850a7bf7827a0a2c2e130919b06a3db98` | `6ba13dda3b59b4fbfd472dd8b19e2a0827c9dc5adf7bbf3a2abdc35ae03a2a07` |

### Fresh response examples through current public parsers

```sh
cargo build -p szamlazz-agent --locked --offline
rustc --edition=2024 /tmp/opencode/receipts-28dcec1-response.rs --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -L dependency=/home/laborant/szamlazz-rs/target/debug/deps -o /tmp/opencode/receipts-28dcec1-response
/tmp/opencode/receipts-28dcec1-response
```

Build and executable succeeded. Results:

- Exact freshly fetched C example: `invalid base64 payload: Invalid symbol 46, offset 0.`
- Remove only `<nyugtaPdf>...</nyugtaPdf>`: parsed `NYGT-2017-123`, two items, two tenders, gross 254.
- Replace placeholder with synthetic `JVBERi0=`: create parser returns `%PDF-` bytes. Changing NY to SN additionally exercises storno projection and original reference `NYGT-2017-100`; this is explicitly synthetic, not a vendor reversal.
- Fresh E success: `Ok(())`; fresh E failure: `MissingData` (7), exact `Hiányzó adat: emailtargy elem.` preserved.
- A number-selector mismatch still parses the data, confirming the documented projection/adoption distinction.

Existing receipt-wire and broader parsing tests were inspected, **not rerun here**. The parent runs broad cargo tests; its results are not asserted in this report. No cached prior review test count was reused. Downloaded response schemas were read field-by-field, not used to certify the inconsistent response examples as schema-valid financial documents. No PDF rendering, email transmission, NAV query, live probe or production-account validation occurred.

Final code-state check confirmed HEAD still `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2` and no diff in the Agent crate, the two evidence documents or Cargo inputs. This report is the only repository file authored by this review; temporary artifacts and all prior reviews are preserved.
