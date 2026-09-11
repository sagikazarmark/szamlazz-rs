# Current Számla Agent conformance review — invoice creation

Reviewed **2026-09-11**, starting HEAD **61c334f9508e8b63df2f3db182d6ca83f4feb8f0**, against the **current working tree**. Scope: `crates/szamlazz-agent` invoice creation requests, kinds, shared request values and line items, and waybill. This is the invoice slice of the whole-crate review, not a verdict on the other operations or response ingestion.

## Executive result

- **No high- or medium-severity confirmed invoice field-mapping bug found.** Every element in the current inline invoice request XSD has a representation or a deliberate fixed/derived value in the crate. Required containers, ordinary field order, names, money types and supported token sets match.
- **One low-severity confirmed shared-writer conformance defect:** non-positive `jiff::civil::Date` years can pass request validation and produce dates outside the XSD 1.0 lexical space. Normal invoicing dates are unaffected (I-01).
- **One low-priority convenience omission:** the documented gross-first HUF calculation has no helper; the explicit-value constructor can represent it fully (O-01). This is not a wire-feature omission or incorrect calculation by the existing net-first helper.
- **Two material source conflicts, not established runtime bugs:** preview/simple-items order, and the downloadable schema omitting buyer VAT-group and erasure-count fields. Current code is supported by current first-party sources; changing it just to satisfy one of the conflicting schemas would not be justified (U-01/U-02).
- Relevant offline checks: **204 passed, 0 failed**. Passing golden/outline tests do not establish whole-XSD validity or vendor acceptance.

No previous review report was used as evidence. No code was changed, secrets read, or Számla Agent operation invoked. Only public documentation/downloads were fetched. Only this report was authored. Subagents were requested, but this review session exposed no agent-launch tool; independent source checks were parallelized with the available tools instead.

## Evidence and method

All vendor sources below were fetched afresh on 2026-09-11. The docs site displayed build **v202608271632**. URLs are the authorities for the quoted claims, not archived repository copies.

| ID | Current authoritative source | Evidence used |
|---|---|---|
| S1 | https://docs.szamlazz.hu/agent/category/generating-invoice | Scope/navigation entry point. |
| S2 | https://docs.szamlazz.hu/agent/generating_invoice/request | “Form field name: `action-xmlagentxmlfile` (main file); optional attachments: `attachfile1` … `attachfile5`”; POST, multipart, target URL. |
| S3 | https://docs.szamlazz.hu/agent/generating_invoice/xml | Full inline example and full inline XSD; “the order of the fields is fixed, **they cannot be interchanged**”; “Elements marked `minOccurs="0"` may be omitted.” |
| S4 | https://docs.szamlazz.hu/hu/agent/generating_invoice/xml | Full Hungarian example/XSD and buyer/carrier annotations; checked independently of the English rendering. |
| S5 | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd | Actual linked downloadable XSD, including every complex type, sequence and language enumeration. |
| S6 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types | Flags, references, delivery-note template, electronic/paper selection, one-prepayment/one-final restriction. |
| S7 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency | `simpleItems`, default, permitted kinds, seller conditions, item limits, template override, inheritance. |
| S8 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding | HUF net-first and gross-first arithmetic, examples, server fractional-HUF correction table. |
| S9 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies | Entire accepted currency list; HUF/Ft; foreign-currency bank/rate requirement. |
| S10 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template | Six template tokens, fifteen language tokens, default and simplified-view override. |
| S11 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number | Per-type duplicate check, account toggle, storno/corrective exemptions, two-day matching-request rule. |
| S12 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount | Negative unit price, positive quantity, same VAT rate, preserved adjacent line order; no separate discount field. |
| S13 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification | Email with omitted/true `sendEmail`, explicit false suppression, comma-separated recipients, BBCode/newlines, five attachments at 2 MB each. |
| S14 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code | Integer ≥0, at most 400 per item, feature enablement. |
| S15 | https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor | Exact count semantics: “a … `<torloKod>` mezőben az igényelt kódok **darabszámát**”; at end of `tetel`; `SzlaMost` requirement. |
| S16 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates | Complete numeric and special VAT token lists; `eusAfa` conditions and effect. |
| S17 | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates | Hungarian token meanings and `eusAfa` guidance. |
| S18 | https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere | Current linked VAT explanation and link to detailed first-party table. |
| S19 | https://tudastar.szamlazz.hu/hs-fs/hubfs/GYIK/AFA-kulcsok_NOSZ-segedlet_2025-11-04.png?width=1340&name=AFA-kulcsok_NOSZ-segedlet_2025-11-04.png | Visually read detailed first-party VAT table: KBAUK new means of transport, KBAET supply of goods, K.AFA subtype text/default. The linked PDF was fetched, but text extraction was unavailable; the image is the evidence actually read. |
| S20 | https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip | Fresh in-memory inspection of official `Header/InvoiceHeader.php`, particularly lines 393–404: omit unpaid `fizetve`, template → preview → simple items. Supporting source, not proof of server execution. |
| S21 | https://docs.szamlazz.hu/agent/basics/sending-requests | One document per XML; case-sensitive names; recommendation to validate against linked XSD; misspelled tags may be refused or ignored. |
| S22 | https://www.w3.org/TR/xmlschema-2/#date and https://www.w3.org/TR/xmlschema-2/#dateTime-lexical-representation | XSD 1.0 date grammar, including prohibition on year zero and leading zeros in expanded years. |
| S23 | https://www.w3.org/TR/xml/#sec-line-ends | XML 1.0 end-of-line normalization, checked against actual quick-xml 0.42 text escaping. |

