# Current Számla Agent invoice-request compliance review

**Revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`  
**Reviewed / sources fetched:** 2026-09-10  
**Result:** **No confirmed current functional defect in this scope.** The previous Medium-severity derived-arithmetic finding is closed by this revision, with its failure cases independently rerun. Two unresolved vendor contradictions remain material: combined preview/simplified-image ordering and layout labels. Neither warrants an unsupported change to the writer.

## Scope and method

Reviewed the complete invoice creation request: all six `InvoiceKind` forms, credentials/settings, header, seller, buyer, postal address, both ledger blocks, all four carrier blocks, attachments, shared item calculations and domain wire tokens. Source references below are at the revision above; paths abbreviated `I`, `W`, `N`, `T`, `X` mean `crates/szamlazz-agent/src/ops/invoice.rs`, `ops/waybill.rs`, `number.rs`, `types.rs`, and `xml.rs`, respectively.

This is a current-implementation review against current official sources, not a diff-only review. The implementation inventory and initial official-source comparison preceded reading `docs/review/2026-09-10-agent-api-invoices.md`. That report was then consulted for closure, not used as proof of a current defect. No further delegation was performed for this scoped review.

HEAD matched the requested revision at the start and final pre-report check. `git diff HEAD -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md` was empty. Existing untracked reviews/research were preserved. Only this new report was added in the repository; scratch source was created with `apply_patch` under `/tmp/opencode/invoice-current-fbda137-review/`. No live Számla Agent calls, credential access, commits, or production/test-source edits were made. Documentation retrieval was unauthenticated GET only.

“Covered” means compared with the published contract and emitted representation, not proof of vendor execution or rendered-PDF correctness. Missing duplicate business validation is not automatically a client defect: the crate deliberately passes most content decisions to szamlazz.hu. Response parsing and transport are outside the primary scope, except for the invoice's multipart contribution and checked request construction.

## 1. Fresh official source register

The index [Generating invoice](https://docs.szamlazz.hu/agent/category/generating-invoice) and [Invoicing settings and rules](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) were fetched to enumerate the current surface. All EN/HU links in the following table were fetched independently. The combined documentation pages report build `v202608271632`, which is a site build, not a per-rule publication date.

| Label | Fetched URLs | Source rule / reviewed content |
|---|---|---|
| R | [EN request](https://docs.szamlazz.hu/agent/generating_invoice/request), [HU request](https://docs.szamlazz.hu/hu/agent/generating_invoice/request) | POST to `https://www.szamlazz.hu/szamla/`, `multipart/form-data`, `action-xmlagentxmlfile`, optional `attachfile1`…`attachfile5`. |
| S | [EN XML + inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU XML + inline XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Complete examples and schemas. “The order of the fields is fixed, they cannot be interchanged”; `minOccurs="0"` may be omitted. Every complex type, child name, sequence, cardinality, scalar type and language enumeration reviewed. |
| D | [Download XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Independently fetched, unmodified schema; kept distinct from inline sources and the locally patched fixture. |
| K | [EN document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) | Invoice default; prepayment/final/corrective/proforma flags; corrective reference; delivery layout; `eszamla`; proforma reference; one prepayment to exactly one final. |
| SI | [EN travel agency](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency) | Exact `simpleItems` case; false/omission normal mode; full NAV data; OSS/Hungarian-tax-number condition; 2-item/4-final-item limits; allowed VATs; inherited final state; corrective/delivery exclusions; errors 551–556. |
| V | [EN VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | Complete special/numeric token set; `eusAfa` means no Hungarian VAT, accepted true suppresses NAV submission; seller must be OSS registered or have non-Hungarian tax number; line VAT codes still required. |
| Q | [EN rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) | Net-first and gross-first calculations; whole HUF net/VAT/gross; eight server normalization cases; foreign fractional values permitted. |
| C | [EN currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/currencies) | Full supported list, HUF/Ft alias; foreign-currency invoice bank/rate requirement. |
| L | [EN templates/languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template) | Six layout tokens, omission means default; 15 languages; language also affects automatic email and buyer portal. |
| O | [EN order number](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number) | Optional query handle; account-controlled per-document-type duplicate check; storno/corrective exemptions; repeat requires matching buyer, gross, three dates and invoice age within two days. |
| DI | [EN discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/discount) | Negative net price, positive quantity, same VAT, separate row immediately after target; no separate percentage/total discount field. |
| E | [EN email notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification) | Email present + true/omitted send flag sends; false suppresses; comma recipients; BBCode/newlines; five attachments, 2 MB each; invalid attachments handled individually; test delivery goes to account email. |
| ER | [EN erasure codes](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | `torloKod` nonnegative integer, max 400 per item, account enablement, 537/539. |

Additional fetched linked/first-party sources:

- [Authentication](https://docs.szamlazz.hu/agent/basics/authentication): “either an Agent key (recommended) or a username and password”; legacy use of the same key in username/password is possible. Key case is significant.
- [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests): one XML per document, exact tag case, wrong tags may fail or be ignored. No automatic retry requirement inferred from order-number guidance.
- [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere), and its [2025-11-04 VAT guide PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf), page 1: KBAUK is new means of transport; EUT/KBAET and EUKT/EAM mappings; TAHK/ATK distinction from TAM; K.AFA subtype selected by exact wording in invoice comment/item name/item comment, with used goods as fallback. PDF bytes were freshly fetched and hashed; a pre-existing scratch PDF with that exact hash was read through the PDF tool without modification. The initial web text conversion returned binary PDF data, not readable content.
- [Erasure knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor): “a … `torloKod` mezőben az igényelt kódok darabszámát” (requested **count**), row tail, 400 maximum; `SzlaMost` required; uploaded stock or vendor-supplied codes.
- [Layout knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok): different labels for SzlaAlap/SzlaNoEnv than L; no data means recommended layout; postal service is UI-only. This is why postal **address** coverage does not imply a missing postal-service request field.
- [PHP download page](https://docs.szamlazz.hu/php/) and [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip): fetched and inspected in memory. `Header/InvoiceHeader.php:47–54` corroborates payment tokens; `398–404` corroborates template → preview → simple order. `SzamlaAgentSetting.php:77,116,258` describes aggregator as webshop-engine name; `340` corroborates settings order. PHP is corroboration, not a universal authority over the schemas or live service.

### Acquisition hashes

Inline hashes identify HTML-decoded `<pre>` schema text without reformatting or an added newline. Download hashes identify raw bytes. These were recalculated in this review, not copied as evidence from the prior report.

| Artifact | SHA-256 |
|---|---|
| Download invoice XSD | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| EN inline invoice XSD | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| HU inline invoice XSD | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| PHP ZIP | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| VAT guide PDF | `bb5a52eda87e383be3276870fee937c6a34d87f7a2a3542a03a0bf90a63d9465` |

## 2. Current findings and prior closure

### Confirmed functional findings: none

No new demonstrated missing invoice-request field, wrong ordinary sequence, malformed ordinary lexical value, token mismatch, attachment truncation, or incorrect successful derived calculation was identified. This does not establish all possible inputs/server combinations as correct.

### Prior INV-01 — Medium, now CLOSED: silent intermediate precision loss

**Prior impact:** Decimal checked operations could return a rounded/underflowed successful value before the explicit rounding policy, including `Rounding::Exact`. The old report described a one-cent double-rounding case, underflowed net/VAT and lost fractional gross.

**Current source:** `item.rs:191–211` now uses `number::exact_mul`, `exact_div_100` and `exact_add`; `number.rs:6–59` uses normalized integer coefficients and checked representability. The public error contract at `item.rs:176–182` explicitly includes unrepresentable intermediates, precision loss and underflow, even when later rounding could fit. This is a conservative, documented boundary, not a promise of arbitrary-precision final-result calculation.

**Exact official rules relevant to closure:** S: “all amounts shown on the invoice must be provided explicitly”; Q: multiply price by quantity, round net, calculate and round VAT, then add net and VAT. The stronger promise of no rounding inside Exact is the crate's own (`item.rs:24–30`). A server's rounding cannot justify client-side silent loss before transmission.

Independent scratch repro results against current path dependency, `rust_decimal = 1.43.0`:

| Input | Policy | Current result |
|---|---|---|
| price `0.005`, quantity `0.9999999999999999999999999999`, AAM | Exact | `Err(NetOverflow)` |
| Same input | Scale(2) | Error, not the old incorrect `Ok(0.01)` |
| price `1e-28`, quantity `0.1`, AAM | Exact | `Err(NetOverflow)` |
| price `1e-28`, quantity `1`, 27% | Exact | `Err(VatOverflow)` |
| price `1e28`, quantity `1`, percentage `1e-28` | Exact | `Err(GrossOverflow)` |
| price `100`, quantity `1`, 27% | Scale(0) | net `100`, VAT `27`, gross `127` |
| price `-2000`, quantity `1`, 27% | Scale(0) | net `-2000`, VAT `-540`, gross `-2540` (DI example) |

The tiny synthetic VAT rate tests the general local arithmetic invariant; it is not claimed to be a vendor-accepted rate. Existing `item.rs:232–295` tests independently cover refusal and exact representable boundary products, including cross-operand cancellation of powers of ten. All passed. The prior report's “fix INV-01” recommendation is therefore obsolete for this revision.

**Other prior notes:** item net/VAT field docs now say “subject to rounding” (`item.rs:105–108`), closing that wording issue. Prior schema-order/layout contradictions still reproduce, below. Aggregator, taxpayer `-1`, test-email and blanket absent-boolean descriptions remain limited semantic-doc opportunities, not reproduced request failures.

## 3. Complete request field/order/default coverage

Fresh EN/HU schemas each have **118 child declarations in named complex types**, plus six root-block declarations. The download has 116, missing group id and erasure count. Counts include containers and mutually exclusive credential/kind slots, not the number emitted by one request.

All lists below retain schema order. `?` means optional and absent by default unless stated. Optional strings preserve `Some("")` as a present empty element; optional bools preserve explicit false. Required data is caller-supplied, not silently filled with today's dates or a chosen currency/language.

### Kinds and root

| Form | Wire and source | Assessment |
|---|---|---|
| Invoice | No true kind flag; optional `dijbekeroSzamlaszam` before flags. I:34–40, 772–779. | K/S/D covered. |
| Proforma | `dijbekero=true`, I:41–43,794. | K covered. |
| DeliveryNote | `szallitolevel=true` plus forced `SzlaFuvarlevelesAlap`, I:44–46,795,811–818. | S/D flag plus K layout. Override explicitly documented at I:193–196. |
| Prepayment | `elolegszamla=true`; optional proforma reference, I:47–59,780. | S/D coverage. Explicit-reference execution remains unverified; not confused with observed implicit linking. |
| Final | `vegszamla=true`, `elolegSzamlaszam?`; optional proforma reference earlier, I:60–78,781–788. | Requires nonblank prepayment number or order number, I:686–701. Exactly one reference slot matches one-prepayment model. |
| Corrective | `helyesbitoszamla=true`, `helyesbitettSzamlaszam`, I:79–84,790–793. | K covered; string presence by construction does not prove nonempty content or existence. |

Root: XML 1.0 UTF-8 `xmlszamla` in `http://www.szamlazz.hu/xmlszamla`; sequence **beallitasok → fejlec → elado → vevo → fuvarlevel? → tetelek** (I:738–920; X:19–40). Required seller container remains present even empty. One XML represents one document. `xsi:schemaLocation` and its namespace declaration in the sample are optional schema hints, not omitted business fields. Item vector order is preserved.

The enum intentionally does not expose arbitrary combinations of independent XSD booleans or proforma references on corrective/proforma/delivery-note forms (I:21–30,104–116). Schema independence alone does not prove a meaningful accepted operation is missing.

### Settings — `beallitasok`

| Ordered XML field | Model/default and writer | Assessment |
|---|---|---|
| `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | Credentials writes key **or** username/password; X:456–465, I:740. | Authentication/S/D agree; supplied case preserved. |
| `eszamla` | `e_invoice=false`, always written; I:532–539,604,741. | Paper/e-invoice selection correct. |
| `szamlaLetoltes` | `download_pdf=false`, always written; I:540–542,605,742. | Valid local default; no need to copy sample true. |
| `szamlaLetoltesPld?` | `Option<u8>`; I:543–547,743–745. | S annotates obsolete/ignored. Narrower than int but no demonstrated current capability loss. |
| `valaszVerzio` | Always shared constant `2`; I:746, `ops.rs:27–31`. | Documented structured mode deliberately selected. |
| `aggregator?` | String; I:548–549,747. | Correct slot/type; semantic wording caveat below. |
| `guardian?` | Bool; I:550–551,748–750. | Correct slot/type; operational meaning underspecified in sources. |
| `cikkazoninvoice?` | Bool; I:552–553,751–753. | Correct slot/type; explicit false retained. |
| `szamlaKulsoAzon?` | String; I:554–556,754. | Query handle preserved; no uniqueness guarantee implied. |

### Header — `fejlec`

| Ordered XML field(s) | Model/default and writer | Assessment |
|---|---|---|
| `keltDatum?` | `issue_date: Option<Date>`; I:141–150,758. | S optional; bounded date-replacement caveat preserved. |
| `teljesitesDatum`, `fizetesiHataridoDatum` | Required fulfillment/due Date; I:151–156,759–760. | HU meaning correct; EN sample's “payment date” for fulfillment is a translation error. |
| `fizmod`, `penznem`, `szamlaNyelve` | Required PaymentMethod/Currency/Language; I:157–162,761–763. | Token inventory below. |
| `megjegyzes?` | String; I:163–164,764. | Free text including K.AFA information supported. |
| `arfolyamBank?`, `arfolyam?` | Optional ExchangeRate: bank string, optional Decimal; I:165–166,765–770. | Foreign currencies require specification; MNB omission exception honored. |
| `rendelesSzam?` | String; I:167–169,771. | O optional query/order handle, not unconditional deduplication. |
| `dijbekeroSzamlaszam?`, `elolegszamla?`, `vegszamla?`, `elolegSzamlaszam?`, `helyesbitoszamla?`, `helyesbitettSzamlaszam?`, `dijbekero?`, `szallitolevel?` | Selected by kind, I:772–796. | All eight schema slots represented on applicable forms. Only selected true flags emitted. |
| `logoExtra?`, `szamlaszamElotag?` | Optional extra_logo/number_prefix; I:170–175,797–798. | Correct fields; registration/upload are account concerns. |
| `fizetendoKorrekcio?` | Decimal payable_adjustment; I:176–177,799–801. | Preserved, no automatic gross-total alteration. |
| `fizetve?` | `paid=false`; only true emitted; I:178–180,802–804. | Explicitly documented emission choice; explicit false unavailable. |
| `arresAfa?`, `eusAfa?` | Optional margin_vat/eu_vat bools; I:181–192,805–810. | Full wire coverage; V restrictions documented for eusAfa. |
| `szamlaSablon?` | Optional template, except forced delivery layout; I:193–196,811–818. | Six tokens + open token; omission distinct from SzlaAlap. |
| `simpleItems?`, `elonezetpdf?` | Optional simple_items/preview_pdf; I:197–225,819–826. Writer reverses this inline order. | Known D/PHP versus S contradiction; see §5, not an unqualified compliance pass. |

Header/new-request defaults are explicit at I:228–259,596–618. Foreign-rate gate I:719–729 requires bank nonblank and already trimmed; absent rate allowed only for exact bank `MNB`. `Currency::is_huf` exempts HUF/Ft case-insensitively while preserving the sent spelling. Whether all case variants are accepted server-side was not newly tested.

### Seller, buyer, postal and buyer ledger

| Ordered XML fields | Representation / writer | Assessment |
|---|---|---|
| Seller `bank?`, `bankszamlaszam?`, `emailReplyto?`, `emailTargy?`, `emailSzoveg?`, `alairoNeve?` | Seller optional fields, nested SellerEmail flattened; I:262–275,828–837; T:1022–1032. | All six covered; account data supplies omissions. No seller identity replacement field in schema. BBCode and newline body transport supported. |
| Buyer `nev`, `orszag?`, `irsz`, `telepules`, `cim` | Four required strings, optional country; I:296–306,840–844. | Correct type/order; empty required strings still emitted, business validity left to server. |
| `email?`, `sendEmail?` | String and tri-state bool; I:307–315,845–848. | E matches: omitted/true sends when email filled, false suppresses. Multiple recipients possible. |
| `adoalany?`, `adoszam?`, `csoportazonosito?`, `adoszamEU?` | TaxpayerStatus and three strings; I:316–324,849–854. | S covered; D's missing group field is source drift. |
| `postazasiNev?`, `postazasiOrszag?`, `postazasiIrsz?`, `postazasiTelepules?`, `postazasiCim?` | Five optional PostalAddress strings; I:277–291,855–861. | Correctly flattened, no invented postal container; partial addresses representable. |
| `vevoFokonyv?` with `konyvelesDatum?`, `vevoAzonosito?`, `vevoFokonyviSzam?`, `folyamatosTelj?`, `elszDatumTol?`, `elszDatumIg?` | Date/string/string/bool/date/date; I:120–135,862–873. | All six fields and empty-present container covered; explicit false retained. |
| `azonosito?`, `alairoNeve?`, `telefonszam?`, `megjegyzes?` | Buyer id/signer/phone/comment; I:329–347,874–877. | Correct order. Partner id warning covers partner update and portal access consequences, distinct from queried numeric id. |

### Items and item ledger

| Ordered XML fields | Representation / writer | Assessment |
|---|---|---|
| `megnevezes`, `azonosito?` | Name/optional id; `item.rs:91–94`; I:885–886. | Covered. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs` | Decimal quantity/string unit/Decimal price/VatRate; `item.rs:95–102`; I:887–890. | Fractional quantity/price, signed price and exact special tokens supported. |
| `arresAfaAlap?` | Decimal margin_vat_base; `item.rs:103–104`; I:891–893. | Emitted before net, not dropped. Caller computes margin amounts. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | Explicit Decimals; `item.rs:105–110`; I:894–896. | All three always sent, zero and negative included; current derived arithmetic no longer silently loses precision. |
| `megjegyzes?` | String; I:897. | Preserved; PDF truncation in some layouts is different from transport loss. |
| `tetelFokonyv?`: `gazdasagiEsem?`, `gazdasagiEsemAfa?`, `arbevetelFokonyviSzam?`, `afaFokonyviSzam?`, `elszDatumTol?`, `elszDatumIg?` | Four strings and two Dates; `item.rs:52–72`; I:898–913. | Complete and ordered. |
| `torloKod?` | `Option<u32>` count; `item.rs:115–131`; I:914–916. | Last in row; 0–400 gate I:703–710. Account/layout conditions documented, not locally looked up. |
| `tetelek/tetel` | Vec with minimum one via validate/to_wire, I:682–685,882–919. | Unbounded schema repetition; discount adjacency preserved. |

### Waybill — `fuvarlevel`

| Ordered fields | Model/writer | Assessment |
|---|---|---|
| `uticel?`, `futarSzolgalat?`, `vonalkod?`, `megjegyzes?`, then `tof?`, `ppp?`, `sprinter?`, `mpl?` | Waybill W:94–112,133–177. | All slots covered. Destination documented unused; general barcode fallback documented. |
| TOF `azonosito?`, `shipmentID?`, `csomagszam?`, `countryCode?`, `zip?`, `service?` | W:11–24,137–148. | Exact case/order. Id string preserves leading zeroes; parcel count u32 checked against XSD int maximum. |
| PPP `vonalkodPrefix?`, `vonalkodPostfix?` | W:28–33,149–154. | Strings preserve vendor 3-character prefix / max-7 suffix; lengths caller-owned. |
| Sprinter `azonosito?`, `feladokod?`, `iranykod?`, `csomagszam?`, `vonalkodPostfix?`, `szallitasiIdo?` | W:37–50,155–166. | Vendor 3-character id, 10-digit sender code, 7–13 suffix supported. Parcel gate shared with TOF (I:711–718; W:118–129). |
| MPL `vevokod`, `vonalkod`, `tomeg`, `kulonszolgaltatasok?`, `erteknyilvanitas?` | W:57–84,167–177. | Three required strings always emitted; weight correctly string, optional declared value Decimal. |

Carrier tokens `TOF`, `PPP`, `SPRINTER`, `FOXPOST`, `MPL`, `GLS`, `EMPTY` all fit the carrier string. FOXPOST/GLS/EMPTY have no missing dedicated child type in the schema. Multiple subblocks are allowed by its sequence, not a choice. All optional fields default absent; MPL intentionally has no Default. Actual barcode/rendering eligibility with partial subblocks was not executed.

### Attachments and XML fidelity

- I:379–516 implements a collection bounded to five files and decimal **2,000,000 bytes per file**, including serde and Vec conversion. The MB interpretation is expressly qualified. No mutable slice exposes a way to enlarge a stored attachment beyond the bound.
- I:938–948 contributes exactly `attachfile1`…`attachfile5` with filename, MIME and raw bytes. `wire.rs:66–125` frames multipart, avoids boundary collisions with XML/file content, and escapes/removes header metacharacters. I:1249–1280 asserts a complete two-attachment body; it passed.
- Empty collection is the default. Attachments are not XML children. Sending them with `sendEmail=false` is not a defect: E says the **server** ignores them. Rejecting oversized files locally is a conservative boundary, not a claim that a vendor error would prevent invoice creation.
- X:414–453 uses escaped text, plain locale-independent decimal notation, Date formatting and true/false booleans. None omits; Some-empty remains present. Scratch text `A&B <tag> "quote" 'apostrophe' ]]> é\r\nnext` recovered unchanged through an independent XML parser.
- `wire.rs:402–429` validates XML 1.0 characters after serialization; I:1496–1502 rejects NUL through `to_wire`. Lower-level `write_xml` does not promise the checked request boundary. Ordinary dates passed real XSD validation; astronomical/year-zero dates were not treated as established business requirements.

## 4. Shared domain tokens and arithmetic boundaries

| Surface | Reviewed coverage |
|---|---|
| VAT special codes | T:186–248,266–324 represents every V invoice token: `TAHK TAM AAM EUT EUKT F.AFA K.AFA HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET`. PDF page 1 supports KBAUK's new-means-of-transport meaning and TAHK versus TAM. K.AFA exact phrases/default documented at T:203–210. |
| VAT numeric values | All listed rates fit Decimal: `0,1,2,2.1,3,4,4.8,5,5.5,6,7,7.7,8,8.1,9,9.5,10,11,12,13,13.5,14,15,16,17,18,19,20,21,22,23,24,25,25.5,26,27`. Percent normalizes trailing zeroes; Other preserves original spelling. |
| Numeric Other arithmetic | `item.rs:194–207`, N:63–137 parses finite numeric tokens including XML whitespace/scientific spelling. Out-of-domain numeric rates error; they do not silently become zero-VAT special codes. Tests `numeric_fidelity.rs:15–46` passed. Scientific/whitespace acceptance by the vendor was not inferred from this local parsing. |
| Open tokens | Unknown VAT/payment/template values are intentionally representable. Shared receipt tokens ÁKK/EU/EUK/MAA are labelled receipt vocabulary (T:238–246), not guaranteed invoice VATs. No missing runtime allowlist is reported. |
| Payment methods | T:588–687 matches PHP's `átutalás`, `készpénz`, `bankkártya`, `csekk`, `utánvét`, `PayPal`, `SZÉP kártya`. `Other("OTP Simple")` supports the additional PHP constant without new enum sugar. |
| Currency | T:369–437 preserves every C token, including historical EEK/HRK/LTL/LVL and vendor spelling `KSH`; no limited enum blocks them. Named constants are conveniences. |
| Language | T:480–585 covers exactly S/L's 15: `hu en de it ro sk hr fr es cz pl bg nl ru si`. Do not change vendor `cz`/`si` to ISO `cs`/`sl`. |
| Taxpayer status | T:690–755 covers S's `7` non-EU, `6` EU, `1` Hungarian tax number, `0` unknown, `-1` no tax number. String serialization is the local JSON representation; writer emits int lexical text. |
| Invoice templates | T:983–1020 exposes all six L/D tokens plus Other. Default means explicit `SzlaAlap`, not omission. Label contradiction below does not change wire coverage. |
| Document/number tokens | T:22–57 preserves invoice-number references as supplied. T:768–800 maps queried `SZ/D/ES/VS/HS/SS/SL` consistently with behaviour notes; creation selects flags, not `tipus`. No request number validator is promised. |
| Exchange rate | T:943–980 exposes explicit bank/Decimal or `MNB`/absent automatic rate; the specific invoice annotation in both S and D supports it. |
| Rounding | `item.rs:9–49,163–223` applies half away from zero to net before VAT, then exact sum. HUF/Ft whole-forint, EUR cents, KWD thousandths etc. are explicitly local choices (T:412–435), not claims of vendor storage precision. |
| Explicit/gross-first amounts | `LineItem::new` (`item.rs:133–161`) preserves Q's B2C example: quantity 3, net price 393.66, net 1181, VAT 319, gross 1500. A gross-price-derived convenience is absent but the capability exists. |

Representability review: N:6–30 cancels decimal powers of ten before coefficient multiplication, including factors split across operands; N:33–35 divides by 100 by increasing scale; N:37–48 aligns scales before checked addition; N:50–59 strips trailing zeroes and refuses excessive scale/coefficient. Every successful derived step therefore has an exact Decimal representation before requested rounding. Refusing an oversized **intermediate** even if division or later rounding could make the final result fit is an intentional contract limit. `Decimal` is narrower than XSD double, including no NaN/INF; no meaningful missing invoicing capability was established from that alone.

## 5. Vendor contradictions, intentional boundaries and justified live deviations

### VC-1 — Combined `elonezetpdf` / `simpleItems` sequence: unresolved vendor contradiction

**Potential impact: Medium interoperability risk; not a confirmed Rust defect.** Confidence high that sources contradict; actual server consequence unverified.

- S's current EN/HU `fejlecTipus`: **szamlaSablon → simpleItems → elonezetpdf**.
- D and freshly fetched PHP `Header/InvoiceHeader.php:398–404`: **szamlaSablon → elonezetpdf → simpleItems**.
- I:819–825 follows D/PHP deliberately; I:220–222 warns of the disagreement.

**Concrete offline repro:** a rich request with both `Some(true)` fields produces `<szamlaSablon>SzlaMost</szamlaSablon><elonezetpdf>true</elonezetpdf><simpleItems>true</simpleItems>`. libxml2 validates the header against D, while each inline schema rejects `simpleItems` as unexpected after preview. Removing only `simpleItems` from each synthetic instance makes all six kinds validate against either inline schema. Booleans set to false still have the same sequence issue when present; existing `simple_items.rs` tests all nine presence/value pairs over all six kinds.

S explicitly says order cannot be interchanged, making this real schema nonconformance to **those sources**, not proof that the deployed server rejects or ignores the request. Switching order would simply reverse which source is violated. Preserve the documented choice pending vendor clarification or an authorized preview/no-issuance probe. No such probe ran here.

### VC-2 — Download lacks buyer group and item erasure fields

**Potential impact: Low tooling/drift risk; supported capability, not a code defect.** D omits `csoportazonosito` and `torloKod`; S includes both and ER explicitly supports erasure count. Full generated instances fail D with “Element … csoportazonosito … not expected” and “Element … torloKod … not expected”. Removing only these two fields from the **synthetic instances** yields 6/6 D passes; vendor schema untouched.

Preserve both fields at I:853 and914–916. `fixtures/SOURCES.md:126–140` honestly records the hand-patched cached invoice schema; it is not an unmodified download. No universal inline-versus-download precedence rule is justified.

### VC-3 — Layout names disagree

**Potential impact: Low display-selection ambiguity; not a confirmed token defect.** L maps SzlaAlap to traditional and SzlaNoEnv to envelope-friendly. Its linked knowledge base says `Tradicionális: … SzlaNoEnv` and `Borítékbarát: … SzlaAlap`. T:991–1017 follows L. The same knowledge-base snippet lowercases `szamlasablon` and the delivery token, unlike S's exact case. No token swap/case change recommended without clarification; PDF rendering not tested.

### Other source tensions

- S says all fields shown in the example are mandatory, then explicitly permits `minOccurs="0"` omission; its sample contains numerous such optional fields. Required/optional writer behavior follows the actual schema, not that blanket sentence.
- C requires bank **and** rate for foreign currency, but S/D explicitly say omitted rate with bank MNB selects current automatic MNB rate. The specific exception justifies `automatic_mnb()` for invoices.
- Q EN says B2B/B2C “have to” use net/gross-first; HU says “valószínűleg” (probably). The library offers net-first derivation and explicit totals, so no practical request capability is missing. Q also documents server normalization of fractional HUF amounts; local rejection of all fractional HUF invoices is not required.
- V's terse KBAUK/ET labels should not override its linked detailed VAT PDF; the current Rust meaning of KBAUK is supported by the PDF, not a defect caused by expanding “UK”.

### Live-tested deviations preserved

Evidence is `docs/szamlazz-hu-behaviour.md`, one TEST account on recorded September dates; raw probe logs are explicitly not in the repository (lines 3–24). These are bounded observations, not universal guarantees or fresh execution in this review.

| Observation | Evidence / current treatment |
|---|---|
| Requested create issue date may become today | P48-P5, behaviour:91; I:141–148 correctly says request, not guarantee. Do not add an unsupported “always rejected” rule. |
| Final does not automatically deduct prepayment | C6-2, behaviour:117–119; I:62–70 tells caller to supply negative same-VAT deduction line. |
| Proforma link may be implicit or silently dropped | C1-3, C2-3/C2-6, D4/D5, behaviour:104–108. Explicit ES/VS proforma references remain unverified at behaviour:234–240 and I:50–54,69–70. Preserve supported fields without overstating probes. |
| External ids are nonunique/newest-holder handles | behaviour:63–71. No request-side uniqueness assumption is introduced. |
| Net check has tolerance; EUR monetary values round independently server-side | P60, behaviour:159–161. Whole/minor-unit derivation is sensible; explicit values and Exact remain deliberate choices, with warning. Tiny coefficient repros concern client arithmetic, not this server behavior. |
| Numeric VAT accepts 27.00/27.0 | P60-V1/V2, behaviour:162; T:259–264 correctly calls normalization hygiene. |
| Buyer data can change with later partner updates | D6, behaviour:111; I:329–339 documents partner-id semantics. No false “immutable buyer snapshot” promise introduced. |
| Duplicate replay conditions differ from naïve request-byte identity | behaviour:33–57; O's dates/age caveat retained. Create date replacement explains why a changed requested kelt could still replay. No new idempotency guarantee inferred. |

### Intentional boundaries / non-findings

No automatic final deduction, discount computation, gross-price constructor, arbitrary mixed kind flags, response-version-1 mode, or explicit false `fizetve`. Caller-supplied line amounts cover the actual discount/final/gross-first cases. Account configuration, certificate and prefix registration, legal tax choice, logo upload, carrier barcode generation and seller identity are outside the schema/request's job.

`simpleItems` business eligibility, reference existence, date chronology, taxpayer/country consistency, erasure account/layout enablement and carrier identifier lengths are documented but mainly server-validated. I:199–218 covers all principal SI rules; `tests/simple_items.rs:72–73` deliberately accepts content-invalid combinations for wire testing. Neither those tests nor schema validity claims that the combinations can legally issue. Requiring exchange-rate specification for every foreign kind, even a non-taxable proforma/delivery note, remains the conservative known boundary at behaviour:216–223.

### Small semantic-doc opportunities (not functional finding count)

- **Aggregator:** I:548 calls it “for contracted integrations”; PHP explicitly calls it webshop-engine name, with WooCommerce/OpenCart/PrestaShop examples. The field works; a contractual prerequisite was not established. Guardian's contract-only description is similarly uncorroborated by the inspected schema. Prefer qualified descriptions over invented operational guarantees.
- **Taxpayer -1:** T:703 adds “private individual” to the schema's broader “no tax number”. The correct token/variant exists; no runtime classifier was found. The PDF also discusses organizations without tax numbers, so the parenthetical is not exhaustive advice.
- **Test email:** I:307–313 describes normal recipient selection. E expressly says test accounts redirect to the configured account email. The existing D6 speculation about test email failures is not evidence that test accounts never send mail.
- **Absent flags:** `ops.rs:18–21` says absence is “exactly false” on the server. True-only paid emission is intentional, but no reviewed source proves all account/payment-method effects equivalent to an explicit false. Narrower emission wording would be more defensible.

## 6. Tests, fixture provenance, commands and limits

### Executed commands/results

```text
git status --short && git rev-parse HEAD
git show --stat --oneline fbda137e79dc8f5a40016ee03cd5997ed4e0ea78
git diff HEAD -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md
cargo test --locked -p szamlazz-agent --lib --test numeric_fidelity --test simple_items --test upstream
python3 -c 'import lxml.etree; print("lxml available")'
python3 /tmp/opencode/invoice-current-fbda137-review/check.py
git rev-parse HEAD && git status --short
```

- Git: requested HEAD confirmed; relevant tracked diff empty; initial unrelated untracked work preserved.
- Final post-report verification: HEAD still `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`; the same in-scope tracked diff remained empty and `git diff --check` passed. Concurrent unrelated edits appeared in the Restate worker contract/e2e files, new recovery files and `docs/design/order-write-protocol.md`; this review did not edit or revert them. The report is untracked, so the ordinary diff check does not itself validate its content.
- Cargo: **185 unit + 6 numeric-fidelity + 2 simple-items + 11 upstream = 204 passed**, zero failed/ignored. No live test target selected. Unit suite includes invoice settings/kinds, attachment exact body/bounds, XML-character refusal, final-reference check, parcel int boundaries, automatic MNB and arithmetic regression/control cases.
- Python lxml probe failed with `ModuleNotFoundError`; the independent checker instead called installed **libxml2's real XSD validator** through ctypes. This is full schema validation, not just a home-grown name/order comparator.
- Checker runs `cargo run --quiet --offline --manifest-path /tmp/opencode/invoice-current-fbda137-review/Cargo.toml`, parses its JSON stdout, fetches the three current schemas in memory, prints hashes and child counts, validates six rich generated requests against each schema, and asserts known-conflict failures plus passes after source-specific conflict isolation. Scratch Cargo pins rust_decimal 1.43.0; no repo lock/source modification.
- Full request results were `[1871,1871,1871,1871,1871,1871]` for each source (libxml2 unexpected-element validation errors described in §5). Conflict-isolated results were `[0,0,0,0,0,0]` for each, **18 successful full-XSD controls**. Only synthetic instance fields were removed; schemas were never patched/merged. All other rich optional blocks remained populated. These deliberately artificial combinations test shape, not account/tax eligibility.
- Arithmetic: five old failure/policy cases now refuse as expected; positive and negative controls pass. Escaped text including CRLF is recovered unchanged independently.
- Supplemental in-memory Python GET/ZIP inspection and SHA-256 commands confirmed PHP sequence/payment/settings rules and PDF identity listed in §1. The PDF was read using the dedicated PDF reader after binary web conversion proved unsuitable.

### What existing fixtures/tests establish

`fixtures/SOURCES.md:15–39` distinguishes workspace-only official corpus from packaged synthetic/golden fixtures. Official request examples originate in July; current fields/rules were fetched anew. The cached invoice XSD is explicitly patched (`126–140`). September source-tail excerpts are not complete original schemas (`193–228`). Golden output is project-authored, not independent vendor truth.

`tests/upstream.rs:1022–1114` reconstructs request examples with declared differences: response version 2 instead of 1, lowercase payment token, omitted false flags. Its outline trims text and ignores empty containers (`1225–1234`); a passing outline does **not** prove empty/omitted semantic equivalence. `simple_items.rs:20–112` checks actual header order and option states across all six kinds while retaining amounts/group/erasure fields. The fresh libxml2 checks add independent schema evidence beyond these tests.

### Limits

No live requests, rendered PDF/barcode inspection, email delivery, account-toggle manipulation, or full workspace/client-feature matrix was run. Authentication alternatives were inspected; the fresh rich executable used an agent-key placeholder. Validity for every combinatorial partial block, arbitrary date, unknown wire token, or vendor account policy is not established. Raw historical account logs were unavailable; recorded observations were preserved with their stated scope. Documentation and downloads disagree, so even a full-XSD pass cannot establish deployed server behavior. Receipt-specific calculations and response/transport conclusions remain outside this invoice-focused report.

**Disposition:** close prior INV-01 at this revision; retain the documented request implementation and justified observed behavior. Seek vendor resolution of combined-preview order and layout labels before changing either. No new functional change is justified by the verified evidence in this scope.
