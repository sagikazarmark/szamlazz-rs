# Invoice request API conformance — current code at 77d53c5

**Reviewed:** 2026-09-11. **Baseline and observed HEAD:** `77d53c553c9ecdc86d5fa72ca932c636256ae807`.

## Conclusion

**No confirmed implementation defect found in the reviewed invoice-request surface.** All documented invoice elements are expressible. Current checked requests have correct common-schema order, cardinality and lexical forms. The historical outbound date-domain defect is fixed at `to_wire` and the bundled client's checked boundary.

There are **three material first-party schema contradictions**, independently reproduced against fresh sources: `simpleItems`/`elonezetpdf` order, downloadable-schema omission of `csoportazonosito`, and downloadable-schema omission of `torloKod`. A still-accessible legacy inline XSD also lacks `simpleItems`. These prevent an unconditional “valid against every published XSD” claim, but do not establish that the current writer fails against the server.

The audit ran 134 generated invoice requests through four freshly acquired, unmodified XSDs. Every declared element path occurred in at least one completely valid request for each source. The only invalidity diagnostics concerned those documented contradictions. The ordinary documented gross-based HUF payload, negative discount, supported VAT/language tokens and attachment limits also passed offline controls.

## Scope and method

This is a **whole-current-surface review, not a diff review**: `ops/invoice.rs`, `ops/waybill.rs`, `item.rs`, outbound shared types in `types.rs`, request XML helpers, and invoice attachment construction. Response/query interpretation, other mutations, receipts, taxpayer operations and transport policy belong to the other reviews. Shared multipart construction was inspected only to verify invoice files, not to repeat the transport audit.

`git diff -- crates/szamlazz-agent` was empty. Existing unrelated worker edits were present. No implementation, fixture or shared test source was edited. Scratch acquisition, build output and controls used `/tmp/opencode/invoices-77d53c5-*`. No live test, probe, vendor POST or `.env` read occurred. HTTP GETs fetched public documentation/assets only. No subdelegation occurred; this session exposes no subagent tool.

The requested directory URL `https://docs.szamlazz.hu/agent/generating_invoice/` returned 403, as did its no-trailing-slash spelling. Its actual request, XML, settings index and all ten linked rule pages were successfully fetched. Thus the index failure did not prevent auditing the documented invoice content. Current pages identify build `v202608271632`; the still-accessible `/xsd` page identifies the older `v202606031507`.

Read historical execution evidence in `docs/szamlazz-hu-behaviour.md`, plus both `docs/research/*live*` records (`2026-09-11-receipts-live.md`, `2026-09-11-credit-clearing-live.md`). Receipt MNB/email observations are not generalized to invoice behavior. No old review supplied a verdict: conclusions below come from current code and fresh primary sources.

## Fresh primary-source register

Identifiers below are citations used in the matrices. All URLs were fetched during this review.

