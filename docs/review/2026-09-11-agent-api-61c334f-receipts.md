# Számla Agent receipt API review — `61c334f`

## Result

**No verified defect in handling documented, ordinary valid receipt requests or responses was found.** All four operations and every field in the current published receipt request/response definitions are represented. There are **zero ranked valid-response bugs**. One verified **P3 malformed-artifact recovery improvement** is recorded separately below; it is not evidence that valid vendor replies fail.

The significant remaining risks are evidence gaps and conflicting vendor sources: downloadable schemas omit two explicitly supported fields; automatic MNB omission has receipt-specific PHP documentation but no established receipt execution evidence; order-toggle and NAV-rollout pages disagree. These are not grounds for deleting supported fields or importing invoice behavior into receipts.

### Scope and revision boundary

- Reviewed on **2026-09-11**, starting at HEAD **`61c334f9508e8b63df2f3db182d6ca83f4feb8f0`**, against the **current worktree**, as requested.
- Whole `crates/szamlazz-agent/src/ops/receipt.rs` (lines 1–1479): create, storno, query, send, public input/output types, templates, item/tender serialization, parsing and tests. Also reviewed relevant `item.rs`, `types.rs`, `xml.rs`, `wire.rs`, `client.rs`, `error.rs`, `recovery.md`, README, test corpus and recorded behavior.
- Another session committed existing work during the audit, advancing HEAD to **`5c6d5ead33a3587c4ea29cc973bedaefc3ddcb1f`**. `git diff 61c334f --` over the receipt implementation and shared dependencies showed only `wire.rs` extracting the unchanged XML-character check into public `validate_xml_text`. Receipt code, shared parsing, types, arithmetic, errors and client were unchanged. Citations below use the read worktree lines; the extraction is after all cited receipt transport paths. Tests ran against the worktree, not a detached immutable checkout.
- This audit writes **only this report**. No source/test edit, credential access, vendor operation, live probe or commit was performed. Network requests fetched public documentation/assets only. Existing unrelated work and sibling reports were left alone.
- Independent tool calls were parallelized. No subagent capability was available in this session; no subagent review is claimed. The previous `837dad0` receipt report was consulted as a lead after the implementation/primary-schema comparison; its conclusions and test results were not substituted for fresh checks.

Unless otherwise stated, `receipt.rs`, `types.rs`, `item.rs`, `xml.rs`, `wire.rs`, `client.rs`, `error.rs` and `recovery.md` below mean files under `crates/szamlazz-agent/src/` (`receipt.rs` is under `ops/`).

## Ranked verified finding: malformed-response hardening

### H1 — P3: corrupt nonblank PDF discards an otherwise readable receipt result

**Class:** recovery ergonomics for a malformed artifact, **not a valid-response compatibility bug**. High confidence in local behavior; live occurrence unestablished.

- **Location:** `receipt.rs:690–701`, particularly the fallible `Pdf::from_base64(&encoded)?` at line 694; `types.rs:104–116`.
- **Source:** [create response][C-response] promises receipt data in `nyugta` and a **base64-encoded** `nyugtaPdf` when requested. Its illustrative example literally supplies `...`, not base64. The response XSD types the element as `string`, but the prose imposes the artifact encoding; schema validity alone does not make those dots a valid PDF response.
- **Verified behavior:** the existing source-corpus test explicitly checks that this example fails base64 decoding, then substitutes `JVBERi0=` and verifies receipt identity, metadata, both item spelling forms, tenders and totals (`tests/upstream.rs:852–945`). That test passed in this audit. Missing, empty and whitespace-only PDF already preserve the receipt with `pdf: None` (`tests/receipt_wire.rs:44–64`, also passed).
- **Impact:** a corrupt nonblank PDF on a successful create or storno prevents the caller receiving the typed receipt number and other readable fields. The convenience client returns a parse error classified `Unknown`, requiring recovery through a retained call ID/managed order/known number. This does **not** authorize another issuance. The same issue on query loses that observation.
- **Correction if pursued:** separate artifact decoding failure from receipt-data decoding, preserving the successfully parsed receipt plus an explicit artifact diagnostic. Do not silently turn corrupt bytes into a valid PDF or promise success when receipt identity itself is unreadable. Add a focused test distinguishing valid PDF, absent/blank PDF, corrupt PDF with readable receipt data, and malformed identity.
- **Disposition:** optional lower-priority improvement. The current behavior is already deliberate and tested; this review does not relabel an illustrative placeholder or hypothetical corruption as a vendor-valid reply.

