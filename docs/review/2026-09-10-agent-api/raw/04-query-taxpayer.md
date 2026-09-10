# Queried invoice XML and taxpayer review — 2026-09-10

## Summary

Reviewed current code at **`f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3`**, not the implementation described by the September 9 report. All paths below beginning `src/` are relative to **`crates/szamlazz-agent/`**. The working tree had other reviewers' untracked work; none was used as implementation evidence or edited.

**Two reproduced parser defects and one bounded response-model capability gap remain. No P0/P1 finding is established.**

| ID | Classification | Severity | Finding | Confidence |
|---|---|---|---|---|
| QX-01 | Defect | P3 | Invoice child namespaces are discarded: foreign or undeclared-prefix elements can supply invoice identity and reversal fields | High in behavior; no live incidence established |
| QX-02 | Defect | P3 | Decimal response parsing silently rounds nonzero out-of-scale values, including to zero; equivalent exponent spelling instead errors | High in behavior; no live incidence established |
| TP-01 | Capability gap | P3 | Taxpayer business data is complete against the current documented record, but NAV response metadata, success messages and notifications are not exposed | High in omission; current notification forwarding unverified |

Both request writers cover all documented request elements in their supported credential/selector alternatives, with correct order and namespaces. **Every element in the freshly fetched `szamla.xsd` has a response-model destination.** All current NAV taxpayer business fields are represented, including the five additions requested by old finding C3. Missing NAV envelope metadata is separately inventoried below, rather than hidden by that statement.

Scope: request XML, operation-specific parsing, complete response-model inventory, shared lexical adapters, source conflicts and preservation. Shared HTTP/header/error-envelope policy belongs to the other reviewer; this report does not claim to re-audit it. No live-account call, production edit, historical-report edit or delegation occurred.

## Sources fetched and their authority

All sources below were fetched on **2026-09-10** using unauthenticated GETs. The docs pages displayed build `v202608271632`; this is not a per-page publication date. Current sources were read directly, not inferred from fixtures or the old review.

| Ref | Exact source URL | Scope and short source quotation |
|---|---|---|
| Q1 | https://docs.szamlazz.hu/agent/querying_xml/request | “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved”; form field `action-szamla_agent_xml`; “if multiple documents share the same order number, the last one is returned” |
| Q2 | https://docs.szamlazz.hu/agent/querying_xml/xml | Request example and inline XSD. “the order of the fields is fixed, **they cannot be interchanged**”; `szamlaszam`, `rendelesSzam`, `pdf`, `szamlaKulsoAzon` |
| Q3 | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Downloaded request XSD: `targetNamespace="http://www.szamlazz.hu/xmlszamlaxml"`, `elementFormDefault="qualified"`; selector/PDF elements all `minOccurs="0"` |
| Q4 | https://docs.szamlazz.hu/agent/querying_xml/response | “Full `szamla` XML document”; on error “`xmlszamlavalasz` XML with `<sikeres>false</sikeres>`, `<hibakod>` and `<hibauzenet>`”; missing selector is code 7 |
| Q5 | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Complete downloaded response XSD: `targetNamespace="http://www.szamlazz.hu/szamla"`, `elementFormDefault="qualified"` |
| Q6 | https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak | Independent first-party annotated example and inline copy of the same invoice schema. Appearance: “0: nem számla, 1: papír számla, 2: e-számla, 3: e-számla”. Credit-entry bank: “A kifizetés ténylegesen erről a bankszámláról érkezett, vagy a számlán szereplő bankszámlaszám (ha a küldő bankszámlaszám nem ismert)” |
| T1 | https://docs.szamlazz.hu/agent/querying_taxpayer/request | “The data is **from the Online Invoice Platform of NAV**”; form field `action-szamla_agent_taxpayer` |
| T2 | https://docs.szamlazz.hu/agent/querying_taxpayer/xml | Request example and inline XSD: `<length value="8"/>`, `<pattern value="[0-9]{8}"/>`; required `beallitasok`, then `torzsszam` |
| T3 | https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd | Working request XSD download, same credential block and prefix restriction as T2 |
| T4 | https://docs.szamlazz.hu/agent/querying_taxpayer/response | “The response always matches the `QueryTaxPayerResponse` type of the NAV Online Invoice Platform. Last update for example responses: 2020-11-04.” Success, error 57 and `taxpayerValidity=false` examples remain OSA 2.0 |
| N1 | https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf | The primary NAV link actually present on T4. Read §§1.4–1.4.1, 1.8.9.1–1.8.9.2, printed pp.10–12, 63–69. “Az infoDate az adózó adatainak utolsó változását mutatja.” |
| N2 | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd | NAV-owned OSA 3.0 schema: `QueryTaxpayerResponseType`, `BasicOnlineInvoiceResponseType`, `TaxpayerDataType`, address-list/item and software types |
| N3 | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd | NAV-owned OSA 3.0 component schema: `TaxNumberType`, `DetailedAddressType`; “County code, two digits” |
| N4 | https://raw.githubusercontent.com/nav-gov-hu/Common/release/common-1.0.x/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd | NTCA **1.0**, the version imported by N2/N3 (not Common main's 2.0). `BasicResponseType`, `BasicHeaderType`, `BasicResultType`, notifications, string/pattern types |
| N5 | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd | Tagged older NAV source for the namespaces still used by T4's examples; `taxpayerAddress` has `type="data:DetailedAddressType"` |
| N6 | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceData.xsd | Tagged OSA 2.0 component definitions; `TaxNumberType`, `DetailedAddressType` |
| W1 | https://www.w3.org/TR/xmlschema-2/#double | “double values have a lexical representation consisting of a mantissa followed, optionally, by the character ‘E’ or ‘e’, followed by an exponent”; also defines boolean/date/string/whitespace behavior in the same fetched recommendation |
| W2 | https://www.w3.org/TR/xml-names/#ns-qualnames | “An expanded name is a pair consisting of a namespace name and a local name”; §5: a namespace prefix “MUST have been declared” |