| ID | First-party source |
|---|---|
| S1 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request) |
| S2 | [EN XML example and inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) |
| S3 | [HU XML example and inline XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) |
| S4 | [Downloadable xmlszamla.xsd](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) |
| S5 | [Legacy EN inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xsd) |
| S6 | [Settings/rules index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) |
| S7 | [Document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types), [HU counterpart](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) |
| S8 | [Tour-operator simpleItems](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) |
| S9 | [VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [HU counterpart](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) |
| S10 | [Rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [HU counterpart](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) |
| S11 | [Supported currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) |
| S12 | [Templates and languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) |
| S13 | [Order number and duplicate checking](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) |
| S14 | [Discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount) |
| S15 | [Email notification and attachments](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) |
| S16 | [Erasure-code request rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) |
| S17 | [HU erasure-code knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) |
| S18 | [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere), its [2025-11-04 VAT guide PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf) |
| S19 | [Dynamic email fields/BBCode](https://tudastar.szamlazz.hu/gyik/szamlaertesito-egyedi-mezok) |

Fresh schema bytes were saved under `/tmp/opencode/invoices-77d53c5-{label}.xsd`; inline sources are the HTML-decoded `pre` block, not hand-reconstructed schemas:

| Label | SHA-256 |
|---|---|
| `en-inline` | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| `hu-inline` | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| `download` | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| `legacy-inline` | `508162a8a38ec80648db3d013b7ab1b258532cae914997887192a17dcbada801` |

## Element coverage matrix

Paths below are relative to `xmlszamla`. **R** = exactly one; **O** = zero or one. Every listed leaf is singleton. Optional fields are omitted on `None`, with explicit `Some(false)` retained for optional booleans unless separately noted. The schemas declare no XML `default=` values; business defaults come from prose or the crate's explicit policy. Code references are exact current line ranges relative to `crates/szamlazz-agent/`.

### Root, settings and header

| Elements, in schema order | Cardinality/type, meaning/default | Current code and result |
|---|---|---|
| `xmlszamla` | R; namespace `http://www.szamlazz.hu/xmlszamla`; UTF-8 XML 1.0 | `src/ops/invoice.rs:758-760`, `src/xml.rs:159-178`. Correct. `xsi:schemaLocation` is an example hint, not a required request field. |
| `beallitasok`, `fejlec`, `elado`, `vevo`, `fuvarlevel`, `tetelek` | R,R,R,R,O,R in that order (S2–S4) | `src/ops/invoice.rs:760-940`. Empty seller remains present. Optional waybill precedes items. |
| `beallitasok/felhasznalo`, `jelszo`, `szamlaagentkulcs` | O strings in this sequence; password pair or agent key | `src/xml.rs:630-637`, `src/ops/invoice.rs:761`. Both forms exercised in matrix. |
| `eszamla`, `szamlaLetoltes` | R booleans; electronic/paper and returned PDF | `src/ops/invoice.rs:532-542,604-605,762-763`. Both explicitly false by constructor; true is expressible. S7 confirms electronic/paper meaning. |
| `szamlaLetoltesPld` | O int; copies setting now deprecated | `src/ops/invoice.rs:543-547,764-766`. `u8` always emits valid `xs:int`; narrower domain not a defect. |
| `valaszVerzio` | O int; 1 text/PDF, 2 XML/base64 (S2 example) | `src/ops/invoice.rs:767` pins `ops::RESPONSE_VERSION`. Intentional v2-only request. |
| `aggregator`, `guardian`, `cikkazoninvoice`, `szamlaKulsoAzon` | O string, boolean, boolean, string; integration settings/item identifiers/external query identifier | `src/ops/invoice.rs:548-556,768-775`. All exposed, correctly ordered. External id does not claim server uniqueness. |
| `fejlec/keltDatum` | O date; issue date, omitted for server date | `src/ops/invoice.rs:141-150,683-684,779`. Historical test-account date replacement explicitly documented. |
| `teljesitesDatum`, `fizetesiHataridoDatum` | R date/date; fulfillment and due date | `src/ops/invoice.rs:151-156,685-689,780-781`. Correct; EN sample's “payment date” translation is resolved by HU “teljesítés dátuma.” |
| `fizmod`, `penznem`, `szamlaNyelve` | R string/string/15-token enumeration | `src/ops/invoice.rs:157-162,782-784`; `src/types.rs:369-478,480-586,588-688`. See semantic matrix. |
| `megjegyzes` | O string; invoice comment | `src/ops/invoice.rs:163-164,785`. Text escaped, content preserved as XML text. |
| `arfolyamBank`, `arfolyam` | O string/double; bank/rate, conditionally needed for foreign currency | `src/ops/invoice.rs:165-166,740-750,786-791`, `src/types.rs:943-982`. Explicit and automatic-MNB forms supported. |
| `rendelesSzam`, `dijbekeroSzamlaszam` | O strings; order identifier/proforma reference | `src/ops/invoice.rs:167-169,104-117,792-798`. Proforma reference is offered for invoice, prepayment and final. |
| `elolegszamla`, `vegszamla`, `elolegSzamlaszam` | O bool/bool/string; prepayment/final/reference | `src/ops/invoice.rs:47-78,707-723,801-810`. One final reference or order number required locally. |
| `helyesbitoszamla`, `helyesbitettSzamlaszam`, `dijbekero`, `szallitolevel` | O bool/string/bool/bool; corrective/base/proforma/delivery note | `src/ops/invoice.rs:79-85,811-817`. Corrective base carried with flag; kinds mutually exclusive; irrelevant false flags omitted. |
| `logoExtra`, `szamlaszamElotag` | O strings; extra logo and pre-registered prefix | `src/ops/invoice.rs:170-175,818-819`. Exposed verbatim. |
| `fizetendoKorrekcio`, `fizetve` | O double/bool; payable adjustment, paid marker | `src/ops/invoice.rs:176-180,820-825`. Adjustment signed decimal; paid=false deliberately omitted, true emitted. No documented requirement to emit false was found. |
| `arresAfa`, `eusAfa` | O booleans; margin VAT / non-Hungarian VAT reporting flag | `src/ops/invoice.rs:181-192,826-831`. Independent, no incorrect substitution of item VAT code. |
| `szamlaSablon` | O string; six documented layouts, otherwise server default | `src/ops/invoice.rs:193-196,832-839`; `src/types.rs:984-1021`. All six plus `Other`; delivery note forces documented layout. |
| `elonezetpdf`, `simpleItems` | O booleans; preview without issue / simplified image | `src/ops/invoice.rs:197-225,840-847`. Both independently expressible. Order matches download, conflicts with current EN/HU inline; C1 below. |

### Seller, buyer and buyer ledger

| Elements, in schema order | Cardinality/type and meaning | Current code and result |
|---|---|---|
| `elado/bank`, `bankszamlaszam`, `emailReplyto`, `emailTargy`, `emailSzoveg`, `alairoNeve` | All O strings; bank/account, reply-to/subject/body, signer | `src/ops/invoice.rs:262-275,849-858`, `src/types.rs:1023-1033`. All exposed. Seller name/address/tax identity is account data, not a missing request capability. |
| `vevo/nev`, `orszag`, `irsz`, `telepules`, `cim` | R,O,R,R,R strings; buyer identity/address | `src/ops/invoice.rs:296-306,861-865`. No whitespace normalization or leading-zero loss. |
| `email`, `sendEmail` | O string/bool; comma-separated recipients; omitted sendEmail means send if address supplied (S15) | `src/ops/invoice.rs:307-315,866-869`. Omission, explicit false and true available. |
| `adoalany`, `adoszam`, `csoportazonosito`, `adoszamEU` | O int/string/string/string; taxpayer status, domestic number, VAT-group id, EU number | `src/ops/invoice.rs:316-324,870-875`, `src/types.rs:690-756`. All five documented status tokens map correctly. Group id conflicts only with download, C2. |
| `postazasiNev`, `postazasiOrszag`, `postazasiIrsz`, `postazasiTelepules`, `postazasiCim` | All O strings, flat buyer siblings; postal recipient/address | `src/ops/invoice.rs:277-291,876-882`. Rust grouping does not introduce a spurious XML container. |
| `vevoFokonyv` | O ledger container | `src/ops/invoice.rs:327-328,883-894`. Empty container valid. |
| `vevoFokonyv/konyvelesDatum`, `vevoAzonosito`, `vevoFokonyviSzam`, `folyamatosTelj`, `elszDatumTol`, `elszDatumIg` | All O; date/string/string/bool/date/date; accounting date, buyer ledger identity/account, continuous fulfillment, settlement period | `src/ops/invoice.rs:120-135,691-697,884-893`. Correct sequence and positive-year checking at all date positions. |
| `azonosito`, `alairoNeve`, `telefonszam`, `megjegyzes` | All O strings; partner id, signer, phone, buyer comment | `src/ops/invoice.rs:329-346,895-898`. Partner-id master-data update and customer-account access consequences accurately documented against S2/S3. |

### Items and item ledger

| Elements, in schema order | Cardinality/type and meaning | Current code and result |
|---|---|---|
| `tetelek/tetel` | 1..unbounded | `src/ops/invoice.rs:704-706,903-905`. Empty list refused; supplied row order preserved. |
| `megnevezes`, `azonosito` | R/O strings; name and account-side item identifier | `src/item.rs:91-94`, `src/ops/invoice.rs:906-907`. Correct. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs` | R double/string/double/string; quantity/unit/net unit price/VAT token | `src/item.rs:95-102`, `src/ops/invoice.rs:908-911`. Signed/fractional decimal values and all documented VAT tokens expressible. |
| `arresAfaAlap` | O double; invoice margin VAT base | `src/item.rs:103-104`, `src/ops/invoice.rs:912-914`. Exposed independently of printed amounts. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | R doubles; caller-supplied net/VAT/gross | `src/item.rs:105-110,133-223`, `src/ops/invoice.rs:915-917`. Explicit constructor preserves asserted values; calculated constructor has explicit rounding. |
| `megjegyzes`, `tetelFokonyv` | O string/container | `src/item.rs:111-114`, `src/ops/invoice.rs:918-934`. Both exposed. |
| `tetelFokonyv/gazdasagiEsem`, `gazdasagiEsemAfa`, `arbevetelFokonyviSzam`, `afaFokonyviSzam`, `elszDatumTol`, `elszDatumIg` | All O; string/string/string/string/date/date; economic events, ledger accounts, period | `src/item.rs:52-72`, `src/ops/invoice.rs:698-703,920-933`. Correct sequence; later rows included in date validation. |
| `torloKod` | O nonnegative int; count of requested codes, maximum 400 in prose | `src/item.rs:115-131`, `src/ops/invoice.rs:724-731,935-937`. 0/400 supported, 401 refused. Correct final position; C3 download conflict. |

### Waybill

All waybill and carrier fields are O unless marked R. All are strings except parcel counts (`xs:int`) and declared value (`xs:double`).

| Elements in order | Meaning/cardinality and code | Result |
|---|---|---|
| `fuvarlevel/uticel`, `futarSzolgalat`, `vonalkod`, `megjegyzes` | Unused destination, carrier token, fallback barcode, comment. `src/ops/waybill.rs:87-104,132-136` | All exposed. Carrier string admits TOF, PPP, SPRINTER, FOXPOST, MPL, GLS, EMPTY. No invented missing FOXPOST/GLS XML child. |
| `tof` → `azonosito`, `shipmentID`, `csomagszam`, `countryCode`, `zip`, `service` | Five-digit TOF identifier, shipment id, count, destination country/ZIP, service. `src/ops/waybill.rs:9-24,137-148` | All exposed in exact sequence. String identifiers preserve leading zeros. |
| `ppp` → `vonalkodPrefix`, `vonalkodPostfix` | Agreed three-character prefix, per-invoice suffix ≤7 characters. `src/ops/waybill.rs:26-33,149-154` | Both exposed; carrier content restrictions left to caller/server. |
| `sprinter` → `azonosito`, `feladokod`, `iranykod`, `csomagszam`, `vonalkodPostfix`, `szallitasiIdo` | Three-character abbreviation, ten-digit sender code, routing code, count, unique 7–13-character suffix, time text. `src/ops/waybill.rs:35-50,155-166` | All exposed in sequence. |
| `mpl` → `vevokod` R, `vonalkod` R, `tomeg` R, `kulonszolgaltatasok`, `erteknyilvanitas` | Customer code, barcode source, weight text, service-icon config, declared value. `src/ops/waybill.rs:52-85,167-176` | Mandatory fields always written. Weight correctly remains string, not forced to numeric. |
| All carrier containers together | `tof`, `ppp`, `sprinter`, `mpl` are a sequence, not an XSD choice | `src/ops/waybill.rs:105-112,137-177`. Coexistence is schema-valid. Actual carrier choice/rendering belongs to vendor. |
| Parcel count bounds | Nonnegative subset of signed int | `src/ops/waybill.rs:116-129`, `src/ops/invoice.rs:732-739`. Values above `i32::MAX` rejected; maximum passes. Negative package counts are not a missing normal capability. |

## Capability, rules, defaults and meaning audit

| Capability and quoted contract | Assessment |
|---|---|
| **File submission** (S1): “Content type: `multipart/form-data`”; main field `action-xmlagentxmlfile`, attachments `attachfile1` … `attachfile5` | `src/ops/invoice.rs:679,959-969`, `src/wire.rs:66-99` match. Attachment bytes separate from XML; boundary collisions in content avoided (`wire.rs:109-124`). |
| **Document variants** (S7): invoice “default type; no special field required”; one prepayment has “exactly one final invoice” | Six kinds modeled at `invoice.rs:21-85`; storno correctly separate. Final takes one reference, supports order identification; no unsupported multiple-prepayment combination promised. |
| **Final totals** (S7 says final settles remaining amount) | Crate accurately warns at `invoice.rs:60-70` that caller supplies negative prepayment row. Live C6-2 (`docs/szamlazz-hu-behaviour.md:132-139`) rules out assuming server netting. |
| **Proforma conversion** (S7): fill `dijbekeroSzamlaszam` | Supported on regular/prepayment/final in correct position. Explicit prepayment/final links remain unverified live (`behaviour.md:255-261`); implicit linking is not evidence for explicit-reference acceptance. |
| **Preview** (S2–S4): “no actual document is created” | `preview_pdf` sent as requested; no implicit kind conversion. Combined preview/simpleItems uncertain (C1), response interpretation outside scope. |
| **simpleItems** (S8): “false or omitted … default (non tour operator) mode”; controlled “per document” | `invoice.rs:199-225,840-847`, `tests/simple_items.rs:20-112`: preserves all three states. Full monetary data remains present. |
| **simpleItems restrictions** (S8): OSS off, Hungarian seller number; max 2 rows, final 4; rates `0,5,18,27,TAM,AAM,K.AFA,F.AFA`; template overridden | Accurate rustdoc at `invoice.rs:204-218`. No local account-dependent gate; forbidden combinations can be sent for vendor refusal, intentionally tested at `tests/simple_items.rs:72-88`. This is not loss of a supported payload. |
| **simpleItems inheritance** (S8): final inherits prepayment, final rates must match; corrective/original correction and delivery note forbidden | Expressible and documented. Server knows prepayment/original and applies inheritance. No missing request field. |
| **VAT tokens** (S9) | All 16 invoice special codes and all listed percentages including `2.1`, `4.8`, `8.1`, `25.5` are represented by `VatRate` (`types.rs:178-365`). Scratch enumerated each current token. Open `Other` preserves future tokens. Receipt-only known tokens being representable is not a claim they work on invoices. |
| **VAT meaning** (S18 guide) | `KBAUK` means new means of transport, not United Kingdom; `KBAET` is exempt intra-community supply. PDF confirms current `types.rs:232-237`; EN API table's “to UK / to ET” labels are misleading. `TAM` vs `TAHK`, `EUT` vs `EUKT`, `HO/EUE/EUFADE/EUFAD37/ATK/NAM/EAM` meanings match HU/guide. |
| **K.AFA subtype** (S18 PDF): search invoice comment, item name/comment for “utazási irodák”, “használt cikkek”, “műalkotások”, “gyűjtemény darabok és régiségek”; unmatched defaults used goods | `types.rs:203-211` accurately documents the text-driven selection. All three text positions are exposed. No claim to infer subtype automatically. |
| **eusAfa** (S9): “does not contain Hungarian VAT”; only OSS/non-Hungarian seller; “does not replace item-level VAT code”; “retroactive data submission is not possible” | `invoice.rs:183-192` accurately documents meaning and consequences. Optional bool remains caller-selected; no assumption that every EU transaction needs true. |
| **Currencies** (S11): “HUF or Ft”; foreign document requires bank/rate | Every listed currency can be constructed (`types.rs:390-399`), including unusual vendor `KSH` and legacy codes. `is_huf` recognizes aliases; its case-insensitive classification preserves requested spelling. Code does not impose an ISO-only currency list. |
| **Automatic MNB** (S2/S4): `arfolyamBank='MNB'` and absent rate uses current MNB rate | `ExchangeRate::automatic_mnb` and invoice writer preserve absence; no contradictory “must always provide numeric rate” local refusal. Empty/padded bank refused for foreign currency. |
| **Net-based HUF arithmetic** (S10): round net, then VAT, gross = sum | `item.rs:183-223` follows it with explicit `Rounding::Scale(0)`/HUF policy. Midpoints half away from zero; no scientific/nonfinite output. |
| **Gross-based HUF arithmetic** (S10): `393.66 × 3`, net `1181`, VAT `319`, gross `1500` | Explicit `LineItem::new` supports this exact official example. Scratch request accepted by `to_wire` and all four XSDs. No dedicated gross-price calculator, but capability is present; optional improvement below. HU says “valószínűleg” (probably) for B2B/B2C method choice, less absolute than EN “have to.” |
| **Fractional HUF input** (S10): vendor rounds according to eight net/VAT/gross integrality combinations | Explicit values remain sendable: the crate does not incorrectly block the vendor's documented normalization path. It does not promise to reproduce server recalculation locally. |
| **Foreign rounding** | `types.rs:412-435`, `item.rs:9-39` identify minor units as local arithmetic policy, not vendor storage guarantee. P60-E1/E3 support independent EUR two-decimal storage; not KWD/JPY or HUF evidence. Narrow Decimal domain is not a defect. |
| **Discount** (S14): “no dedicated discount field”; negative unit price, positive quantity, same VAT, row immediately after affected item | Signed explicit/calculated rows and vector order implement this. Scratch `1 × -2000 @27%` gives `-2000/-540/-2540`. No missing percentage-discount field. |
| **Payment method** (S2 string; example `Átutalás`) | `types.rs:588-688` accepts free text and known Hungarian tokens. Lowercase `átutalás` is exercised in recorded live invoice requests; `Other` can send title case exactly. |
| **Language and template** (S12): 15 languages; affects PDF, notification, buyer portal; absent layout uses default | All 15 exact tokens, including vendor `cz` and `si`, supported. Six templates plus `Other`. `InvoiceTemplate::Default` names `SzlaAlap`, not omission (`types.rs:990-1004`). |
| **Order repetition** (S13): account toggle, same-kind check, reversal/corrective exemptions, successful duplicate needs matching buyer/gross/dates and ≤2 days | Request exposes order string, not an invented toggle or invoice idempotency key. Existing crate README explains external-id uncertainty (`README.md:91-167`). Historical date replacement/replay is treated as observation, not overriding all documented fingerprint conditions. |
| **Notification** (S15): supplied email and true/absent sendEmail sends; false prevents; comma-separated recipients | Current `Buyer` and writer match. SellerEmail contains reply-to/subject/body. BBCode and dynamic labels from S19 pass as text; no client interpolation needed. Manual newline can be supplied. |
| **Attachments** (S15): “up to 5 files”, “2 MB” each; invalid files separately notified while valid ones still sent; false sendEmail means unprocessed | Bounded private collection, checked `push`/`TryFrom`/Deserialize at `invoice.rs:405-501`, sequential field names at 959-969. Exact 2,000,000-byte bound is deliberate conservative interpretation. Local all-or-nothing rejection is an intentional narrower policy, not a normal supported-payload failure. Binary bytes preserved. |
| **Erasure codes** (S17): put requested “darabszámát” (count) at end of item, maximum 400, `SzlaMost`, enable account setting; stock or vendor-generated | `item.rs:115-131` and `invoice.rs:724-731,935-937` correct. Template/account preconditions documented, not forced. S17's lowercase `<szamlasablon>` example contradicts case-sensitive XSD spelling; writer correctly uses `szamlaSablon`. |

## Confirmed defects

**None in this scope.** There is no P0/P1/P2 implementation finding to reproduce. The following items retain separate classifications rather than turning every documentation disagreement or optional client-side validation into a bug.

## Contradictory documentation and operational uncertainty

### C1 — Medium interoperability uncertainty: preview/simpleItems order

**Code:** `src/ops/invoice.rs:840-847`; explicit disclosure at 220-222.

**Contract:** S2/S3 say field order is fixed (“they cannot be interchanged”; HU “kötött, nem felcserélhetők”). Their `fejlecTipus` tail is:

```xml
<element name="szamlaSablon" type="string" maxOccurs="1" minOccurs="0"/>
<element name="simpleItems" type="boolean" maxOccurs="1" minOccurs="0"/>
<element name="elonezetpdf" type="boolean" maxOccurs="1" minOccurs="0"/>
```

S4 reverses the last two. Current code follows S4. S5 omits `simpleItems` entirely.

**Concrete reproducer:** set `header.preview_pdf = Some(true)` and `header.simple_items = Some(true)` on an otherwise minimal invoice. Writer emits `<elonezetpdf>true</elonezetpdf><simpleItems>true</simpleItems>`. Fresh EN/HU validation rejects `simpleItems` as unexpected; fresh download accepts it. Explicit false values also count as present and exhibit the same conflict. Across the matrix, 10/134 requests fail each current inline schema for this reason.

**Impact:** integrations validating against the inline schema reject a combined request; server rejection or differing preview behavior remains unproved. Omitting one field avoids this particular ordering conflict, but is not equivalent to every intended combined request. No combined-preview execution exists in the read live records. The code's PHP-order comment was not used as fresh PHP evidence; download evidence alone establishes the contradictory contract. Do not change ordering solely on this review.

### C2 — Medium interoperability uncertainty: VAT-group identifier absent from download

**Code:** `src/ops/invoice.rs:321-322,873-875`.

**Contract:** S2/S3 declare optional `<element name="csoportazonosito" type="string" maxOccurs="1" minOccurs="0">` between `adoszam` and `adoszamEU`. S4 goes directly from `adoszam` to `adoszamEU`.

**Reproducer:** on a minimal request set `buyer.group_id = Some("12345678".into())`. Current inline schemas accept its position; download rejects `csoportazonosito`. Download matrix diagnostics named it eight times (some requests also contain erasure codes).

**Impact:** callers requiring download-XSD validation cannot submit a modeled group id without reconciling the source discrepancy. No evidence establishes that the server rejects the field; removing support would itself discard the inline-documented capability. No live exemption is claimed.

### C3 — Medium interoperability uncertainty: erasure count absent from download

**Code:** `src/ops/invoice.rs:935-937`; `src/item.rs:115-131`.

**Contract:** S2/S3 declare optional nonnegative `int` `torloKod`; S16 and S17 describe it, and S17 explicitly says to place it at the end of `tetel`. S4 has no such element.

**Reproducer:** set `items[0].erasure_code_count = Some(1)`. It validates against both current inline schemas and fails the download at `torloKod`. Ten download diagnostics named it across 134 requests.

**Impact:** download-based preflight rejects the documented erasure-code capability. This is not a writer-position bug; the current writer obeys the explicit contemporary rule. Test-account execution cannot establish its success because the feature is unavailable there per the crate's documented limitation. No live exemption is claimed.

### C4 — Low documentation inconsistencies, not implementation defects

- S2/S3 say **“All fields shown in the example are mandatory”**, but the same page explicitly allows `minOccurs="0"` omission, and its example contains optional fields. Actual XSD cardinality, not blanket sample prose, governs the matrix. Empty optional containers and omitted optional leaves are valid.
- S2's short example-language comment lists eight languages; S2's enum and S12 list 15. Code covers 15.
- S9 EN describes KBAUK/KBAET as “to UK” / “to ET”; HU leaves the abbreviations unexplained. S18's fresh PDF resolves meaning and confirms the code's current rustdoc.
- S10's gross example calls `(1500 - 319) / 3 = 393.66` the total net value; its XML correctly uses 393.66 as **unit** price and 1181 as total net. The explicit request constructor accepts the XML, not the mislabeled prose.
- S17 uses lowercase `szamlasablon` in an illustrative line; S2–S4 and S12 consistently require `szamlaSablon`. Code is correct.

## Historical date-domain finding: fixed

`src/xml.rs:19-35` rejects every outbound `Date` whose year is ≤0. `CreateInvoice::validate` checks all eight positions at `src/ops/invoice.rs:683-703`: header issue/fulfillment/due date, buyer accounting/settlement-from/settlement-to, each row's settlement-from/to. It iterates every row, not only the first. `AgentRequest::to_wire` validates before writing at `src/wire.rs:405-409`; the bundled client calls that boundary at `src/client.rs:375`.

`tests/request_dates.rs:42-74,77-132` independently exercises years `-9999`, `-1`, `0`, `1`, `2024`, `9999`, leap day and a later item. All passed. Scratch checked requests with years 1, 2024 and 9999 also validated against **all four freshly fetched XSDs**, establishing that supported emitted dates are valid lexical values. Mutating a normal invoice to `0000-02-29` was rejected by all four validators.

The public `write_xml` trait method remains explicitly **unchecked** (`src/wire.rs:367-373`) and can still render an unsupported date if called directly. That is not a regression in the checked boundary. The restricted positive-year domain is intentional; absence of BCE/year-zero/timezone-bearing civil-date support is not counted as a conformance defect. No current ordinary supported date failed.

## Intentional differences and evidence-qualified behavior

1. **v2 only, enum-selected kinds, omitted false kind flags:** the typed operation chooses structured answers and one kind. Schemas permit those emissions, and normal invoice kind needs no special field (S7). Independent schema flags do not prove useful combined kinds exist. A proforma reference on corrective/delivery-note/proforma remains outside the enum without documented business evidence for that combination (`invoice.rs:21-30,104-117`).
2. **Delivery-note layout override:** `invoice.rs:832-839` forces `SzlaFuvarlevelesAlap`, disclosed at 193-196 and aligned with S7. Ordinary invoices can still choose a waybill-capable template and carry waybill data.
3. **Business validation remains vendor-side:** empty required strings, unusual VAT tokens, mismatched totals, template/erasure restrictions and account-dependent simpleItems rules are not all locally gated. Types preserve valid payloads; `validate()` is not advertised as a full account-aware business simulation. Adding every server condition is not necessary to conform.
4. **Attachment local validation:** stricter preflight means the caller must remove an oversized file before building the collection, rather than relying on the vendor's partial email processing. The 2 MB interpretation is disclosed (`invoice.rs:405-407,511-514`). No evidence supports claiming 2,097,152 bytes is the contractual minimum.
5. **Numeric domain:** finite Decimal output is valid `xs:double` text; integer copy/count bounds emit valid lexical values. No requirement to expose infinity, NaN, enormous exponents or every XSD integer value was inferred. Exact arithmetic may reject an unrepresentable intermediate even if subsequent rounding would fit (`item.rs:178-182`, `number.rs:4-59`); this is documented and not an ordinary-input failure shown by this review.
6. **Live-supported qualifications:** issue-date replacement (`behaviour.md:110,218-222`), implicit proforma/prepayment/final links (`123-127,136-139`), duplicate/order/external-id behavior (`52-90`) and EUR arithmetic/storage (`175-182`) were read before adjudication. In particular EUR two-decimal storage is not generalized to all currencies. Explicit final/prepayment proforma references remain unverified (`255-261`).
7. **Foreign currency without any exchange block:** fresh S11 explicitly requires bank/rate; S2/S4 additionally permit automatic MNB. The older hypothesis that VAT-free foreign proformas/delivery notes may need neither remains unverified (`behaviour.md:237-244`). Current local requirement is supported by prose and no ordinary documented valid request was shown blocked.

## Optional improvements and remaining uncertainty

These are **low-priority improvements or unclassified gaps**, not confirmed defects:

- A gross-price calculation example/helper would make S10's B2C path easier to discover. `LineItem::new` already supports the official payload; neither `try_calculated` nor `Rounding` claims to take gross unit price.
- `Language`/header rustdoc could mention that the selected language also controls notification and buyer-portal language (S12). Serialization already delegates that behavior correctly.
- `Waybill` carrier rustdoc could list all seven carrier tokens and PPP/Sprinter length annotations. Current strings can express each; rendering/contract-specific validation remains server-side.
- `InvoiceAttachments` rustdoc could explicitly repeat that false `sendEmail` causes vendor-side nonprocessing and that invalid files do not suppress valid vendor-side attachments (S15). The actual multipart request has correct files and flag.
- Exact carrier rendering, custom templates/logos, guardian/aggregator contractual behavior, email attachment delivery and dynamic-label substitution have schema/prose support but no execution proof in this audit. Passing XML is not proof of document rendering or NAV reporting.
- `arresAfa`, `arresAfaAlap`, `fizetendoKorrekcio`, guardian and some ledger/economic-event fields have limited explanation in the fetched invoice pages beyond names and schema types. No extra arithmetic or default semantics were invented for them.

## Test review and commands actually executed

### Existing tests inspected

- `src/ops/invoice.rs:1021-1256`: golden invoice XML, corrective/reference handling, optional field and carrier ordering.
- `src/ops/invoice.rs:1259-1349`: delivery-note template and exact multipart attachments, collection bounds.
- `src/ops/invoice.rs:1479-1576,1594-1649`: missing items/final references, XML characters, erasure/parcel bounds, foreign currency and MNB omission.
- `src/item.rs:232-440`: exact arithmetic, precision/overflow, example, local minor units and negative rounding.
- `tests/request_dates.rs:42-132`: every invoice date position and later rows.
- `tests/simple_items.rs:20-112`: all 3×3 optional states across six kinds, preservation of full monetary fields and group/erasure data.
- `tests/numeric_fidelity.rs:15-47`: numeric `Other` VAT strings calculate what they send.
- `tests/schema_requests.rs:155-418`: full source-derived invoice inventory, minimal/full/individual blocks, every kind, empty containers, booleans, foreign rate forms, rows and five attachments; both credential forms at 58-90.
- `tests/upstream.rs:1051-1116`: example-outline equivalence; it intentionally ignores some empty/omitted distinctions, so it is not a substitute for XSD validation.
- `scripts/check-agent-schemas.py:35-85,124-171`: actual validator and path-coverage checks with explicit expected conflicts. The complete workspace script was inspected, **not executed**; the scoped scratch validator used fresh invoice sources instead.

### Rust commands

Each command used `CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target`; default features (no vendor client) and offline dependency resolution. Tests were synthetic, and no live/probes test target was selected.

```sh
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --test request_dates --test simple_items
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --lib ops::invoice::tests
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --lib item::tests
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --test numeric_fidelity numeric_vat_tokens_calculate_the_percentage_they_send -- --exact
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target SZAMLAZZ_SCHEMA_OUTPUT=/tmp/opencode/invoices-77d53c5-matrix.json cargo test -p szamlazz-agent --locked --offline --test schema_requests -- --ignored --exact emit_request_matrix
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --lib types::tests
CARGO_TARGET_DIR=/tmp/opencode/invoices-77d53c5-target cargo test -p szamlazz-agent --locked --offline --lib wire::tests::multipart
```

Results: **3 + 2 + 31 + 8 + 1 + 1 + 11 + 4 = 61 tests passed**. Some selected unit modules include incidental response/shared-type tests; their passing does not extend the substantive review scope. The ignored schema test only emits synthetic XML; it is not a vendor probe.

### Additional offline controls and fresh-schema validation

```sh
rustc --edition=2024 /tmp/opencode/invoices-77d53c5-controls.rs -L dependency=/tmp/opencode/invoices-77d53c5-target/debug/deps --extern szamlazz_agent=/tmp/opencode/invoices-77d53c5-target/debug/deps/libszamlazz_agent-5ed1935b35a951f9.rlib --extern rust_decimal=/tmp/opencode/invoices-77d53c5-target/debug/deps/librust_decimal-02185a70be272d4b.rlib -o /tmp/opencode/invoices-77d53c5-controls
/tmp/opencode/invoices-77d53c5-controls
python3 /tmp/opencode/invoices-77d53c5-audit.py
```

The Python script was run twice, with the second adding schema validation of the scratch gross-price/date controls and improving diagnostic grouping. It fetched the four public XSD sources with `urllib.request`; HTML extraction used stdlib `HTMLParser`. It selected only the **134 `xmlszamla` rows (67 cases × two credential forms)** from the generated matrix. XSD validation used installed libxml2 2.15.3:

```sh
/nix/store/6xp8y3aclw6m89sy7r12sf6l98s2di0m-libxml2-2.15.3-bin/bin/xmllint --nonet --noout --schema /tmp/opencode/invoices-77d53c5-en-inline.xsd -
```

The same command was applied to each source and request via stdin. No schema was patched or merged. The download and inline schemas are self-contained. Four negative controls per source changed a boolean to `not-bool`, removed mandatory `elado`, changed a monetary value to `not-money`, and changed fulfillment date to year zero. All 16 rejected with document-invalid exit code 3.

| Source | Valid requests | Invalid requests | Only invalidity causes | Element-path coverage in fully valid requests |
|---|---:|---:|---|---:|
| Current EN inline | 124 | 10 | `simpleItems` after preview | 125/125 |
| Current HU inline | 124 | 10 | `simpleItems` after preview | 125/125 |
| Download | 120 | 14 | `csoportazonosito`, `torloKod` (some overlap) | 123/123 |
| Legacy EN inline | 118 | 16 | `simpleItems` unsupported | 124/124 |

Coverage includes root and container paths, not merely leaves. It means every declared path was exercised in a wholly schema-valid request, **not** that all optional combinations or business semantics are thereby proven.

Scratch Rust assertions passed for:

- official gross-based HUF item (`393.66`, quantity 3, net 1181, VAT 319, gross 1500);
- official 27% negative discount (`-2000/-540/-2540`);
- all 15 language tokens and all invoice VAT tokens/percentages listed in S9;
- five files each exactly 2,000,000 bytes accepted; sixth file and 2,000,001-byte file refused;
- checked date years 1, 2024 and 9999 emitted and valid against all four sources.

The fresh S18 PDF was fetched separately by the scratch script and converted with:

```sh
/nix/store/g0f2man6jdwimdpz383l8p11r1rzx9hs-poppler-utils-26.06.0/bin/pdftotext -layout /tmp/opencode/invoices-77d53c5-vat.pdf /tmp/opencode/invoices-77d53c5-vat.txt
```

The initial tool availability checks found no `xmllint` on PATH and no Python `lxml`; the installed Nix validator was then located and used successfully. These availability checks were not test failures. Cargo version was `1.98.0 (797e8a9bc 2026-08-05)`.