No P0/P1/P2 receipt defect was verified. In particular, code 7's `OutcomeClass::NotFound` name is broad, but its current documented contract explicitly includes operation-dependent missing data; receipt-send code 7 is not documented as proof of absent receipt (`error.rs:365,385,450–454`, `recovery.md:17`). No contrary classification guarantee is inferred from the name alone.

## Fresh primary-source register

All links in this register were fetched during this audit. The docs pages displayed **`v202608271632`**, a site build identifier, not a per-claim publication date. XML pages expose both inline examples and schemas; create/send response pages expose examples and response schemas. Storno/query response pages refer to the common receipt envelope and do not provide independent complete success bodies.

| ID | Sources and claims checked |
|---|---|
| C | [Generating category][C-category], [request][C-request], [XML/example/XSD][C-xml], [response/example/XSD][C-response]: action, fields, call-ID duplicate refusal, PDF, codes 336–340. |
| S | [Reversing category][S-category], [request][S-request], [XML/example/XSD][S-xml], [response][S-response]: by-number reversal, SN result, already-reversed/storno-target refusals. |
| Q | [Querying category][Q-category], [request][Q-request], [XML/example/XSD][Q-xml], [response][Q-response]: receipt-number or order-number selector; optional call ID/template; common result. |
| E | [Sending category][E-category], [request][E-request], [XML/example/XSD][E-xml], [response/examples/XSD][E-response]: issued receipt only, email child presence, verdict-only result, code 7 example. |
| R | [Settings index][R-index], [NAV reporting][R-nav], [order number][R-order], [PDF template][R-template], [erasure count][R-erasure], [amounts/rounding][R-amounts]. |
| B | [Authentication][B-auth], [sending requests][B-send], [errors/retry limit][B-errors], receipt-linked [currencies][B-currency]. |
| P | Official PHP [create][P-create], [storno][P-storno], [query][P-query], [send][P-send], [PDF][P-pdf]. Includes wrapper defaults, item/tender tables and last-match order queries. |
| P-ZIP | [PHP 2.12.4 ZIP][P-zip], downloaded and inspected **in memory, not executed**. SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Receipt MNB comments inspected in `szamlaagent/src/szamlaagent/Header/ReceiptHeader.php:62–79` and `szamlaagent/examples/document/receipt/create_receipt_with_custom_data.php:35–50` within the package. |
| K | Linked knowledge-base [receipt order numbers][K-order], [erasure usage][K-erasure], reporting [Hungarian][K-nav-hu] and [English][K-nav-en]. |
| X | Directly fetched full [create][X-create], [storno][X-storno], [query][X-query], [send][X-send], [receipt response][X-response] and [send response][X-send-response] XSDs. |

The four example `schemaLocation` download paths under `https://www.szamlazz.hu/docs/xsds/` were independently attempted and returned **404**: `nyugtast/xmlnyugtast.xsd`, `nyugtaget/xmlnyugtaget.xsd`, `nyugtasend/xmlnyugtasend.xsd`, `nyugta/xmlnyugtasendvalasz.xsd`. Working `/szamla/docs/xsds/` counterparts are linked as X above. The create example also has a single URI in `xsi:schemaLocation` rather than namespace/location pairs. The crate does not emit that hint; its absence is not a missing receipt field.

## Complete request-field comparison

“Covered” means representable and correctly serialized against the cited definitions, not that arbitrary caller content passes account/business rules.

### Common HTTP/XML envelope

| Operation | Multipart field / root | Code |
|---|---|---|
| Create | `action-szamla_agent_nyugta_create` / `xmlnyugtacreate` | `receipt.rs:189–191,223–231` |
| Storno | `action-szamla_agent_nyugta_storno` / `xmlnyugtast` | `receipt.rs:340–352` |
| Query | `action-szamla_agent_nyugta_get` / `xmlnyugtaget` | `receipt.rs:420–432` |
| Send | `action-szamla_agent_nyugta_send` / `xmlnyugtasend` | `receipt.rs:502–511` |

All match C/S/Q/E request pages and B-send. Each XML namespace is exactly `http://www.szamlazz.hu/<root>`; the HTTPS transport destination is separately `https://www.szamlazz.hu/szamla/` (`wire.rs:14`). UTF-8 XML declaration/default namespace come from `xml.rs:139–160`; one XML file part with filename, `text/xml`, CRLF framing and multipart content type from `wire.rs:66–100`. The receipt operations add no attachments. `Client::send` POSTs this body (`client.rs:374–405`).