NAV Online-Invoice `master` resolved to **`cc7a775d6dce361311e409abb9934eb755f2749c`** during acquisition. Stable equivalents for N2/N3 replace `master` with that SHA. N1 downloaded successfully (SHA-256 **`54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2`**). The web-fetch tool's 5 MB limit initially prevented PDF extraction; a direct GET followed by `nix shell nixpkgs#poppler-utils -c pdftotext -layout` made the linked PDF readable. PDF and extracted text remain only under `/tmp/opencode/query-taxpayer-review-nav.{pdf,txt}`. N6 was downloaded to `/tmp/opencode/query-taxpayer-review-nav2-data.xsd` and its relevant types read there.

Local constraints read: `CONTEXT.md`; `docs/szamlazz-hu-behaviour.md`, especially lines 59–80, 96–98, 111, 129–145, 155–162; `fixtures/SOURCES.md`, especially 121–140, 172–180, 230–250. The historical `docs/review/2026-09-09-agent-api/FINAL.md` supplied only the closure checklist in §7 below.

## 1. Request coverage

### Query invoice XML

Implementation: `src/ops/query_xml.rs:46–70,530–553`; common credential writer `src/xml.rs:348–357`.

| Documented element / property (Q1–Q3) | Current implementation | Assessment |
|---|---|---|
| `xmlszamlaxml` / `http://www.szamlazz.hu/xmlszamlaxml` | Correct root/default namespace | Covered |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs`, directly under root | Either username/password pair or agent key; no `beallitasok` | Covered; credentials alternative is intentional |
| `szamlaszam` | `InvoiceSelector::InvoiceNumber` | Covered; text escaped without trimming |
| `rendelesSzam` | `InvoiceSelector::OrderNumber` | Covered; exact capitalization distinct from response `rendelesszam` |
| `pdf` optional boolean | `include_pdf`, always emits true/false | Covered; false and omission need not be modeled separately to request no PDF |
| `szamlaKulsoAzon`, after PDF | `InvoiceSelector::ExternalId` | Covered; exclusive selector avoids ambiguous multi-selector requests |
| Multipart field | `ACTION = "action-szamla_agent_xml"` | Correct operation discriminator; transport construction outside scope |
| `xsi:schemaLocation` in example | Not emitted | Schema-location hint, not required request data |

The XSD permits all selector elements to be absent or several to coexist; the enum deliberately offers exactly one selector kind. This is a clearer supported interface, not missing documented selection capability. Inner strings are unvalidated wire values, so an empty string remains possible; server refusal is not evidence of a writer defect. No response-version field belongs to this request.

Q1's internal-outgoing-only limitation is absent from the short operation rustdoc (`src/ops/query_xml.rs:1–2,46–50`). This is a **documentation opportunity**, not a broken selector or a request for imported-document support. Shared `szamla.xsd` containing `forras=34` does not expand this operation's documented reach.

### Taxpayer

Implementation: `src/ops/taxpayer.rs:17–110,264–279`.

| Documented element / property (T1–T3) | Current implementation | Assessment |
|---|---|---|
| `xmltaxpayer` / `http://www.szamlazz.hu/xmltaxpayer` | Correct root/default namespace | Covered |
| Required `beallitasok` | Always emitted | Covered |
| Its optional username/password/key elements, in that order | Shared credential writer | Covered |
| Required `torzsszam` after settings | `TaxpayerPrefix`, exactly eight ASCII digits; serde construction validates too | Covered; leading zeroes retained |
| Multipart field | `action-szamla_agent_taxpayer` | Covered |

NAV's own `QueryTaxpayerRequest` includes NAV user/signature/software/header data. Those are **not missing Számla Agent request fields**: szamlazz.hu constructs that inner request. T2's English comment calling an eight-digit prefix an “adóazonosító jel” is misleading terminology; the actual restriction and N1's “magyar adószám első 8 jegye” support this crate's törzsszám vocabulary.

## 2. Complete invoice-response inventory

Source: Q5, corroborated by Q4's example and Q6's same-schema annotations. In the tables, **R/O** means required/optional in Q5, not in Rust. Listed leaves have XSD maximum one unless a list is identified. All recognized descendants should be in `http://www.szamlazz.hu/szamla`; see QX-01 for the implementation's namespace weakness.

### Root, seller, addresses and bank