I read the complete scoped implementations and their invoice/unit tests, relevant `simple_items.rs`, `numeric_fidelity.rs`, and the request-comparison machinery in `upstream.rs`; also inspected `xml.rs`, current `wire.rs`, and `number.rs` where shared implementation affects outbound requests. The working-tree `wire.rs` addition extracts and publicly exposes `validate_xml_text`; its current implementation was included in this review.

A fresh in-memory extraction parsed both inline XSDs and the downloaded XSD with Python's XML parser and enumerated every element declaration, type, requiredness and sequence. Both inline schemas contain **125 element declarations** (including root, containers and nested declarations); the download contains **123**. EN and HU element sequences/attributes agree; the only complex-type structural differences from the download are `vevoTipus`, `tetelTipus` and `fejlecTipus`, detailed below. This is a source comparison, not an XSD validator run over generated requests.

`docs/szamlazz-hu-behaviour.md` was read as requested. Its rows are reported observations from one test account, not current universal protocol guarantees; underlying raw vendor-call logs are not in the repository and were not accessed.

## Ranked confirmed issue

### I-01 — P3: Shared date writer accepts Jiff years which are not valid XSD 1.0 dates

**Classification:** confirmed outbound lexical conformance defect; low practical severity; shared concern for the other request reviewers.

**Code:**

- `crates/szamlazz-agent/src/xml.rs:598–607`: dates are written with `Date::to_string()` without an XSD-domain check.
- `crates/szamlazz-agent/src/ops/invoice.rs:682–732`: invoice validation checks items, final reference, erasure/parcel counts and exchange rate, but not date-year representation.
- Affected invoice emissions: `invoice.rs:758–760`, `864–871`, `910–911` (header, buyer ledger, item settlement dates).
- `crates/szamlazz-agent/src/wire.rs:402–405,412–438`: `to_wire` checks XML characters, not date lexical validity.

**Authoritative requirement:** S3/S5 declare, for example, `<element name="teljesitesDatum" type="date" maxOccurs="1" minOccurs="1">`. S22 defines date as `'-'? yyyy '-' mm '-' dd zzzzzz?` (optional timezone), with the year taken from the dateTime grammar; that grammar says **“if more than four digits, leading zeros are prohibited, and '0000' is prohibited”**. This is XSD 1.0; XSD 1.1 changes year-zero handling, but does not justify assuming that change for this vendor.

**Evidence:** the resolved Jiff 0.2.35 permits years `-9999..=9999`, explicitly including zero (`jiff/src/civil/date.rs:398–426`). Its actual printer (`src/fmt/temporal/printer.rs:379–388`) prefixes a negative year with `-00` before writing four digits. Therefore an otherwise ordinary invoice with `fulfillment_date = jiff::civil::date(0, 1, 1)` emits `0000-01-01`; `date(-1, 1, 1)` emits `-000001-01-01`. Both pass the present invoice checks and character validation, but neither is an XSD 1.0 `date`. This conclusion follows directly from current source; no vendor rejection or new executable reproduction is claimed.

**Impact:** malformed upstream business data or a year-zero sentinel can escape the typed request boundary as an ostensibly ready-to-send request. A schema-aware recipient must reject the date; normal positive-year requests are unaffected. The defect is not evidence of a valid contemporary invoice being refused.

**Suggested correction:** add fallible validation for the outbound date domain, consistently at all request date positions. For this invoicing domain, explicitly supporting positive years only is simpler than silently translating astronomical BCE years into the different XSD 1.0 era convention. Document that policy; do not shift dates. Keep ordinary dates rendered as `YYYY-MM-DD`.