All four inject either `szamlaagentkulcs` or `felhasznalo` followed by `jelszo` inside `beallitasok` (`xml.rs:610–620`), matching B-auth and the schemas. Create/storno/query always write required `pdfLetoltes`; defaults are false. Send has no PDF flag. **No receipt schema offers `valaszVerzio`**, so lack of that selector is correct; general version-1 plain-text guidance lists invoice/storno/credit/PDF operations, not receipts (B-errors).

### Create header

| Wire field | Public field / writer lines in `receipt.rs` | Check |
|---|---|---|
| `hivasAzonosito` | `call_id`, 118–122 / 233 | Optional; defaults absent; preserves caller's stable logical call ID. |
| `elotag` | `prefix`, 123–125 / 234 | Required string; C-response specifies receipt-only prefix (336), capitals/digits (337). Server validates stock/format. |
| `fizmod` | `payment_method`, 126–127 / 235 | Required open `PaymentMethod`; all listed free-text methods representable. |
| `penznem` | `currency`, 128–129 / 236 | Required open currency, supports every listed code including `Ft` and `KSH`; no ISO-only whitelist. |
| `devizabank`, `devizaarf` | `exchange_rate.bank`, `.rate`, 130–134 / 237–242 | Correct receipt-specific names; optional on HUF. Foreign currency requires exchange structure, with MNB omission exception discussed below. |
| `megjegyzes` | `comment`, 135–136 / 243 | Optional free text. |
| `pdfSablon` | `template`, 137–138 / 244–246 | Optional template token. |
| `fokonyvVevo` | `ledger_customer`, 139–141 / 247 | Optional customer ledger identifier. |
| `rendelesSzam` | `order_number`, 142–149 / 248 | Optional, correct capitalization and header-tail placement. |

Constructor defaults match the optionality (`receipt.rs:163–187`); PHP's default cash/Ft are wrapper defaults, not a requirement that Rust default those constructor arguments. Root order is settings → header → items → optional tenders, matching C-xml's example.

### Create items, ledger and tenders

| Wire field | Input / writer | Check |
|---|---|---|
| `tetelek/tetel` | `items`, `receipt.rs:153–155,193–199,250–277` | Repeated rows; empty list refused locally. |
| `megnevezes`, `azonosito` | `LineItem.name`, `.id`, `receipt.rs:256–257` | Required name; optional item identifier, not receipt identity. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `.quantity`, `.unit`, `.unit_price`, 258–260 | Exact decimal/string/decimal output. |
| `afakulcs` | `.vat_rate`, 261 | Numeric percentage or special/open token; all C lists representable. |
| `netto`, `afa`, `brutto` | `.net_value`, `.vat_value`, `.gross_value`, 262–264 | Receipt spelling, not invoice `…Ertek`; asserted values are not recalculated by serialization. |
| `fokonyv/arbevetel`, `fokonyv/afa` | `.ledger.revenue_account`, `.vat_account`, 265–270 | Complete receipt ledger. |
| Item `megjegyzes` | `.comment`, 271 | Supported optional field. |
| `torloKod` | `.erasure_code_count`, 272–274 | Count, not code text; u32 excludes negative counts, validation limits to 400 (200–207). Inline C-xml/R-erasure/P-create support it despite X-create omission. |
| `kifizetesek/kifizetes/fizetoeszkoz` | `ReceiptPayment.method`, 278–285 | Free-text tender, independently modeled from header payment method. |
| `osszeg`, `leiras` | `ReceiptPayment.amount`, `.description`, 283–284 | Decimal amount; optional **string** description, despite example annotation calling it double. XSD confirms string. |

Item order matches C-xml: name → identifier → quantity → unit → unit price → VAT → amounts → ledger → comment → erasure count. `LineItem`'s entire field set was checked (`item.rs:90–127`). Invoice-only `margin_vat_base`, ledger economic events and settlement dates are explicitly refused (`receipt.rs:655–686`), never dropped silently. No other shared field is lost by the writer.

Empty `payments` omits the optional container; nonempty produces one-or-more entries, with no invoice-credit five-entry limit. C-xml and code 340 require tender sum equal gross; the crate explicitly delegates this to szamlazz.hu (`receipt.rs:156–158`). It does not impose equality between tender names and `fizmod`.

### Storno, query and send