| Wire path / all fields | Schema | Public destination and parsing | Code |
|---|---|---|---|
| `/szamla/szallito`, `alap`, `vevo`, `tetelek`, `osszegek` | R blocks | Required wire blocks | `query_xml.rs:605–619` |
| `/szamla/qutetek`, `cimkek`, `kifizetesek` | O blocks | Absent → empty vectors | `query_xml.rs:611–617,987–1043` |
| `/szamla/pdf` | O string | Absent/blank → None; nonblank standard base64 → `Pdf`; malformed base64 errors | `query_xml.rs:591–594,618–619`; `types.rs:104–117` |
| `szallito/id`, `nev` | R int/string | `Supplier.id: Option<i64>`; `name: String` | `query_xml.rs:659–688` |
| `szallito/cim` | R `cimTipus` | Required `Supplier.address` | same |
| `szallito/postacim` | O **`cimTipus`** | Optional `Address`, not buyer's postal type | same |
| `cimTipus/{orszag,irsz,telepules,cim}` | O country, R other strings | `Address.{country,zip,city,address}`; country optional business text; required strings retained | `query_xml.rs:622–639` |
| `szallito/{adoszam,csoportazonosito,adoszameu}` | R tax number; O group/EU | `tax_number`, `group_id`, `eu_tax_number`, all optional business text | `query_xml.rs:667–686` |
| `szallito/bank/{nev,bankszamla}` | O block and leaves | `Bank.{name,account}`, optional business text | `query_xml.rs:642–656,673–687` |

### Core invoice data (`alap`)

All 28 leaves are mapped at `src/ops/query_xml.rs:695–785`; public semantics at `228–316`.

| Wire leaves | Schema | Public destination / interpretation |
|---|---|---|
| `id`, `szamlaszam` | R int/string | Required `id: i64`, `invoice_number: InvoiceNumber`; invoice number characters retained |
| `gazdEsemAzon`, `forras` | R/O int | Optional `economic_event_id`, `source`, i64 |
| `iktatoszam` | O string | Optional `registration_number`; receiver-assigned, not vendor id |
| `tipus` | R string | Required open `DocumentType`; unknown code preserved |
| `eszamla` | R int | Required `InvoiceAppearance`; 0/1/2/3 mapped; other integer preserved |
| `hivszamlaszam`, `hivdijbekszam` | O string | Optional referenced invoice/proforma numbers, preserving nonblank text |
| `kelt`, `telj`, `fizh` | R date | Optional issue/fulfillment/due dates; complete supported timezone suffix discarded, civil day retained |
| `fizmod`, `fizmodunified` | R string/restricted string | Optional original `PaymentMethod` and unified string, retained separately |
| `keszpenz` | R boolean | `cash_payment`; absent/empty → false |
| `rendelesszam` | O string | Optional `order_number`, no Agent-layer trim |
| `nyelv`, `devizanem` | R restricted string/string | Optional language string and open `Currency` |
| `devizabank`, `devizaarf` | O string/double | Optional exchange-bank string and Decimal exchange rate |
| `megjegyzes`, `afatipus` | O string | Optional comment and invoice-level VAT category |
| `penzforg`, `kata`, `katafokonyv` | R boolean | `cash_accounting`, `kata`, `kata_ledger`; independent flags, absent/empty → false |
| `email` | O string | Document-associated email distinct from buyer email |
| `teszt` | R boolean | `test: Option<bool>`; absent is not manufactured as live-account false |
| `sztornozott` | O boolean | `reversed: Option<bool>`; exact optional marker, not storno type |

The current downloadable Q5 contains 28 `alap` leaves, all listed above. Date optionality and missing flags are deliberate leniency, not a claim that Q5 allows omission. Appearance semantics match Q6 and recorded P73 observations.

### Buyer and ledger

| Wire path / all fields | Schema | Public destination and parsing | Code |
|---|---|---|---|
| `vevo/id`, `nev`, `azonosito` | O int, R/O string | Optional i64 id, required name, optional partner identifier | `query_xml.rs:852–898` |
| `vevo/cim` | R `cimTipus` | Optional `Address` (lenient missing block); children as seller billing address if present | same; `622–629` |
| `vevo/postacim/{nev,orszag,irsz,telepules,cim}` | O block/all leaves | `BuyerPostalAddress.{name,country,zip,city,address}`, all optional business text | `query_xml.rs:789–813` |
| `vevo/{email,adoszam,csoportazonosito,adoszameu}` | O/R/O/O string | Separate optional email, tax number, group id, EU tax number | `query_xml.rs:863–895` |
| `vevo/lokacio` | R int | Optional i64 `location`; future code retained | `query_xml.rs:871–872,895` |
| `vevo/privatePersonIndicator` | R boolean | `private_person`, absent/empty → false | `query_xml.rs:873–878,896` |
| `vevo/fokonyv/{vevo,vevoazon,datum,folyamatostelj,elszDatTol,elszDatIg}` | O block/all leaves | Optional ledger account, buyer id, date, continuous-fulfillment bool, settlement-from/to dates | `query_xml.rs:815–850,879–897` |

### Printed items, financial items, labels, totals and credit entries

