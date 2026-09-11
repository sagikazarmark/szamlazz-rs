# Számla Agent invoice-request API compliance review

**Reviewed HEAD:** `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`

**Review and fresh source acquisition:** 2026-09-11

**Result:** **No confirmed actionable functional defect in this scope.** Every current invoice-request schema field is representable on its applicable request form. The significant remaining interoperability question is the conflicting official order of `simpleItems` and `elonezetpdf`; it is not evidence for an unconditional writer change. The historical arithmetic precision-loss defect does not reproduce at this HEAD.

## Scope and evidence standard

This is the invoice-request slice of the parallel review: all six invoice kinds, settings/credentials, header, seller, buyer/postal/buyer ledger, four carriers, items/item ledger, attachments, request vocabulary and shared derived arithmetic. No further delegation was performed. Source locations below are relative to `crates/szamlazz-agent/src/`: **I** = `ops/invoice.rs`, **W** = `ops/waybill.rs`, **T** = `types.rs`, **A** = `item.rs`, **N** = `number.rs`, **X** = `xml.rs`.

Current implementation and the primary invoice pages were inspected before consulting the historical `2026-09-10-agent-api-current-invoices.md`. Its conclusions were not carried forward as proof. Its scratch Rust/Python checker was subsequently read in full and rerun against the **current path dependency and freshly retrieved schemas**; its historical directory name does not identify the code tested today.

“Covered” means compared with published field/sequence/content rules and the actual implementation, with offline execution where recorded. It does not mean exercised against the vendor. A missing duplicate business-rule validator is not by itself a client bug: this crate deliberately delegates most content validation to szamlazz.hu. Account eligibility, rendering, barcode generation and email delivery require evidence beyond XML validity.

The in-scope tracked diff against HEAD was empty. Existing unrelated work was preserved. The only repository file created by this review is this report. Scratch acquisition code was added with `apply_patch` under `/tmp/opencode`; no production/test source was edited, no account credentials were read, and no live-account request was made. Documentation and artifact acquisition used unauthenticated GETs only. The full crate suite belongs to the lead review; the focused results below are the checks actually executed here.

## 1. Fresh official source register

The [settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) was fetched and all **ten** linked settings/rules pages were fetched independently. Documentation pages report site build `v202608271632`, not the date of every rule. HU counterparts were fetched where they help resolve wording/sequence differences.