**Meaningful regression:** exercise `to_wire` with year 0 and a negative year at one mandatory and one nested optional invoice date, plus an ordinary leap-day control. Ensure a future common validator covers all operations rather than fixing only `fulfillment_date`.

## Documented convenience omission

### O-01 — P3 enhancement: no gross-first HUF calculation helper

**Code:** `crates/szamlazz-agent/src/item.rs:163–222` only derives from net unit price; `item.rs:133–160` accepts all explicit amounts.

**Authority:** S8, “Rounding based on GROSS value (HUF invoice)”: **“In case you are issuing an invoice to a natural person (B2C) you have to use this method.”** Its worked example sends quantity `3`, net unit price `393.66`, net `1181`, VAT `319`, gross `1500`.

**Assessment/impact:** callers starting from a consumer-facing gross price must implement that derivation themselves and use `LineItem::new`. Repeated net-first recalculation is not a substitute for preserving an agreed gross total. However, `try_calculated` accurately describes its net-first inputs and algorithm; the documented example is fully representable today. **This is not an unsupported XML feature and not a defect in its existing calculation.**

**Suggested improvement:** document the gross-first recipe and explicit constructor alongside the derived constructor; add a separately named fallible gross-first helper only if the crate intends to own that calculation. A helper should have tests against the vendor example and a fractional/negative boundary, rather than tests merely restating its arithmetic.

No other documented in-scope feature was found unrepresentable. In particular, free-string currency/payment/carrier/template escape hatches count as support; absence of a named enum variant for every token is not a missing wire feature.

## Source conflicts and unresolved questions

### U-01 — P2 investigation priority: two optional header elements have contradictory authoritative orders

**Code:** `invoice.rs:819–826`; public caveat at `220–222`; `tests/simple_items.rs:20–91` checks the actual chosen order for all three presence states and every kind.

**Exact sources:**

- S3 and S4 inline `fejlecTipus`: `<element name="szamlaSablon" ...>` → `<element name="simpleItems" type="boolean" ...>` → `<element name="elonezetpdf" type="boolean" ...>`.
- S5 download: `szamlaSablon` → `<element name="elonezetpdf" type="boolean" maxOccurs="1" minOccurs="0">` → `<element name="simpleItems" type="boolean" maxOccurs="1" minOccurs="0">`.
- S20 `InvoiceHeader.php:398–404`: `$data['szamlaSablon']`, then `$data['elonezetpdf']`, then `$data['simpleItems']`.
- S3 explicitly says order cannot be interchanged; S7 instructs placing `simpleItems` after `szamlaSablon`, but its example contains no preview field and cannot resolve this conflict.

**Impact:** when **both elements are present**, the crate's request fails the inline sequence, while reversing their order fails the downloadable sequence. This also matters when either value is explicit `false`, since element presence determines XSD ordering. When at least one is absent, there is no tail-order conflict.

**Disposition:** deliberate source-backed choice, already disclosed accurately in rustdoc. **Not a confirmed vendor-runtime failure, and not a live-tested deviation.** Request current vendor clarification of the effective schema before changing the serializer. Existing offline tests prove emission and the source disagreement, not acceptance of combined simplified-preview requests.

### U-02 — P2 documentation/schema investigation: downloaded XSD lacks two actively documented fields

**Code:** `invoice.rs:321–322,852–854` (`csoportazonosito`), `item.rs:115–131` and `invoice.rs:914–916` (`torloKod`).

**Exact sources:** S3/S4 declare `<element name="csoportazonosito" type="string" maxOccurs="1" minOccurs="0">` between `adoszam` and `adoszamEU`, and a `torloKod` restricted `int` with `<minInclusive value="0"/>` after `tetelFokonyv`. S5 has `adoszam` immediately followed by `adoszamEU`, and ends `tetelTipus` at `tetelFokonyv`; both fields are absent. S14 explicitly permits the count and limits it to 400, and S15 confirms its placement and meaning.

**Impact:** the crate correctly exposes current documented capabilities, but requests using either field cannot validate against the current download recommended by S21. No one downloaded schema can be treated as comprehensive proof of conformance. This is especially clear for `torloKod`, which the current official example itself sends.

**Disposition:** do not remove these fields to satisfy the incomplete download. Track/cite the discrepancy and obtain a corrected authoritative schema. Neither buyer-group execution nor erasure-code allocation was live-tested in this review; the behaviour note does not establish them.