| Wire path / all fields | Schema | Public destination and parsing | Code |
|---|---|---|---|
| `tetelek/tetel` | 1..unbounded | `items: Vec<DocumentItem>`; empty wrapper accepted | `query_xml.rs:902–905` |
| `tetel/{nev,azonosito,mennyiseg,mennyisegiegyseg,nettoegysegar}` | R/O/R/R/R | Name, optional id, Decimal quantity, unit string, Decimal unit price | `query_xml.rs:908–955` |
| `tetel/{afatipus,afakulcs,netto,arresafaalap,afa,brutto}` | O restricted token, R nonnegative double, R/O/R/R double | Separate optional VAT type and raw rate string, Decimal net/optional margin base/VAT/gross | same |
| `tetel/{megjegyzes,sztetordering}` | O string, R int | Optional comment and i64 ordering | same |
| `tetel/fokonyv/{arbevetel,afa,gazdasagiesemeny,gazdasagiesemenyafa,elszdattol,elszdatig}` | O block/all leaves | Optional revenue/VAT accounts, economic-event/VAT-economic-event strings, settlement dates | `query_xml.rs:958–984` |
| `qutetek/qutet` | 0..unbounded | `financial_items` | `query_xml.rs:987–991` |
| `qutet/{nev,afatipus,afakulcs,netto,afa,brutto,elszdattol,elszdatig,afalevon,cimkek}` | R/O/R/R/R/R/O/O/R/O | Name; optional special VAT type; raw numeric VAT token; Decimal net/VAT/gross; optional dates; required i64 deductible-VAT value; labels | `query_xml.rs:993–1029` |
| `cimkek/cimke` at invoice and financial-item levels | **0..1 in Q5** | `Vec<String>` at both positions; multiple labels accepted as leniency | `query_xml.rs:1032–1037` |
| `osszegek/afakulcsossz` | 1..unbounded | `Totals.by_vat_rate`; absent list accepted | `xml.rs:519–547,563–580` |
| `afakulcsossz/{afatipus,afakulcs,netto,afa,brutto}` | O restricted VAT token, R nonnegative double, R doubles | Optional VAT type, raw numeric rate string, Decimal net/VAT/gross | same |
| `osszegek/totalossz/{netto,afa,brutto}` | R block/all doubles | Required `GrandTotal` Decimal fields | `xml.rs:549–560,584–590` |
| `kifizetesek/kifizetes` | Optional wrapper; 1..unbounded rows when present | `credit_entries`, empty accepted; order retained without implying submission order | `query_xml.rs:1039–1043` |
| `kifizetes/{datum,jogcim,osszeg,megjegyzes,bankszamlaszam,banktranzid,devizaarf}` | R date/string/double; O string/string/int/double | Required Date, open PaymentMethod title, Decimal amount; optional comment/bank account/i64 transaction id/Decimal exchange rate | `query_xml.rs:1045–1073` |

No current Q5 field was found without a mapping. In particular `fuvarlevel`, a standalone `arfolyam` block, external id, outstanding amount, raw NAV invoice data and `simpleItems` are **not fields of this fetched response schema**. Request-side waybill support or names in CONTEXT do not establish a missing queried field. Invoice `devizaarf` and credit-entry `devizaarf` are independently preserved.

## 3. Complete taxpayer-response inventory

### Namespace and path selection

Implementation `src/ops/taxpayer.rs:317–403,408–520` correctly distinguishes:

| Location | OSA 2.0 | OSA 3.0 |
|---|---|---|
| Root, taxpayer data/list/item containers, `infoDate`, `taxpayerValidity` | `http://schemas.nav.gov.hu/OSA/2.0/api` | `http://schemas.nav.gov.hu/OSA/3.0/api` |
| `result` and its children | Same 2.0 api | `http://schemas.nav.gov.hu/NTCA/1.0/common` |
| `taxNumberDetail` **children**, `taxpayerAddress` **children** | `http://schemas.nav.gov.hu/OSA/2.0/data` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

The typed containers themselves remain API-namespaced even when their children come from an imported type. Arbitrary prefixes and inherited default namespaces are supported. Unknown parent frames stay unknown: familiar descendants in unrelated subtrees cannot populate a recognized field. Recognized scalar children and duplicate singletons are refused, including duplicates of empty fields; address-item repetition is allowed and singleton checking is per item. This is materially improved from old R1.

### All business fields

N2/N3 and N5/N6 are authoritative for the structure; T4 supplies actual published forwarding examples. R/O denotes the schema, conditional on the containing optional node existing.

| Path | Source type/presence | Public destination | Code |
|---|---|---|---|
| `infoDate` | O xs:dateTime | `info_date: Option<String>`; advisory source text, not lookup timestamp | `taxpayer.rs:211–218,587,609` |
| `taxpayerValidity` | O xs:boolean in XSD; true/false described on successful lookup in N1 p.68 | `valid: bool`; required on OK, not required on error | `taxpayer.rs:570–580,599–603` |
| `taxpayerData` | O block | Flattened into optional business fields; absent/empty remain sparse | `taxpayer.rs:354–363,600–613` |
| `taxpayerData/taxpayerName` | R nonblank string, max512 | `name` | `taxpayer.rs:582,604` |
| `taxpayerData/taxpayerShortName` | O nonblank string, max200 | `short_name` | `taxpayer.rs:583,605` |
| `taxpayerData/taxNumberDetail` | R block | Flattened tax-number components | `taxpayer.rs:356,364–368` |
| `taxNumberDetail/taxpayerId` | R string, `[0-9]{8}` | `tax_number`, eight-digit core only, not assembled full number | `taxpayer.rs:588,610` |
| `taxNumberDetail/vatCode` | O string, `[1-5]{1}` | `vat_code`; source string retained rather than numeric conversion | `taxpayer.rs:589,611` |
| `taxNumberDetail/countyCode` | O string, `[0-9]{2}` | `county_code`, including leading zeroes | `taxpayer.rs:584,606` |
| `taxpayerData/incorporation` | R in N2 3.0; absent in tagged N5 2.0 | Optional open `Incorporation`, known three tokens plus exact Other | `taxpayer.rs:113–184,586,608` |
| `taxpayerData/vatGroupMembership` | O eight-digit string | `vat_group_membership`; group identifier, not boolean | `taxpayer.rs:585,607` |
| `taxpayerData/taxpayerAddressList/taxpayerAddressItem` | O list wrapper, 1..unbounded items | Ordered `addresses: Vec<TaxpayerAddress>`; empty list permitted leniently | `taxpayer.rs:369–370,460–462,612` |
| `taxpayerAddressItem/taxpayerAddressType` | R HQ/SITE/BRANCH enum | Optional `kind` string, future token preserved | `taxpayer.rs:233–234,544` |
| `taxpayerAddressItem/taxpayerAddress` | R **DetailedAddressType** | One flattened optional-fields address | `taxpayer.rs:370–389` |
| `taxpayerAddress/{countryCode,region,postalCode,city,streetName,publicPlaceCategory}` | R/O/R/R/R/R strings with source facets | `country_code`, `region`, `postal_code`, `city`, `street_name`, `public_place_category` | `taxpayer.rs:235–246,545–552` |
| `taxpayerAddress/{number,building,staircase,floor,door,lotNumber}` | O strings | Same-named optional fields, including lot number | `taxpayer.rs:247–258,553–558` |

All taxpayer business strings intentionally preserve decoded nonblank characters without enforcing source lengths, patterns, enum closure or requiredness. Unknown VAT digit/country/address tokens remain data. This is **deliberate leniency**, exercised by current tests (`tests/taxpayer_paths.rs:160–174` even retains padded tax numbers and NBSP-only VAT text). Do not reapply the outgoing request's `TaxpayerPrefix` validation to returned data.

Additional `taxpayerAddress/additionalAddressDetail` is accepted/exposed at `taxpayer.rs:259–261,387,559–560`, but neither N2 nor N5's taxpayer address uses `SimpleAddressType`; both use **DetailedAddressType**. `additionalAddressDetail` belongs to the separately defined simple-address type in N3/N6. This is an **extra lenient capability**, not a missing nested simple/detailed-address choice. Current rustdoc “used by NAV simple addresses” is true of the field generally, but should not be read as a guarantee this operation emits it. Synthetic tests prove acceptance, not NAV emission. Likewise recognition of `incorporation` in a 2.0 body is forward-compatible extension handling, not proof it existed in the tagged 2.0 schema.

### Envelope and diagnostic coverage (part of the complete response model)

| Documented content | What happens now | Assessment |
|---|---|---|
| `header/{requestId,timestamp,requestVersion,headerVersion?}` | Entire subtree ignored; no `TaxpayerInfo` fields | TP-01: traceability metadata omitted |
| `result/funcCode` | Required; OK produces info; every non-OK produces ApiError | Appropriate conservative success gate. Exact non-OK token retained only in fallback message when no errorCode/message is supplied |
| `result/errorCode?` | Numeric szamlazz code or unknown NAV string retained on error; missing → `ErrorCode::Absent` | Covered on error; deliberately does not manufacture code |
| `result/message?` | Decoded message retained on error; discarded on OK | TP-01 |
| `result/notifications/notification[]/{notificationCode,notificationText}` (N4 1.0, N1 §1.4.1) | Entire subtree ignored | TP-01; notifications do not change known OK into failure |
| `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact,softwareDevCountryCode?,softwareDevTaxNumber?}` | Entire subtree ignored | TP-01: Számlázz.hu/NAV request software metadata, not taxpayer business data |
| Future unknown elements/attributes | Ignored, not stored in an extension map | Forward-compatible acceptance, not lossless XML/JSON round-trip |

On OSA 3.0 `header` and its leaves are common-namespaced, `software` and its leaves API-namespaced; on 2.0 both are API-namespaced. They do not affect current extraction because neither subtree is read. N1 describes header/software as copies of NAV request data, so `header/timestamp` must not be confused with `infoDate` or a local receipt time.

Generic NAV `GeneralErrorResponse` / `GeneralExceptionResponse` roots are defined in primary NAV schemas, but T4 says this forwarding operation always returns QueryTaxpayerResponse. Supporting those generic roots is **not established as a Számla Agent requirement**. Their technical-validation-message trees are not fields missing from QueryTaxpayerResponse itself.

## 4. Detailed findings

### QX-01 — Foreign invoice children can populate recognized fields

**Defect · P3 · High confidence; live trigger unobserved.**

Locations: `src/ops/query_xml.rs:575` (namespace-insensitive serde deserialization), `695–751` (identity/type/reversal fields); `src/xml.rs:87–112` checks the root's namespace but only counts later starts. The same flaw applies to mapped seller, buyer, item and total children, not only `alap`.

Official contract: Q5, https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd, declares `targetNamespace="http://www.szamlazz.hu/szamla"` and `elementFormDefault="qualified"`; its reversal field is `<element name="sztornozott" type="boolean" ... minOccurs="0">`. W2, https://www.w3.org/TR/xml-names/#ns-qualnames, defines expanded name as “a pair consisting of a namespace name and a local name”, not local name alone.

**Reproduced counterexamples**, each applied independently to otherwise accepted `tests/synthetic/szamla_query.xml`:

```xml
<!-- Insert immediately before </alap>, where the original has no marker -->
<f:sztornozott xmlns:f="urn:foreign">true</f:sztornozott>
```

Actual: parse succeeds with `info.reversed == Some(true)`. This is not the schema's reversal field. Replacing the invoice-number element with `<f:szamlaszam xmlns:f="urn:foreign">OTHER-9</f:szamlaszam>` succeeds and returns `OTHER-9` as identity. Changing `<alap>` to `<alap xmlns="">` also succeeds. An **undeclared** `<x:sztornozott>true</x:sztornozott>` succeeds too.

Impact: a foreign same-local-name extension or incorrectly namespaced response can be interpreted as actual invoice identity/reversal data. A consumer branching on reversal can stop issuing or take reissue-related decisions on a value the invoice vocabulary did not supply. This is not demonstrated remote exploitability or observed vendor traffic; the low priority reflects an unobserved trigger and a trusted upstream, not uncertainty about the wrong interpretation.