| Fields / behavior | Code | Comparison |
|---|---|---|
| Storno `nyugtaszam` → `pdfSablon` → `hivasAzonosito` | `receipt.rs:314–359` | All fields and exact S-xml/X-storno sequence. Number required; template/call ID optional. |
| Query `nyugtaszam` **or** `rendelesSzam` | `receipt.rs:369–395,433–439` | Enum implements exactly one selector, matching Q-request and P-query/P-pdf. No empty selector combination. |
| Query `hivasAzonosito`, `pdfSablon` | `receipt.rs:399–416,440–443` | Both optional, ordinary query omits call ID; field presence is not evidence of call-ID lookup semantics. |
| Send `nyugtaszam` | `receipt.rs:483–499,512–514` | Required issued-receipt number, no order selector. |
| `emailKuldes/email` → `emailReplyto` → `emailTargy` → `emailSzoveg` | `receipt.rs:454–470,515–522` | Exact case/order and independent child optionality in E-xml/X-send. All details for first send are representable. |
| Default resend | `receipt.rs:486–498,515–522` | `email: None` writes **present empty** `emailKuldes`; matches documented previous-email behavior. `Some(default())` does the same. |

For individual email children, `None` omits, `Some("")` writes an empty element. The dedicated test observes container presence, rather than relying only on the upstream outline comparison that intentionally treats empty and absent as equivalent (`tests/receipt_wire.rs:66–154`; `tests/upstream.rs` outline tests).

### Templates and shared arithmetic/value types

- `ReceiptTemplate::{A4Default,Ticket,TicketWithLogo,Roll80mm}` map to **A/J/L/N** (`receipt.rs:25–58`), agreeing with R-template and S/Q schemas. `Other` preserves unknown/empty XML tokens. C-xml documents invalid/empty fallback to A; R-template documents absent/empty default. JSON uses request-enum snake-case names; the hand-written XML mapping supplies vendor tokens. That JSON representation is not a vendor mismatch.
- `ReceiptType` reads **NY/SN**, with unknown text preserved (`types.rs:861–940`); distinct from invoice document type. `ReceiptNumber` preserves string text (`types.rs:59–94`).
- `PaymentMethod` retains arbitrary receipt free text (`types.rs:588–687`); limited named variants do not exclude other methods. `Currency` is likewise open (`types.rs:369–477`).
- `VatRate` has all documented receipt special-code families including legacy EU/EUK/MAA/ÁKK, plus `Other` (`types.rs:178–366`). A response category takes precedence over numeric rate without discarding either field (`receipt.rs:646–652`, `types.rs:1087–1093`). Scientific numeric spellings and XML padding are correctly interpreted within Decimal's exact domain.
- R-amounts requires HUF/Ft gross whole, net/VAT at most two decimal places, and **exact** net + VAT = gross. Unit/net and VAT equations have a separately documented 2-HUF tolerance. `LineItem::new` correctly preserves `787.40 / 212.60 / 1000`; `try_calculated` requires explicit rounding and exact representability (`item.rs:163–222`). HUF minor-unit rounding is stricter than the vendor rule, while `Scale(2)` alone cannot guarantee whole gross. Current rustdoc says both explicitly.
- HUF alias recognition is case-insensitive, while emitted currency remains as supplied. Per-currency minor-unit digits are local policy, not verified storage rules (`types.rs:401–435`). Invoice P60 observations are not used as receipt rounding proof.

## Complete response-field comparison

Create/storno/query share `parse_receipt` (`receipt.rs:293–295,364–366,449–451,688–702`). It requires `xmlnyugtavalasz` in `http://www.szamlazz.hu/xmlnyugtavalasz`; send uses `xmlnyugtasendvalasz` and returns `()` (`receipt.rs:527–532`). Q-response explicitly delegates to C-response; S-response explicitly returns the **new SN**, not the original.