### Other bounded uncertainties, not ranked bugs

- **Foreign-currency exemptions:** the XSD makes bank/rate optional, while S9 says foreign-currency documents need both. Automatic MNB omission is expressly supported by S3/S4/S5. The crate's requirement on every foreign-currency kind, including AAM/proforma/delivery note, follows the general rule. The behaviour note (`216–223`) explicitly leaves exemptions untested. No evidence justifies relaxing validation here.
- **Case-insensitive HUF recognition:** `types.rs:401–410` recognizes `huf`, `ft`, etc. locally and preserves the caller's token. S9 specifically lists `HUF` and `Ft`, not every case variant. Local tests establish only local classification/emission. This is not evidence that lowercase requests are accepted, but documented spellings work and the open currency type intentionally allows server decisions.
- **Kind/reference combinations:** the XSD declares booleans and references independently, but S6 documents ordinary document kinds, not arbitrary combinations. `InvoiceKind` models six meaningful exclusive kinds, explicit corrective reference, and proforma references on invoice/prepayment/final. Lack of proforma-reference setters on corrective/delivery-note/proforma is not an established omitted supported workflow. Explicit proforma links on prepayment/final remain unprobed (`behaviour.md:234–240`).
- **Paid false:** `InvoiceHeader::paid=false` omits `fizetve`, so the model cannot force the literal false element. Current official PHP does the same (S20 line 393: `if ($this->isPaid()) ...`). No fetched source establishes that omission and explicit false differ for invoice creation; do not claim a false-state bug without that evidence.
- **Money semantics for margin VAT:** `arresAfa` and `arresAfaAlap` are represented correctly, but the schema does not specify a complete margin-tax calculation. `try_calculated` openly produces zero VAT for nonpercentage tokens, including `K.AFA`; use explicit amounts for a different required calculation. No unsupported formula is inferred from those field names.
- **VAT translations:** S16's English KBAUK/ET descriptions read as geographic abbreviations. S19 identifies KBAUK as **“közösségen belüli új közlekedési eszköz értékesítés”**; the crate's new-means-of-transport comment is supported. Its K.AFA subtype explanation is also supported by the detailed table. These are not token-mapping defects.

## Field-by-field coverage matrix

Notation: **R** = XSD element must be present; **O** = optional. Unless stated otherwise, optional Rust values start as `None` and are omitted. Strings are escaped, booleans use `true`/`false`, `Decimal` emits plain finite decimal notation compatible with `xs:double`, and `Date` emits a civil date (I-01 is the exceptional-year caveat). Within each block the order below is the actual serializer order. Shared strings have no invented NAV/account validation.

### Envelope and settings — S2–S5/S13/S20/S21

| XML element | Rust representation / default | Check and code |
|---|---|---|
| `xmlszamla` | Fixed root, namespace `http://www.szamlazz.hu/xmlszamla` | UTF-8 XML 1.0; one document; `invoice.rs:737–739`, `xml.rs:139–160`. `xsi:schemaLocation` is not required instance data. |
| `beallitasok`, `fejlec`, `elado`, `vevo`, `fuvarlevel`, `tetelek` | R, R, R, R, O, R respectively | Exact root order; empty seller still emitted; `invoice.rs:739–919`. |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | O strings injected from one credential form | Username then password, or key; `xml.rs:610–619`. No secret values read. |
| `eszamla` | R bool, `e_invoice=false` | Explicit false for paper; `invoice.rs:539,604,741`. |
| `szamlaLetoltes` | R bool, `download_pdf=false` | Explicit false; independent of kind/preview flag; `542,605,742`. |
| `szamlaLetoltesPld` | O `u8`, `download_copies` | Valid subset of `int`; deprecated/ignored per inline annotation and rustdoc, `543–547,743–745`. No meaningful >255 omission. |
| `valaszVerzio` | O int fixed to `RESPONSE_VERSION` (`2`) | Deliberate structured response selection, `746`; official example documents both 1 and 2. |
| `aggregator` | O string | `747`; absent normally, contract-specific optional input. |
| `guardian` | O bool | `748–750`; preserves false as well as true. |
| `cikkazoninvoice` | O bool, `item_identifiers_on_invoice` | `751–753`; preserves explicit false. |
| `szamlaKulsoAzon` | O string, `external_id` | Correct settings location, `754`; not manufactured if absent. No uniqueness claim. |
| `attachfile1` … `attachfile5` | `InvoiceAttachments`, empty by default | Each filename/content/MIME retained; up to five files and 2,000,000 bytes each; private vector, checked push/TryFrom/Deserialize; `invoice.rs:379–516,938–948`. Conservative documented MB interpretation. |