Bounded remedy: apply namespace/parent-path identity to invoice recognized elements, as taxpayer parsing now does. Foreign optional fields must not supply values; foreign required identity fields must not satisfy required fields. Allow arbitrary prefixes bound to the right URI, retain unknown subtrees as ignored, and reject undeclared prefixes. Coordinate common namespace well-formedness with the shared-envelope owner; do not solve this by rejecting every extension or enforcing the whole XSD. Sparse/live-backed omissions are independent and must remain supported.

### QX-02 — A nonzero numeric response can silently become zero

**Defect · P3 · High confidence; extreme precision trigger, no live incidence.**

Locations: `src/xml.rs:431–442,495–503` pass text directly to `Decimal::from_str` through generic adapters. Example callers: `src/ops/query_xml.rs:734–735` exchange rate, `913–928` item quantities/amounts, `999–1004` financial amounts, `1050–1059` credit amounts/rates; `src/xml.rs:539–560` totals.

Official contract: Q5 https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd declares, for example, `<element name="mennyiseg" type="double" ...>`, `<element name="devizaarf" type="double" ...>` and `<element name="osszeg" type="double" ...>`, without a decimal-scale restriction. W1 https://www.w3.org/TR/xmlschema-2/#double defines both ordinary mantissa and exponent spellings for doubles. `0.00000000000000000000000000001` is a nonzero representable double, not mathematical zero.

**Reproduced** through public `QueryInvoiceXml::parse`:

| Input text | Actual result |
|---|---|
| `1E2` | Decimal 100 — correct; ordinary exponent support is present |
| `1e-2` | Decimal 0.01 — correct |
| `0.00000000000000000000000000001` | Decimal `0.0000000000000000000000000000` (zero) |
| `0.00000000000000000000000000002` | Also zero |
| `1e-29` | Parse error: `Scale exceeds the maximum precision allowed: 29 > 28` |
| `1.23456789012345678901234567895` | Rounded to `1.2345678901234567890123456790` |

Zero conversion was independently asserted for both optional invoice exchange rate and required credit-entry amount. The many-digit nonzero rounding case establishes additional loss of source precision, but the **nonzero→zero** case is the decisive value error; the finding does not depend on claiming arbitrary decimal digits are distinct IEEE-double values.

Impact: a parsed number may claim zero quantity/credit/rate when the source was nonzero, and equivalent finite numeric spellings have inconsistent failure behavior. The typed object/its JSON retains no original token to reveal this. This is **not** the documented live server rounding of invoice money to two decimals: the probe injects already-received XML, and the loss happens locally. P60 records ordinary monetary values and does not establish these extreme scales are emitted.

Bounded remedy: define an explicit response-number conversion policy. Retaining Decimal is sensible; reject unrepresentable nonzero values/precision loss consistently or offer raw numeric text where lossless representation is required. Do not silently round financial source data as a side effect of generic `FromStr`; do not switch all money to binary float. Exact exponent normalization and nonzero-underflow tests should distinguish lexical acceptance from value preservation. Supporting `INF`/`NaN` is not the recommended repair.

### TP-01 — The taxpayer result is a business projection, not the complete NAV response

**Capability gap · P3 · High confidence in omitted fields; current forwarding of messages/notifications unverified.**

Locations: `src/ops/taxpayer.rs:190–227` public response, `340–403` recognized paths, `595–625` result conversion. `Layout::child` recognizes only `funcCode`, `errorCode`, `message` under result. `into_info` drops even a recognized `message` on OK. Header/software/notifications have no destination.

Official evidence:

- T4 https://docs.szamlazz.hu/agent/querying_taxpayer/response actually contains `<header><requestId>38046_g2z6726bg67ymdt3p56bg6</requestId>...` and `<software><softwareId>SZAMLAZZHU34540973</softwareId>...` in its success example.
- N1 https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf §1.4.1, p.12: “A message opcionális szöveges üzenet, ami a funcCode-ot vagy az errorCode-ot kíséri.” (Optional message accompanying either function code or error code.) It describes notifications as future informational key/value messages, not new validity verdicts.
- N4 https://raw.githubusercontent.com/nav-gov-hu/Common/release/common-1.0.x/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd: `<xs:element name="notifications" type="NotificationsType" minOccurs="0">`, unbounded `notification` children containing `notificationCode` and `notificationText`.

**Reproduced**: add the following before `</common:result>` in the existing accepted mixed-namespace OSA 3.0 fixture:

```xml
<common:message>Advisory text</common:message>
<common:notifications>
  <common:notification>
    <common:notificationCode>FUTURE_NOTICE</common:notificationCode>
    <common:notificationText>Keep this notice</common:notificationText>
  </common:notification>
</common:notifications>
```

The parsed `TaxpayerInfo` equals the original result exactly; the added information is gone. Header/software absence follows directly from the same parser whitelist and public model inventory.

Impact: typed-client users cannot retain the NAV request correlation id, forwarded software/version, success advisory or notification for diagnostics without separately keeping/parsing `RawResponse`. This does **not** make `valid=true` wrong and does not establish any current notification delivery through szamlazz.hu. NAV explicitly leaves how much business information the caller uses discretionary (N1 p.68 point 5); this is an API completeness choice, not mandatory whole-response exposure by every SDK.