| Wire field/group | Public projection / parser | Assessment |
|---|---|---|
| `sikeres`, `hibakod`, `hibauzenet` | `xml.rs:443–546` | Header/status and body verdict checked before receipt payload. Failure does not require `nyugta`; absent codes remain absent. |
| `nyugtaPdf` | `Receipt.pdf`, `receipt.rs:693–710`, `types.rs:104–116` | Base64 → bytes; wrapped whitespace supported; absent/blank → None; corrupt nonblank → H1. No fabricated raw-PDF response mode. |
| `alap/id` | `.id`, `receipt.rs:544–545,727,755` | Required integer, i64 wider than XSD int. |
| `hivasAzonosito` | `.call_id`, 546–548,728,756–761 | Optional creation-call text; blank → None, nonblank preserved. |
| `nyugtaszam`, `tipus` | `.receipt_number`, `.document_type`, 549–554,729–730,762–763 | Required strings/open receipt type. |
| `stornozott` | `.reversed`, 555–558,731,764–765 | Required true/false/1/0 fact; absent/empty/unknown refuses, not default false. Correct receipt spelling differs from invoice `sztornozott`. |
| `stornozottNyugtaszam` | `.reversed_receipt_number`, 559–561,732,766–771 | Optional original number carried by SN. |
| `kelt` | `.issue_date`, 562–563,733,772–773 | Required civil date; valid complete timezone suffix/outer XML padding supported. |
| `fizmod`, `penznem` | `.payment_method`, `.currency`, 564–567,734–735,774–775 | Required, open text/value types. |
| `devizabank`, `devizaarf` | `.exchange_bank`, `.exchange_rate`, 568–571,736–737,776–779 | Optional bank/exact Decimal. |
| `megjegyzes`, `fokonyvVevo` | `.comment`, `.ledger_customer`, 572–576,738–739,780–787 | Optional free text and ledger. |
| `teszt`, `rendelesSzam` | `.test`, `.order_number`, 577–584,740–741,788–795 | Optional bool/text in model; missing test is unknown, not live. |
| `tetelek/tetel/megnevezes`, `azonosito` | `ReceiptItem.name`, `.id`, 608–612,806–808,842–843 | All declared name/ID fields. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `.quantity`, `.unit`, `.unit_price`, 613–618,809–817,844–846 | Required Decimal/string/Decimal. |
| `afatipus`, `afakulcs` | `.vat_type`, `.vat_rate_code`, 619–625,818–820,847–848 | Optional category plus required raw numeric token; typed helper preserves precedence. |
| `netto`, `afa`, `brutto` | item amounts, 626–631,821–826,849–851 | Exact decimals; also accepts published second-row `nettoErtek`/`afaErtek`/`bruttoErtek` aliases. |
| Item `fokonyv/arbevetel`, `fokonyv/afa` | `ReceiptItemLedger`, 632–644,827–837,852–855 | Entire declared response ledger represented. |
| `kifizetesek/kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | `Receipt.payments`, 588–591,718–719,743–746,860–883 | Optional repeated tenders, full amount/description projection. |
| `osszegek/afakulcsossz/afatipus`, `afakulcs`, `netto`, `afa`, `brutto` | `Totals.by_vat_rate`, `xml.rs:815–843,859–877` | Every per-rate field and repeated subtotal retained. |
| `osszegek/totalossz/netto`, `afa`, `brutto` | `Totals.total`, `xml.rs:845–857,880–887` | Required grand total, retained rather than recomputed. |

`ReceiptItem` has no returned item comment/erasure field because **neither response schema nor example declares one**. Request acceptance of a field does not establish that it is echoed. No seller/buyer party block, external ID, NAV-report status or email-delivery record exists in the reviewed receipt result definition.

### HTTP, errors and recovery particular to receipts

1. **Precedence:** nonblank `szlahu_down` → nonblank error-code header → known non-2xx status → expected XML root/namespace/complete syntax → body verdict → receipt fields (`wire.rs:251–310`, `xml.rs:510–535`). Body-only HTTP-200 refusal is parsed. Non-2xx without decisive header remains `HttpStatus`, not an invented vendor refusal. This is shared implementation policy; no receipt live header-presence matrix is recorded.
2. **Receipt catalogue:** 336 prefix already used for invoices, 337 prefix format, 338 reused call ID, 339 unknown receipt, 340 tender mismatch; 363/364/365 amount constraints; 537/538/539 erasure restrictions. Named mappings and classes exist (`error.rs:155–178,244–252,296–304,375–421`). 339 is NotFound; the other listed receipt codes are Rejected. Open codes/absent code/transport/parse/unavailability stay Unknown.
3. **Code 7 context:** E-response's actual example is `Hiányzó adat: emailtargy elem.` (missing subject), not unknown receipt. Parser preserves message/code. `OutcomeClass::NotFound` explicitly includes missing input on writes; callers must interpret the operation. Storno's three refusal messages have **no numeric code assignments** on S-response. The synthetic 339 storno test proves propagation, not those assignments.
4. **No receipt numbered-56 exception:** receipt create/storno do not create/send an invoice notification and have no `notification_delivery_failed` member. The generic verdict returns an error header/code as such. The reviewed receipt docs establish no numbered-56 warning semantics; importing invoice success handling would be speculative.
5. **Call ID:** C-response documents uniqueness and unsuccessful duplicate call, not replayed original success. `receipt.rs:94–102,118–122` and `recovery.md:15` correctly retain the ID and require identity/type/reversal checks on recovery. No new ID while unresolved. Call-ID retention, scope and concurrency behavior remain unspecified.
6. **Storno:** S-response documents already-reversed and storno-target refusal; it does not promise invoice-style repeat success. `receipt.rs:298–310,322–325` correctly qualifies call-ID/338 semantics and original-query limitations. The parser reports the response's identity rather than asserting it matches the request; adoption requires checking SN and its original reference.
7. **Query:** P-query/P-pdf say last matching document for order queries. No exact newest criterion or NY/SN selection rule is established; the crate states that (`receipt.rs:376–379`). Optional query call ID is not a selector (`401–404`).
8. **Email:** E-xml distinguishes omitted container (no mail) from present container without details (previous mail). `SendReceipt` intentionally exposes sending/resending, not the absent-container no-op. No source proves independent partial-field merging, multi-recipient delimiters or inbox delivery from acknowledgement. Lost acknowledgement remains unresolved (`receipt.rs:473–479`, `recovery.md:20`).
9. **Transport uncertainty:** body-transfer failure retains status/headers in `IncompleteResponse` and stays Unknown, without inventing an empty completed response (`client.rs:66–100,117–125,385–403`). Native timeout is 60 seconds and default redirects are disabled; no application retry loop is present. Injected transport retry behavior remains caller-owned. B-errors limits the same request to five total sends, then operator action; `recovery.md:36–42` accurately avoids inventing combined write/query accounting.

## Vendor conflicts and deliberate deviations

| ID | Fresh conflict / boundary | Impact and disposition |
|---|---|---|
| D1 | C-xml inline XSD/R-erasure/P-create include `torloKod`; X-create omits it. Q-request/Q-xml/P-query/P-pdf support `rendelesSzam`; X-query omits it. | Download-only validation rejects documented capabilities. Keep both fields; obtain vendor schema alignment. This audit compared schema content directly, not automatic XSD validation. |
| D2 | X-response enumerates `TEHK`, absent from inline C-response. Examples/list annotations also mix current and legacy VAT codes. | Open `VatRate::Other` preserves TEHK; no data-loss defect or reason to close the set. |
| D3 | C/Q XML prose says order cannot change, but their schemas use `all`; S/E use actual `sequence`. Create sample places bank before rate while XSD lists rate first. | Writer follows example order and genuine sequences. `all` declaration order is not evidence of a writer-order bug. No server-reordering experiment was performed. |
| D4 | B-currency/C-response require bank and rate. P-ZIP `ReceiptHeader.php:65,74–75` says MNB with absent/zero rate uses current MNB if currency exists there; custom example line 43 says omission works but line 44 sends 300.0. | `ExchangeRate::automatic_mnb` is supported by receipt-specific documentation, not a demonstrated omitted-rate execution. `types.rs:966–973` already qualifies this. Foreign-currency validation permits missing numeric rate only with exact bank `MNB`; explicit bank/rate remains available. |
| D5 | R-order says receipts have their **own toggle**, separate from invoices. K-order says the setting applies to receipts/invoices/proformas and `bizonylattípusonként nem állítható be külön` (cannot be set separately per type). | Current receipt rustdoc follows Agent-specific R-order. Vendor must reconcile sources; no account was inspected. No proven code defect. |
| D6 | R-nav/C-request still say nothing to do yet and automation is under development. K-nav-en says automatic reporting from September 1; K-nav-hu says September 10 with retrospective September receipts, NAV connection and receipt-interface permission. | README:167–174 already reflects the more specific current HU setup and distinguishes issuance from reporting. No missing XML reporting flag/status is established. This is vendor documentation evidence, not a receipt reporting test. |
| D7 | C-response sample has two gross item amounts totaling 50,800, payments totaling 4,000, grand gross 254; NY/unreversed with an original-number reference; second item uses invoice amount names; PDF is `...`; empty `hibakod` contradicts its XSD int type. | Illustrative source is not a valid financial/lifecycle record. Parser tolerates the amount aliases and empty code and retains reported numbers; literal PDF causes H1. Do not treat repaired samples as live replies. |
| D8 | R-amounts claims `787.40157480315 + 212.59842519685 = 999.999999…`. Their exact decimal sum is 1000, though precision limits are violated. | Do not encode that arithmetic explanation or claimed first-error ordering as a Rust invariant. The corrected `787.40/212.60/1000` rule is independently clear. |
| D9 | Missing/empty test marker, empty item/subtotal collections and future tokens can be accepted despite stricter XSD cardinality/enumeration. Required receipt/reversal structure remains mandatory. | Intentional leniency, not full schema validation. Test None never means live; missing reversal never means false. |
| D10 | Decimal and civil Date have finite domains narrower than all theoretical XSD double/date values. Exact numeric parsing deliberately refuses underflow/precision loss; date reader also preserves legacy accepted spellings. | Explicit domain boundary, not a claim of total XSD value-space conformance. Existing precision and date tests make the policy observable. No ordinary vendor receipt requiring an unrepresentable value was established. |

Full XML/namespace checks apply before data projection (`xml.rs:183–359`): complete root through EOF, valid bindings, ignored foreign subtrees, canonicalized protocol aliases for repeated rows. Tests cover truncation, malformed tails, foreign identity/verdict data, duplicate singleton fields and nested scalar content. These are malformed-response hardening controls, not observations that szamlazz.hu emits such documents.

Other semantic checks are intentionally outside this low-level parser: matching response number to query target, matching SN original to requested target, validating sums, or proving intended issuance ownership. It exposes the fields needed for those checks and documents recovery. Missing selectors for call-ID-only/internal-ID/external-ID query, order-based storno/send, receipt edit/delete, separate receipt payment registration, buyer/seller parties, language/date/invoice flags and response-version controls are **not omissions from the reviewed official wire definition**. PHP PDF download/file-naming convenience is not a separate Számla Agent operation.

## Live evidence and supported deviations

**No receipt-specific live deviation from the published rules was established in the requested repository evidence.**

`docs/szamlazz-hu-behaviour.md:3–24,30–31,82–98,155–172,249–254` records invoice-family TEST-account probes, with raw logs outside the repository. B4 invoice repeat-storno success, B8 removal of credit entries, invoice order fingerprint/reuse, external-ID behavior, P60 monetary storage/tolerance and P73 appearance observations do **not** establish receipt behavior. No receipt deviation is justified by those observations.

The current code now contains receipt probes, which the older `837dad0` review could not cover:

| Current probe | What it would establish on a successful dated execution | Evidence limit |
|---|---|---|
| `tests/probes/receipts.rs:176–241`, `receipt_lifecycle` | Fractional-net HUF create; call/order/number/ID/type/test identity; number/order reads; totals/tenders; PDF signatures; deliberate repeat of completed create → 338; SN/original reads. | New hypothesis-driven ignored probe. No execution result was established in the reviewed repository record. It does not test arbitrary concurrent retries or call-ID lifetime. |
| `:243–264`, `receipt_automatic_mnb` | EUR create with MNB and no numeric rate; positive stored rate, currency/gross; cleanup reversal. | Presence is not omitted-rate acceptance evidence. No broader foreign-currency storage rule follows. |
| `:266–312`, `receipt_email_resend` | Full-detail send followed by empty-block resend, both acknowledged. | Inbox inspection is explicitly manual; protocol success alone does not prove two deliveries or inherited contents. |

The helper verifies queried original/order/type before reversal, checks returned SN and original reference, and defers cleanup on unresolved writes (`tests/probes/receipts.rs:115–172`). Current core `tests/live.rs:25–169` covers taxpayer/invoice/proforma, not receipts. `docs/testing.md:152–172` explicitly calls receipt probes documentation hypotheses and requires separately recording actual results before promoting them to verified behavior. No ignored/live target was run here; no external diagnostic directories were read as a substitute for published evidence.

Still unresolved: receipt storno repeat numeric codes, storno call-ID scope, call-ID retention/concurrency/reuse, query call-ID effect, order uniqueness/settings conflict, exact last-match and post-storno NY/SN selection, receipt whitespace normalization, reversed-receipt tenders, MNB omission execution, foreign storage precision, all four rendered templates, erasure issuance, partial email inheritance/multiple recipients/inbox delivery, and actual NAV reporting.

## Verification performed in this audit

```sh
cargo test -p szamlazz-agent --locked --offline --lib --test receipt_wire --test upstream --test business_text --test numeric_fidelity --test error_classification --test response_namespaces --test response_completion --test client
cargo test -p szamlazz-agent --locked --offline --features client-reqwest --test client
```

**237 passed, zero failed:** first command 228 tests (185 library, 6 receipt-wire, 11 upstream, 2 business-text, 6 numeric-fidelity, 3 error-classification, 11 namespace, 4 completion). Its client target ran **zero tests** because the transport feature was absent; the second command explicitly enabled it and passed **9 loopback client tests**. No ignored tests were selected. The shared client tests mostly use invoice/taxpayer controls; they establish shared transport behavior, not a receipt-specific HTTP acceptance matrix.

Coverage inspected and executed includes:

- `receipt.rs:929–980`: civil date forms for create/storno/query, valid timezone boundaries and malformed/calendar/multibyte controls.
- `receipt.rs:987–1245`: all four goldens, exchange-rate choices, shared item metadata, invoice-only-field refusal, template/storno-call order, both query selectors and present-empty resend.
- `receipt.rs:1248–1479`: receipt field projection, totals/tenders, optional test, PDF, JSON roundtrip, body errors/open codes and header precedence.
- `tests/receipt_wire.rs:12–64,118–244`: required reversal fact, blank PDF identity preservation, exact email presence, ordinary query call-ID omission, fractional HUF values.
- `tests/upstream.rs:852–961`: explicitly repaired receipt PDF placeholder, both amount naming forms, full receipt/tender/total mapping, actual send success/error examples.
- `tests/business_text.rs` and `tests/numeric_fidelity.rs`: decoded nonblank text fidelity, category precedence, numeric spellings and exact-domain refusal.
- `tests/error_classification.rs:44–242`: receipt catalogue and contextual code 7, unknown-code controls; synthetic cases, not live errors.
- `tests/response_namespaces.rs`, `tests/response_completion.rs`: shared receipt namespace/list/singleton and completed-XML checks.
- `tests/client.rs:108–181,183–337,339–435`: loopback multipart, errors/unavailability, jar ownership, incomplete transfer evidence, pre-send request refusal and failure classification.

Fresh source comparisons were field-by-field reads of current inline and downloaded XSDs/examples. **No new automated XSD-validation or fresh-example parser harness was run**, and no full workspace, feature-platform matrix, PDF rendering or inbox test is claimed. `fixtures/SOURCES.md:74–85,103–108,121–140,182–191` was read for corpus provenance; cached schemas are not universally authoritative. In particular cached create erasure-field provenance is qualified and the cached query schema was refreshed from inline docs. Passing the corpus suite does not independently confirm today's server acceptance.

## Source links

[C-category]: https://docs.szamlazz.hu/agent/category/generating-a-receipt
[C-request]: https://docs.szamlazz.hu/agent/generating_receipt/request
[C-xml]: https://docs.szamlazz.hu/agent/generating_receipt/xml
[C-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[S-category]: https://docs.szamlazz.hu/agent/category/reversing-a-receipt
[S-request]: https://docs.szamlazz.hu/agent/reversing_receipt/request
[S-xml]: https://docs.szamlazz.hu/agent/reversing_receipt/xml
[S-response]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[Q-category]: https://docs.szamlazz.hu/agent/category/querying-a-receipt
[Q-request]: https://docs.szamlazz.hu/agent/querying_receipt/request
[Q-xml]: https://docs.szamlazz.hu/agent/querying_receipt/xml
[Q-response]: https://docs.szamlazz.hu/agent/querying_receipt/response
[E-category]: https://docs.szamlazz.hu/agent/category/sending-a-receipt
[E-request]: https://docs.szamlazz.hu/agent/sending_receipt/request
[E-xml]: https://docs.szamlazz.hu/agent/sending_receipt/xml
[E-response]: https://docs.szamlazz.hu/agent/sending_receipt/response
[R-index]: https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules
[R-nav]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting
[R-order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[R-template]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template
[R-erasure]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code
[R-amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[B-auth]: https://docs.szamlazz.hu/agent/basics/authentication
[B-send]: https://docs.szamlazz.hu/agent/basics/sending-requests
[B-errors]: https://docs.szamlazz.hu/agent/basics/error-handling
[B-currency]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[P-create]: https://docs.szamlazz.hu/php/nyugta-generalas
[P-storno]: https://docs.szamlazz.hu/php/sztorno-nyugta-generalas
[P-query]: https://docs.szamlazz.hu/php/nyugta-lekerdezes
[P-send]: https://docs.szamlazz.hu/php/nyugta-kuldes
[P-pdf]: https://docs.szamlazz.hu/php/nyugta-pdf
[P-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
[K-order]: https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan
[K-erasure]: https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor
[K-nav-hu]: https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg
[K-nav-en]: https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting
[X-create]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd
[X-storno]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd
[X-query]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd
[X-send]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd
[X-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[X-send-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd
