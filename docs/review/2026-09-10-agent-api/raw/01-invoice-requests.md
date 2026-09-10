# Invoice CREATE requests: current-source conformance review

**Reviewed:** 2026-09-10. **Code:** `f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3` and the working-tree files read during this review. No tracked production modifications were present in the initial status. Other review/research files were already untracked.

**Scope:** `crates/szamlazz-agent/src/ops/invoice.rs`, `ops/waybill.rs`, `item.rs`, and request-related `types.rs`. Shared XML writing and multipart construction were followed only as needed to verify these requests. Response envelopes, storno, queries and receipt operations have separate owners. All abbreviated `src/...` references below are under `crates/szamlazz-agent/`.

## Executive conclusion

The current invoice request model exposes **every element in the current EN/HU invoice-create inline XSD**, including `simpleItems`, buyer group identifiers, erasure-code counts, both ledger blocks and all four carrier sub-blocks. The ordinary writer sequences, required containers, boolean emission and token spelling match. No missing invoice kind, language, currency or VAT wire token was established.

The review retains these narrowly scoped actionable findings:

| ID | Priority | Finding | Confidence / impact limit |
|---|---|---|---|
| IR-01 | P3 | `try_calculated` silently calculates zero VAT for a numeric token carried by `VatRate::Other`, although that token is sent as a taxable percentage. | High, reproduced offline. Narrow Rust-construction path; ordinary `VatRate::from`/serde paths classify numeric tokens correctly. No observed vendor incident. |
| IR-02 | P3 | Invoice text containing an XML 1.0-forbidden control character passes validation and is emitted as malformed XML. | High, reproduced through the real writer and rejected by an independent XML parser. Caller-data edge case, not a new vendor field omission. |
| IR-03 | P3 | Several VAT-code descriptions are broader than the current official distinctions: EUT/EUKT omit goods, HO omits third-country scope, EUE omits “not reverse charged.” | High for wording mismatch; no observed tax-reporting incident. Wire tokens are correct. |
| IR-04 | P3 | `K.AFA` documentation omits the vendor's exact-text-dependent NAV subtype selection and its used-goods default. | High for the documented processing rule and missing rustdoc guidance; no local execution of that rule. Existing string fields can express it. |

Two important **unresolved source conflicts**, not proven runtime defects or live-backed exceptions:

1. **SC-01:** the combined preview/`simpleItems` writer violates the current EN/HU inline XSD order but agrees with the downloadable XSD and freshly fetched first-party PHP 2.12.4. Its deployed acceptance and non-issuing preview behavior are unverified.
2. **SC-02:** current API docs/PHP and the linked knowledge-base article disagree about which of `SzlaAlap`/`SzlaNoEnv` is traditional versus envelope-friendly. Current code follows API docs/PHP. Rendering is unverified.

No P0/P1 defect was found in this ownership slice. “All schema elements exposed” does **not** mean all possible combinations are represented or all server content rules are locally validated. The capability and coverage sections spell out that boundary.

## Source register and acquisition

These were unauthenticated documentation GETs made during this review, not Számla Agent account calls. The main site pages report **`v202608271632`**; this is a site build identifier, not proof of when each rule changed. The separately served old `/xsd` route reports **`v202606031507`**.

| Ref | Exact official URL | Relevant source text / purpose |
|---|---|---|
| S1 | https://docs.szamlazz.hu/agent/generating_invoice/request | POST, `multipart/form-data`, main field `action-xmlagentxmlfile`, attachments `attachfile1`…`attachfile5`. |
| S2 | https://docs.szamlazz.hu/agent/generating_invoice/xml | Current example **and** inline XSD. “The order of the fields is fixed”; `minOccurs="0"` elements may be omitted. All complex types inspected. |
| S3 | https://docs.szamlazz.hu/hu/agent/generating_invoice/xml | Current Hungarian example/XSD, buyer partner identity and waybill annotations. “a felismert azonosító alapján … automatikusan frissítjük a partner adatait.” |
| S4 | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd | Download URL in the example's `schemaLocation` and linked by sending-requests. Independent schema; not equivalent to S2/S3. |
| S5 | https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules | Current navigation index; followed all ten invoice settings/rules pages. |
| S6 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types | Six create kinds, paper/electronic flag, proforma reference and “one prepayment invoice can have exactly one final invoice.” |
| S7 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency | `simpleItems` spelling, defaults, per-document control, eligibility, full NAV data, kinds/inheritance and 551–556 rules. |
| S8 | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates | Current numeric and special VAT tokens; `eusAfa` suppresses NAV submission when accepted. |
| S9 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding | Net-first and gross-first examples, HUF integer amount rules and eight server rounding cases. |
| S10 | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding | Hungarian counterpart qualifies B2B/B2C rounding advice with “valószínűleg” (probably); EN says “have to.” |
| S11 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies | Complete published code list including `HUF`/`Ft`, `KWD`, `KSH`; foreign currency requires bank/rate. |
| S12 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template | Six layout tokens, omission default, fifteen languages; language also controls notification/portal. |
| S13 | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template | Same token/name mapping as S12. |
| S14 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number | Per-kind duplicate toggle, storno/corrective exemption, replay comparison and two-day window. |
| S15 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount | “There is no dedicated discount field”; negative unit price, positive quantity, same VAT, preserve adjacency. Includes XML example. |
| S16 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification | Email present plus true/omitted send flag sends; comma recipients; BBCode/newlines; five files, 2 MB each; valid-attachment partial delivery; test-account redirection. |
| S17 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code | Optional nonnegative `int`; maximum 400 per item; account enablement; sample XML. |
| S18 | https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor | Linked from S17. “igényelt kódok darabszámát”; `SzlaMost` prerequisite; uploaded stock or vendor-supplied codes. |
| S19 | https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere | Linked from S8. VAT guidance and link to detailed PDF below. |
| S20 | https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf | Linked from S19; downloaded and read as a one-page PDF. Detailed EUT/EUKT/HO/EUE/KBAUK meanings, NAV mappings, `K.AFA` subtype text matching. |
| S21 | https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok | Linked from S12/S13; conflicting layout-name/token mapping and `SzlaMost` default. |
| S22 | https://docs.szamlazz.hu/php/ | Current first-party PHP package link, version 2.12.4. |
| S23 | https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip | Downloaded independently; inspected `szamlaagent/src/szamlaagent/Header/InvoiceHeader.php`, especially `buildXmlData`. Not executed. |
| S24 | https://docs.szamlazz.hu/agent/generating_invoice/xsd | Still-served older separate schema route, with group/erasure elements but **without `simpleItems`**. Current navigation uses S2 instead. |
| S25 | https://docs.szamlazz.hu/agent/basics/sending-requests | One XML per document, case-sensitive tags, possible 57 or ignored mistyped setting; links S4. |

An initial guess at `https://docs.szamlazz.hu/agent/generating_invoice/` returned 403. All operative current pages were then obtained through the actual navigation links. Binary webfetch output for S20/S23 was not used as readable evidence: the original files were separately obtained with `curl` under `/tmp/opencode`, then read as PDF / ZIP member text.

### Independently reproduced schema drift

SHA-256 below identifies the fetched download bytes, or HTML-decoded inline `<pre>` text **without an added newline**. The comparison script parsed each source separately, without constructing a merged XSD.

| Source | SHA-256 | Header tail | Other difference from current EN |
|---|---|---|---|
| S2 EN | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` | template → simple items → preview | Baseline |
| S3 HU | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` | template → simple items → preview | Same element membership |
| S4 download | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` | template → preview → simple items | No buyer `csoportazonosito`, no item `torloKod` |
| S24 older route | `508162a8a38ec80648db3d013b7ab1b258532cae914997887192a17dcbada801` | EU VAT → template → preview | No `simpleItems` |

This independently corroborates the first three observations in `fixtures/SOURCES.md:193–228`, and adds the still-served older-route distinction. The repository's `fixtures/upstream/agent/xsd/xmlszamla.xsd` is explicitly a **project-modified** download (`fixtures/SOURCES.md:126–140`); passing it would not prove conformance to an unmodified current official source.

## Actionable findings

### IR-01 — Numeric `Other` VAT tokens produce a contradictory calculated item

**P3; implementation edge case; high confidence.**

**Code:** `src/item.rs:163–171,179–212` and `src/types.rs:239–240,258–282,286–315`. The calculator tests the Rust variant, not the token it will send: only `VatRate::Percent` computes VAT; every other variant yields `Decimal::ZERO` (`item.rs:191–198`). The invoice writer then emits the original token next to those amounts (`src/ops/invoice.rs:889–896`).

**Official contract:** S8 explicitly allows numeric percentages such as `27`, `5.5`, `25.5`; S15 explicitly gives `afaErtek = nettoErtek × afakulcs / 100`. S2/S9 require caller-supplied amounts. Recorded live evidence in `docs/szamlazz-hu-behaviour.md:162` additionally establishes acceptance of `27.00` and `27.0` as percentage tokens.

**Reproduction through current code:**

```rust
let item = LineItem::try_calculated(
    "Item", Decimal::ONE, "db", Decimal::from(100),
    VatRate::Other("27.00".into()), Rounding::Scale(0),
).unwrap();
// item.vat_rate.as_wire() == "27.00"
// item.net_value == 100; item.vat_value == 0; item.gross_value == 100
// VatRate::percent(27), otherwise identical, produces 100 / 27 / 127.
```

This is not a hypothetical unknown future VAT rule: it is a numeric percentage already accepted by the vendor. The raw-token path is expressly contemplated by `src/types.rs:1140–1142`, whose comment calls `Other` the way to send literal `27.00`. The derived item is internally consistent as `net + VAT = gross`, but wrong for its emitted percentage. Likely user consequence is a vendor arithmetic rejection; server acceptance or correction of this exact compound request was **not** tested.

**Boundary/remedy direction:** make the calculator interpret supported numeric raw tokens consistently or return a typed “cannot derive” error when its representation cannot establish the arithmetic. Keep explicit `LineItem::new` for caller-computed special cases and keep open wire tokens. `VatRate::from("27.00")` and normal string serde already select `Percent` and are unaffected; do not describe this as a common JSON decoding failure. No runtime change was made.

### IR-02 — Forbidden XML characters escape the request boundary

**P3; request-shape defect; high confidence.**

**Code:** `src/ops/invoice.rs:682–732` validates items, final references, erasure counts, parcel counts and exchange-rate shape, but no XML character legality. Buyer strings reach `v.text` at `invoice.rs:840–845`; the shared helper uses `BytesText::new` without XML-character validation at `src/xml.rs:306–316`. Other free text, including seller/email/item strings, takes the same path.

**Official contract:** S1 requires an XML file, S2 requires the supplied XSD, and both example/schema declare XML 1.0. S25 explains request XML validation and code 57. An XML 1.0 document cannot contain raw U+000B, even within a string-typed element. This is separate from whether the vendor permits an empty buyer name or an unknown business token.

**Reproduction:** start with an otherwise ordinary HUF invoice; assign `buyer.name = "Buyer\u{000b}Co"`. The real `request.validate()` returns `Ok(())`, and the writer emits the raw character in `<nev>`. Python's independent `xml.etree.ElementTree.fromstring` rejects the produced bytes:

```text
control XML rejected not well-formed (invalid token): line 1, column 481 request.validate True
```

Ordinary markup characters work: the same probe with `Buyer & Co` emits `Buyer &amp; Co`. The finding is **not** missing ampersand escaping, an XML injection exploit, or evidence of vendor response behavior. A customer name/comment imported from another system with a vertical tab is enough to make a crate-accepted request unusable.

**Remedy direction:** reject prohibited XML 1.0 characters at the request boundary with a field-attributed error. Do not silently delete customer data or encode illegal characters as numeric references (those are illegal too). This can be much smaller than full XSD validation. No account call was made, and no particular vendor error code was observed for the generated request.

### IR-03 — VAT vocabulary loses materially important distinctions

**P3; documentation defect; high confidence.**

**Code:** `src/types.rs:197–210`:

| Variant | Current description | Current official meaning | Why the difference matters |
|---|---|---|---|
| `Eut` | “EU-n belüli ügylet (intra-EU transaction)” | S8: “EU-n belüli **termék értékesítés**”; S20: intra-EU exempt goods supply, mapped to `KBAET`. | The generic transaction wording can be read to cover a service, for which the PDF explicitly directs the user to `EUFAD37`, `EUFADE` or `EUE`. |
| `Eukt` | “EU-n kívüli ügylet (transaction outside the EU)” | S8: “EU-n kívüli **termék értékesítés**”; S20 maps it to exempt goods export `EAM`. | Does not distinguish exported goods from third-country services (`HO`). |
| `Ho` | “területi hatályon kívül (outside the territorial scope of the Hungarian VAT act)” | S8/S20: “**Harmadik országban teljesített ügylet**”; PDF destination: “Nem EU és nem Magyarország.” | “Outside Hungary” is broader than “third country” and includes EU cases with separate codes. |
| `Eue` | “EU-n belüli, másik tagállamban teljesített ügylet” | S8/S20: “Másik tagállamban teljesített, **nem fordítottan adózó** ügylet.” | Omits precisely the distinction from adjacent reverse-charge variants. |

**User impact:** a Rust caller selecting a VAT variant through its documentation can select the wrong tax treatment even though serialization is technically correct. `VatRate`'s top-level advice to consult a tax adviser (`types.rs:178–182`) does not repair a misleading definition. Update the short definitions and point to S8/S19/S20; keep all wire tokens and the open enum.

**Dismissed nearby concern:** `Kbauk` means new means of transport, **not the United Kingdom**. S8's terse “UK” is expanded unambiguously by S20. Current `types.rs:224–229` correctly distinguishes `KBAUK` from ordinary `KBAET`; it should not be “fixed” from the abbreviation alone. TAHK's earlier error is also fixed (`types.rs:194–196`).

### IR-04 — `K.AFA` has a documented text-driven NAV subtype that callers are not told about

**P3; documentation/semantic coverage gap; high confidence.**

**Code:** `VatRate::KAfa` is documented only as margin scheme (`src/types.rs:203–204`); `LineItem::comment` is generic (`src/item.rs:111–112`); the invoice comment is generic (`src/ops/invoice.rs:163–164`). The `simple_items` rustdoc says to put margin-scheme information in the comment for `K.AFA` (`invoice.rs:216–217`), but does not give the processing consequence of the exact text.

**Source:** the linked official PDF S20, `K.AFA` row, says it sends a **type rather than a code** to NAV. It searches invoice comment, item name and item comment for exact text among:

```text
utazási irodák
használt cikkek
műalkotások
gyűjtemény darabok és régiségek
```

Brief source excerpt: “**Kizárólag pontos egyezés esetén** … Ha nem írnak semmit, vagy nincs egyezés, akkor **alapértelmezetten használt cikk típussal** küldjük be a K. ÁFA típusát.” In English: exact matching determines the subtype; absent/unmatched wording defaults to used goods. S7 independently requires manually entered margin-scheme information for `K.AFA` in simplified tour-operator invoices.

**Impact:** callers can send a schema-valid `K.AFA` invoice, with a plausible English or paraphrased comment, while the vendor selects its used-goods subtype. This is a current official processing rule, not an inferred tax rule. It deserves a concise source-linked note on `KAfa`, with the exact source wording retained. The PDF's matching algorithm beyond that wording is unspecified: do not invent substring/case/normalization guarantees or a new XML subtype field. All three text locations are already serializable, so this is **not** an unsupported wire capability. No live subtype observation was made.

## Conflicts and unknowns: explicit conformance qualifications

### SC-01 — Preview + simplified image cannot be claimed conformant to all current schemas

**Code:** `src/ops/invoice.rs:193–225,811–826`; current writer emits `szamlaSablon`, `elonezetpdf`, `simpleItems`. It transparently documents this choice. `tests/simple_items.rs:20–90` asserts that chosen order, including false/true combinations, and preserves full amounts/group/erasure data.

S2/S3 explicitly require fixed order and place `simpleItems` **before** `elonezetpdf`. S4 places it **after**. Fresh S23 PHP `Header/InvoiceHeader.php:398,400,404` writes template, preview, then simple items, agreeing with the crate. S24 has no simple-items element at all. The actual generated header from the scratch probe fails the S2/S3 sequence comparison and passes S4. This was a structural member/order check, **not** an XSD engine claiming full validity.

If both options are `Some`, even `Some(false)`, the writer differs from S2/S3. With either absent, their relative order cannot conflict. PHP corroborates the true/true emission order, but PHP itself omits false values; it is not evidence of every explicit-false combination.

**Disposition:** a real current-document conformance exception, already recorded as a policy, with unresolved deployment impact. It is **not live-backed**, so cannot be placed in the user's allowed “recorded live-test deviation” bucket. Nor is choosing the inline order an established repair: that would disagree with S4/PHP. Obtain vendor confirmation or an authorized, separately recorded combined-preview probe before asserting which order the running service accepts and whether it preserves preview. Do not retry automatically with the opposite order or drop preview. Retain group/erasure support rather than choosing the stale download wholesale.

### SC-02 — Layout vocabulary conflicts inside the official source set

`src/types.rs:983–987,1003–1008` maps `Default` to traditional `SzlaAlap` and `NoEnvelope` to `SzlaNoEnv`. The former comment carefully distinguishes a named template from omission. S12/S13 and PHP S23's invoice-template comments agree with that traditional/envelope-friendly token assignment.

But the knowledge base **linked from those very pages**, S21, says:

- “Tradicionális: … `SzlaNoEnv`”
- “Borítékbarát: … `SzlaAlap`”

The page's linked preview filenames follow its assignment, and it also describes omission as `SzlaMost`. It contains other spelling inconsistencies (`szamlasablon`, lowercase delivery-note token), so it should not silently displace the XSD/API writer vocabulary.

**Disposition:** record the conflicting UI meanings. The code's exact tokens conform to S2/S4, and its descriptions follow S12/PHP; actual rendered layouts were not verified. Do not invert tokens or declare the crate's layout wrong solely from S21. `NoEnvelope` and `Continuous` are also less direct names than the current UI labels “envelope-friendly” and “retro”; that is naming debt, not evidence of a serialization defect.

### Other bounded unknowns

- **`fizetve=false` versus omission:** `paid: false` omits the element (`invoice.rs:178–180,802–804`). There is no way to explicitly send false through `CreateInvoice`. S2 permits an optional boolean but specifies no default or distinction for cash/card account behavior. PHP also emits it only for true. This is a representational limitation, **not proof** of wrongly marked paid invoices. No recorded live omitted/false comparison resolves it.
- **Foreign-currency proforma/delivery note without a rate:** the XSD permits omission; S11 says foreign-currency documents require bank/rate, and S2 explicitly permits automatic MNB. The current gate at `invoice.rs:719–729` follows that reading, with `automatic_mnb()` available. `docs/szamlazz-hu-behaviour.md:216–223` records the rate-free special-kind case as unverified. Do not label the stricter gate a rejected documented capability on present evidence.
- **Contract-only fields:** `aggregator`, `guardian`, `cikkazoninvoice`, `logoExtra`, `arresAfa`, `arresAfaAlap` have schema surfaces but incomplete public processing descriptions. The writer preserves them; no current account-specific rendering or taxation behavior was established here. PHP describes `fizetendoKorrekcio` as affecting payable amount rather than gross; the crate's “payable adjustment” is consistent, and item totals are not recomputed from it.
- **Carrier details:** schema comments give TOF's five-digit id, PPP three-character prefix/max-seven suffix, Sprinter three-character id/ten-digit sender/seven-to-thirteen suffix and routing code. Current string types permit them but do not validate those lengths. This is server-owned content validation, not lost capability. No barcode/rendering observation was performed.

## Operation and field coverage

**Legend:** R = XSD-required element; O = optional element. Unless stated otherwise, O fields are `Option`, `None` is omitted, `Some("")` emits a present empty string, and explicit optional booleans emit true/false. Schema order is reflected by each listed sequence; numbers use exact decimal text, not binary-float formatting. Code citations identify the actual writer, not just model declarations.

### Operations / document selections

| Operation/capability | Current representation / actual emission | Assessment and evidence |
|---|---|---|
| Invoice CREATE | `CreateInvoice`, `ACTION=action-xmlagentxmlfile`; root `xmlszamla`, namespace `http://www.szamlazz.hu/xmlszamla` (`invoice.rs:678–680,737–739`) | Matches S1/S2/S25. One request contains one document. |
| Ordinary invoice | `InvoiceKind::Invoice`, no special kind flag; optional proforma number | Matches default in S6; `invoice.rs:774–780`. |
| Proforma | `Proforma` → `dijbekero=true` | Matches S6; `invoice.rs:794`. |
| Delivery note | `DeliveryNote` → `szallitolevel=true` and forced `SzlaFuvarlevelesAlap` | S6 calls for layout; S2 supplies flag. Both are sent (`invoice.rs:795,811–818`). Header docs disclose override; invoice-with-waybill remains separately expressible. |
| Prepayment | `Prepayment { proforma_number }` → reference before `elolegszamla=true` | Matches independent fields/order in S2; explicit proforma-link execution remains unprobed (`invoice.rs:774–780`). |
| Final | `Final { prepayment_number, proforma_number }` → proforma reference, `vegszamla=true`, prepayment reference | `invoice.rs:781–789`; either nonblank prepayment number or order required (`686–701`), per S2 identification annotation. One-prepayment/one-final rule is server-owned. |
| Corrective | `Corrective { corrected_number }` → `helyesbitoszamla=true`, `helyesbitettSzamlaszam` | Matches S6/S2 (`invoice.rs:790–793`). String is not locally nonblank-validated; schema has no nonempty restriction. |
| Preview | `preview_pdf: Option<bool>` → `elonezetpdf` | Field covered (`invoice.rs:819–821`). No-issue semantics documented in S2; combined simple-items ordering SC-01. Response handling not audited here. |
| Simplified tour-operator image | `simple_items: Option<bool>` → exact `simpleItems` | Present and documented (`invoice.rs:199–225,824–826`); full rows retained. S7 eligibility, item limits, inheritance, forbidden kinds and VAT list accurately described. SC-01 remains. |
| Email attachments | Bounded `InvoiceAttachments`, fields `attachfile1`…`attachfile5` (`invoice.rs:405–501,938–948`) | All five supported; sixth and >2,000,000-byte file refused; serde cannot bypass bounds. S16 gives “2 MB” without exact bytes; decimal choice is disclosed. |

### Settings / root structure

| Wire fields, in order | Presence/default/type and code | Assessment |
|---|---|---|
| `beallitasok`, `fejlec`, `elado`, `vevo`, `fuvarlevel`, `tetelek` | All R except O waybill; seller container emitted even when empty (`invoice.rs:739,756,828,838,879–882`) | Matches S2/S4; zero items rejected (`682–685`); one or more `tetel` in caller order. |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | Credentials helper selects login pair or agent key, at start of settings (`invoice.rs:740`; `xml.rs:348–357`) | XSD has all three O, authentication chooses meaningful alternatives. Full authentication behavior belongs to transport owner. |
| `eszamla`, `szamlaLetoltes` | R booleans, both explicitly false by `CreateInvoice::new` (`invoice.rs:603–605,741–742`) | Correct mandatory-presence defaults; S6 true electronic/false paper. |
| `szamlaLetoltesPld`, `valaszVerzio` | O `u8` copies; version always `RESPONSE_VERSION` (2) (`743–746`) | Version 2 explicitly allowed by S2. Copies deprecated/ignored, disclosed (`543–547`); narrower integer range has no established lost useful capability. |
| `aggregator`, `guardian`, `cikkazoninvoice`, `szamlaKulsoAzon` | O string/bool/bool/string (`747–754`) | All covered in S2 order. No invented external-id uniqueness or account constraint. |

### Header

The XML declaration and default namespace are emitted by `src/xml.rs:19–40`. The sample's `xmlns:xsi` and `xsi:schemaLocation` attributes are omitted: these are schema-location hints, not required invoice business elements; the root and all children remain in the correct namespace. Dates are serialized as civil `YYYY-MM-DD` without optional XSD timezone suffixes; that is a valid ordinary-date representation, not a missing timezone requirement.

| Wire fields, in order | Presence/default/type and code | Assessment |
|---|---|---|
| `keltDatum` | O civil date, default None (`invoice.rs:141–150,758`) | XSD optional; today substitution is explicitly documented as bounded live behavior. |
| `teljesitesDatum`, `fizetesiHataridoDatum` | R civil dates (`759–760`) | Fulfillment/due vocabulary correct; EN sample's “payment date” for fulfillment is a translation error, HU says “teljesítés dátuma.” |
| `fizmod`, `penznem`, `szamlaNyelve` | R payment token/currency/language (`761–763`) | All published tokens expressible. Closed language set exactly matches 15 XSD enumerations, including non-ISO `cz` and `si`. |
| `megjegyzes` | O string (`764`) | Correct free text; IR-02 XML characters; IR-04 semantic documentation. |
| `arfolyamBank`, `arfolyam` | `ExchangeRate { bank: String, rate: Option<Decimal> }` (`765–770`; `types.rs:935–973`) | Explicit or automatic MNB supported. S2's bank=MNB/no rate exception is more specific than S11's general requirement. HUF can omit whole block. |
| `rendelesSzam` | O string preserved as supplied (`771`) | Matches S14 optionality; does not impose worker-specific alphabet or claim guaranteed dedupe. |
| `dijbekeroSzamlaszam` | O reference on invoice/prepayment/final (`774–777`) | Correct position before flags; source does not encode a restriction to those three variants. Model acknowledges that (`23–30`). |
| `elolegszamla`, `vegszamla`, `elolegSzamlaszam`, `helyesbitoszamla`, `helyesbitettSzamlaszam`, `dijbekero`, `szallitolevel` | O, selected by `InvoiceKind` (`778–796`) | Emitted subsequence is ordered correctly for every kind. Unused kind flags omitted rather than false, permitted by XSD. |
| `logoExtra`, `szamlaszamElotag` | O strings (`797–798`) | Covered; prefix registration responsibility documented; S25 recommends per-webshop prefix. |
| `fizetendoKorrekcio`, `fizetve`, `arresAfa`, `eusAfa` | O Decimal; true-only paid; O bool; O bool (`799–810`) | Order correct. Paid false expressivity unknown noted above. `eusAfa` docs accurately state suppression/prerequisites/line-rate obligation/no retroactive submission (S8). |
| `szamlaSablon`, `elonezetpdf`, `simpleItems` | O template/bool/bool; delivery-note template forced (`811–826`) | All fields exposed. SC-01 order; SC-02 layout labels. S7 says simplified image overrides supplied layout server-side. |

### Seller, buyer and buyer ledger

| Wire fields, in order | Presence/type and code | Assessment |
|---|---|---|
| Seller `bank`, `bankszamlaszam` | O strings (`invoice.rs:829–830`) | Matches S2/S4; no seller-name/tax-number CREATE element is documented here, so their absence is not a missing field. |
| Seller `emailReplyto`, `emailTargy`, `emailSzoveg`, `alairoNeve` | O strings; email grouped in `SellerEmail` (`831–836`; `types.rs:1014–1024`) | Matches; BBCode strings and literal newlines pass through escaped text. Empty email struct emits no children. |
| Buyer `nev`, `orszag`, `irsz`, `telepules`, `cim` | R/O/R/R/R strings (`840–844`) | Exactly XSD presence/order. Empty required string still present; no schema minLength asserted. IR-02. |
| Buyer `email`, `sendEmail` | O string/O bool (`845–848`) | S16 confirms None flag plus present email sends, explicit false suppresses, comma recipients allowed. Model documents all three states. |
| Buyer `adoalany`, `adoszam`, `csoportazonosito`, `adoszamEU` | O enum/string/string/string (`849–854`) | Status tokens `7,6,1,0,-1` match S2 annotations (`types.rs:682–747`); group element retained despite S4 omission. No inferred tax-number validation. |
| `postazasiNev`, `postazasiOrszag`, `postazasiIrsz`, `postazasiTelepules`, `postazasiCim` | Five O strings, grouped only in Rust (`855–861`) | Flat XML fields in exact order; a present empty `PostalAddress` produces no postal fields. No fictitious postal wrapper. |
| `vevoFokonyv` | O container (`862–873`) | All child fields O. A present default ledger emits empty-present container, distinct from None. |
| Ledger `konyvelesDatum`, `vevoAzonosito`, `vevoFokonyviSzam`, `folyamatosTelj`, `elszDatumTol`, `elszDatumIg` | O date/string/string/bool/date/date (`864–871`) | Complete and in S2 order; explicit false preserved. |
| Buyer `azonosito`, `alairoNeve`, `telefonszam`, `megjegyzes` | O strings (`874–877`) | Complete, ordered. Partner identity/update/customer-account-link consequences now accurately documented (`329–339`, S2/S3). |

### Items and item ledger

| Wire fields, in order | Presence/type and code | Assessment |
|---|---|---|
| `megnevezes`, `azonosito` | R/O strings (`invoice.rs:885–886`) | Correct; item id independent of `cikkazoninvoice` display flag. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs` | R Decimal/string/Decimal/VAT token (`887–890`) | Numeric text valid finite `xs:double` subset; signed quantity/price preserved. S15 discounts expressible. IR-01 raw numeric calculator case; IR-03 vocabulary. |
| `arresAfaAlap` | O Decimal (`891–893`) | Correct location before net/VAT/gross; no documented computation inferred from presence. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | R Decimal (`894–896`) | All sent explicitly, as required by S2; derived and caller-computed constructors both available. Simplified image does not drop money. |
| `megjegyzes`, `tetelFokonyv` | O string/container (`897–913`) | Correct; all nested ledger children O. IR-04 margin-scheme comment semantics. |
| Ledger `gazdasagiEsem`, `gazdasagiEsemAfa`, `arbevetelFokonyviSzam`, `afaFokonyviSzam`, `elszDatumTol`, `elszDatumIg` | O strings ×4, O dates ×2 (`900–911`) | All six fields present in model/writer and in S2 order. |
| `torloKod` | O `u32`, locally limited to 400 (`914–916`, `703–710`; `item.rs:115–131`) | Count, not arbitrary erasure identifier; S17/S18 support nonnegative/max-400 semantics. At row tail, retained despite S4 omission. |
| Derived arithmetic | Explicit `Rounding`, checked decimal multiplication/division/addition (`item.rs:179–212`) | S9 net-first method implemented; gross-first values can be supplied explicitly. Exact is optional, not universal default; HUF minor-unit policy is whole-forint. |

### Waybill/carrier blocks

| Wire fields, in order | Presence/type and code in `src/ops/waybill.rs` | Assessment |
|---|---|---|
| `uticel`, `futarSzolgalat`, `vonalkod`, `megjegyzes` | Four O strings (`95–104,133–136`) | S2 unused-destination note and barcode fallback accurately documented. Carrier string can express `TOF, PPP, SPRINTER, FOXPOST, MPL, GLS, EMPTY`; no missing FOXPOST/GLS sub-block is documented. |
| `tof` → `azonosito`, `shipmentID`, `csomagszam`, `countryCode`, `zip`, `service` | All O; parcel count `u32`, remainder strings (`11–24,137–148`) | Complete and ordered. Negative parcel counts cannot be built; >i32::MAX rejected by CREATE validation to fit XSD `int`. |
| `ppp` → `vonalkodPrefix`, `vonalkodPostfix` | Both O strings (`28–33,149–154`) | Complete and ordered. Length/content rules remain server-owned. |
| `sprinter` → `azonosito`, `feladokod`, `iranykod`, `csomagszam`, `vonalkodPostfix`, `szallitasiIdo` | All O; parcel count `u32`, remainder strings (`37–50,155–166`) | Complete and ordered; same int-range validation. |
| `mpl` → `vevokod`, `vonalkod`, `tomeg`, `kulonszolgaltatasok`, `erteknyilvanitas` | R/R/R strings, O string, O Decimal (`57–68,167–176`) | Required MPL fields enforced by shape/no Default; weight correctly a string in XSD, not mistakenly narrowed to integer. |
| Multiple carrier sub-blocks | `tof` → `ppp` → `sprinter` → `mpl`, independently optional | XSD sequence, not choice; model correctly permits multiple blocks. `fuvarlevel` also works on an invoice with compatible layout, not only a delivery note (S2/S3). |

## Unsupported capabilities versus defects

1. **Gross-price-based derived constructor:** S9 documents gross-first B2C calculations. The only built-in calculator starts from net unit price (`item.rs:179–186`). This is a missing convenience, not missing wire support: S9's `3 × 500 gross` example can be sent with `LineItem::new("Könyv", 3, "db", 393.66, 27%, 1181, 319, 1500)`. No need for an invented gross-unit XML element. HU S10 says the B2B/B2C preference is “probably,” rather than EN's mandatory wording; no buyer-status-based algorithm switch is justified solely from EN.
2. **Independent kind flag/reference combinations:** S2's independent optionals are narrowed by `InvoiceKind`. One cannot send a proforma reference on a corrective or arbitrary combinations of true flags. Current model rustdoc discloses the distinction (`invoice.rs:21–30`). S6 does not demonstrate a useful additional combination, so this is **schema expressivity**, not an established omitted business capability. Prepayment/final proforma references are already exposed.
3. **Explicit `fizetve=false`:** unavailable, while omission and true are available. No documented behavioral distinction was recovered; keep as an open question rather than a wrong-default defect.
4. **Version-1 reply selection:** CREATE pins version 2; callers cannot request text/raw-PDF version 1 through this operation. Both are documented, and version 2 covers PDF download. This is a deliberate response-mode restriction, not request nonconformance or dropped document functionality.
5. **Full XSD/business-rule preflight:** the crate does not gate every valid combination, OSS eligibility, tax-number meaning, carrier content or arithmetic of explicit items. The schemas themselves do not encode most such rules. Invalid business inputs being sent for a vendor refusal are not automatically serializer defects. IR-02 is distinguished because the output ceases to be XML at all.

The following are **supported**, and should not be reported as omissions: negative discount lines (preserved Vec order); all 15 languages; every listed numeric/special VAT code; currencies via open `Currency` including vendor-specific `KSH`; all six invoice templates and account-specific tokens via `Other`; custom payment names including first-party PHP's `OTP Simple` via `PaymentMethod::Other`; notification formatting/dynamic text via strings; preview; simplified image; bank overrides; all documented carrier tokens; and both ledger metadata blocks.

No public source in this scope specifies a CREATE XML seller-name/tax-number override, separate discount-percentage field, bulk multi-invoice request, or arbitrary many-prepayment settlement. Their absence is not a missing documented CREATE feature. S6 expressly rules out multiple prepayments into one final.

## Live-backed behavior retained, with limits

Read `CONTEXT.md` as supplied and `docs/szamlazz-hu-behaviour.md` before judging deviations. The latter describes one TEST account on 2026-09-03/06/07, with the duplicate toggle on; original account-exchange logs are not in the repository (`behaviour.md:3–28`). This review did not independently repeat those calls.

| Behavior / code | Recorded evidence | Review disposition |
|---|---|---|
| Non-today CREATE issue date can be silently replaced by today (`invoice.rs:141–148`) | `behaviour.md:91`, P48-P5 | Keep the qualified rustdoc; do not introduce a universal “server rejects non-today” rule from storno. Paper/test-account observation only. |
| `e_invoice=false` → queried code 1; true → 3 (`invoice.rs:532–536`) | `behaviour.md:97`, P73 | Correct appearance terminology; do not confuse request boolean with queried code. |
| Missing/consumed/deleted proforma reference can be silently ignored | `behaviour.md:104–108`, C2/D4/D5 | Low-level request sends caller reference; no claim that send guarantees linkage. Explicit prepayment/final references remain unprobed (`234–240`), distinguished in kind rustdoc. |
| Final does not automatically deduct prepayment; caller needs negative settlement row (`invoice.rs:60–70`) | `behaviour.md:117–120`, C6 | Keep guidance; source's “settle remaining amount” is not evidence of automatic arithmetic. |
| External id not unique; newest holder; attaches only on actual creation | `behaviour.md:63–71`, A3/XPRB | Current field simply sends the token and promises later querying; no uniqueness constraint should be added merely from the word identifier. |
| Net arithmetic tolerates some HUF differences; EUR independently rounded to two decimals | `behaviour.md:159–161`, P60 | Keep explicit `Rounding` and precise EUR observation. Does not establish HUF fractional-storage behavior or KWD precision. S9's HUF rounding table is not contradicted by the EUR probe. |
| `27.00`/`27.0` accepted | `behaviour.md:162`, P60-V1/V2 | Normalization is hygiene, not vendor requirement. This supports IR-01's token semantics, not acceptance of its zero-VAT amounts. |
| Queried buyer data may change after later creation | `behaviour.md:111`; bounded qualification at `fixtures/SOURCES.md:262–265` | Consistent with S2/S3's partner-update warning; do not infer matching algorithm or universal immutable/mutable behavior. |

**Not live-backed:** combined simpleItems/preview ordering; carrier rendering; template identity conflict; erasure-code assignment; TAHK/OSS or `K.AFA` NAV effects; omitted/false paid flags; arbitrary combinations; zero-rate automatic MNB behavior. PHP and docs are legitimate primary evidence, but they are not account executions.

## Historical findings independently dismissed or narrowed

The old `docs/review/2026-09-09-agent-api/FINAL.md` was used only as a checklist. Its baseline and “unimplemented” statement do not describe today's files.

| Historical item | Current verification / disposition |
|---|---|
| C1 — missing simpleItems | Fixed surface: `invoice.rs:199–225,257,824–826`. Tests exist in `tests/simple_items.rs`. SC-01 remains a source conflict; not a still-missing field. |
| D1 — TAHK conflated with TAM | Fixed: `types.rs:190–196`; current S8/S20 agree. IR-03 covers other independently verified definitions, not a repeated TAHK finding. |
| D2 — eusAfa suppression omitted | Fixed: `invoice.rs:183–192` accurately covers processing consequences and prerequisites from S8. |
| D4 — overgeneralized rounding | Largely fixed: `item.rs:9–18,24–29,163–173` and `types.rs:404–412` distinguish policy from server evidence. IR-01 is a different raw-token calculation case. |
| D7 — buyer id/update/portal consequences | Fixed: `invoice.rs:329–339`, consistent with current S2/S3. |
| D9 — erasure count and prerequisites | Fixed: `item.rs:115–131`; count/max/settings/template distinction supported by S17/S18. |
| D12 — claiming XSD restricts references to three kinds | Fixed: `invoice.rs:23–30` says the XSD declares references independently; `proforma_number()` documents the crate's exposed subset. |
| D13 — waybill only on delivery notes, hidden override/default | Fixed: `waybill.rs:87–104`, `invoice.rs:193–196,564–566`, `types.rs:983–985`. SC-02 is a fresh cross-source vocabulary qualification. |
| D8 — stale fixed currency count | Fixed: `types.rs:361–365` links the supported list and keeps an open string. |
| D11 — false defaults incorrectly described as omitted | Fixed: `invoice.rs:574–578` expressly documents explicit eszamla/download flags and present seller container. |
| E.E6 — unqualified cached-schema provenance | `fixtures/SOURCES.md:126–140,193–228` now identifies the project modification and separate competing sources. Fresh comparison corroborates it; the older `/xsd` route adds another source caveat. |
| Missing `csoportazonosito` / `torloKod` inferred from stale download | Dismissed: both are in current inline schemas and actual writers (`invoice.rs:853,914–916`). They must not be removed to pass S4. |
| All example tags allegedly mandatory | Dismissed as conflicting vendor prose: S2/S3 say every example field is mandatory, then explicitly say minOccurs=0 fields may be omitted. Their own comments call fields optional. The crate follows the detailed XSD optionality, not the blanket sentence. |

Historical response/parser/transport/storno findings were not re-adjudicated in this ownership slice.

## Verification performed and limits

- Manually traced **every supported invoice XSD field**, all six kind paths, nested sequence order, required/optional constructors, default emission and the ten current settings/rules pages. Read EN/HU examples, both current inline schemas, the linked download, and older separately served XSD. Followed linked VAT/erasure/template references where they resolve meanings.
- Built and ran a tiny scratch crate at `/tmp/opencode/invoice-review-20260910` against the current path dependency with **`cargo run --offline --quiet --manifest-path /tmp/opencode/invoice-review-20260910/Cargo.toml`**. It calls the actual calculator/validator/writer; credentials are the literal dummy string `offline`, and no HTTP client is used. Confirmed numeric `Other` versus `Percent`, actual combined-option XML, ampersand escaping and prohibited-control output.
- Ran **`python3 /tmp/opencode/invoice-review-20260910/compare.py`**: independently acquired and hashed schema blocks, parsed schema membership, checked generated header member/order, checked emitted XML with Python's parser, and printed the downloaded PHP's actual header-tail line numbers. Results are quoted above.
- No full or targeted repository test suite was run. Existing tests were read, not reported as passing. `lxml`/`xmllint` were unavailable; no full XSD-engine validation claim is made. The structural comparison establishes the specific header-order counterexample without one.
- No account calls, no server-rendered document inspection, no PHP execution, no new tax/legal conclusion. Public-source tax descriptions are reported as the vendor's semantics. Source conflicts are kept explicit rather than resolved by assuming one source universally authoritative.
- No production code, fixtures or existing reports were edited. The only repository write is this report. Scratch source/downloads/build outputs remain under `/tmp/opencode`; no temporary repository test was added.
