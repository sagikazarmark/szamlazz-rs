# Számla Agent receipt surface — `837dad0`

## Verdict and scope

**No confirmed current receipt implementation defect found.** All four operations and every request/response data field in the freshly fetched receipt schemas are represented. There are **zero severity-ranked defect findings**. Downloadable-schema disagreements, intentional validation boundaries, and unresolved vendor behavior are recorded separately below; they are not evidence of server rejection.

Reviewed HEAD **`837dad024300e2a202c2b6351fcba73df82a7744`**, verified with `git rev-parse`, on **2026-09-11**. The working tree was initially clean. Scope: `crates/szamlazz-agent/src/ops/receipt.rs` in full, receipt use of `item.rs` and `types.rs`, and the shared XML/wire/error machinery needed to assess those operations. Public receipt guidance, fixture provenance, tests, and recorded live evidence were also checked. All code line references below refer to this commit.

This is a whole-surface audit, not a diff review. Official category pages were fetched first, followed through request, response, XML/example/XSD and all five receipt settings pages, then their relevant first-party links. Pages displayed site build `v202608271632`; that is not a publication date for each rule. Existing historical review findings were not the audit baseline: the September 11 receipt report was consulted only after the independent field/source comparison, to check disposition. Its NAV setup omission is resolved in this HEAD (`crates/szamlazz-agent/README.md:165–172`) and is not repeated as a finding.

Only this report was added in the repository by this audit. Scratch checks under `/tmp/opencode/receipts-837dad0*` used public Rust serialization/parsing interfaces and unauthenticated documentation GETs. No vendor operation, credential access, source/test edit, or change to an existing report was performed. No additional agents were used.

## Fresh source register

All URLs in this register were fetched during this audit on **2026-09-11**. XML pages include both example and inline-XSD tabs; response pages include their complete examples and inline schemas where present.

