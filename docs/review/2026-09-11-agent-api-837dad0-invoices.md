# Számla Agent conformance review — invoice creation requests

**HEAD:** `837dad024300e2a202c2b6351fcba73df82a7744`
**Independent audit and official-source retrieval:** 2026-09-11

## Result

**No confirmed functional defect found in the assigned scope.** All fields in the current invoice-request schemas have representations on the applicable request forms. All six creation kinds, the four carrier blocks, both ledger blocks, advanced header/item settings and email attachments are covered.

Three official-source disagreements remain: combined preview/simple-item ordering (**potential Medium interoperability impact**), missing fields in the downloadable schema (**Low tooling impact**), and conflicting layout labels (**Low selection ambiguity**). These are not established deployed-server failures. The prior arithmetic precision-loss defect remains resolved at this HEAD. **47 focused offline tests passed.**

This report covers the full current implementation of `ops/invoice.rs`'s request portion, `item.rs`, `types.rs`'s invoice-request vocabulary and `ops/waybill.rs`. Shared XML/multipart plumbing and exact arithmetic were followed where these requests depend on them. Response parsing, other operations and the worker are not independently certified by this slice.

No further agents were started. No source, tests or existing reports were edited; no credentials or live Számla Agent operations were used. Retrieval was unauthenticated documentation/artifact GET only. The source inventory and first comparison preceded consulting earlier invoice reviews. Their results were used for historical closure, not as primary contract evidence.

## Evidence standard and location key

Paths in the tables are relative to `crates/szamlazz-agent/src/`:

- **I** — `ops/invoice.rs`
- **W** — `ops/waybill.rs`
- **T** — `types.rs`
- **A** — `item.rs`
- **X** — `xml.rs`
- **N** — `number.rs`

“Covered” means the current model and serializer were compared with names, order, namespace, scalar types, cardinality and applicable published rules. It does not mean the vendor executed every combination. Synthetic/golden assertions demonstrate local behavior only. Most business-content rules deliberately remain server decisions; absence of a duplicate client-side validator is not itself a conformance defect.

`docs/szamlazz-hu-behaviour.md:3–28` limits its observations to one TEST account and dated September runs. Its original raw exchange logs are expressly outside the repository. This audit accepts the recorded observations within that scope, rather than claiming independent access to those logs. Existing `tests/probes.rs:12–63` supplies appearance-probe code, not proof of a new execution.

## Official URLs actually fetched

Every URL below was retrieved during this audit. The docs pages report site build `v202608271632`; that is not a publication date for every statement.