### Header and kinds — S3–S12/S16/S17

| XML element | Rust representation / default | Check and code |
|---|---|---|
| `keltDatum` | O `issue_date: Date` | `758`; omission lets server choose; observed replacement documented at `141–148`. |
| `teljesitesDatum` | R `fulfillment_date: Date` | `759`; no confusion with payment date despite English example's loose label. |
| `fizetesiHataridoDatum` | R `due_date: Date` | `760`; no timezone conversion. |
| `fizmod` | R `PaymentMethod`, supplied to constructor | `761`; known Hungarian literals and `Other(String)`; free text possible. |
| `penznem` | R `Currency`, supplied | `762`; open string covers the full S9 list, including `Ft` and vendor spelling `KSH`. |
| `szamlaNyelve` | R `Language`, supplied | `763`; all 15 enum tokens exactly match XSD, including vendor `cz`/`si`. |
| `megjegyzes` | O string, `comment` | `764`; empty and absent remain distinct on emission. |
| `arfolyamBank`, `arfolyam` | O string and O double via `ExchangeRate { bank, rate }` | `765–770`; explicit Decimal or automatic MNB; bank before rate; `validate:719–730`. |
| `rendelesSzam` | O string, `order_number` | `771`; no unrequested normalization or account toggle in payload. |
| `dijbekeroSzamlaszam` | O `InvoiceNumber`, derived from kind | `774–777`; precedes all flags; available on regular/prepayment/final. |
| `elolegszamla` | O bool derived from `Prepayment` | True only for this kind, `780`. |
| `vegszamla`, `elolegSzamlaszam` | O bool and O reference via `Final` | Flag before reference, `781–789`; requires nonblank explicit reference or order number, `686–702`. |
| `helyesbitoszamla`, `helyesbitettSzamlaszam` | O bool and reference via `Corrective` | Flag then number, `790–793`; reference member required by type, but blank string remains server content validation. |
| `dijbekero` | O bool derived from `Proforma` | True, `794`. |
| `szallitolevel` | O bool derived from `DeliveryNote` | True, `795`; template also selected below. |
| `logoExtra` | O string, `extra_logo` | `797`; passes account token verbatim. |
| `szamlaszamElotag` | O string, `number_prefix` | `798`; registered-prefix restriction documented; no locally invented prefix alphabet. |
| `fizetendoKorrekcio` | O Decimal, `payable_adjustment` | `799–801`; correct double lexical form; not mistaken for a discount-line feature. |
| `fizetve` | O wire bool, Rust `paid=false` | True sent, false omitted, `802–804`; source-backed policy, uncertainty noted above. |
| `arresAfa` | O bool, `margin_vat` | `805–807`; explicit false supported. |
| `eusAfa` | O bool, `eu_vat` | `808–810`; explicit false supported; seller/OSS/NAV effect accurately documented at `183–192`. |
| `szamlaSablon` | O `InvoiceTemplate` | `811–818`; all six named tokens plus `Other`; delivery note forces `SzlaFuvarlevelesAlap`, explicitly documented. |
| `elonezetpdf` | O bool, `preview_pdf` | `819–821`; independent preview request; U-01 for combined tail. |
| `simpleItems` | O bool, `simple_items` | `824–826`; correct casing, absent/false/true preserved, monetary rows still complete; U-01. |

Kind audit: ordinary invoice sends no special flag; one selected special kind sends only its flag/reference. Unused optional flags are omitted instead of copied from the example as false. S3 explicitly permits their omission. All six crate kinds map correctly to S6/S5. Storno is correctly separate. Final invoice callers supply the negative prepayment line; the enum docs make this explicit. Nothing promises that the server deducts it automatically.

Simplified-item audit: `invoice.rs:199–222` correctly covers per-document selection, no NAV monetary-data loss, OSS off/Hungarian seller, ≤2 items (≤4 final), exact allowed VAT tokens, final inheritance/rate matching, template override, corrective and delivery-note prohibitions, and K.AFA comment requirement. These are sent as caller data and validated by the vendor; the crate does not pretend to know account state or original-document state locally.

### Seller, buyer and buyer ledger — S3/S4/S13

| XML element | Rust representation / default | Check and code |
|---|---|---|
| `elado/bank` | O string, `Seller.bank` | `invoice.rs:829`. |
| `bankszamlaszam` | O string, `Seller.bank_account` | `830`; leading zeros/text retained. |
| `emailReplyto`, `emailTargy`, `emailSzoveg` | Three independent O strings in O `SellerEmail` | `831–835`; reply-to, subject, body; BBCode and newlines are text, not injected XML. |
| `elado/alairoNeve` | O string, `signer_name` | `836`. |
| `vevo/nev` | R string, `Buyer.name` | `840`; required member, escaped. |
| `orszag` | O string, `country` | `841`. |
| `irsz`, `telepules`, `cim` | Three R strings: `zip`, `city`, `address` | `842–844`; postal codes are strings, not integers. |
| `email` | O string | `845`; comma-separated values possible; docs correctly warn that omission of `sendEmail` permits notification. |
| `sendEmail` | O bool, `send_email` | `846–848`; explicit false does not collapse to omission. |
| `adoalany` | O `TaxpayerStatus` | `849–851`; exact integer tokens `7,6,1,0,-1` per inline annotations. |
| `adoszam` | O string, `tax_number` | `852`. |
| `csoportazonosito` | O string, `group_id` | `853`; correct inline position; U-02. |
| `adoszamEU` | O string, `eu_tax_number` | `854`. |
| `postazasiNev`, `postazasiOrszag`, `postazasiIrsz`, `postazasiTelepules`, `postazasiCim` | Five independent O strings in O `PostalAddress` | `855–861`; flattened in exact sequence; no invented wrapper. |
| `vevoFokonyv` | O `BuyerLedger` container | `862–873`; no required child, so present-empty is schema-valid. |
| `konyvelesDatum` | O Date, `accounting_date` | `864`; I-01. |
| `vevoAzonosito`, `vevoFokonyviSzam` | O strings, `buyer_id`, `buyer_account` | `865–866`. |
| `folyamatosTelj` | O bool, `continuous_fulfillment` | `867–869`; false preserved. |
| `elszDatumTol`, `elszDatumIg` | O dates, `settlement_from/to` | `870–871`; I-01. |
| `vevo/azonosito` | O string, `id` | `874`; partner-association/update consequences correctly documented at `329–340`, not confused with queried numeric id. |
| `vevo/alairoNeve`, `telefonszam`, `megjegyzes` | O strings, `signer_name`, `phone`, `comment` | `875–877`; correct tail. |

### Line items and ledger — S3/S5/S8/S12/S14–S19

| XML element | Rust representation / default | Check and code |
|---|---|---|
| `tetelek/tetel` | R unbounded, `Vec<LineItem>` | `invoice.rs:882–917`; validates nonempty before wire; preserves order, including adjacent discounts. |
| `megnevezes` | R string, `name` | `885`. |
| `azonosito` | O string, `id` | `886`; occurs before quantity. |
| `mennyiseg` | R Decimal, `quantity` | `887`; supports fractional and negative quantities. |
| `mennyisegiEgyseg` | R string, `unit` | `888`; not an invented restricted enum. |
| `nettoEgysegar` | R Decimal, `unit_price` | `889`; fractional price retained; negative-price discount possible. |
| `afakulcs` | R `VatRate`, string on wire | `890`; all sixteen invoice special tokens and every listed fractional/numeric rate supported; normalization does not discard value. |
| `arresAfaAlap` | O Decimal, `margin_vat_base` | `891–893`; invoice-specific optional field retained. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | Three R Decimals: `net_value`, `vat_value`, `gross_value` | `894–896`; all explicitly sent; no reliance on server deriving omitted amounts. |
| `megjegyzes` | O string, `comment` | `897`. |
| `tetelFokonyv` | O `LineItemLedger` container | `898–913`; all optional children. |
| `gazdasagiEsem`, `gazdasagiEsemAfa` | O strings, `economic_event`, `vat_economic_event` | `900–904`; names and order correct. |
| `arbevetelFokonyviSzam`, `afaFokonyviSzam` | O strings, `revenue_account`, `vat_account` | `905–909`; account numbers remain text. |
| `elszDatumTol`, `elszDatumIg` | O dates, `settlement_from/to` | `910–911`; I-01. |
| `torloKod` | O `u32`, `erasure_code_count` | `914–916`; count, not code id; validates ≤400, within XSD int range; last field; U-02. |

Arithmetic check: `item.rs:183–222` computes exact net intermediate, applies selected rounding, computes percentage VAT from rounded net, rounds it, then adds net + VAT exactly. Overflow/precision loss is fallible (`number.rs:6–59`), not silently rounded by unchecked Decimal operations. Numeric `VatRate::Other` is interpreted numerically (`item.rs:194–201`) rather than incorrectly treated as zero VAT. Nonnumeric codes yield zero by documented helper policy. `LineItem::new` retains caller calculations. `Rounding::minor_unit` is explicitly local currency policy, including whole HUF; KWD and other currency precision are not claimed to be live verified.