| ID | URLs and coverage |
|---|---|
| C | [Generating category](https://docs.szamlazz.hu/agent/category/generating-a-receipt), [request](https://docs.szamlazz.hu/agent/generating_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/xml), [response + inline XSD](https://docs.szamlazz.hu/agent/generating_receipt/response) |
| S | [Reversing category](https://docs.szamlazz.hu/agent/category/reversing-a-receipt), [request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/reversing_receipt/xml), [response/refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response) |
| Q | [Querying category](https://docs.szamlazz.hu/agent/category/querying-a-receipt), [request](https://docs.szamlazz.hu/agent/querying_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml), [response](https://docs.szamlazz.hu/agent/querying_receipt/response) |
| E | [Sending category](https://docs.szamlazz.hu/agent/category/sending-a-receipt), [request](https://docs.szamlazz.hu/agent/sending_receipt/request), [XML + inline XSD](https://docs.szamlazz.hu/agent/sending_receipt/xml), [success/error responses + inline XSD](https://docs.szamlazz.hu/agent/sending_receipt/response) |
| R | [Settings index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules), [NAV reporting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting), [order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number), [PDF template](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template), [erasure count](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code), [amounts/rounding](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) |
| B | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication), [sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests), [error handling/codes/retry limit](https://docs.szamlazz.hu/agent/basics/error-handling), receipt-linked [currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) |
| P | Official PHP: [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [query](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [PDF](https://docs.szamlazz.hu/php/nyugta-pdf), [send](https://docs.szamlazz.hu/php/nyugta-kuldes) |
| P-ZIP | [PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), inspected in memory, never executed. Receipt-specific exchange-rate comments in `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/Header/ReceiptHeader.php:62–79` and `examples/document/receipt/create_receipt_with_custom_data.php:42–44`. SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. |
| K | Linked knowledge base: [erasure usage](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor), [receipt order number](https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan), reporting [EN](https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting) / [HU](https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg), [NAV connection guide, step 13](https://www.szamlazz.hu/nav-online-szamlazas-regisztracios-segedlet/#lepesek) |
| X-C | [Downloaded create XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd): read fully; lacks `torloKod`. SHA-256 `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f`. |
| X-S | [Downloaded storno XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd): read fully; matches writer sequence. SHA-256 `59c7e8564875da3c675a4ef8d1eba543d19b766551c7b3e2959bcea08d3aebcb`. |
| X-Q | [Downloaded query XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd): read fully; lacks `rendelesSzam`. SHA-256 `b7c04396435b5da9bfaa4193834c13136e3a412821f3cc7373cf66b0774b69ec`. |
| X-E | [Downloaded send XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd): read fully; matches writer sequence. SHA-256 `6ba13dda3b59b4fbfd472dd8b19e2a0827c9dc5adf7bbf3a2abdc35ae03a2a07`. |
| X-R | [Downloaded receipt response XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd): read fully; also lists `TEHK`, absent from the inline enumeration. |
| X-ER | [Downloaded send response XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd): successfully fetched and read fully; same three verdict fields as inline. Located through `fixtures/SOURCES.md:107`; this working URL is distinct from the URL printed in the example. |

Independent hashes of HTML-decoded inline request XSD blocks, without formatting edits or added newline:

| Operation | SHA-256 |
|---|---|
| Create | `eeda3911ba443f2938925a86da037348bdd6b4849af22497197f76293dc31f6b` |
| Storno | `98c6764084bb68f21c5f68199893b520721ff9246fb84c743e982de75eeafb21` |
| Query | `2785b4d32a5b8959f27007e55b282ebbd9964950bb897fc29071d16b67b815ec` |
| Send | `3caf37114072dcb49783bf5034171db4556560cba4263de267200c8d62cd71a4` |

Fresh response-page HTML hashes: create `a60d1f598d6f91dd02557f0ddb5bed037c65d16285ae3049b4d2b244a86182ea`; send `a1161fe936eef55293f53a4b0a40b7f694dae5bcc0ea79685bf1fc14eab56fa5`. Hashes identify acquired bytes, not server truth.

## Complete request coverage

Paths abbreviated here: **receipt.rs** = `crates/szamlazz-agent/src/ops/receipt.rs`; **item.rs**, **types.rs**, **xml.rs**, **wire.rs**, **error.rs** are under `crates/szamlazz-agent/src/`. “Covered” means the field is correctly representable/emitted against the cited sources, not that arbitrary values will pass vendor business checks.

### Envelope, authentication and operation selection

| Operation | Multipart field / XML root | Implementation | Assessment |
|---|---|---|---|
| Create | `action-szamla_agent_nyugta_create` / `xmlnyugtacreate` | receipt.rs:189–191,223–231 | Covered (C/B). |
| Storno | `action-szamla_agent_nyugta_storno` / `xmlnyugtast` | receipt.rs:340–352 | Covered (S/B). |
| Query | `action-szamla_agent_nyugta_get` / `xmlnyugtaget` | receipt.rs:420–432 | Covered (Q/B). |
| Send | `action-szamla_agent_nyugta_send` / `xmlnyugtasend` | receipt.rs:502–511 | Covered (E/B). |

Each root uses exactly `http://www.szamlazz.hu/<root>`, including `www` and the `http` namespace scheme. The HTTP destination is separately HTTPS (`wire.rs:14`). `xml.rs:141–160` writes XML 1.0/UTF-8 and the default namespace; `wire.rs:66–100` wraps one XML file in multipart, including a filename. Namespace identity is not a download location. Omitting `xsi:schemaLocation` is not a missing business field.

All four settings blocks support either `szamlaagentkulcs` or `felhasznalo` followed by `jelszo` (`xml.rs:610–620`), matching B and the schemas. Create/storno/query always emit required `pdfLetoltes` as `true`/`false`; send emits no such field. No receipt schema has `valaszVerzio`, and none is invented. Text escaping, absent-option omission and decimal emission are shared at `xml.rs:568–595`.

### Create: every header, item and payment field

| XML fields | Public representation / writer | Coverage and behavior |
|---|---|---|
| `hivasAzonosito` | `call_id`, receipt.rs:118–122,233 | Optional, absent by default; caller supplies stable logical issuance identity. No regenerated ID in serialization. |
| `elotag` | `prefix`, :123–125,234 | Required string; server allocates the full receipt number. C specifies uppercase letters/digits (337), receipt-only prefix (336). No invoice-prefix registry assumption. |
| `fizmod`, `penznem` | `payment_method`, `currency`, :126–129,235–236 | Required constructor arguments. Open values accommodate all documented UI payment strings and currencies, including `Ft` and `KSH`. PHP defaults cash/Ft are PHP defaults, not missing Rust/server defaults. |
| `devizabank`, `devizaarf` | `exchange_rate`, :130–134,208–218,237–242 | Exact receipt spellings; explicit bank/rate or deliberately documented automatic MNB omission. Foreign currency without exchange information fails locally. |
| `megjegyzes` | `comment`, :135–136,243 | Optional free text. |
| `pdfSablon` | `template`, :137–138,244–246 | Optional `A`, `J`, `L`, `N`, or unknown string; exact mapping at :25–58. Empty/absent default A4 and invalid-token fallback follow C/R. |
| `fokonyvVevo`, `rendelesSzam` | `ledger_customer`, `order_number`, :139–149,247–248 | Correct capitalization; optional, order at header tail as example. |
| `tetelek/tetel` | `items`, :153–155,193–199,250–277 | Repeated items, at least one enforced locally. |
| `megnevezes`, `azonosito` | `LineItem.name`, `.id`, :256–257 | Required name, optional item ID; item ID is distinct from receipt ID/number and call ID. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | `quantity`, `unit`, `unit_price`, :258–260 | Required decimal/string/decimal. No binary-float conversion. |
| `afakulcs`, `netto`, `afa`, `brutto` | `vat_rate`, `net_value`, `vat_value`, `gross_value`, :261–264 | Exact receipt names, not invoice `…Ertek`. Caller amounts retained; normalized numeric VAT tokens or verbatim special/unknown token. |
| `fokonyv/arbevetel`, `fokonyv/afa` | `LineItemLedger.revenue_account`, `.vat_account`, :265–270 | Only the receipt-supported ledger half is emitted. Empty ledger is representable. |
| Item `megjegyzes`, `torloKod` | `comment`, `erasure_code_count`, :271–274 | Both supported by C inline/R/P. Nonnegative count, local cap 400 (:200–207; item.rs:115–131); not literal erasure-code text. |
| `kifizetesek/kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | `ReceiptPayment.method`, `.amount`, `.description`, :61–88,278–288 | Free-text tender, Decimal amount, optional string description. Optional root block; empty vector omits it, nonempty emits repeated tenders. No invoice five-entry limit. |

Root order is settings → header → items → optional payments. Item order is name → ID → quantity → unit → unit price → VAT token → net/VAT/gross → ledger → comment → erasure count. It matches the example, and create's actual XSD groups use `all`. Invoice-only `margin_vat_base` and ledger economic-event/settlement fields are rejected by `UnsupportedOnReceipt` (`receipt.rs:655–686`), not silently discarded.

### Storno, query and send: all fields/defaults

| Surface | Implementation | Coverage |
|---|---|---|
| Storno `nyugtaszam` → `pdfSablon` → `hivasAzonosito` | receipt.rs:314–325,328–336,353–359 | Exactly the inline/downloaded `sequence` (S/X-S). Required target number; optional template/call ID. PDF defaults false. No caller date, order selector or copied invoice `eszamla` flag. |
| Query `nyugtaszam` OR `rendelesSzam` | :369–395,433–439 | Enum selects exactly one, as Q/P require. No internal-ID/external-ID/call-ID-only lookup. |
| Query `hivasAzonosito`, `pdfSablon` | :399–416,440–443 | Optional, emitted after selector; ordinary query omits call ID. Query XSD uses `all`. PDF defaults false, can be true for either selector. |
| Send `fejlec/nyugtaszam` | :483–499,512–514 | Required issued-receipt number; no order selector. |
| `emailKuldes/email` → `emailReplyto` → `emailTargy` → `emailSzoveg` | :454–470,515–522 | Exact case and sequence (E/X-E). Each is independently representable as absent or present-empty text. Full details/one recipient recommended for first send. |
| Default resend | :486–498,515–522 | `email: None` emits a **present empty** `emailKuldes`, not an absent block. `Some(ReceiptEmail::default())` does likewise. Correctly implements E's previous-email behavior. |

The request XML contains no extra buyer/seller party model on receipt create: the PHP send example's Buyer/Seller objects translate into the email fields above, not additional wire blocks. Send has no PDF, template or call-ID field in its schema.

## Complete response coverage

Create, storno and query call the same parser (`receipt.rs:293–295,364–366,449–451,690–702`) for `xmlnyugtavalasz` in `http://www.szamlazz.hu/xmlnyugtavalasz`. Storno documentation explicitly says the result contains the **storno receipt**, type `SN`, rather than the original (S). Query expressly reuses the create response (Q). Send uses its own `xmlnyugtasendvalasz` namespace and returns `()` (`receipt.rs:527–532`).

| Wire field/group | Mapping and precise locations | Assessment |
|---|---|---|
| `sikeres`, `hibakod`, `hibauzenet` | xml.rs:443–546 | Shared verdict precedes payload; failure requires no `nyugta`. Success/failure boolean forms, unknown/absent codes and optional diagnostics handled. Unusable optional diagnostic does not discard a readable code. |
| `nyugtaPdf` | receipt.rs:693–708; types.rs:104–116 | Base64 decoded to bytes, whitespace wrapping supported. Missing/blank → `None`; nonblank invalid base64 → parse error. See boundaries below. |
| `nyugta/alap/id` | receipt.rs:544–545,727,755 | Required integer → i64; deliberately wider than XSD `int`. |
| `hivasAzonosito`, `nyugtaszam`, `tipus` | :546–554,728–730,756–763 | Optional creation call ID, required receipt number, open `ReceiptType` NY/SN/Other (`types.rs:861–940`). |
| `stornozott`, `stornozottNyugtaszam` | :555–561,731–732,764–771 | Required boolean fact, optional original receipt number for SN. Correct receipt spelling has no `sz` after initial `s`; not invoice `sztornozott`. No sign-based reversal inference. |
| `kelt`, `fizmod`, `penznem` | :562–567,733–735,772–775 | Required civil date, open payment method and currency. Valid complete timezone suffix discarded without UTC date conversion. |
| `devizabank`, `devizaarf`, `megjegyzes`, `fokonyvVevo` | :568–576,736–739,776–787 | Optional bank/rate/comment/ledger, all represented. |
| `teszt`, `rendelesSzam` | :577–584,740–741,788–795 | Test marker optional in model despite XSD requirement: absence/empty is unknown, never presumed live. Order text retained. |
| `tetelek/tetel/megnevezes`, `azonosito`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | :608–618,798–817,839–846 | All item fields and multiplicity represented. |
| Item `afatipus`, `afakulcs`, `netto`, `afa`, `brutto` | :619–631,646–652,818–826,847–851 | Category and raw rate retained independently; category wins in typed helper. Three `…Ertek` aliases support the official example's second item. |
| Item `fokonyv/arbevetel`, `fokonyv/afa` | :632–644,827–837,852–855 | Complete two-field response ledger. Neither response XSD promises item comment or erasure count/code. |
| `kifizetesek/kifizetes/fizetoeszkoz`, `osszeg`, `leiras` | :588–591,718–719,743–746,860–883 | Optional repeated tenders, Decimal amounts, optional descriptions; absent block → empty vector. No truncation to five entries. |
| `osszegek/afakulcsossz/afatipus`, `afakulcs`, `netto`, `afa`, `brutto` | xml.rs:815–843,859–877; receipt.rs:747 | All per-rate data mapped to shared `Totals`. Special code takes precedence without losing raw numeric text. |
| `osszegek/totalossz/netto`, `afa`, `brutto` | xml.rs:845–857,880–887 | Required grand total; returned figures are not recomputed from items or tenders. |

`xml.rs:183–283,285–359` checks one complete expected root, XML syntax and namespace bindings, then projects only the protocol namespace. Foreign subtrees cannot supply receipt fields; correctly bound aliases and interleaved ignored extensions preserve repeated rows. Duplicate recognized singletons and scalar child elements remain refused. `xml.rs:727–738` preserves decoded nonblank business text, including padding/NBSP, rather than trimming identifiers. Decimal parsing is exact within the crate's finite domain (`xml.rs:629–646`); date behavior is documented and tested at `receipt.rs:929–980`.

## Business rules, recovery and shared types

| Rule | Source quotation/evidence | Current disposition |
|---|---|---|
| Creation call ID | C response: “if the same XML is posted multiple times, it will not duplicate an existing receipt”; reused ID is 338. | receipt.rs:94–102,118–122 and `src/recovery.md:15` distinguish duplicate refusal from replayed success. Persist before send; retain through recovery; no fresh ID while unresolved. |
| Order selection | P query/PDF: “last matching document”; exactly one number/order input. | receipt.rs:373–404, README:194–211 require checking returned identity/type/reversal. Exact last criterion and SN selection are not invented. |
| Receipt reversal | S response: “The receipt has already been reversed” is an error case; also refuses a storno target. | receipt.rs:298–310 / recovery.md:16 explicitly distinguish receipt refusal from invoice repeat-storno success. Original reversal alone does not recover SN number/PDF or actor. |
| Header method vs tender | C: `fizmod` free text; tender sums “should be equal with the total amount of the receipt.” Code 340 confirms mismatch refusal. | `PaymentMethod` supports all tokens through named variants or `Other` (types.rs:588–687); `ReceiptPayment` is free text. receipt.rs:156–158 explicitly delegates tender-sum validation to server. No requirement invented that each tender equal header method. |
| HUF amounts | R: gross “must be a whole number”; net/VAT ≤2 decimals; net + VAT “exactly”; unit/net and VAT equations within small tolerance (2 HUF). | receipt.rs:104–108, item.rs:9–18,74–87,163–222 correctly distinguish a calculator from acceptance validation. Explicit `787.40 / 212.60 / 1000` is preserved. No unrequested rounding occurs in serialization. |
| Shared calculator | item.rs:191–211 computes exact representable net, rounds it, computes/rounds VAT, adds exactly. | Explicit rounding, half away from zero; precision loss/overflow is an error. `Scale(2)` may leave fractional gross; HUF minor-unit rounding is a stricter local choice, not the only accepted receipt representation. Numeric `VatRate::Other` tokens calculate numeric VAT rather than silently zero. |
| Foreign currency | B currencies explicitly names receipt `devizabank`/`devizaarf`; R excludes foreign currencies from HUF-only rules. | All listed currencies representable. ISO-like minor-unit digits, HUF case-insensitive detection and explicit money are local policy, not per-currency server-storage guarantees (types.rs:369–436). |
| Erasure count | R: optional nonnegative int, enable account feature (539), maximum 400 (537); B: demo/test unavailable (538). P calls it “Count of Data eraser code.” | item.rs:115–131 and receipt.rs:200–207 comply. Invoice-only `SzlaMost` restriction is not applied to receipt templates. |
| Errors | C: 336–340; R/B: 261,363–365,537–539; E failure example: code 7, `Hiányzó adat: emailtargy elem.` | Typed codes at error.rs:155–178 and mappings :244–252,296–304; shared header/down/status checks precede body. Code 7 is documented as operation-dependent missing data (:43–55), not proof a receipt is absent. Unknown codes stay unclassified as refusal. |
| Email uncertainty | E response: success depends on `sikeres`; request only for already issued receipts. | receipt.rs:473–479 and recovery.md:20 correctly leave lost delivery unresolved. Existence of a receipt is not evidence of an email send. |
| NAV setup | K HU says automatic reporting from September 10 retrospectively, conditional on NAV connection and receipt-interface permission; connection guide step 13 corroborates permission. | README:165–172 already links current setup and says issuance is not reporting confirmation. No missing XML control/status is established. |
| Retry limit | B: “at most five times” for the same request, then stop. | README:159 and recovery.md:36–42 state five total sends and uncertain write/query combined accounting; no receipt operation contains a retry loop. |

## Accepted deviations and unresolved source ambiguities

These are **not severity-ranked implementation findings**. Each records the source, current code, impact and a concrete check/reproduction where applicable.

### A1 — Downloaded XSDs lag supported request fields

- **Source:** C inline XSD and R erasure page contain `<element name="torloKod" …>`; X-C omits it. Q request says use “either `nyugtaszam` … or `rendelesSzam`”; Q inline XSD contains the latter, X-Q omits it. P create/query/PDF independently corroborate both capabilities.
- **Code:** receipt.rs:272–274 and :438 correctly retain these fields.
- **Impact:** validating supported requests against downloads alone produces false rejections. Neither source family is universally newer: X-R additionally lists `TEHK`, which the inline response enum omits; `VatRate::Other` preserves it.
- **Fresh reproduction:** erasure-enabled create and order query both pass the inline XSD, fail the download with libxml2 code 1871 on precisely `torloKod` / `rendelesSzam`. The same create without erasure and a number query pass both. Keep support; do not remove fields merely to satisfy the download.
- **Provenance:** `fixtures/SOURCES.md:103–108,126–140,182–191` is a historical acquisition guide. The cached create XSD's erasure element has unverified acquisition history; it is not used to override today's independent fetches.

### A2 — “Fixed order” prose versus `all`

C/Q XML pages say fields “cannot be interchanged”, but their root/header groups and create items use XSD `all`. S/E actually use `sequence`. The crate follows sample order and both true sequences (`receipt.rs:228–288,349–359,429–444,511–522`). Create writes bank before rate as the example does, although the `all` declaration lists rate before bank. This is not an order defect. Fresh full-field S/E schema checks passed; no claim is made that the server accepts every theoretically valid reordering.

### A3 — Automatic MNB is receipt-specific documentation evidence, not execution

B currencies says rate and bank “must also be provided”. P-ZIP `ReceiptHeader.php:65` instead says MNB with no rate uses current MNB; :74–75 also mentions absent/zero rate. The custom-data example :43 says omission uses current MNB but :44 actually sends `300.0`.

`receipt.rs:208–218,237–242` permits omitted rate only with exact `MNB` for foreign currency; `types.rs:966–973` already explains provenance. The `accepts_automatic_mnb_receipt_exchange_rate` test (:1039–1050) proves wire omission, not server execution. No claim is established that MNB publishes every Agent-supported currency, or that EUR/JPY/KWD receipts share invoice storage precision. Explicit bank/rate remains representable.

### A4 — Order-toggle sources conflict; identity lifetime remains unspecified

R order page: “Receipts have their **own toggle**, separate from the invoice setting.” K order article instead says the setting applies to receipts, invoices and proformas and “bizonylattípusonként nem állítható be külön” (cannot be configured separately per document type). It also discusses subscription/UI availability. `receipt.rs:142–149` follows the Agent-specific source. This is a vendor-source conflict, not a proven incorrect writer; no account setting was inspected or changed.

Call-ID retention, cross-operation uniqueness scope, concurrent duplicate processing, reuse after reversal, query-call-ID behavior, and exact newest-SN selection are not specified by these sources. Optional query/storno call-ID elements alone do not establish create-like 338 semantics. Receipt order whitespace normalization, invoice two-day fingerprint replay and post-storno order reuse are likewise not established by the invoice probes. Existing guidance appropriately avoids those guarantees.

### A5 — Email omission and PDF robustness boundaries

E inline schema: “If omitted, no e-mail is sent.” Its example says without email details “the previous e-mail will be sent”. `SendReceipt` therefore emits a present empty container for resend. The six `receipt_wire` tests independently observe absence versus paired/self-closing presence and `None` versus `Some("")` children; the outline test alone would not distinguish them. Partial overrides, clearing stored values with empty strings, multi-recipient delimiters and delivery after a lost acknowledgement remain unverified.

For PDF, C promises base64 when requested. `receipt.rs:693–696` intentionally keeps a typed receipt when PDF is missing/blank, even if requested; README:257–260 tells callers to retain the number and query the artifact. No separate missing-PDF diagnostic is returned. **Nonblank corrupt base64 still loses the typed receipt** at :694: the official example's literal `<nyugtaPdf>...</nyugtaPdf>` reproduces `Err(Parse(Base64("Invalid symbol 46, offset 0.")))`. Removing just that placeholder produces a complete `Receipt`; replacing it with `JVBERi0=` produces five decoded bytes. These are offline controls, not a real corrupt vendor PDF or rendered artifact. A future independent artifact-error result could improve recovery ergonomics, but no conforming-response incompatibility is established here.

### A6 — Published examples are not internally consistent financial records

C response's two item gross values total **50,800**, payments total **4,000**, and grand gross is **254**. It also shows `NY`/unreversed together with `stornozottNyugtaszam`, and switches the second row to invoice-style `…Ertek` amount names. The parser deliberately retains those reported values and accepts the aliases (`receipt.rs:821–826`); it does not claim this is a reconciled business result. Neither the response example nor XSD contains item comment/erasure-return fields.

R's rejected example `787.40157480315 + 212.59842519685` actually sums to exactly 1000 in decimal arithmetic, despite its `999.999999…` explanation. It still violates the published net/VAT precision limits. Do not import that floating-point explanation as an invariant of Rust Decimal, nor infer a tested boundary for the vendor's tolerance.

### A7 — Reporting rollout pages disagree, and this HEAD already qualifies them

R reporting still says “nothing you need to do for now” and automation is being developed. K EN says automatic reporting from September 1; K HU now specifies September 10, retrospective processing and the technical-user permission. The linked connection guide confirms the permission. README:165–172 handles that disagreement explicitly. None of the four schemas exposes a NAV accepted-report status, and a downloaded/emailed receipt PDF is not an ePénztárgép receipt. Reporting execution itself was not tested.

### A8 — Deliberate parsing domain and semantic boundaries

The model is not an XSD validator: `teszt` can be absent, item/subtotal lists can be empty, unknown VAT/type tokens survive, and i64 admits a wider ID range than XSD int. Conversely finite exact Decimal cannot represent all `xs:double` values, and a civil `Date` is not the complete XSD date value space. These choices are explicit, with exactness failures rather than silent money changes. Required reversal cannot be empty/unknown (`receipt.rs:764–765`; tests/receipt_wire.rs:12–42).

`parse_receipt` does not validate that a returned number equals the query target, an SN refers to the requested original, tender sums equal totals, or the result belongs to the caller's intended logical issuance. Those checks belong to adoption/reconciliation; receipt.rs:99–102,376–379 and README:194–213 say so. The S refusal table gives messages for missing/already-reversed/storno targets but does not assign numeric codes. The synthetic 339 test establishes propagation, not an observed numeric mapping for each storno refusal.

## Intentional nonfeatures

- No call-ID-only, external-ID or internal numeric-ID receipt query; no order-based storno/send. Current operation requirements specify the supported selectors.
- No receipt edit/delete operation, independent payment-registration operation, invoice-kind flags, caller issuance/storno date, or invoice-only ledger/margin fields. The reviewed receipt schemas provide none of these.
- No absent-email-container no-op option: `SendReceipt` intentionally sends/resends. Supplying no details is the documented empty-present resend.
- No separate raw-PDF response/version-1 operation: receipt data/PDF share `xmlnyugtavalasz`; `QueryReceipt.download_pdf` covers retrieval by either selector. PDF file naming/download helpers in PHP are wrapper behavior, not additional XML fields.
- No automatic call-ID persistence, recovery workflow, template rendering, email delivery verification, account-setting mutation or NAV reporting client/status fabricated by this crate.
- No complete local business validator for prefix stock, tender sums, item money equations or acceptable tax treatment. Empty items, unsupported fields, foreign exchange structure and erasure cap are checked; vendor codes remain authoritative for the rest. This delegation is documented, not silent omission.

## Verification performed

### Existing locked offline suite

Executed successfully:

```sh
cargo test -p szamlazz-agent --locked --offline --lib --test receipt_wire --test upstream --test business_text --test numeric_fidelity --test error_classification --test response_namespaces --test response_completion
```

**228 passed, zero failed/ignored:** 185 library tests, 6 receipt-wire, 11 upstream, 2 business-text, 6 numeric-fidelity, 3 error-classification, 11 namespace, 4 completion. Upstream tests actually read the workspace corpus; their success is not a fresh vendor execution. This selected suite includes shared/invoice controls because the receipt code uses the same plumbing; it is not a full workspace or transport-feature matrix.

Evidence-bearing receipt tests reviewed include:

- `receipt.rs:987–1245`: canonical four-operation writes, explicit/automatic exchange rate, receipt item metadata, refusal of invoice-only fields, selectors, template and storno call-ID order, present-empty resend.
- `receipt.rs:1248–1477`: all projected receipt fields/tenders, shared totals, optional test marker, base64, body/header errors and unknown large error code.
- `tests/receipt_wire.rs:12–64,118–154,171–244`: required reversal fact, blank artifact identity retention, exact email-container semantics, ordinary query omission and fractional HUF amounts.
- `tests/business_text.rs:50–86`, `tests/numeric_fidelity.rs:15–94,112–166`: business-text preservation, numeric token interpretation and exact-money failure.
- `tests/response_namespaces.rs:36–115` and `tests/response_completion.rs:171–184`: receipt repeated rows, shared create/storno/query parsing, send verdict namespaces, completed-document checks.
- `tests/upstream.rs:852–958`: published receipt response with explicitly substituted PDF test content, two item spelling forms and two tenders, send success/error. `fixtures/SOURCES.md:23–39,74–85,230–247` distinguishes synthetic/golden/cached reference material and the outline test's limitations.

### Fresh-source independent checks

Scratch Rust uses public `AgentRequest::{to_wire,write_xml,parse}`; Python extracts current HTML code blocks and validates actual output with system libxml2. The executable links the workspace's freshly built library and dependencies from its locked build, not a separately resolved scratch Cargo graph.

```sh
cargo build -p szamlazz-agent --locked --offline
rustc --edition=2024 /tmp/opencode/receipts-837dad0.rs --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -L dependency=/home/laborant/szamlazz-rs/target/debug/deps -o /tmp/opencode/receipts-837dad0
python3 /tmp/opencode/receipts-837dad0.py
```

| Case | Observed result |
|---|---|
| Rich HUF create: call/order IDs, header/item comments, customer/item ledgers, template/PDF, two tenders, `787.40/212.60/1000` | Inline and downloaded XSD pass. |
| Same create with erasure count 400 | Inline pass; download fails specifically at `torloKod`. |
| Storno with PDF, template N, call ID | Both XSDs pass. |
| Number query with PDF, template J, call ID | Both pass. |
| Order query with same options | Inline pass; download fails specifically at `rendelesSzam`. |
| Send with all four details; default resend | Both cases pass both XSDs. |
| Fresh response example, verbatim | Base64 failure on documented `...` placeholder. |
| Same example with only PDF placeholder element removed | Full receipt parses: both item spellings, two tenders/descriptions, category/total, identity/date/reversal/test data. |
| Same example with synthetic base64 `JVBERi0=` | Full receipt plus `Pdf(5 bytes)`; no rendered-PDF claim. |
| PDF-removed example with only NY → SN changed | Storno type and original-number reference retained. Synthetic parsing control; no amount/reversal execution evidence. |
| Fresh send success/error examples verbatim | `Ok(())`; `Api(MissingData)` with exact `Hiányzó adat: emailtargy elem.` |

The two expected download-validation failures are A1's source drift, not failing crate tests. Scratch sources are ephemeral; relevant input transformations, results, code lines and source fingerprints are recorded here.

## What was actually live-tested

`docs/szamlazz-hu-behaviour.md:3–24,30–31,155–172` records **invoice-family** test-account probes on September 3/6/7, with raw logs outside the repository. P60 concerns invoice arithmetic/storage/VAT formatting. B4 repeat-storno success, B8 credit removal, order fingerprint/reuse and external-ID behavior are invoice facts. They do not establish any receipt behavior. Its remaining-rounding questions (:249–254) likewise do not supply receipt observations.

Current `crates/szamlazz-agent/tests/live.rs:19–23,25–170` implements taxpayer smoke, invoice lifecycle/credit entries and proforma lifecycle. `tests/probes.rs:1–64` implements mismatching invoice/storno appearance probes. Neither contains a receipt lifecycle. Presence of a live test is also not proof it was run at this HEAD. No such test was run in this audit.

**Receipt-specific live execution evidence was not established from the requested repository sources.** In particular: creation-call-ID duplicate behavior is documented rather than independently probed here; storno call-ID scope/repeat codes, exact order newest selection, post-storno tenders, foreign-rate omission/storage, PDF layout, email inheritance/delivery, erasure issuance and actual NAV reporting remain unobserved. The offline coverage is substantial, but it must not be described as tested vendor acceptance of the complete receipt surface.