Bounded remedy if full response coverage is desired: add optional response metadata and open notification code/text records, retaining source/versioned path rules. Carry success messages as data, without turning notices into failure. Keep the existing business projection usable and do not automatically enlarge the Restate worker projection/journal. Alternatively document the projection and the raw-response escape hatch clearly. Old C3 is closed regardless of this separate capability decision.

## 5. Shared lexical helpers and preservation decisions

| Concern | Observed behavior | Review disposition |
|---|---|---|
| UTF-8 and complete document | `xml.rs:69–140` checks UTF-8, one completed expected root, trailing content, declaration placement and DTD rejection | Old R1 completeness repaired; selected regression tests passed. This is not claimed to be a complete XML conformance validator |
| Invoice child namespaces | Root checked; serde maps later children by local name | QX-01 |
| Taxpayer expanded names, nesting, duplicates | Root-selected API/common/component namespaces plus parent frames and per-container singleton set | Correct for documented 2.0/3.0 shapes |
| Business strings | `business_text` (`xml.rs:445–456`) makes absent/empty/XML-whitespace-only optional text None, otherwise preserves decoded characters; required strings also preserved | Old R2 repaired. Probe confirmed required seller name retained leading/trailing spaces, NBSP, entity CR and normalized literal CRLF |
| Taxpayer character data | Accumulates Text/CDATA/character/predefined entity references; ignores comments/PI; rejects undefined entities | `taxpayer.rs:475–507`; current tests prove mixed chunks and independent addresses |
| Dates | Complete XSD timezone suffix accepted, civil date retained without UTC shifting; optional missing/empty date None, malformed nonempty date errors | `xml.rs:367–427`; all eleven invoice date positions covered by tests. This is Agent behavior, distinct from Adatkapcsolat's invalid-date→None rule |
| Date domain | Legacy Jiff parse tried first; year zero/BCE forms retained by explicit tests, rather than full XSD 1.0 year-domain validation | Deliberate compatibility policy. No new strict era range recommended |
| Booleans | `true/false/1/0`; optional blank→None, nonoptional blank/default→false; unknown nonblank error | `xml.rs:459–487`; intentional empty leniency. Taxpayer validity blank on OK errors rather than fabricating false |
| Scalar whitespace | `empty_as_none`, `from_text`, bools, optional dates and taxpayer result/validity use Rust Unicode trim, broader than XML's four whitespace characters | Schema-nonconforming NBSP padding is accepted (numeric NBSP probe succeeds); a leniency note, not business-string loss or a high-impact defect |
| Numeric grammar | Ordinary exponent notation works; comma decimals error; underscores such as `1_000` accepted; Decimal finite range narrower than xs:double | Underscore acceptance is undocumented lexical leniency. `INF`/`NaN` errors are a reasonable financial-domain restriction, not a request for nonfinite money. Silent rounding is separately QX-02 |
| Integer widths | XSD int fields read as i64; negative/large-within-i64 values retained; overflow errors | Explicit module policy, `query_xml.rs:10–25`; padded required id and appearance parsed successfully in probe |
| VAT type/rate | Special `afatipus` wins in convenience `vat_rate()`, while raw numeric `afakulcs` string remains available; unknown type tokens survive | `query_xml.rs:451–457,494–499`; `xml.rs:530–580`. No arithmetic recomputation or enum rejection |
| Numeric VAT facets | Raw `afakulcs` string does not enforce double lexical grammar/minInclusive | Deliberate semantic preservation; do not mistake raw token storage for validated nonnegative numeric rate |
| Currency/payment/document tokens | Original payment versus unified value retained; known and unknown document/payment/VAT tokens supported | `query_xml.rs:240–265,413–416`; language and unified-payment strings avoid closing upstream sets |
| PDF | Standard base64 with whitespace removed; no PDF signature validation; invalid nonblank encoding rejects Agent result | `query_xml.rs:591–594`, `types.rs:104–117`; Q4's placeholder is not valid base64. Do not import Adatkapcsolat's lenient PDF semantics |
| Numeric text/unknown XML round-trip | Typed JSON preserves the projection, not raw XML spellings, unrecognized fields, namespace declarations or source element presence everywhere | Normal projection limitation; exact unknown tokens are preserved where modeled. TP-01 inventories known omissions |

Taxpayer optionality deserves a specific qualification: XSD `taxpayerValidity minOccurs=0` supports error responses without it; N1 p.68 says “Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra.” A missing validity on OK is therefore not safely equivalent to false. Current failure on that case is conservative and reasonable, not an uncovered valid-negative response. Conversely, valid=true with missing name/incorporation/address remains accepted as sparse data; it is not a claim of XSD validation.

## 6. Source conflicts and live-backed differences