### Waybill — S3/S4/S5

Every type/field below was compared with both current schema forms; they agree throughout this block.

| XML element(s), in order | Representation | Check and code in `ops/waybill.rs` |
|---|---|---|
| `uticel` | O string, `destination` | `95–97,133`; correctly marked unused; points to Sprinter routing replacement. |
| `futarSzolgalat` | O string, `carrier` | `98–99,134`; supports `TOF`, `PPP`, `SPRINTER`, `FOXPOST`, `MPL`, `GLS`, `EMPTY`. |
| `vonalkod`, `megjegyzes` | O strings, `barcode`, `comment` | `100–104,135–136`; documented carrier fallback retained. |
| `tof` | O `TransOFlex` | `137–148`; follows generic fields. |
| `tof/azonosito`, `shipmentID` | O strings, `id`, `shipment_id` | `139–140`; TOF identifier retains leading zeros; schema annotation says five digits, not an XSD pattern. |
| `tof/csomagszam` | O `u32`, `parcel_count` | `141–143`; `invoice.rs:711–718` checks ≤`i32::MAX`; negative parcel counts not meaningful supported business values. |
| `tof/countryCode`, `zip`, `service` | O strings | `144–146`; exact case and order. |
| `ppp` | O `PickPackPoint` | `149–154`; separate from TOF. |
| `ppp/vonalkodPrefix`, `vonalkodPostfix` | O strings, `barcode_prefix/suffix` | `151–152`; can represent carrier-agreed 3-character prefix and ≤7-character suffix. No local carrier-format check claimed. |
| `sprinter` | O `Sprinter` | `155–166`. |
| `sprinter/azonosito`, `feladokod`, `iranykod` | O strings, `id`, `sender_code`, `routing_code` | `157–159`; represent annotated 3-character id, 10-digit sender code and routing text without numeric conversion. |
| `sprinter/csomagszam` | O `u32`, `parcel_count` | `160–162`; same signed-XSD upper-bound validation. |
| `sprinter/vonalkodPostfix`, `szallitasiIdo` | O strings, `barcode_suffix`, `delivery_time` | `163–164`; annotated 7–13-character suffix and delivery text representable. |
| `mpl` | O `Mpl` | `167–177`; no `Default` on MPL, mandatory members supplied to constructor. |
| `mpl/vevokod`, `vonalkod`, `tomeg` | Three R strings: `customer_code`, `barcode`, `weight` | `169–171`; weight really is XSD string, not double; fractional textual weight supported. |
| `mpl/kulonszolgaltatasok`, `erteknyilvanitas` | O string and O Decimal: `extra_services`, `declared_value` | `172–175`; correct double type for value and final position. |

`Waybill` allows all four subblocks independently, as the XSD uses a sequence of optional elements, not a choice. Carrier selection/rendering is the vendor's concern. Invoices as well as delivery notes can carry the block, subject to compatible layout; the crate documents this correctly. No carrier field was found dropped or mistyped.

## Supported choices and live observations: not defects

| Area | Current evidence and assessment |
|---|---|
| Omitted optional example placeholders | S3/S4 contradict their own broad “all fields shown … mandatory” sentence by explicitly explaining `minOccurs=0` and declaring optional fields. The serializer follows the formal schema. Empty seller container remains because the container is required. |
| Lowercase `átutalás` | Current example uses `Átutalás`, but the field is string and lowercase is supported by repository test-account invoice observations. No forced title-casing correction warranted. |
| Issue date | `behaviour.md:90–91,198–202`: paper test-account create replaced yesterday with today. `InvoiceHeader::issue_date` documents this as an observation, not a guarantee that server accepts every arbitrary date. |
| E-invoice flag | `behaviour.md:97–98`: `true` queried as appearance 3, false as 1. Request bool is correct; queried appearance is a separate coded value. |
| Proforma consumption | `behaviour.md:104–108`: explicit ordinary invoice reference, implicit prepayment linking, and silently dropped stale/deleted references. The client sends the requested reference; no request-local existence check is possible. |
| Final totals | `behaviour.md:117–119`: shared-order linking and no automatic netting. Caller-supplied negative line is correctly documented and representable. |
| Rounding tolerance | `behaviour.md:159–162`: small net discrepancy accepted; EUR monetary totals independently rounded to two decimals; `27.00`/`27.0` accepted. Crate normalization is hygiene, and explicit rounding prevents the observed inconsistent stored EUR total. Do not extrapolate the EUR observation to every currency or claim HUF fractional behaviour was probed. |
| Negative rows | S12 and `behaviour.md:126`: discount/corrective negatives are representable; no positivity check blocks them. |
| External-id and order semantics | S11 and `behaviour.md:37–69`: account-specific duplicate checking, bounded replay, external ids not unique. The request model does not promise durable idempotency or locally enforce worker-specific order alphabets. |
| Automatic MNB | S3/S4/S5 expressly state MNB bank + absent rate uses the current rate. `ExchangeRate::automatic_mnb()` correctly writes this exception to S9's general rule. Invoice documentation support is direct; receipt acceptance is a separate review concern. |
| XML escaping | Actual quick-xml **0.42.0** `escape.rs:93–110` escapes carriage returns as `&#13;` as well as XML metacharacters. `BytesText::new` uses that function. Therefore an older-version CR-loss allegation would not apply to this tree. `to_wire` also rejects XML 1.0 forbidden characters. |