| ID | Freshly fetched source | Contract checked |
|---|---|---|
| R | [Generating invoice / request](https://docs.szamlazz.hu/agent/generating_invoice/request) | POST to `https://www.szamlazz.hu/szamla/`, multipart field `action-xmlagentxmlfile`, attachments `attachfile1`…`attachfile5`. |
| S | [EN XML + inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU XML + inline XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Complete field inventory, fixed sequence, scalar types, cardinality, examples, partner-id behavior. |
| D | [Download invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Full schema independently downloaded; never patched/merged for this review. |
| RS | [Generating invoice / response](https://docs.szamlazz.hu/agent/generating_invoice/response) | Request version 2 selects `xmlszamlavalasz`, requested PDF is base64, response metadata/preview context. Response parser audit is outside this slice. |
| K | [Document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types) | Six creation kinds; paper/e-invoice, references, one prepayment to exactly one final; separate storno operation. |
| SI | [Tour operators / simpleItems](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Case-sensitive optional field, full NAV amounts, account conditions, limits, inherited final state, unsupported corrective/delivery forms. |
| V | [VAT rates EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | Special/numeric rates; `eusAfa` suppresses NAV submission when accepted, with OSS/non-Hungarian-seller restriction. |
| Q | [Rounding EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) | Net-first/gross-first HUF arithmetic; whole row totals, fractional unit price, eight server-normalization cases; foreign fractions. |
| C | [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) | Complete supported list, HUF/Ft aliases, foreign bank/rate requirement. |
| L | [Invoice template and languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) | Six layouts and fifteen language tokens; language also affects email and buyer portal. |
| O | [Order number and duplicate checking](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Account toggle, per-kind check, exemptions, conditional two-day replay; not unconditional idempotency. |
| DI | [Discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount) | Negative unit price, positive quantity, same VAT, adjacent separate item; no percentage/total discount field. |
| E | [Email notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Email plus omitted/true send flag; explicit false suppresses; comma-separated recipients; BBCode/newlines; five 2 MB files; test-account redirection. |
| ER | [Data erasure codes](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Nonnegative integer count, maximum 400, enablement, 537/539. |
| AU | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Key or user/password; key lowercase/case sensitivity; legacy key in both user/password fields; one account per login. |

Additional linked first-party sources freshly fetched and inspected:

- [Erasure knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor): `torloKod` is the requested **count**, last in each item; `SzlaMost` required; uploaded stock or vendor-provided codes. The lowercase `szamlasablon` in its prose is not used over the schema's case-sensitive `szamlaSablon`.
- [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere) and its [2025-11-04 PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf), page 1: KBAUK is new means of transport, KBAET is intra-community exempt goods, TAHK differs from TAM, K.AFA subtype depends on exact phrases in comments/name. The PDF was freshly downloaded and read with the PDF reader.
- [Layout knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok): labels conflict with L; recommended layout default; postal service is UI-only; narrow layouts may truncate displayed comments, which is distinct from request truncation.
- [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip): freshly downloaded and inspected in memory. `Header/InvoiceHeader.php:47–54` lists payment tokens; `398–404` orders template → preview → simple items. `SzamlaAgentSetting.php:77,116,258` describes aggregator as webshop-engine name; `340` confirms settings order. PHP corroborates specific behavior; it is not universally preferred over other vendor sources.

Fresh artifact hashes (inline hashes are decoded `<pre>` schema text without added newline):

| Artifact | SHA-256 |
|---|---|
| Download invoice XSD, 16,772 bytes | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| EN inline XSD | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| HU inline XSD | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| PHP ZIP, 145,183 bytes | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| VAT PDF, 39,139 bytes | `bb5a52eda87e383be3276870fee937c6a34d87f7a2a3542a03a0bf90a63d9465` |

## 2. Findings and historical closure

### Confirmed actionable functional findings: none

No demonstrated missing applicable request field, wrong ordinary field order/token, lost attachment/item data, or incorrect **successful** derived calculation was found. Source disagreements and conservative local policies are explicitly qualified below rather than counted as proven server failures.

### Historical arithmetic precision loss: remains closed at current HEAD

**Current locations:** A:176–211; N:6–59. The old failure involved Decimal operations returning rounded or underflowed successful intermediates before the selected rounding policy. Current code uses integer-coefficient exact multiplication, division by 100 and addition, and documents refusal whenever an intermediate cannot fit exactly, even if later rounding could fit.

**Official rule:** S requires all row amounts, not server calculation from omitted values. Q requires multiplying unit price by quantity, rounding net, deriving/rounding VAT, then summing. The stronger no-hidden-rounding guarantee is the crate's own `Rounding::Exact` contract.

**Reproduction actually rerun against HEAD:**

| Quantity / net unit price / VAT | Rounding | Result |
|---|---|---|
| `0.9999999999999999999999999999 / 0.005 / AAM` | Exact | `Err(NetOverflow)` |
| Same | Scale(2) | Error, rather than the historical incorrect `Ok(0.01)` |
| `0.1 / 1e-28 / AAM` | Exact | `Err(NetOverflow)` |
| `1 / 1e-28 / 27%` | Exact | `Err(VatOverflow)` |
| `1 / 1e28 / 1e-28%` | Exact | `Err(GrossOverflow)` |
| `1 / 100 / 27%` | Scale(0) | `100 / 27 / 127` |
| `1 / -2000 / 27%` | Scale(0) | `-2000 / -540 / -2540`, matching DI |

These are local representability tests, not vendor acceptance evidence for synthetic tiny rates. The source audit also checked cancellation of factors of ten across multiplication operands, signed values, zero, scale alignment and checked coefficient bounds. N's intermediate contract is conservative by design; failure of an oversized intermediate that could be reduced later is not silent loss.

## 3. Complete field and capability inventory

Fresh EN/HU inline schemas each have **118 child declarations in named complex types**, plus the six root-block declarations. D has 116, lacking group id and erasure count. Counts include containers and alternative credential/kind slots, not fields simultaneously emitted by a single request.

Below, `?` means schema-optional. All ordered lists were compared with the actual writer. Required strings may still be empty as caller content; optional strings preserve `Some("")` as a present empty element. `Option<bool>` preserves explicit false and true. All optional fields default absent through constructors unless noted.

### Root and all six kinds

Root is UTF-8 XML 1.0 `xmlszamla` in `http://www.szamlazz.hu/xmlszamla`, ordered **beallitasok → fejlec → elado → vevo → fuvarlevel? → tetelek** (I:737–920; X:132–153). Required seller container is emitted even empty. `xsi:schemaLocation` in examples is a schema hint, not a missing business capability. One request creates one document.

| Kind | Emitted selector/reference | Coverage and limits |
|---|---|---|
| Invoice | No special true flag; `dijbekeroSzamlaszam?` | I:35–40,774–779. Standard creation and explicit proforma conversion. |
| Proforma | `dijbekero=true` | I:41–43,794. Independent proforma request, not an invoice storno target. |
| DeliveryNote | `szallitolevel=true`; forced `szamlaSablon=SzlaFuvarlevelesAlap` | I:44–46,795,811–818. Schema flag and K's required layout both covered; override of caller template documented at I:193–196. |
| Prepayment | `dijbekeroSzamlaszam?` before `elolegszamla=true` | I:47–59,774–780. Explicit-reference execution is not conflated with observed implicit consumption. |
| Final | `dijbekeroSzamlaszam?`, `vegszamla=true`, `elolegSzamlaszam?` | I:60–78,774–788. Validation requires nonblank prepayment number or order (686–701). Single reference matches K's 1:1 rule. Caller supplies deduction row. |
| Corrective | `helyesbitoszamla=true`, `helyesbitettSzamlaszam` | I:79–84,790–793. Reference present by construction; existence and permitted correction remain server decisions. |

Arbitrary mixed kind flags and proforma references on corrective/proforma/delivery forms are not exposed (I:21–30,104–116). Independent XSD slots do not prove those combinations are meaningful supported document operations. Storno uses its own endpoint operation and is not a seventh `InvoiceKind`.

### Settings and credentials

| Ordered XML fields | Current representation / location | Assessment |
|---|---|---|
| `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | `Credentials` key **or** user/password; `credentials.rs:53–79`, X:603–612, I:740 | AU and schema agree. Key sent unchanged; legacy same-key user/password possible. No need to send both authentication forms. |
| `eszamla`, `szamlaLetoltes` | Required bools, defaults false, always emitted; I:532–542,604–605,741–742 | Correct paper/PDF request settings. Sample true values are not protocol defaults. |
| `szamlaLetoltesPld?` | Optional u8, I:543–547,743–745 | Narrower than XSD int, but explicitly obsolete/ignored in S; no demonstrated capability loss. |
| `valaszVerzio?` | Always shared constant `2`; I:746, `ops.rs:27–31` | RS structured XML/base64 mode; version 1 is intentionally not offered. |
| `aggregator?`, `guardian?`, `cikkazoninvoice?`, `szamlaKulsoAzon?` | String, bool, bool, string; I:548–556,747–754 | All exact slots/types/order, including explicit false and external-id query handle. Semantics caveats below. |

### Header

| Ordered XML fields | Model / writer | Assessment |
|---|---|---|
| `keltDatum?` | `issue_date: Option<Date>`, I:141–150,758 | Optional; replacement-by-today caveat is accurately scoped to a test-account observation. |
| `teljesitesDatum`, `fizetesiHataridoDatum` | Required Date fulfillment/due, I:151–156,759–760 | Correct HU meanings; EN sample's “payment date” for fulfillment is misleading. |
| `fizmod`, `penznem`, `szamlaNyelve` | PaymentMethod, Currency, Language; I:157–162,761–763 | Complete token support detailed below. |
| `megjegyzes?` | Optional string, I:163–164,764 | Supports invoice comment and required K.AFA subtype/information text. |
| `arfolyamBank?`, `arfolyam?` | ExchangeRate bank plus optional Decimal, I:165–166,765–770 | Foreign currency gate 719–729; specific automatic-MNB exception supported by S/D. |
| `rendelesSzam?` | Optional string, I:167–169,771 | Preserves caller order; does not promise unique/idempotent issuance. |
| `dijbekeroSzamlaszam?`, `elolegszamla?`, `vegszamla?`, `elolegSzamlaszam?`, `helyesbitoszamla?`, `helyesbitettSzamlaszam?`, `dijbekero?`, `szallitolevel?` | Kind-selected slots, I:772–796 | Eight slots covered by applicable kind; omitted unselected flags maintain sequence. |
| `logoExtra?`, `szamlaszamElotag?` | Extra logo / prefix strings, I:170–175,797–798 | Account upload/registration separate from serialization. |
| `fizetendoKorrekcio?` | Optional Decimal payable adjustment, I:176–177,799–801 | Preserved without changing explicit item totals. Not a substitute discount feature. |
| `fizetve?` | Bool paid, default false, only true emitted; I:178–180,802–804 | Intentional omission policy; explicit false unavailable. No proven missing business operation established. |
| `arresAfa?`, `eusAfa?` | Optional margin/eu VAT bools; I:181–192,805–810 | Correct fields; V's NAV and seller eligibility implications documented. |
| `szamlaSablon?` | InvoiceTemplate, I:193–196,811–818 | Six tokens + Other; omission distinct from explicit `SzlaAlap`; forced delivery layout. |
| `elonezetpdf?`, `simpleItems?` | Optional preview/simple bools; I:197–225,819–826 | Correct names/values; order follows D/PHP and conflicts with S. See §4. |

The foreign-currency gate requires an already-trimmed nonblank bank and a rate unless bank is exactly `MNB`. HUF/Ft case-insensitive detection preserves the supplied currency spelling; all lowercase variants' server acceptance was not live-tested. Foreign proformas, delivery notes and exempt invoices are also subject to the gate. C supports the general requirement; schema optionality alone is not proof the gate rejects valid vendor operations.

**Simple items:** I:199–218 documents per-document selection; OSS off and Hungarian seller tax number; max two items, max four on final (two negative + two new); allowed VAT `0/5/18/27/TAM/AAM/K.AFA/F.AFA`; final inherits prepayment state and rates; corrective/delivery unsupported; simplified original cannot be corrected; template overridden server-side; full monetary data still sent to NAV. The implementation writes complete items and leaves these content rules to the server. It does not claim schema-valid combinations are eligible to issue.

### Seller, buyer, postal address and buyer ledger

| Ordered fields | Model / writer | Assessment |
|---|---|---|
| Seller `bank?`, `bankszamlaszam?`, `emailReplyto?`, `emailTargy?`, `emailSzoveg?`, `alairoNeve?` | Seller and flattened SellerEmail, I:262–275,828–837; T:1022–1032 | All six covered, absent account defaults. BBCode/newlines transported as text. No request seller-name/tax-number override exists in schema. |
| Buyer `nev`, `orszag?`, `irsz`, `telepules`, `cim` | Four required strings + optional country; I:296–306,840–844 | Correct order/types; no invented normalized address. |
| `email?`, `sendEmail?` | String / tri-state bool; I:307–315,845–848 | E's omitted/true versus false behavior supported; comma recipients representable. |
| `adoalany?`, `adoszam?`, `csoportazonosito?`, `adoszamEU?` | TaxpayerStatus and three strings; I:316–324,849–854 | All current S fields covered, even group id missing from D. |
| `postazasiNev?`, `postazasiOrszag?`, `postazasiIrsz?`, `postazasiTelepules?`, `postazasiCim?` | PostalAddress, I:277–291,855–861 | All five flattened fields; partial address representable. Postal service is not an absent API field. |
| `vevoFokonyv?`: `konyvelesDatum?`, `vevoAzonosito?`, `vevoFokonyviSzam?`, `folyamatosTelj?`, `elszDatumTol?`, `elszDatumIg?` | BuyerLedger: Date/string/string/bool/Date/Date; I:120–135,862–873 | All six ordered fields and empty-present container supported. |
| `azonosito?`, `alairoNeve?`, `telefonszam?`, `megjegyzes?` | Buyer id/signer/phone/comment; I:329–347,874–877 | Partner id docs explicitly describe master-data update and portal-document exposure, distinct from queried numeric id. |

### Items and item ledger

| Ordered fields | Model / writer | Assessment |
|---|---|---|
| `megnevezes`, `azonosito?` | Name/id, A:91–94, I:885–886 | Covered. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs` | Decimal/string/Decimal/VatRate, A:95–102, I:887–890 | Fractional quantity and unit price, signed discount price, exact wire tokens. |
| `arresAfaAlap?` | Optional Decimal, A:103–104, I:891–893 | Correct position; margin-base data not discarded. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | Explicit Decimals, A:105–110, I:894–896 | Always supplied, zero/negative included. Derived calculator discussed below. |
| `megjegyzes?` | Comment, I:897 | Full text emitted; rendering limits separate. |
| `tetelFokonyv?`: `gazdasagiEsem?`, `gazdasagiEsemAfa?`, `arbevetelFokonyviSzam?`, `afaFokonyviSzam?`, `elszDatumTol?`, `elszDatumIg?` | Four strings/two Dates, A:52–72, I:898–913 | All six covered; invoice-only shared-item metadata retained. |
| `torloKod?` | Optional u32 count, A:115–131, I:914–916 | Last in row; 0–400 validation at I:703–710. Account/template conditions documented, not auto-configured. |
| `tetelek/tetel` | Vec, I:682–685,882–919 | At least one at checked boundary; unbounded repetition and row order preserved. |

Discount/final deduction capability is present via negative-price, positive-quantity rows at the same VAT rate. Adjacency is caller-controlled through vector order. No separate gross-price calculator exists, but `LineItem::new` can express Q's gross-first example exactly: quantity 3, price 393.66, net 1181, VAT 319, gross 1500. Missing convenience is not missing request capability.

### Waybill and carriers

| Ordered fields | Location | Assessment |
|---|---|---|
| `uticel?`, `futarSzolgalat?`, `vonalkod?`, `megjegyzes?`, `tof?`, `ppp?`, `sprinter?`, `mpl?` | W:94–112,133–177 | Complete; destination marked unused; general-barcode fallback described. Block also usable on invoices with suitable layout. |
| TOF: `azonosito?`, `shipmentID?`, `csomagszam?`, `countryCode?`, `zip?`, `service?` | W:11–24,137–148 | All six; five-digit id is string, preserving leading zeroes. |
| PPP: `vonalkodPrefix?`, `vonalkodPostfix?` | W:28–33,149–154 | Three-character prefix / max-seven suffix can be supplied. |
| Sprinter: `azonosito?`, `feladokod?`, `iranykod?`, `csomagszam?`, `vonalkodPostfix?`, `szallitasiIdo?` | W:37–50,155–166 | Three-character id, ten-digit sender code, routing code, count, 7–13 suffix and delivery text supported. |
| MPL: `vevokod`, `vonalkod`, `tomeg`, `kulonszolgaltatasok?`, `erteknyilvanitas?` | W:57–84,167–175 | Required strings always emitted, weight correctly string, declared value optional Decimal. |

Carrier string supports all documented tokens `TOF/PPP/SPRINTER/FOXPOST/MPL/GLS/EMPTY`; no dedicated FOXPOST/GLS child is missing from the schema. Multiple child blocks are allowed by its sequence. Parcel counts are u32 but checked against `i32::MAX` for XSD int (I:711–718, W:118–129). Identifier lengths and partial-block rendering remain vendor/content concerns.

### Attachments and serialization boundaries

- I:379–516 bounds the collection at five attachments and 2,000,000 bytes per file through push, Vec conversion and serde. Only immutable slice access is exposed, so stored bytes cannot grow past the bound. Decimal MB is a stated conservative interpretation of E's unspecified “2 MB”.
- I:938–948 contributes exactly sequential `attachfile1`…`attachfile5`, filename, MIME and raw bytes. `wire.rs:66–125` writes multipart with CRLF, boundary collision avoidance and header metacharacter handling. Exact two-file framing and collection/error tests passed.
- E says attachments with `sendEmail=false` are not processed by the server; the writer need not silently remove caller files. E's per-bad-file notification/valid-file sending behavior does not mean local oversized-file refusal is a wire defect.
- X:561–600 escapes text, writes decimal plain notation, dates and lowercase true/false; None omits, Some-empty remains present. An independent XML parser recovered `A&B <tag> "quote" 'apostrophe' ]]> é\r\nnext` unchanged from the rich request.
- `to_wire` validates request constraints and XML 1.0 characters (`wire.rs:402–429`). Lower-level `write_xml` is not the checked boundary. Ordinary dates passed full XSD validation. Year zero, arbitrary carrier text, nonexistent references and unknown tokens are not established meaningful invoicing use cases merely because a Rust value is constructible.

### Shared vocabulary and arithmetic

| Surface | Checked result |
|---|---|
| Special VATs | T:186–324 covers every current invoice token: `TAHK TAM AAM EUT EUKT F.AFA K.AFA HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET`. Receipt-only shared codes `ÁKK/EU/EUK/MAA` are labelled as such. Unknown codes remain Other. |
| Numeric VATs | All listed rates fit Decimal: `0,1,2,2.1,3,4,4.8,5,5.5,6,7,7.7,8,8.1,9,9.5,10,11,12,13,13.5,14,15,16,17,18,19,20,21,22,23,24,25,25.5,26,27`. Percent normalizes zeros. Numeric Other tokens are calculated numerically while preserving raw wire text; unrepresentable numeric tokens return an error (A:194–207, N:63–137). |
| K.AFA | T:203–210 matches PDF page 1: exact phrases `utazási irodák`, `használt cikkek`, `műalkotások`, `gyűjtemény darabok és régiségek`; default used goods if absent/unmatched. Matching precedence remains unspecified. Calculator's special-code zero VAT is documented; caller uses explicit amounts/margin metadata where appropriate. |
| Currency | T:369–477 is open string vocabulary, so all C currencies are representable, including historical EEK/HRK/LTL/LVL and vendor spelling `KSH`. Five named constants are conveniences, not an allowlist. HUF/Ft detection is case-insensitive. |
| Language | T:480–585 matches all fifteen S/L tokens: `hu en de it ro sk hr fr es cz pl bg nl ru si`. Vendor `cz`/`si` must not be replaced with ISO `cs`/`sl`. |
| Payment | T:596–687 matches first-party PHP `átutalás/készpénz/bankkártya/csekk/utánvét/PayPal/SZÉP kártya`; `Other("OTP Simple")` covers the extra PHP constant and free text. |
| Taxpayer | T:690–755 sends `7/6/1/0/-1`, matching S; `-1` means no tax number, with doc caveat below. |
| Template | T:983–1020 exposes all six tokens plus Other. `Default` explicitly selects `SzlaAlap`; it is not omitted-default mode. |
| Identifiers | T:22–57 retains requested invoice numbers; kind references use them. Queried document codes `SZ/D/ES/VS/HS/SS/SL` (T:768–800) agree with recorded observations; create selects flags, not a `tipus` element. |
| Exchange rate | T:943–980 supports explicit bank/rate or exact `MNB` with omitted numeric rate. Invoice-specific S/D annotation justifies the latter despite C's general bank-and-rate statement. |
| Rounding | A:9–49,183–223: half away from zero at net then VAT, exact gross sum. HUF whole forints, EUR cents, KWD thousandths, etc. are expressly local policies (T:412–435), not server-precision promises. Scale above current precision leaves it unchanged; Exact does not bypass intermediate representability. |

## 4. Source conflicts and ambiguities

### SC-1 — Combined preview/simplified-image order

**Potential severity: Medium interoperability risk; not a confirmed implementation bug.**

- S EN and HU require **szamlaSablon → simpleItems → elonezetpdf** and state order cannot be interchanged.
- D and PHP `Header/InvoiceHeader.php:398–404` require/write **szamlaSablon → elonezetpdf → simpleItems**.
- I:819–825 follows D/PHP intentionally; I:220–222 and README:408 explicitly acknowledge the unresolved combined-preview behavior.

**Offline reproduction:** populate both `preview_pdf=Some(true)` and `simple_items=Some(true)`. Current XML contains `<szamlaSablon>SzlaMost</szamlaSablon><elonezetpdf>true</elonezetpdf><simpleItems>true</simpleItems>`. The freshly acquired inline schemas reject `simpleItems` as unexpected. Removing only simpleItems from each synthetic instance yields six passes per inline source. D accepts the header as emitted; D's separate missing-field conflicts are isolated below. False values still cause the same ordering disagreement if both elements are present.

**Disposition:** seek vendor clarification of order and preservation of non-issuing preview. Reversing the writer's order would just violate the other official source. No server rejection, silent flag loss, or accidental issuance was observed here; this is not a live-evidence exemption.

### SC-2 — Download omits current group-id and erasure fields

**Potential severity: Low schema-tooling drift; supported fields, not code defects.**

D lacks `vevo/csoportazonosito` and `tetelek/tetel/torloKod`; both exist in S, and ER plus its linked knowledge base explicitly support erasure counts. Fully populated instances fail D with unexpected-element errors. Removing **only those two fields from synthetic instances** produces six D passes, retaining both preview/simple flags and all other rich blocks. Preserve I:853,914–916. The cached workspace schema is explicitly hand-patched (`fixtures/SOURCES.md:126–140`), not an unmodified vendor download.

### SC-3 — Layout labels conflict

**Potential severity: Low display-selection ambiguity.** L maps `SzlaAlap` to traditional and `SzlaNoEnv` to envelope-friendly; its linked knowledge base maps traditional to `SzlaNoEnv` and envelope-friendly to `SzlaAlap`, reinforced by its linked example filenames. T:991–1017 follows L. The knowledge base also uses lowercase tag/token examples inconsistent with S. Token coverage is complete; do not swap variants or casing without better evidence. Actual rendering was not tested.

### Other tensions and non-findings

- S's blanket “all fields shown in the example are mandatory” conflicts with its own minOccurs rules and optional-feature prose. The writer correctly follows individual declarations rather than forcing every example field.
- C's generic foreign bank-and-rate requirement has the explicit automatic-MNB exception in both S and D. This is documentation-backed, not a live-probed exemption.
- Q English says B2B/B2C “have to” use net/gross-first; HU says “valószínűleg” (probably). The crate offers net-first calculation and explicit totals, covering both capabilities. Q itself describes server normalization of fractional HUF inputs; absence of blanket local fractional-HUF rejection is not a bug.
- V's English expansion of KBAUK as “to UK” is inconsistent with its linked detailed PDF's **new means of transport within the EU**. Current `VatRate::Kbauk` follows the detailed source correctly.
- RS's successful example is illustrative: URL contains unescaped ampersands and PDF text contains ellipsis. Literal rejection of that example is not proof of parser noncompliance. Request version 2 is supported, but actual preview response shape/combined-preview behavior remains unverified live.
- No automatic final deduction, discount calculation, unlimited Decimal precision, arbitrary mixed kind flags or response-version-1 request mode is promised. Explicit totals cover valid gross-first/discount/final operations.
- Reference existence, date chronology, tax/country consistency, prefix/account eligibility, simplified-image conditions and carrier identifier lengths are mostly server-side content rules. A locally serializable request is not claimed to satisfy all of them.

Small semantic documentation opportunities, **not counted as functional findings**:

1. I:548 describes aggregator as “for contracted integrations”; first-party PHP describes webshop-engine name (WooCommerce/OpenCart/etc.). The field works; the contractual prerequisite is not established. Guardian's contract-only wording (I:550) is likewise not established by inspected schemas.
2. T:703 narrows `-1` to “private individual”, whereas S says “no tax number”; PDF page 1 explicitly includes organizations without tax numbers. The correct token is available; no erroneous runtime classifier was found.
3. I:307–313 gives normal email behavior; E states test accounts redirect notification to the account-configured address. Historical D6 inability to trigger code 56 does not establish that test accounts never send mail.
4. `ops.rs:18–21` says omission is “exactly false” on the server. True-only paid emission is documented, but the inspected sources do not establish all account/payment-method interactions as equivalent to an explicit false.

## 5. Evidence-backed deviations preserved

`docs/szamlazz-hu-behaviour.md:3–28` explicitly limits observations to one TEST account on dated September runs; raw historical request/response logs are **not in this repository**. Its design-consequence column includes worker-specific decisions and should not be treated as independent vendor evidence. README:344–362 correctly distinguishes local arithmetic and invoice probes from receipt or currency-wide guarantees.

| Recorded observation | Provenance and current handling |
|---|---|
| Create issue date may be replaced with today | P48-P5, behavior:91; I:141–148 qualifies the caller's date as a request, not guaranteed stored date. No unsupported rejection rule added. |
| Final does not net prepayment automatically | C6-2, behavior:117–119; I:62–70 requires caller-supplied negative same-VAT line. |
| Proforma may be implicitly consumed; stale explicit reference may be silently ignored | C1-3, C2-3/C2-6, D4/D5, behavior:104–108. ES/VS **explicit** proforma-reference execution remains unverified (234–240), accurately called out at I:50–54,69–70. |
| External ids are nonunique and newest-holder queries | A3/XPRB, behavior:63–71. Correctly exposed as query handles, not idempotency keys. |
| HUF net discrepancy tolerated at tested amounts | P60-H1…H5, behavior:159; 0.5/1/2 HUF accepted, 5/10 rejected. Does not determine absolute versus relative tolerance or receipt behavior. |
| EUR net/VAT/gross rounded independently to two decimals | P60-E1/E3, behavior:160; Exact may store inconsistent gross versus net+VAT. A:24–29 and README:351 warn accurately. No KWD-wide inference. |
| Minor-unit EUR values accepted | P60-E2, behavior:161; `100.01/27/127.01` stored as sent. |
| VAT `27.00` and `27.0` accepted | P60-V1/V2, behavior:162; T:259–264 calls normalization hygiene, not a requirement. |
| Paper/e-invoice creation flag confirmed | P73, behavior:97; false queried as 1, true as 3. Request remains boolean, queried appearance is a separate numeric code. |
| Partner data may change with later creation; replay is conditional | D6 and A4, behavior:47–57,111. Request docs avoid immutable partner or unconditional replay guarantees. |

These are bounded exemptions from naïve schema/prose-only findings, not newly confirmed live results. Automatic MNB, layout labels, simpleItems order and explicit ES/VS proforma linking are not upgraded to live evidence.

## 6. Executed checks, reproduction and limits

Commands actually executed in this slice (routine dedicated reads/searches and webfetch URLs are represented by the inventory above):

```text
git status --short && git rev-parse HEAD
ls /tmp/opencode
python3 /tmp/opencode/invoice-2ba5fb86-sources.py
python3 /tmp/opencode/invoice-current-fbda137-review/check.py
git diff HEAD -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md && cargo test --locked -p szamlazz-agent --test simple_items --test numeric_fidelity
cargo test --locked -p szamlazz-agent --lib ops::invoice::tests
cargo test --locked -p szamlazz-agent --lib item::tests
git rev-parse HEAD && git diff HEAD -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md && git status --short
git diff --no-index --check /dev/null docs/review/2026-09-11-agent-api-invoices.md
```

- **47 focused tests passed:** 31 invoice unit tests, 8 item unit tests, 6 numeric-fidelity tests, 2 simple-items tests. Zero failures/ignored. Some invoice-filtered tests exercise response controls too; this does not expand the claimed response audit scope. No live test target was selected.
- Final Git verification retained the requested HEAD and an empty in-scope tracked diff. Concurrent unrelated worker edits were left intact. The report whitespace check initially flagged two Markdown hard-break spaces; those were removed and the check rerun.
- The existing scratch checker runs `cargo run --quiet --offline --manifest-path /tmp/opencode/invoice-current-fbda137-review/Cargo.toml` against the current crate, with `rust_decimal=1.43.0`. Its Rust source emits six rich requests and checks the five historical arithmetic failures plus two controls. Both scratch sources were read before execution; no old result file was reused.
- Its Python process freshly fetches D/S EN/S HU, hashes them, and uses installed **libxml2 full XSD validation via ctypes**, not a hand-written order matcher. All six rich requests fail each source only at the stated conflicts (validation code 1871). Removing source-conflicting fields from the **instances**, never changing schemas, gives **18/18 full-XSD control passes**. It separately checks XML text fidelity. The artificial all-fields combinations test shape, not eligibility to issue.
- The new acquisition script downloaded D/PHP/PDF under `/tmp/opencode/invoice-2ba5fb86*`, printed hashes and inspected first-party PHP sequence/payment/settings lines. The PDF was independently read. The initial mistyped category URL `/agent/category/generating_invoice` returned 403; the correct request and settings-index URLs and every source listed above succeeded.
- Existing test provenance was inspected: `tests/simple_items.rs` asserts nine presence/value pairs over six kinds, preserves group/erasure/full money and explicitly delegates content rules; `fixtures/SOURCES.md:126–140,193–228` records patched cached schema and independently acquired tail excerpts. Golden expectations are project-authored. Upstream outline matching deliberately loses empty containers/surrounding whitespace (`fixtures/SOURCES.md:232–234`), so it is not proof of absent/empty equivalence.

Reproduce the principal ambiguity without the scratch runner by constructing any ordinary `CreateInvoice`, setting both header flags to `Some(true)`, serializing with placeholder credentials, and validating the generated XML against fresh S and D. To isolate D's separate drift, leave buyer group and item erasure absent. S fails on the tail; D passes it. No network POST is needed.

**Limits:** no rendered PDF/barcode inspection, mail delivery, account-toggle changes, live or test-account operation, credential checks, full feature/platform matrix, or exhaustive partial-block/input fuzzing. Historical raw probe logs were unavailable. Full schema acceptance cannot settle deployed-server behavior when official sources disagree. No new functional change is justified by the confirmed evidence in this scope; vendor clarification is the next step for the material ambiguities.