1. **Q4 is an illustrative sparse record, not an XSD-valid exhaustive response.** It omits Q5-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer location/private-person, item ordering; it gives `fizmodunified=other`, outside Q5's Hungarian enum, and contains a prose PDF placeholder (“The receipt .pdf can be found here in BASE64 encoding”). Current optional/open fields intentionally accept its useful contents; raw placeholder PDF still errors. The existing upstream test documents replacing that placeholder before asserting other fields (`tests/upstream.rs:714–833`). Do not “fix” production parsing to accept the placeholder or call transformed XML an observed exchange.
2. **The response schema link on Q4 is imprecise.** It points readers to invoice-generation XML/XSD, whose request root is `xmlszamla`; Q5 is the actual `szamla` response schema and Q6 reproduces it. Request-only fields cannot be counted as missing response fields merely because of that link.
3. **Tagged NAV 2.0 examples and current NAV 3.0 guidance coexist.** T4 keeps its dated 2.0 api/data examples but links N1 3.0. Current two-layout parsing is appropriate. Neither the new link nor synthetic 3.0 fixtures establishes the currently forwarded account response version.
4. **Taxpayer example's schemaLocation is obsolete.** T2 names `http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd`; `fixtures/SOURCES.md:109` records it as broken and supplies T3. T3 was freshly fetched successfully; the obsolete URL was not re-fetched in this pass. No automatic source replacement was made.
5. **Primary NAV PDF/schema facets can drift.** N1 p.67 lists postal pattern `[A-Z0-9]{4,10}`, while N4's `PostalCodeType` allows 3..10 with internal whitespace/hyphen. Current response strings preserve both; this conflict supplies no reason to introduce a new regex gate. N6's 2.0 `countryCode` has `default="HU"`; current nonvalidating sparse reader keeps empty as None and does not invent a country, whereas N3 3.0 has no such default.
6. **Live observations stand:** external id not echoed; duplicate external id selects newest holder; query-by-order exact text and latest matching document; proforma may be consumed/absent; reverse marker absent before storno and present true after it; reversal can wipe credit entries; appearance 1 is paper, 3 electronic; buyer data may change after issuance. References: `docs/szamlazz-hu-behaviour.md:59–80,96–98,100–111,129–145`. No code change to force Q5-required optional fields, infer outstanding from incomplete payment state, add a seller/account pin, or normalize Agent order text is warranted.
7. **Recorded P60 money rounding is vendor-side and bounded.** Ordinary invoice values and queried numeric VAT tokens reflect source output (`docs/szamlazz-hu-behaviour.md:155–162`). This does not excuse QX-02's local extreme-scale rounding or prove its live occurrence.

## 7. Closed old findings and non-reopened items

These are dispositions of relevant rows from **old FINAL.md**, checked against current code and sources; unrelated error-code, receipt, HTTP and create-operation rows belong to other owners.

| Old ID | Current disposition | Evidence |
|---|---|---|
| F1, queried invoice date slice | **Closed** | `xml.rs:367–427`; all eleven invoice date positions use adapters; fresh 22-test query unit run includes all-position suffix/date-negative/empty tests |
| R1, taxpayer complete-document/path/entity/duplicate parsing | **Closed for the reported cases** | `taxpayer.rs:317–520`; `response_completion` and all 10 `taxpayer_paths` tests passed. Invoice namespace finding QX-01 is a separate remaining boundary; no claim of universal XML conformance closure |
| R2, queried invoice and taxpayer business-string loss | **Closed** | Separate business-text helper, invoice-number helper `query_xml.rs:1076–1082`, taxpayer text accumulator; fresh business-text/path tests and required-string probe |
| C3, five taxpayer fields | **Closed** | `short_name`, `county_code`, `vat_group_membership`, open `incorporation`, source-text `info_date` all exposed, extracted and tested in both layouts; new JSON optionals have serde defaults |
| D5, credit-entry bank-account meaning | **Closed** | `query_xml.rs:520–523` now states sender if known, otherwise printed bank account; agrees with Q6 |
| D7, queried-buyer mutability / identifier slice | **Closed** | `query_xml.rs:355–371` qualifies test-account mutation and distinguishes internal id from account-local partner identifier |
| N.AF, deductible-VAT unit | **Closed** | `query_xml.rs:486–489` calls it reported integer and explicitly refuses unsupported percentage/range interpretation |
| D10, credential-placement prose | **Closed within this operation** | `query_xml.rs:48–49` explicitly says no settings block; actual root-level writer is correct |
| E.E6, NAV version and lexical evidence slice | **Substantially closed for prior acceptance cases** | Genuine common/base versus api/data fixtures; semantic tests for paths, metadata, decoded text, all date positions. Existing tests did not cover QX-01/QX-02; this review adds independent evidence, not permanent implementation tests |

Previously deliberate behavior also retained: `InvoiceAppearance` is a code, `DocumentType` remains open, `afalevon` not forced into a percentage, sparse taxpayer records not rejected for missing optional business data, and `infoDate` not converted into an assumed UTC/caching timestamp. None is re-raised based solely on schema strictness.

## 8. Verification and limits

Fresh offline commands and results:

```text
cargo test -p szamlazz-agent --lib ops::query_xml::tests
  22 passed
cargo test -p szamlazz-agent --lib ops::taxpayer::tests
  15 passed
cargo test -p szamlazz-agent --test taxpayer_paths --test business_text --test response_completion
  10 + 2 + 1 passed
cargo test -p szamlazz-agent --test review_04_query_taxpayer_20260910 -- --nocapture
  final run: 2 passed (review assertions establish current counterexamples, not desired behavior)
```

The uniquely named temporary test was added with `apply_patch`, ran against the current public parsers, then was **removed with `apply_patch`**. It reused a synthetic accepted invoice and NAV 3.0 fixture; minimal mutations and outputs are recorded in findings above. Its first exploratory version printed the matrix; the final version additionally asserted foreign reversal, optional/required nonzero→zero conversion and loss of success message/notifications. No production file was edited. Other reviewers' temporary tests were left alone.

The targeted checks establish current implementation behavior and closure of selected old cases. They do not establish frequency of the counterexamples, actual production forwarding of new NAV fields, request acceptance against a live account, complete XSD/XML validator conformance, or exact server treatment of every optional request combination. No XML request was POSTed to either vendor. The coordinator owns the full suite. This report does not relabel old suite results or fixture acquisition as fresh live evidence.