## Tests and meaningful residual gaps

Executed without the network-enabled client feature or any live/probe target:

```text
cargo test -p szamlazz-agent --offline --lib --test simple_items --test numeric_fidelity --test upstream
```

Results: library **185**, numeric fidelity **6**, simple items **2**, upstream **11**; all passed, none ignored. Some library/upstream tests cover other operations because the targets are shared; this report draws no whole-operation conformance conclusion from those incidental passes.

Existing meaningful protection includes:

- Canonical invoice XML and optional block ordering, all four carrier blocks, delivery-note flag/template.
- Explicit prepayment/final proforma-reference placement, kind serde shape, final reference validation.
- Nonempty line items, erasure limit, both parcel-count upper-bound paths, forbidden XML characters, currency/bank validation and automatic MNB omission.
- Attachment multipart shape and collection bounds; default/false/true simple-items and preview permutations, including full monetary rows and the current schema-conflict fields.
- Exact arithmetic overflow/underflow and precision refusal, fractional percentages in `Other`, signed half-away-from-zero rounding, explicit currency policies.

Residual gaps worth addressing:

1. **I-01 exceptional outbound date years** are not tested. Normal-date goldens cannot detect an overbroad date domain.
2. **No full authoritative-XSD request-validation test.** `upstream.rs:1049–1113` compares examples via a lossy outline; `1225–1234,1287–1295` drops empty elements and trims text. It does not prove required empty containers, complete optional-field ordering or numeric facets. The new source comparison confirmed this matters because the example itself and schemas disagree. A useful offline matrix would validate representative complete documents against separately labeled, unmodified source schemas and explicitly expect/report U-01/U-02, rather than silently patching a synthetic “official” schema until tests pass.
3. **Combined simplified-preview acceptance is unknown**, not fixable with a self-referential golden assertion. Vendor clarification is the next evidence step. No live test is proposed as having run here.

The in-repo request example compared by `upstream.rs:184–258` predates the current example's `simpleItems=false` and even retains an older comment spelling (`Invoce comment`). That explains why the upstream target can pass without matching today's example byte-for-byte. Current `simple_items.rs` independently covers the new field; this is a fixture-freshness limitation, not an unimplemented feature.

## Coordination for whole-crate adjudication

- **Share I-01 with all request reviewers:** common `xml::Element::date` affects other operations. Count this once, with invoice and other affected request sites, rather than as separate per-operation bugs.
- **Preserve U-01/U-02 as source conflicts.** A current download-only validator would incorrectly label actively documented group/count fields unsupported; an inline-only verdict would ignore the direct downloaded/PHP support for the chosen preview order.
- **No shared CR-loss finding:** dependency 0.42 already escapes CR. Review current dependencies rather than earlier reports.
- **Shared arithmetic/type boundaries:** `Decimal` is a deliberate finite representation, and helper calculation does not guarantee vendor storage precision. Receipt gross rules and response numeric parsing should be adjudicated in their own slices. No blanket restriction on invoice amounts should be imported from receipts.
- **Keep identifiers distinct:** buyer `azonosito` is partner association, external id is a nonunique query tag, invoice number identifies a document, and order number has the account-toggle replay semantics. Current request fields preserve those distinctions.

**Bottom line:** the current invoice request surface is field-complete against the freshly retrieved inline specification, with a small shared date-domain hole, an optional gross-first ergonomics enhancement, and explicit source conflicts that prevent an unqualified “valid against the current official XSD” claim. No ordinary invoice-kind, buyer/seller, line-item or waybill mapping defect was established.