| ID | Retrieved source | Reviewed contract |
|---|---|---|
| IDX | [Generating invoice](https://docs.szamlazz.hu/agent/category/generating-invoice), [settings/rules index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) | Descendant inventory; all ten settings pages below fetched. |
| R | [Request](https://docs.szamlazz.hu/agent/generating_invoice/request) | Target, POST, multipart action and attachments. |
| S | [EN XML/example/inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU counterpart](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Complete schema and example, including every nested complex type. |
| D | [Download XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Complete independent downloadable schema; source conflicts retained. |
| RS | [Response](https://docs.szamlazz.hu/agent/generating_invoice/response) | Request response-version choice; not a full response-parser review. |
| K | [Document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types) | Six creation kinds, references, paper/e-invoice, 1:1 prepayment/final. |
| SI | [Tour operators](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Exact `simpleItems`, eligibility, inheritance, item/VAT limits, complete NAV data. |
| V | [VAT EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [VAT HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | Full token list; `eusAfa` and seller conditions. |
| Q | [Rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding) | Net-first and gross-first HUF rules, fractional-input normalization. |
| C | [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) | Full code list, HUF/Ft, foreign bank/rate requirement. |
| L | [Templates/languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) | Six templates, fifteen languages, omitted-template default. |
| O | [Order number](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Account toggle, per-kind duplicate check, conditional replay. |
| DI | [Discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount) | Negative-price adjacent row, positive quantity, same VAT. |
| E | [Email notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Omitted/true/false send flag, recipients, BBCode, five 2 MB files, test redirection. |
| ER | [Erasure codes](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Nonnegative count, 400 maximum, account enablement. |
| ER-KB | [Erasure knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) | Count rather than code identifier, row tail, SzlaMost, stock/vendor allocation. |
| V-KB | [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere) | Detailed VAT guide and EU tax-number qualifications. |
| V-PDF | [Linked VAT guide, 2025-11-04](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf) | Page 1: KBAUK, KBAET, TAHK and K.AFA subtype wording. Fresh PDF read with PDF tool after web text conversion returned binary. |
| L-KB | [Layout knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok) | Layout-label contradiction, postal service UI-only, rendering limits. |
| PHP | [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Fresh in-memory ZIP inspection: header order, payment constants, aggregator description. SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. |

## Findings: source ambiguities, not confirmed functional defects

### INV-S1 — Preview and simplified-image order conflicts across official schemas

**Classification:** source ambiguity; potential **Medium** interoperability impact. High confidence in the contradiction, deployed behavior unverified.

**Code:** I:197–225,811–826, particularly 819–825. Both options independently emit when present. Current tail is `szamlaSablon`, `elonezetpdf`, `simpleItems`.

**Official requirement:** S says, “the order of the fields is fixed, **they cannot be interchanged**.” Both freshly fetched EN/HU inline XSD sequences put `simpleItems` before `elonezetpdf`. D instead puts `elonezetpdf` before `simpleItems`. Fresh PHP `Header/InvoiceHeader.php:398–404` also writes template → preview → simple items.

**Concrete reproduction:** take an ordinary request with one item and set `header.preview_pdf=Some(true)` and `header.simple_items=Some(true)`. Its writer emits:

```xml
<elonezetpdf>true</elonezetpdf><simpleItems>true</simpleItems>
```

That sequence violates the inline XSD's header order and follows the download's. Explicit false values have the same ordering conflict when both elements are present. Existing `tests/simple_items.rs:20–91`, rerun here, confirms all nine presence/value pairs over all six kinds and retention of monetary data. This audit did not execute a full XSD validator; the schema disagreement follows directly from the inspected sequences.

**Impact:** inline-schema validation rejects the combined shape; actual rejection or ignored flags by the deployed service remains unknown. The no-issuance property of a preview should not be inferred from passing a synthetic writer test.

**Live adjudication/disposition:** no relevant combined-preview probe is recorded in the inspected behavior notes. This is an intentional, documented source choice (I:220–222,822–823; `fixtures/SOURCES.md:193–228`), not a live-tested exemption. Reordering unconditionally would contradict another current official source. Vendor clarification or separately authorized evidence is needed before changing the policy.

### INV-S2 — Downloadable invoice XSD omits documented group-id and erasure fields

**Classification:** source drift; **Low** tooling impact; supported fields are not implementation defects.

**Code:** I:321–322,853 (`csoportazonosito`); A:115–131 and I:914–916 (`torloKod`).

**Official requirement:** S declares `csoportazonosito` as optional string after `adoszam`, and `torloKod` as optional nonnegative int at the end of `tetel`. ER says, “At most **400** data erasure codes can be assigned per item.” ER-KB explicitly calls the value “az igényelt kódok **darabszámát**” (the requested number of codes). Fresh D contains neither field.

**Reproduction/impact:** set `buyer.group_id=Some("12345678")` or an item's `erasure_code_count=Some(1)`. The correct emitted elements have no corresponding declaration in D and are incompatible with that schema's sequence, while present in S. A consumer relying exclusively on D would reject supported request content or remove a feature unnecessarily.

**Live adjudication/disposition:** no group/erasure live result was found; ER's documented account conditions cannot be established with a synthetic row. Preserve these fields based on positive current documentation. `fixtures/SOURCES.md:126–140` identifies the cached XSD as manually patched for them. It must not be described as an unmodified vendor download.

### INV-S3 — Traditional and envelope-friendly layout names disagree

**Classification:** source ambiguity; **Low** layout-selection impact.

**Code:** T:988–1018, especially `Default` → `SzlaAlap` and `NoEnvelope` → `SzlaNoEnv`; I:193–196,811–818.

**Official statements:** L labels `SzlaAlap` “Tradicionális számlakép” and `SzlaNoEnv` “Borítékbarát számlakép”. Its linked L-KB instead states “Tradicionális: <szamlasablon>SzlaNoEnv</szamlasablon>” and “Borítékbarát: <szamlasablon>SzlaAlap</szamlasablon>”. The KB's linked example filenames agree with its reversed labels. It also incorrectly lowercases the schema's `szamlaSablon` spelling and delivery token in its snippets.

**Reproduction/impact:** selecting `InvoiceTemplate::Default` explicitly emits `SzlaAlap`, as documented by this crate and L, but the linked KB describes that token as envelope-friendly. All six wire tokens are available; the uncertainty is what named visual layout a caller expects.

**Live adjudication/disposition:** no template rendering probe was found or run. Keep exact schema casing and current token mapping; do not swap tokens solely to agree with one of the competing labels. Omission remains distinct from explicitly selecting `Default`.

## Complete request coverage matrix

`?` denotes an optional XSD element. Lists retain wire order except the explicitly identified header-tail disagreement. Required scalar fields are supplied by the caller; constructors do not invent dates or buyer identity. Optional strings distinguish absent from present-empty. Optional booleans preserve explicit false and true.

### Envelope, kinds and settings

| Surface | Fields / current locations | Assessment |
|---|---|---|
| HTTP contribution | I:678–679,938–948; `wire.rs:14,66–99` | R: “Form field name: `action-xmlagentxmlfile`”; POST multipart with XML file and `attachfile1`…`attachfile5`, target `https://www.szamlazz.hu/szamla/`. Correct. |
| XML root | I:737–920; X:139–160 | XML 1.0 UTF-8, `xmlszamla`, default namespace `http://www.szamlazz.hu/xmlszamla`; `beallitasok → fejlec → elado → vevo → fuvarlevel? → tetelek`. Required empty seller container retained. Example `xsi:schemaLocation` is a schema hint, not a missing business field. |
| Invoice | I:35–40,774–779 | No selected true flag; optional explicit proforma reference. |
| Proforma | I:41–43,794 | `dijbekero=true`. |
| Delivery note | I:44–46,795,811–818 | `szallitolevel=true` and K's `SzlaFuvarlevelesAlap`; caller template override expressly documented. |
| Prepayment | I:47–59,774–780 | Optional proforma reference before `elolegszamla=true`. |
| Final | I:60–78,686–701,774–788 | `vegszamla=true`, optional `elolegSzamlaszam`, optional earlier proforma reference; requires nonblank prepayment number or order. K: “one prepayment invoice can have exactly one final invoice”; single reference is appropriate. |
| Corrective | I:79–84,790–793 | `helyesbitoszamla=true` plus `helyesbitettSzamlaszam`. Reference present by construction; existence/content server-owned. |
| Credentials | I:740; X:610–619 | `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?`: emits key or user/password in sequence. |
| Required settings | I:532–542,604–605,741–742 | `eszamla`, `szamlaLetoltes`: explicit bools, local default false. K confirms true e-invoice / false paper. |
| Copy count / version | I:543–547,743–746 | `szamlaLetoltesPld?` u8 narrower than int but S says obsolete/ignored. `valaszVerzio=2` deliberately chooses RS's structured XML/base64 mode. |
| Advanced settings | I:548–556,747–754 | `aggregator?` string, `guardian?` bool, `cikkazoninvoice?` bool, `szamlaKulsoAzon?` string: complete and ordered. |

### Header

| Ordered XML fields | Model / writer | Assessment |
|---|---|---|
| `keltDatum?`, `teljesitesDatum`, `fizetesiHataridoDatum` | I:141–156,758–760 | Optional issue date, required fulfillment/due Date. HU's fulfillment meaning is used rather than EN example's misleading “payment date”. |
| `fizmod`, `penznem`, `szamlaNyelve`, `megjegyzes?` | I:157–164,761–764 | PaymentMethod/Currency/Language/string; vocabulary below. |
| `arfolyamBank?`, `arfolyam?` | I:165–166,719–729,765–770; T:943–980 | Explicit bank/rate or automatic MNB with omitted rate. S/D explicitly allow the latter despite C's general bank-and-rate requirement. |
| `rendelesSzam?` | I:167–169,771 | Preserved caller string, usable for queries; O's account-controlled conditional replay is not unconditional idempotency. |
| `dijbekeroSzamlaszam?`, `elolegszamla?`, `vegszamla?`, `elolegSzamlaszam?`, `helyesbitoszamla?`, `helyesbitettSzamlaszam?`, `dijbekero?`, `szallitolevel?` | I:772–796 | All eight slots covered through applicable kinds. |
| `logoExtra?`, `szamlaszamElotag?`, `fizetendoKorrekcio?` | I:170–177,797–801 | Two strings and Decimal adjustment retained. Registration/upload remain account concerns. |
| `fizetve?`, `arresAfa?`, `eusAfa?` | I:178–192,802–810 | Paid true-only policy; other two tri-state. V's accepted `eusAfa=true` suppresses NAV submission and requires OSS or non-Hungarian seller; correct item VAT still needed. Docs accurately describe this. |
| `szamlaSablon?`, `elonezetpdf?`, `simpleItems?` | I:193–225,811–826 | All options supported; INV-S1 qualifies combined order. |

SI rules are accurately documented at I:199–218: per-document selection; OSS off/Hungarian seller; max two items, final up to four; allowed VAT `0/5/18/27/TAM/AAM/K.AFA/F.AFA`; final inherits setting and must match prepayment VATs; simplified originals cannot be corrected; corrective/delivery shapes disallowed; template overridden server-side. Full item money is still serialized. Those rules are not inferred from acceptance by `validate()` in synthetic tests.

### Parties and buyer ledger

| Ordered XML fields | Model / writer | Assessment |
|---|---|---|
| Seller `bank?`, `bankszamlaszam?`, `emailReplyto?`, `emailTargy?`, `emailSzoveg?`, `alairoNeve?` | I:262–275,828–837; T:1022–1032 | All six; SellerEmail flattened. BBCode and newline text transport supported. No seller name/tax override exists in this request schema. |
| Buyer `nev`, `orszag?`, `irsz`, `telepules`, `cim` | I:296–306,840–844 | Four required strings, optional country; complete. |
| `email?`, `sendEmail?` | I:307–315,845–848 | E: supplied email plus omitted/true sends; explicit false suppresses. Comma-separated recipients supported. Test accounts redirect to configured account email. |
| `adoalany?`, `adoszam?`, `csoportazonosito?`, `adoszamEU?` | I:316–324,849–854 | Status plus three strings; INV-S2 preserves group-id support. |
| `postazasiNev?`, `postazasiOrszag?`, `postazasiIrsz?`, `postazasiTelepules?`, `postazasiCim?` | I:277–291,855–861 | All five flattened optional fields, partial postal address representable. L-KB says actual postal service is UI-only. |
| `vevoFokonyv?`: `konyvelesDatum?`, `vevoAzonosito?`, `vevoFokonyviSzam?`, `folyamatosTelj?`, `elszDatumTol?`, `elszDatumIg?` | I:120–135,862–873 | Date/string/string/bool/Date/Date; complete, empty-present container possible. |
| `azonosito?`, `alairoNeve?`, `telefonszam?`, `megjegyzes?` | I:329–347,874–877 | Complete. Buyer-id rustdoc explains partner update and shared customer-account document access, distinct from queried numeric id. |

### Items, ledger and carriers

| Ordered XML fields | Current locations | Assessment |
|---|---|---|
| `tetelek/tetel` | I:682–685,882–919 | At least one via checked boundary, unbounded Vec, order preserved. |
| `megnevezes`, `azonosito?`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs`, `arresAfaAlap?`, `nettoErtek`, `afaErtek`, `bruttoErtek`, `megjegyzes?` | A:90–112; I:885–897 | Complete strings/Decimals/VatRate. Optional margin base correctly between VAT token and net. All amounts explicitly supplied as S requires. |
| `tetelFokonyv?`: `gazdasagiEsem?`, `gazdasagiEsemAfa?`, `arbevetelFokonyviSzam?`, `afaFokonyviSzam?`, `elszDatumTol?`, `elszDatumIg?` | A:52–72; I:898–913 | Four strings/two Dates, complete and ordered. |
| `torloKod?` | A:115–131; I:703–710,914–916 | Final row field, u32 count checked at 400. Account/template conditions documented; INV-S2 applies. |
| Waybill `uticel?`, `futarSzolgalat?`, `vonalkod?`, `megjegyzes?`, `tof?`, `ppp?`, `sprinter?`, `mpl?` | W:87–113,132–178 | Complete; unused destination labelled; general barcode fallback documented. Suitable invoice layouts can display waybills, not just delivery notes. |
| TOF `azonosito?`, `shipmentID?`, `csomagszam?`, `countryCode?`, `zip?`, `service?` | W:9–24,137–148 | All six, exact case; string id preserves leading zeros. |
| PPP `vonalkodPrefix?`, `vonalkodPostfix?` | W:26–33,149–154 | Both strings; documented three-character prefix/max-seven suffix representable. |
| Sprinter `azonosito?`, `feladokod?`, `iranykod?`, `csomagszam?`, `vonalkodPostfix?`, `szallitasiIdo?` | W:35–50,155–166 | All six; documented three-character id, ten-digit sender, 7–13 suffix representable. |
| MPL `vevokod`, `vonalkod`, `tomeg`, `kulonszolgaltatasok?`, `erteknyilvanitas?` | W:52–84,167–175 | Three required strings always emitted (weight correctly string), optional services string/value Decimal. |

Carrier string accommodates `TOF/PPP/SPRINTER/FOXPOST/MPL/GLS/EMPTY`; no additional FOXPOST/GLS child type exists in S/D. Multiple carrier children are a sequence, not a schema choice. TOF/Sprinter parcel u32 values are checked against `i32::MAX` at I:711–718 and W:115–129. Carrier identifier lengths and rendering of partial blocks remain server/content matters.

### Attachments, scalar vocabulary and arithmetic

| Surface | Locations and conclusion |
|---|---|
| Attachments | I:379–516,938–948: bounded at five files and 2,000,000 bytes each through push, Vec conversion and serde, immutable slice access. Raw bytes, MIME, filename carried in multipart. E's “2 MB” lacks a byte definition; decimal MB is an explicitly conservative policy. No truncation or sixth-file loss found. |
| XML text/scalars | X:568–607; `wire.rs:402–422`: escaped text, locale-independent decimal text, ISO dates, true/false, XML 1.0 character check in `to_wire`. `write_xml` is the lower-level unchecked boundary. |
| Numeric VAT | T:186–189,251–324 covers every V percentage, including 2.1, 4.8, 5.5, 7.7, 8.1, 9.5, 13.5, 25.5. Percent normalizes trailing zeros; numeric Other values calculate numerically while preserving their sent text (A:194–207). |
| Special VAT | T:190–248,266–324 covers `TAHK TAM AAM EUT EUKT F.AFA K.AFA HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET`; Other permits newer tokens. Shared receipt-only codes are labelled. |
| VAT meanings | T:194–210,232–237 agree with V-PDF page 1: TAHK differs from TAM, KBAUK is new means of transport (not “to UK” as V's terse English table suggests). K.AFA uses exact phrases in invoice comment/item name/item comment; unmatched text defaults to used-goods subtype. |
| Currency | T:369–477 preserves all C codes, including `Ft`, historical EEK/HRK/LTL/LVL and vendor `KSH`. Named constants are not an allowlist. HUF/Ft detection is case-insensitive; sent spelling unchanged. |
| Language | T:480–585 exactly covers `hu en de it ro sk hr fr es cz pl bg nl ru si`. Vendor `cz`/`si` are intentional, not ISO spelling defects. |
| Payment method | T:588–687: all seven named tokens match fresh PHP `Header/InvoiceHeader.php:47–53`; PHP's additional `OTP Simple` is representable through Other, as is free text. |
| Taxpayer status | T:690–755 covers S's `7/6/1/0/-1` codes. XML gets integer lexical text regardless of the local serde string representation. |
| Templates | T:983–1020 covers all six plus Other; explicit SzlaAlap differs from omitted default; INV-S3 qualifies visual names. |
| Number references | T:22–57 preserves invoice numbers; kind flags, not queried `DocumentType` tokens, select creation. Queried-type behavior is outside this request slice. |
| Derived arithmetic | A:9–49,163–223 and N:6–59: exact representable net product, caller-selected net rounding, exact VAT product/division, VAT rounding, exact gross sum. Half away from zero. Refuses unrepresentable intermediates rather than silently rounding/underflowing. |

DI says “net unit price … negative”, quantity positive, same VAT, discount directly after its item. Signed Decimal rows and Vec order cover it. Q's gross-first example (quantity 3, net unit price 393.66, net 1181, VAT 319, gross 1500) is representable with `LineItem::new`; missing gross-first convenience does not remove capability. The net-first helper intentionally calculates zero VAT for nonnumeric codes; callers supply explicit amounts/margin metadata when needed. ISO-like minor units are expressly local policies, not vendor storage guarantees for every currency.

## Deliberate unsupported shapes and conservative policies

- **Arbitrary mixed kind flags / unusual references:** I:21–30,104–116 selects one kind and exposes proforma references for invoice/prepayment/final only. Independent optional XSD slots do not establish a meaningful supported operation for every combination. No confirmed applicable capability is missing.
- **Response version 1:** deliberately unexposed; RS confirms version 2 is a supported alternative.
- **Explicit false `fizetve`:** I:178–180,802–804 omits false. This is documented local emission policy; full account/payment-method equivalence between absence and explicit false has no live evidence here. It is not treated as a proven defective business operation.
- **Foreign currency without any exchange-rate specification:** I:719–729 refuses it for every kind, including proforma/delivery note; requires a trimmed nonblank bank and numeric rate unless bank is exactly MNB. C supports the general requirement and S/D support automatic MNB. Behavior notes:216–223 mark possible exempt-kind relaxation unverified. This is not a live-tested exception.
- **Unlimited numeric precision, NaN/INF, negative parcel counts, deprecated copy counts above u8:** not offered by the typed model. The fact that an XSD scalar has a larger mathematical domain does not demonstrate a meaningful unsupported invoicing operation.
- **Automatic final deductions or percentage discounts:** not promised. Explicit same-VAT negative line amounts cover the intended document operations.
- **Account administration and rendering:** certificates, prefix registration, logo upload, postal dispatch and carrier barcode generation are not omitted invoice XML fields. Business eligibility and date/reference consistency are mostly server-validated.

Low-severity documentation opportunities, not counted as functional defects: I:548 calls aggregator “for contracted integrations”, while fresh PHP `SzamlaAgentSetting.php:116` describes “webáruházat futtató motor neve” (webshop engine name). Guardian's contract-only wording at I:550 is unsubstantiated by S/D. T:703's parenthetical “private individual” is narrower than S's `-1: no tax number`; V-PDF also discusses organizations without tax numbers. The correct token and fields remain usable.

## Accepted recorded live differences and their limits

| Recorded evidence in `docs/szamlazz-hu-behaviour.md` | Adjudication at this HEAD |
|---|---|
| P48-P5, line 91: yesterday's create issue date silently replaced with today | I:141–148 correctly qualifies the requested date. No incorrect always-rejected rule reported. |
| C6-2, lines 117–119: final does not deduct prepayment automatically | I:62–70 correctly requires caller's negative same-VAT row. K's “settle remaining amount” is not evidence of server netting. |
| C1-3/C2/D4/D5, lines 104–108: implicit proforma consumption; stale explicit link can be silently ignored | Keep explicit references and distinguish them from guaranteed linkage. Lines 234–240 explicitly leave ES/VS sent with proforma references unverified, as I:50–54,69–70 also says. |
| A3/XPRB, lines 63–71: external ids nonunique, newest-holder query, stored only on creating call | Request exposes a query handle, not a server idempotency key. |
| A4/C4, lines 40–57: trimmed order/case sensitivity and conditional replay; changed kelt can replay | Raw request retention is not proof that every requested date is stored. O's date/age requirements do not justify automatic resend by this client. |
| P60-H1…H5, line 159: tested HUF discrepancies 0.5/1/2 accepted, 5/10 refused | No exact equality validator added. Does not prove a universal absolute tolerance. |
| P60-E1/E3, line 160: EUR values independently stored at two decimals, possibly inconsistent gross | A:24–29 warns about Exact. This does not establish KWD/JPY or fractional HUF storage rules. |
| P60-E2, line 161: rounded EUR 100.01/27/127.01 stored as sent | Supports this concrete net-first minor-unit case. |
| P60-V1/V2, line 162: VAT 27.00 and 27.0 accepted | T:259–264 correctly calls token normalization hygiene. |
| P73, line 97: created false → queried paper code 1, true → electronic code 3 | Request bool remains correct; queried numeric appearance is a different surface. |
| D6, line 111: later buyer data changes observed | Buyer-id docs do not promise immutable partner master data. No buyer-id collision/portal-access probe is claimed. |

No live exemptions apply to INV-S1/S2/S3, automatic MNB, erasure enablement/rendering, OSS conditions or mail delivery. E explicitly documents test-account email redirection; D6's inability to trigger code 56 is not proof that test accounts send no email.

## Historical closure, checks and limits

After the independent field/source audit, `2026-09-10-agent-api-current-invoices.md` and `2026-09-11-agent-api-invoices.md` were consulted. Their previous no-functional-defect result was not assumed. Their older precision-loss finding remains closed by A:191–211 and N:6–59, with current regression tests rerun:

- Quantity `0.9999999999999999999999999999`, price `0.005`, AAM: error for Exact and Scale(2), rather than a silently rounded successful intermediate.
- Quantity `0.1`, price `1e-28`: net underflow refused.
- Quantity 1, price `1e-28`, VAT 27%: VAT underflow refused.
- Quantity 1, price `1e28`, VAT `1e-28`%: unrepresentable gross refused.
- Normal, negative and exact-representable boundary controls pass. These are synthetic arithmetic assertions, not vendor-accepted edge-rate evidence.

Executed focused commands:

```text
git status --short && git rev-parse HEAD
cargo test --locked -p szamlazz-agent --lib ops::invoice::tests
cargo test --locked -p szamlazz-agent --lib item::tests
cargo test --locked -p szamlazz-agent --test simple_items --test numeric_fidelity
```

Results: **31 invoice unit + 8 item unit + 2 simple-items + 6 numeric-fidelity = 47 passed**, zero failed/ignored. Some filters also exercise response controls; this does not expand the claimed response audit. No live test target was selected. No new tests were written and no previous scratch checker/results were reused.

Fixture checks: `fixtures/SOURCES.md:23–39,126–140,193–228,232–265`, cached `upstream/agent/requests/xmlszamla.xml`, cached `upstream/agent/xsd/xmlszamla.xsd` and `tests/simple_items.rs` were inspected. Cached request/schema acquisition is historical, the invoice schema was patched for two fields and lacks today's `simpleItems`; the separately retained September tails document later source disagreement. Golden fixtures are project-generated. Outline matching loses empty-container/text distinctions and cannot prove absence/presence equivalence. This audit used the current official pages for coverage rather than assuming those cached files are current or universally authoritative.

**Limits:** source/manual sequence comparison plus focused offline execution, not a new full-XSD validation run, exhaustive input fuzzing, rendered-PDF check, mail/carrier execution or live-account verification. Date chronology and every partial-block/unknown-token combination are not certified. The report is a scope-specific contribution to the full-tree review, not a certification of the entire crate.
