# Számla Agent XML/PDF queries — current-revision compliance review

**Reviewed revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`

**Review and source acquisition date:** 2026-09-10

**Scope:** XML/PDF invoice queries, their complete returned models, and shared XML, date, numeric, text, and PDF decoding on those paths.

## 1. Executive result

**All declared query business fields are represented.** The freshly downloaded `szamla.xsd` contains **125 child-element declarations in 19 complex structures**, counting reused address structures once and including the anonymous root structure. Every declaration maps to the current XML result. The PDF result exposes all six successful payload fields in its operation-specific response schema. Both writers support all three independent selectors, with correctly placed credentials, namespaces, and multipart action names.

Two remaining low-severity implementation issues were reproduced:

| ID | Severity / classification | Finding | Confidence |
|---|---|---|---|
| CQ-1 | P3 — returned-field fidelity | PDF-query `vevoifiokurl` is described as opaque but its XML value is Unicode-trimmed, losing surrounding characters, including NBSP. | High behavior/source; low evidence of vendor frequency |
| CQ-2 | P3 — XML robustness/conformance boundary | The shared whole-document check still accepts some non-well-formed XML, including illegal XML 1.0 characters in recognized business fields. | High behavior/standard; vendor emission unestablished |
| CV-1 | P3 — vendor-source conflict | HU PDF inline XSD is malformed and contradicts EN/download selector order and required presence. Current writer agrees with EN/download. | High conflict; deployed alternatives unverified |

**No P0/P1/P2 query defect or missing declared business field was established.** CQ-2 is a malformed-input robustness issue, not evidence of failure to parse a conforming vendor invoice. Exact Decimal representation, sparse content handling, raw/open tokens, and the retained Jiff date domain are documented below rather than conflated with missing fields.

The earlier report's Q-D1 retrieval-scope documentation omission is **closed** at this revision. Its Q-V1 remains a vendor conflict. Its broad claim that a datetime in a date position is refused needs correction: an **unzoned** datetime is accepted and reduced to its civil date.

## 2. Baseline and method

- `git rev-parse HEAD` returned the requested revision before inspection and again after checks. No tracked implementation changes were present initially. Concurrent work later changed `restate-szamlazz` files and added other reports/design files; the reviewed `szamlazz-agent` source, fixture provenance, and behavior notes remained unchanged. All unrelated work was left intact.
- Implementation and current official sources were reviewed first. The stale `docs/review/2026-09-10-agent-api-queries.md` was read only after the independent field comparison and initial lexical reproductions, for closure assessment.
- No delegation, account authentication, live Számla Agent operations, source edits, fixture edits, or new repository tests. This report is the sole repository artifact added by this review.
- Scratch code is under `/tmp/opencode/query-current-fbda137/`, created with `apply_patch`. It calls public `AgentRequest::parse`/`write_xml` against synthetic bodies and freshly fetched documentation samples. Its Python helper performs unauthenticated documentation GETs and structural comparisons.
- Code citations below are relative to `crates/szamlazz-agent/src/`, unless prefixed with `tests/`, `fixtures/`, or `docs/`. Every citation refers to the reviewed revision, not the stale report's line numbers.
- Findings distinguish current source declarations, bounded historical account observations, synthetic parser behavior, and unverified deployed behavior. In particular, Adatkapcsolat's shared `<szamla>` annotations explain field meaning; they do not extend the Agent query's retrieval surface.

## 3. Current primary sources and controlling statements

The two requested category pages were fetched: [XML category][xc] and [PDF category][pc]. All six linked request/response/XML+XSD pages were fetched in **both EN and HU**, including their inline examples and schemas. The four relevant downloadable schemas were fetched independently. Current vendor pages displayed site build `v202608271632`; this is not a date for every statement.

| Source | Statement / evidence used |
|---|---|
| [XML request EN][xr], [HU][xhr] | “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved via this interface”; HU: “csak belső (Számlázz.hu-ban kiállított) kimenő számlák adatait lehet lekérni.” `POST`, `multipart/form-data`, `action-szamla_agent_xml`. |
| [PDF request EN][pr], [HU][phr] | `action-szamla_agent_pdf`; identification by invoice number, order number **or** external identifier. “if multiple documents share the same order number, the last one is returned”; external id must have been set at creation. |
| [XML XML/XSD EN][xx], [HU][xhx], [download][xd] | “the order of the fields is fixed, they cannot be interchanged”; direct-root credentials, `szamlaszam`, `rendelesSzam`, optional boolean `pdf`, `szamlaKulsoAzon`. |
| [PDF XML/XSD EN][px], [HU][phx], [download][pd] | Direct-root credentials and selectors, required integer `valaszVerzio`; source conflict CV-1. |
| [XML response EN][xs], [HU][xhs] | Success: “Full `szamla` XML document”; failure: `xmlszamlavalasz`, `<sikeres>false</sikeres>`, code/message. Unknown selector: **7**, covering number/order/external id. |
| [PDF response EN][ps], [HU][phs] | Version `2`: “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; `1`/omitted: raw PDF or text error. “In both cases, additional parameters may also arrive in the HTTP response header.” |
| [Full response schema][sx] | `targetNamespace="http://www.szamlazz.hu/szamla"`, `elementFormDefault="qualified"`; complete returned field/type inventory in §6. |
| [Envelope download][sd], PDF response inline schemas | Nine declarations: verdict/error fields and six payload fields: `szamlaszam`, `szamlanetto`, `szamlabrutto`, `kintlevoseg`, `vevoifiokurl`, `pdf`. `vevoifiokurl` is `string`; amounts are `double`; PDF is `base64Binary`. |
| [Shared outgoing annotations EN][ae], [HU][ah] | Appearance: “0: nem számla, 1: papír számla, 2: e-számla, 3: e-számla.” Credit-entry bank account: sender's account, otherwise account printed on invoice when sender unknown. `fizmod` may be arbitrary text; `fizmodunified` is normalized. |
| [Invoice-create XML page][ix] | Followed the XML-response page's schema link. It describes `xmlszamla`, the **request** schema, rather than the queried `szamla` schema. This navigation does not make its waybill/layout fields returned query fields. |
| [Invoice-create response][ir] | Shared header vocabulary: invoice number/error text URL-encoded, net/gross not URL-encoded; payment method and buyer-facing URL listed. This is supplementary evidence, not a PDF-specific guarantee of every header's presence. |
| [Error handling][eh] | Version-1 `[ERR]…` text format and operation links; query-specific code 7 comes from the query pages. |
| [W3C XML 1.0][xmlspec] | `Char ::= #x9 | #xA | #xD | [#x20-#xD7FF] | [#xE000-#xFFFD] | [#x10000-#x10FFFF]`; `]]>` cannot occur as ordinary character data; attribute values exclude literal `<`; attributes require separation. |
| [W3C XSD datatypes][datatypes] | `string` represents character strings; boolean literals are `{true, false, 1, 0}`; `double` permits a decimal mantissa and optional integer exponent, plus `INF`, `-INF`, `NaN`; date has a distinct lexical space. These define lexical comparisons, not observed vendor monetary values. |

### Fresh acquisition fingerprints

SHA-256 was calculated from bytes fetched during this review, not copied from the earlier report:

| Download | SHA-256 | Compared with workspace corpus |
|---|---|---|
| [xmlszamlaxml.xsd][xd] | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` | Byte-identical |
| [xmlszamlapdf.xsd][pd] | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` | Byte-identical |
| [szamla.xsd][sx] | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` | Byte-identical |
| [xmlszamlavalasz.xsd][sd] | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` | Byte-identical |

The EN/HU shared outgoing inline schemas independently matched the download's complex-structure child names/types/presence/multiplicity. The manual review also compared their numeric facets and token enumerations. No includes/imports in these four downloads require an additional schema to expand this inventory.

The freshly acquired HU PDF XML/XSD HTML hash was `04fb76c65a80106a45df9bf8d8b3bfc05dd0cece62e7a2c067627b47d60ebc95`; EN was `bc1f0711108ab37ec15f58003b6e53789e3d27874ce3de5197e581c2391df666`. The extracted, unmodified HU schema failed Python ElementTree at **line 1, column 140**. Fresh XML-response HTML hashes were EN `87ead94008aa5323e3ecca7d22b0df29bcd4fc24a912bf0c1ccb0d35a68f7ad3`, HU `911b32b7bbc0e0b54216f188542ca373a0ba1f9d2653bbc8d5fa2a6640ec4fb1`; HTML byte differences from a prior acquisition are not themselves schema changes.

## 4. Findings and concrete reproductions

### CQ-1 — PDF-query opaque XML URL loses surrounding characters

**P3, field-fidelity issue. High confidence in reproduction and schema mapping; occurrence in vendor output unverified.**

**Location:** `ops/envelope.rs:104–117`, specifically `vevoifiokurl` at `:114–115`, uses `xml::de::empty_as_none`. That helper at `xml.rs:559–571` applies `str::trim` before parsing even a `String`. `Body::customer_account_url` at `ops/envelope.rs:135–143` returns the already-trimmed body value. `ops/query_pdf.rs:50–53,83–92` exposes it as an “Opaque buyer-facing URL”.

**Source:** [PDF response inline schema][ps] and [download][sd]: `<element name="vevoifiokurl" type="string" ... minOccurs="0">`. No query-source rule was found prescribing Unicode trimming of this field. Other queried optional business text uses the preservation helper `xml.rs:573–585`.

**Reproduction through `QueryInvoicePdf::parse`:**

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <szamlaszam>I</szamlaszam>
  <vevoifiokurl>&#160;opaque:x&#160;</vevoifiokurl>
  <pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

Result: `customer_account_url == Some("opaque:x")`, rather than `Some("\u{a0}opaque:x\u{a0}")`. `<vevoifiokurl>  opaque:x  </vevoifiokurl>` similarly returns `"opaque:x"`. The body remains well-formed XML and uses the declared string type. `opaque:x` is a synthetic marker, not a claimed vendor URL format.

**Impact:** the typed result and subsequent JSON cannot reproduce the supplied string; consumers comparing, storing, or interpreting an opaque link receive an altered value. No broken real customer link was established. Ordinary vendor example URLs contain no boundary whitespace, so this is not evidence that current downloads fail in normal use.

**Bounded remedy:** apply the business-text preservation policy to `vevoifiokurl`, retaining decoded nonblank characters and body-before-header precedence. Add a focused body/header fidelity control including NBSP. Do not percent-decode the XML URL. Invoice-number trimming in the shared envelope is an explicitly stated separate policy (`:120–132,298–301`), not folded into this finding.

### CQ-2 — Whole-document checking is not complete XML well-formedness checking

**P3, malformed-input robustness/conformance boundary. High confidence. No conforming-response rejection, vendor emission, exploitation, or live incident established.**

**Location:** `xml.rs:63–167` scans to EOF, checks root, nesting, comments, declaration/PI placement and namespace bindings, and iterates attributes. It does not validate XML 1.0 character ranges throughout literal text/CDATA, validate every general reference in ignored data, or fully validate ordinary start-tag syntax. `protocol_text` at `:174–229` filters namespaces, then `ops/query_xml.rs:588–608` deserializes only known fields. The PDF path shares these checks through `ops/envelope.rs:281–287`.

**Sources:** [XML-query response][xs] declares an XML document; [W3C XML 1.0][xmlspec] defines legal `Char`, names, attribute syntax, and ordinary `CharData`. In particular, U+0000 and U+0001 are outside XML 1.0's `Char` production. These are lexical well-formedness requirements, independent of the invoice XSD's business-field requirements.

**Reproduction:** take `fixtures/synthetic/agent/szamla_query.xml` and pass each independent mutation through `QueryInvoiceXml::parse` with `RawResponse::new([], body)`:

| Mutation | Current result | Independent reference |
|---|---|---|
| Replace `Synthetic Supplier Kft.` with literal `A\u{0000}B` | Success; `supplier.name` contains NUL | ElementTree refuses illegal token |
| Replace it with `A&#x1;B` | Success; `supplier.name` contains U+0001 | ElementTree refuses illegal character reference |
| Replace it with `A]]>B` | Success; `supplier.name == "A]]>B"` | ElementTree refuses illegal character-data delimiter |
| Insert `<extension>&unknown;</extension>` before `</szamla>` | Success; ignored unknown entity in unknown subtree | ElementTree refuses undefined entity |
| Insert `<extension a='1'b='2'/>` | Success | ElementTree refuses absent attribute separator |
| Insert `<extension a='<bad'/>` | Success | ElementTree refuses literal `<` in attribute value |
| Insert `<1invalid/>` | Success | ElementTree refuses invalid name |

Controls: `A&#0;B` and `A&B` in a recognized name fail; an undeclared namespace prefix fails even inside an ignored subtree; rebinding reserved `xml` to `urn:wrong` fails. Existing tests also reject truncation, trailing roots/text, bad declaration separators and invalid PIs. The finding is narrower than claiming the entire existing shape guard is ineffective.

**Impact:** successful parsing does not establish that the received body was well-formed XML. Illegal control characters can reach public business strings and persistence/export systems; malformed ignored content can be silently discarded. There is no evidence here that foreign fields can bypass the now-working identity/reversal namespace checks. Severity is low because the vendor contract does not authorize these bodies and vendor emission was not observed.

**Bounded remedy:** if the shared layer is to establish XML well-formedness, validate these lexical conditions on the original document before ignoring subtrees, or use an XML reader mode/library that does. Keep unknown *well-formed* extensions and sparse-content compatibility. This does not call for enforcing XSD field order or all XSD-required content.

### CV-1 — HU PDF request schema conflict remains unresolved

**P3 vendor clarification, not a confirmed Rust defect. High confidence in source conflict; medium confidence about all deployed request alternatives.**

- [HU inline][phx]: `szamlaszam` has `minOccurs="1"`; sequence tail is `szamlaszam`, `valaszVerzio`, `rendelesSzam`, `szamlaKulsoAzon`.
- [EN inline][px] and [download][pd]: `szamlaszam` has `minOccurs="0"`; tail is `szamlaszam`, `rendelesSzam`, `valaszVerzio`, `szamlaKulsoAzon`.
- HU inline is also syntactically malformed: `...xmlszamlapdf"xmlns:tns=...` has no attribute separator. Parsing the unmodified extracted code block fails at column 140.
- [HU request prose][phr] says number/order/external id are alternatives (“vagy”), agreeing with EN prose and the crate's independent selectors.
- Current writer: `ops/query_pdf.rs:62–80`, matching EN/download.

**Concrete consequence:** after repairing the HU schema's syntax, an order-only request such as `<rendelesSzam>O</rendelesSzam><valaszVerzio>2</valaszVerzio>` still conflicts with its required number and ordering. An external-id-only request conflicts with its required number. All twelve generated credential/selector/operation combinations pass structural checks against the fresh downloads.

**Disposition:** retain current writer order and selectors; ask the vendor to align the HU schema or document accepted alternatives. Historical PDF external-id success (`docs/szamlazz-hu-behaviour.md:63–68`) supports that capability, not every possible PDF order-number sequence. No new live probe was performed.

## 5. Request, envelope, PDF, and header coverage

### Requests

| Surface | Declared contract | Current implementation |
|---|---|---|
| Endpoint/method | `https://www.szamlazz.hu/szamla/`, POST | `wire.rs:7–14,395–400`; operation wire request is transport-independent. |
| Multipart XML file | `multipart/form-data`; `action-szamla_agent_xml` / `action-szamla_agent_pdf` | `wire.rs:66–99`; action constants `ops/query_xml.rs:535–537`, `ops/query_pdf.rs:58–60`. |
| XML root/namespace | `xmlszamlaxml`, `http://www.szamlazz.hu/xmlszamlaxml` | `ops/query_xml.rs:539–558`; UTF-8 declaration/default namespace via `xml.rs:19–40`. |
| PDF root/namespace | `xmlszamlapdf`, `http://www.szamlazz.hu/xmlszamlapdf` | `ops/query_pdf.rs:62–81`. |
| Credentials | Optional strings `felhasznalo`, `jelszo`, `szamlaagentkulcs` directly under root, in this order | `xml.rs:456–465`: one key or username/password; no `beallitasok`. |
| Invoice-number selector | `szamlaszam`, string | `InvoiceSelector::InvoiceNumber`; exact supplied text XML-escaped. |
| Order-number selector | `rendelesSzam`, string; newest match | `InvoiceSelector::OrderNumber`; no trimming/case-folding. |
| External-id selector | `szamlaKulsoAzon`, string; supplied at creation | `InvoiceSelector::ExternalId`; emitted last. No claim of uniqueness. |
| XML PDF option | `pdf`, optional boolean | `include_pdf: bool`, default false; explicitly writes true/false before external id (`ops/query_xml.rs:57–73,552–555`). |
| PDF response version | `valaszVerzio`, required int in schemas | Always `super::RESPONSE_VERSION` (`ops/query_pdf.rs:75`), value 2; no legacy-mode setting. |

Selectors are `types.rs:1034–1053`. The enum makes **one selected field** representable; its string values can still be empty. There is no query-specific nonblank validation. This is not a new schema violation: the schemas type selectors as unrestricted strings and put the usable-selector rule on the server. Writing `xsi:schemaLocation` is unnecessary for namespace identity and is not an omitted business field.

### Returned envelope and PDF

| Wire field / outcome | Type and mapping | Evidence |
|---|---|---|
| XML success `<szamla>` | `InvoiceDocument`, full inventory below | `ops/query_xml.rs:563–609`. |
| XML failure `<xmlszamlavalasz>` | `sikeres`, `hibakod`, `hibauzenet` → typed API error, including body-only 7 | `ops/query_xml.rs:580–586`; `xml.rs:319–350`; test `ops/query_xml.rs:1784–1798`. |
| Successful generic envelope on XML query | Refused, even if number exists | `ops/query_xml.rs:583–586`: correct operation-shape distinction. |
| PDF `sikeres` | Required boolean verdict; all four XSD forms handled | Shared `xml::Verdict`; empty is permissively false, not success. |
| PDF `hibakod`, `hibauzenet` | Optional strings → `ApiError`; unknown codes preserved, absent code not invented | `xml.rs:323–344`, shared envelope verdict processing. |
| PDF `szamlaszam` | `InvoiceNumber`, required for usable PDF success, body then decoded header | `ops/envelope.rs:120–133,271–275`; `ops/query_pdf.rs:40–41,87`. |
| PDF `szamlanetto`, `szamlabrutto` | `Option<Decimal>` net/gross, body then numeric header | `ops/query_pdf.rs:42–45,88–89`; `ops/envelope.rs:158–168,224–238`. |
| PDF `kintlevoseg` | `Option<Decimal>` outstanding; absence not zero | `ops/query_pdf.rs:46–49,90`; `ops/envelope.rs:239–244`. |
| PDF `vevoifiokurl` | `Option<String>` customer URL; body then decoded header; CQ-1 applies | `ops/query_pdf.rs:50–53,91`; `ops/envelope.rs:135–143`. |
| PDF `pdf` | Required decoded `Pdf` on successful query | `ops/query_pdf.rs:54–55,92`; missing/empty is `Missing("pdf")`, malformed base64 fails. |

The shared XSD makes all payload fields optional because it also represents failures; requiring identity and PDF in a successful dedicated PDF result is coherent with its advertised operation. No supported successful PDF-without-number example was found.

`Pdf::from_base64` (`types.rs:104–117`) strips whitespace and uses standard base64 decoding. It holds bytes, not a parsed PDF document: no magic/content verification or streaming. It accepts Unicode whitespace as well as XML whitespace; standalone empty base64 yields zero bytes, but query envelope empty handling makes a blank `<pdf>` absent. XML query returns `None` for absent/blank PDF even if requested, decodes any nonblank PDF even if not requested, and fails the whole result on invalid base64 (`ops/query_xml.rs:604–607`). That is an observable all-or-error policy; the vendor's placeholder text is not a valid-PDF counterexample.

### Header processing

- `RawResponse` normalizes header names, retains raw values, and exposes case-insensitive first-match access (`wire.rs:182–199,227–249`). Encoded textual headers decode once; `+` becomes space, `%2B` becomes literal plus. Numeric/code headers are read without percent decoding.
- Query checks run in order: nonblank `szlahu_down`, error header, known non-2xx status, then body (`wire.rs:273–311`). HTTP-200 body-only errors work. Body-only errors at HTTP 500 become `HttpStatus`, an explicit current policy rather than an assertion about who produced the status.
- PDF net/gross/outstanding use body-before-header. Empty body values permit fallback; malformed nonempty body values fail rather than falling back. Money headers accept ungrouped dot/comma decimals and finite exponents; grouping and mixed separators fail (`ops/envelope.rs:316–342`). P60 observed a comma header `100,01` (`docs/szamlazz-hu-behaviour.md:160`).
- XML URLs receive XML entity decoding, not HTTP percent decoding. Header URLs are decoded once. CQ-1 identifies the remaining body trim.
- PDF parsing inherits the shared **numbered code-56** exception (`ops/envelope.rs:179–249`). A synthetic failure-56 body carrying number and valid PDF returns `InvoicePdf`; notification-warning metadata is not exposed on that result. This is a shared implementation behavior, **not** a documented PDF-query notification event. A header-only 56 with no PDF still fails the dedicated query's PDF requirement. No current source establishes 56 emission for PDF retrieval, so this is recorded as an unverified policy, not a new practical defect.
- `document_id`, `payment_method`, and notification flag parsed by `CreatedInvoice` are not projected into `InvoicePdf` (`ops/query_pdf.rs:86–93`). The PDF-specific schema declares none of these. Its prose mentions additional headers generically; that does not establish that all create-only metadata must be public PDF fields. Raw headers remain accessible to integrations using `RawResponse`.
- The XML `<szamla>` schema has no `kintlevoseg` or customer-account URL element. The XML result does not merge unrelated success headers into the document. No missing declared XML body field follows from that choice.

## 6. Complete queried-document inventory

Source: [fresh `szamla.xsd`][sx], corroborated by EN/HU inline shared-document schemas. **R** = XSD `minOccurs=1`; **O** = `minOccurs=0`; default maximum is one. `?` means `Option<T>`. Grouped rows enumerate every member explicitly; mappings in a row are in the same order. All optional business strings below use `business_text` unless stated otherwise: absent or XML-whitespace-only → None, otherwise decoded characters preserved.

### 6.1 Root and reusable structures — 9 + 4 + 5 + 2 declarations

Public models: `ops/query_xml.rs:76–147,324–339`. Wire/conversions: `:618–703,802–826,915–919,1000–1004,1045–1056`.

| Path / declaration | XSD | Public mapping / parser |
|---|---|---|
| `szamla/szallito` | R complex | `supplier: Supplier`, required |
| `szamla/alap` | R complex | `info: InvoiceInfo`, required |
| `szamla/vevo` | R complex | `buyer: BuyerInfo`, required |
| `szamla/tetelek` | R complex | `items: Vec<DocumentItem>` through `tetel`; wrapper required |
| `szamla/qutetek` | O complex | `financial_items: Vec<FinancialItem>`, missing/empty → empty |
| `szamla/cimkek` | O complex | `labels: Vec<String>` through `cimke` |
| `szamla/osszegek` | R complex | `totals: Totals`, required |
| `szamla/kifizetesek` | O complex | `credit_entries: Vec<RecordedCreditEntry>`, missing/empty → empty |
| `szamla/pdf` | O string | `pdf: Pdf?`; nonblank text base64 decoded per operation documentation |
| `cimTipus/orszag` | O string | `Address.country: String?` |
| `cimTipus/irsz`, `telepules`, `cim` | R strings | `Address.zip`, `city`, `address: String`; missing refused, present empty accepted |
| `cimpostaTipus/nev`, `orszag`, `irsz`, `telepules`, `cim` | O strings | `BuyerPostalAddress.name`, `country`, `zip`, `city`, `address: String?` |
| `bankTipus/nev`, `bankszamla` | O strings | `Bank.name`, `account: String?` |

`cimTipus` is used for supplier billing **and postal** addresses and buyer billing address. Buyer postal address uses the genuinely different `cimpostaTipus`; no receiver name is declared on supplier postal address.

### 6.2 Supplier — 8 declarations

Public `ops/query_xml.rs:125–147`; wire/conversion `:672–703`.

| `szallito/…` | XSD | `Supplier` field/type |
|---|---|---|
| `id` | R int | `id: i64?`; missing/empty tolerated, malformed integer refused |
| `nev` | R string | `name: String` |
| `cim` | R `cimTipus` | `address: Address` |
| `postacim` | O `cimTipus` | `postal_address: Address?` |
| `adoszam` | R string | `tax_number: String?` |
| `csoportazonosito` | O string | `group_id: String?` |
| `adoszameu` | O string | `eu_tax_number: String?` |
| `bank` | O `bankTipus` | `bank: Bank?` |

### 6.3 Core invoice data — 28 declarations

Public `ops/query_xml.rs:225–322`; appearance `:149–223`; wire/conversion `:705–800`; optional reference-number helper `:1089–1096`.

| `alap/…` | XSD | `InvoiceInfo` field/type |
|---|---|---|
| `id` | R int | `id: i64`, required |
| `szamlaszam` | R string | `invoice_number: InvoiceNumber`, required element; unrestricted wire string |
| `gazdEsemAzon` | R int | `economic_event_id: i64?` |
| `forras` | O int | `source: i64?`; unknown codes preserved |
| `iktatoszam` | O string | `registration_number: String?` |
| `tipus` | R string | `document_type: DocumentType`, open token |
| `eszamla` | R int | `appearance: InvoiceAppearance`, i64-backed open code |
| `hivszamlaszam` | O string | `referenced_invoice_number: InvoiceNumber?`, nonblank text preserved |
| `hivdijbekszam` | O string | `referenced_proforma_number: InvoiceNumber?`, same |
| `kelt` | R date | `issue_date: Date?` |
| `telj` | R date | `fulfillment_date: Date?` |
| `fizh` | R date | `due_date: Date?` |
| `fizmod` | R string | `payment_method: PaymentMethod?`, open token |
| `fizmodunified` | R restricted string | `unified_payment_method: String?`, open |
| `keszpenz` | R boolean | `cash_payment: bool`, missing/empty → false |
| `rendelesszam` | O string | `order_number: String?`, preserved; **lowercase response spelling**, unlike request |
| `nyelv` | R restricted string | `language: String?`, open |
| `devizanem` | R string | `currency: Currency?`, open string newtype |
| `devizabank` | O string | `exchange_bank: String?` |
| `devizaarf` | O double | `exchange_rate: Decimal?` |
| `megjegyzes` | O string | `comment: String?` |
| `afatipus` | O string | `vat_type: String?`, invoice-level value |
| `penzforg` | R boolean | `cash_accounting: bool`, missing/empty → false |
| `kata` | R boolean | `kata: bool`, missing/empty → false |
| `katafokonyv` | R boolean | `kata_ledger: bool`, missing/empty → false |
| `email` | O string | `email: String?`, document-associated email |
| `teszt` | R boolean | `test: bool?`, missing/empty → unknown, not live |
| `sztornozott` | O boolean | `reversed: bool?`, absence distinct from explicit false |

The current operation rustdoc explicitly states internal-outgoing retrieval (`:51–53`). `source` is qualified as shared-schema metadata without expanding that boundary (`:239–241`). Appearance maps `0 → NotInvoice`, `1 → Paper`, `2/3 → Electronic(code)`, other integers → `Unknown(code)`; exact code survives JSON. The `szamla` download types every listed numeric metadata element as `int`; the crate intentionally widens them to i64 (`:10–25`) rather than enforcing a 32-bit response-width gate.

### 6.4 Buyer and buyer ledger — 12 + 6 declarations

Public `ops/query_xml.rs:341–398`; wire/conversion `:828–913`.

| Path | XSD | Public mapping |
|---|---|---|
| `vevo/id` | O int | `BuyerInfo.id: i64?` |
| `vevo/nev` | R string | `name: String` |
| `vevo/azonosito` | O string | `identifier: String?`, account-local partner identifier, distinct from numeric id |
| `vevo/cim` | R `cimTipus` | `address: Address?`, omission tolerated |
| `vevo/postacim` | O `cimpostaTipus` | `postal_address: BuyerPostalAddress?` |
| `vevo/email` | O string | `email: String?`, separate from `alap/email` |
| `vevo/adoszam` | R string | `tax_number: String?` |
| `vevo/csoportazonosito`, `adoszameu` | O strings | `group_id`, `eu_tax_number: String?` |
| `vevo/lokacio` | R int | `location: i64?`; 1 domestic, 2 EU, 3 outside EU, -1 unknown, future integers preserved |
| `vevo/privatePersonIndicator` | R boolean | `private_person: bool`, missing/empty → false |
| `vevo/fokonyv` | O complex | `ledger: BuyerLedgerInfo?` |
| `vevo/fokonyv/vevo`, `vevoazon` | O strings | `BuyerLedgerInfo.account`, `buyer_id: String?` |
| `vevo/fokonyv/datum` | O date | `date: Date?` |
| `vevo/fokonyv/folyamatostelj` | O boolean | `continuous_fulfillment: bool?` |
| `vevo/fokonyv/elszDatTol`, `elszDatIg` | O dates | `settlement_from`, `settlement_to: Date?`; mixed case handled by serde rename |

### 6.5 Printed items and item ledger — 14 + 6 declarations

Public `ops/query_xml.rs:400–463`; wire/conversion `:921–998`.

| `tetelek/tetel/…` | XSD | `DocumentItem` mapping |
|---|---|---|
| `nev` | R string | `name: String` |
| `azonosito` | O string | `id: String?` |
| `mennyiseg` | R double | `quantity: Decimal` |
| `mennyisegiegyseg` | R string | `unit: String` |
| `nettoegysegar` | R double | `unit_price: Decimal` |
| `afatipus` | O restricted string | `vat_type: String?` |
| `afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`; preserved token, helper interprets |
| `netto` | R double | `net_value: Decimal` |
| `arresafaalap` | O double | `margin_vat_base: Decimal?` |
| `afa` | R double | `vat_value: Decimal` |
| `brutto` | R double | `gross_value: Decimal` |
| `megjegyzes` | O string | `comment: String?` |
| `sztetordering` | R int | `ordering: i64?`, omission tolerated |
| `fokonyv` | O complex | `ledger: DocumentItemLedger?` |
| `fokonyv/arbevetel`, `afa` | O strings | `revenue_account`, `vat_account: String?`; this `afa` is an account, not money |
| `fokonyv/gazdasagiesemeny`, `gazdasagiesemenyafa` | O strings | `economic_event`, `vat_economic_event: String?` |
| `fokonyv/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?`; lowercase, unlike buyer ledger |

No sorting, sign filtering, or recalculation: negative storno quantities and values survive; `ordering` is metadata, not an instruction to reorder the returned vector. `vat_rate()` prioritizes nonblank `afatipus` over numeric `afakulcs` (`:456–462`).

### 6.6 Financial items — 10 declarations

Public `ops/query_xml.rs:465–505`; wire/conversion `:1006–1043`.

| `qutetek/qutet/…` | XSD | `FinancialItem` mapping |
|---|---|---|
| `nev` | R string | `name: String` |
| `afatipus` | O restricted string | `vat_type: String?` |
| `afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`, same special-code precedence |
| `netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross: Decimal` |
| `elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?` |
| `afalevon` | R int | `deductible_vat: i64`, required; no inferred percentage/unit/range |
| `cimkek` | O complex | `labels: Vec<String>` |

`qutet`'s speculative QUiCK expansion is explicitly qualified in rustdoc. The current vendor schema establishes no unit for `afalevon`, so preserving the integer is appropriate.

### 6.7 Totals — 2 + 5 + 3 declarations

Wire/projection `xml.rs:638–721`; public `types.rs:1055–1107`.

| Path | XSD | Public mapping |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded complex | `Totals.by_vat_rate: Vec<VatTotal>`, absence tolerated as empty |
| `osszegek/totalossz` | R complex | `Totals.total: GrandTotal`, required |
| `afakulcsossz/afatipus` | O restricted string | `VatTotal.vat_type: String?` |
| `afakulcsossz/afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`, special type first in `vat_rate()` |
| `afakulcsossz/netto`, `afa`, `brutto` | R doubles | `VatTotal.net`, `vat`, `gross: Decimal` |
| `totalossz/netto`, `afa`, `brutto` | R doubles | `GrandTotal.net`, `vat`, `gross: Decimal` |

### 6.8 Credit entries and remaining list declarations — 7 + 4 declarations

Public `ops/query_xml.rs:507–533`; wire/conversion `:1058–1087`; wrappers `:915–919,1000–1004,1045–1056`.

| Path | XSD | Public mapping |
|---|---|---|
| `kifizetesek/kifizetes/datum` | R date | `RecordedCreditEntry.date: Date`, required/nonempty |
| `…/jogcim` | R string | `title: PaymentMethod`, open token |
| `…/osszeg` | R double | `amount: Decimal` |
| `…/megjegyzes` | O string | `comment: String?` |
| `…/bankszamlaszam` | O string | `bank_account: String?`, sender if known, otherwise printed account |
| `…/banktranzid` | O int | `bank_transaction_id: i64?` |
| `…/devizaarf` | O double | `exchange_rate: Decimal?` |
| `tetelekTipus/tetel` | 1..unbounded | Vector; present empty wrapper tolerated |
| `qutetekTipus/qutet` | 0..unbounded | Vector |
| `kifizetesekTipus/kifizetes` | 1..unbounded | Vector; present empty wrapper tolerated; **no five-entry query cap** |
| `cimkekTipus/cimke` | 0..1 string | `Vec<String>`, permits multiple labels as a lenient extension; reused at root and qutet |

**Inventory total:** 9 + 4 + 5 + 2 + 8 + 28 + 12 + 6 + 14 + 6 + 10 + 2 + 5 + 3 + 7 + 4 = **125**. List wrappers are counted in their own complex structures, not counted again for each runtime instance.

**Fields not declared on this response:** `fuvarlevel` and carrier sub-blocks, request-style `arfolyam` object, erasure codes, layout/preview flags, and `szamlaKulsoAzon`. Exchange information is `alap/devizabank`, `alap/devizaarf`, and credit-entry `devizaarf`. Their absence from the returned model is not a missing supported response capability.

## 7. Shared lexical parsing and open tokens

### 7.1 Numeric values

Every amount/quantity position in §6 uses `xml::de::decimal` or `optional_decimal` (`xml.rs:475–493`) and `number::parse` (`number.rs:63–141`). The shared PDF envelope uses the same exact parser after body/header handling. There is no intermediate floating-point conversion.

- Finite decimal/exponent grammar supports signs, `.5`, `1.`, leading/trailing zeros, and `e`/`E` exponents. Scratch confirmed `1e-2 → 0.01`; existing tests confirm `100e-30 → 1e-28`, arbitrary insignificant zeros, and `Decimal::MAX`.
- Normalization cancels trailing coefficient zeros before checking the Decimal domain; representable source scale is retained where possible. Exponent expansion is bounded before allocation (`number.rs:95–135`).
- `1e-29`, excess significant precision, and overflow fail rather than silently rounding. `NaN` and `INF` fail. This is a deliberate finite/exact monetary domain **narrower than XSD double**, not full double-value-space compliance. No source or historical query established these exceptional monetary values as emitted. Changing to f64 would trade away an explicit fidelity policy.
- Optional missing/blank values become None; required empty amounts fail. XML commas/grouping/underscores fail; comma support belongs to HTTP monetary headers only.
- Optional integer metadata uses trimmed `FromStr<i64>` (`xml.rs:559–571`); `afalevon` uses required `from_text` (`:623–631`); `alap/id` and appearance use serde i64 directly. Signs/XML whitespace work; decimal/exponent spellings for integers fail; overflow beyond i64 fails.
- Several helpers use Unicode `trim` rather than XML's four whitespace characters: scratch `<devizaarf>NBSP 1 NBSP</devizaarf>` returns 1. This is permissive lexical acceptance, not altered valid monetary data. It differs from `business_text`, which preserves NBSP.
- `afakulcs` is intentionally raw `String` at all three positions; the parser does not enforce its double/nonnegative facet. Its helper interprets a representable numeric token, otherwise preserves it in `VatRate::Other`. A special `afatipus` takes precedence. Unrepresentable numeric tokens stay available as raw text; response parsing does not silently turn them into another amount.

The `number.rs:4–59` arithmetic helpers are not used to recompute queried totals. They were inspected for context, but this review makes no comprehensive statement about request-side calculated-line arithmetic.

### 7.2 Dates — eleven positions

Ten optional dates (three core, three buyer ledger, two item ledger, two financial item) plus the required credit-entry date use `xml.rs:495–555`. `ops/query_xml.rs:1333–1469` exercises every position.

| Input | Current behavior | Classification |
|---|---|---|
| `2026-01-09` | Civil 2026-01-09 | Ordinary supported date |
| `2026-01-09Z`, `2026-01-09+14:00`, negative/zero offsets | Same printed date, offset discarded | XSD date spellings supported without shifting day |
| Invalid calendar day, `+14:01`, `+01:60`, trailing junk | Parse error | Correct refusal of tested malformed content |
| Multibyte malformed values | Parse error; no panic | Checked `split_at_checked` at `xml.rs:505–509` |
| Empty/omitted optional date | None | Existing leniency |
| Empty/omitted required credit-entry date | Error | Required field contract |
| `2026-01-09T12:34:56` | **Civil 2026-01-09** | Legacy Jiff acceptance; time silently discarded |
| `2026-01-09T12:34:56Z` | Error | Does not imply all datetime spellings are refused |
| `2026-01-09[Europe/Budapest]` | Civil 2026-01-09 | Legacy annotation acceptance |
| `20240229`, `0000-02-29`, `-000001-02-28` | Accepted by existing tests | Legacy domain, not strict XSD 1.0 date grammar |
| `-0001-01-09`, `10000-01-09` | Error in fresh probes | Incomplete XSD year-lexical/domain coverage |

The initial `value.parse::<Date>()` and immediate return (`xml.rs:499–502`) explain acceptance outside XSD date syntax before suffix checks. This is a retained compatibility policy stated in the helper and tested in part, not a new regression established here. Consequently, neither “complete xs:date validator” nor “rejects every datetime” accurately describes it. BCE/extended-year invoice emission and Jiff annotations from szamlazz.hu were not observed. These are representational/compatibility limits, not promoted to normal-invoice failures.

Malformed nonempty optional dates fail the **Agent** result. Adatkapcsolat's separate tolerant content policy must not be assumed to apply to this outbound query reader.

### 7.3 XML paths, text and namespaces

- Root and namespace checks use expanded names, not prefix spelling: accepted XML roots are `szamla` in `http://www.szamlazz.hu/szamla` and failure `xmlszamlavalasz` in its own namespace. PDF accepts the latter envelope. Missing/wrong namespaces fail; an `https` URI is not equivalent to the documented `http` namespace.
- Foreign subtrees are excluded before serde's local-name matching (`xml.rs:169–229`), including descendants that re-enter the protocol namespace. Unknown same-namespace wrappers are skipped, not searched for nested identity/reversal fields. Alias prefixes work; escaped namespace URI characters are decoded (`:231–248`).
- A placeholder remains for an ignored foreign child inside scalar content so `tr<foreign/>ue` cannot become `true` (`:195–200`). Tests exercise identity/reversal spoofing, foreign verdicts, aliases and undeclared prefixes (`tests/response_namespaces.rs:16–145`).
- Full EOF scan rejects truncation, second roots, trailing text, misplaced declarations, invalid PI targets and malformed comments (`tests/response_completion.rs:12–83,102–124`). CQ-2 records remaining well-formedness gaps; these tests do not prove general XML certification.
- XML field matching follows the private struct parent paths. `tetelek/tetel/fokonyv/afa` is a ledger account, not a document tax amount; buyer dates' mixed-case names and item dates' lowercase names match the schema.
- Recognized singletons are serde fields; duplicates fail. Lists retain repeated values. Field sequence is not XSD-validated on input; a reordered known response can parse. Unknown fields are skipped and are not retained in `InvoiceDocument` as raw XML.
- Required string elements may be empty; optional business strings preserve surrounding decoded characters, entities, CDATA and NBSP, with XML whitespace-only absence. `tests/business_text.rs:9–47` asserts character-reference and CRLF behavior for invoice fields and reference numbers. CQ-1 is the shared PDF-envelope exception.
- XML declaration validation accepts only version 1.0, validates the encoding **name's syntax**, and parses bytes as UTF-8. It is not arbitrary encoding transcoding; a declared alternate encoding is not used to decode bytes. DTDs are refused. No vendor source fetched here requires UTF-16 or DTD-bearing query responses.

### 7.4 Open tokens and public serialization

- `DocumentType` (`types.rs:758–858`) names `SZ`, `D`, `ES`, `VS`, `HS`, `SS`, `SL`; `JS` from the annotated shared example and future strings are retained as `Other(String)`. Named-variant absence is not token loss.
- `InvoiceAppearance` preserves every i64 code, including unknowns. The adjacent vendor example comment calling it “string” conflicts with its XSD int, while the documented 0/1/2/3 meanings agree with the code.
- `PaymentMethod` (`types.rs:590–687`) retains arbitrary `fizmod` and `jogcim` text via `Other`; English example tokens `credit_card` and `transfer` need not be converted to Hungarian variants. `fizmodunified` stays an open string; the example's `other` is retained despite not appearing in the Hungarian XSD enumeration.
- `VatRate` (`types.rs:178–345`) preserves special strings including `TEHK` and future tokens. All 21 schema special tokens are representable; twenty have named variants and `TEHK` uses `Other`. Numeric helper interpretation preserves the raw field separately.
- `nyelv` retains all fifteen schema language tokens and future strings; `Currency` keeps the returned token rather than normalizing `Ft` to `HUF`. `forras`/`lokacio` are open integer metadata.
- Public results serialize Rust field names, monetary Decimal strings, civil-date strings, appearance integers, wire-token strings, and PDFs as base64. Same-version full JSON round trips pass. `InvoicePdf.outstanding` and `customer_account_url` have serde defaults for older JSON (`ops/query_pdf.rs:48,52`; `tests/response_headers.rs:355–359`). This is not a guarantee that arbitrary incomplete JSON or future unknown fields round-trip losslessly.

## 8. Source defects, fixture provenance, and justified deviations

### Provenance

`fixtures/SOURCES.md:3–39,58–79,90–140` distinguishes workspace-only vendor acquisitions from packaged synthetic samples and generated golden expectations. Most query corpus files record acquisition on **2026-07-04**. Cached schemas were not assumed current: all four relevant files were freshly fetched and compared byte-for-byte in this review.

The project-modified invoice-create XSD (`fixtures/SOURCES.md:126–140`) is not the queried `szamla.xsd`. Its added request fields cannot prove a missing response field. Upstream tests read files at runtime and can skip outside the workspace (`tests/upstream.rs:15–55`); the corpus was present in this run. Request-outline tests deliberately erase empty containers and boundary whitespace (`tests/upstream.rs:1225–1234`), so their success is not independent proof of exact request semantics. The separate scratch check examined actual generated trees against fresh declaration order/presence.

### Current example defects

1. **Both XML-query success examples contain a prose PDF placeholder:** “The receipt .pdf can be found here in BASE64 encoding.” Both PDF-query success examples contain `....` in base64. Fresh unmodified examples fail decoding as expected. Replacing **only `<pdf>` content** with synthetic `JVBERi0=` allows all four success bodies to parse. This substitution tests the remaining fields, not a real vendor PDF. Both fresh PDF error examples parse as code 3, with their original EN/HU message-spacing difference.
2. **Sparse example versus XSD:** the XML query example omits XSD-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio`, buyer `privatePersonIndicator`, and row `sztetordering`; several strings are empty. Optional/defaulted handling is necessary to consume that published sample after PDF substitution. Making all XSD R fields mandatory would regress this compatibility.
3. **Arithmetic disagreement in sample:** printed item 380/76/456 versus totals 464/93/557. The parser correctly reports each value without repair/recalculation.
4. **HU request sample `<pdf></pdf>`** conflicts with boolean lexical grammar, but its comment directs true/false. The writer emits valid booleans.
5. **PDF version:** response prose describes omitted/1 legacy mode, while request XSD requires `valaszVerzio`. Explicit version 2 avoids the discrepancy and supplies the requested PDF capability.
6. **Schema navigation:** the XML response page links invoice generation for its schema set, but the actual queried schema is `szamla.xsd`, also linked by the shared outgoing annotations. Request `xmlszamla.xsd` is not interchangeable.
7. **EN/HU semantic caveat:** English shared-document prose reverses the nesting in one sentence (“`<alap>` tag within `<tipus>`”); the HU sentence, example, schema, and implementation correctly place `tipus` within `alap`. English translated payment labels do not redefine Hungarian wire tokens.
8. **`cimke` maxOccurs=1** versus public vectors is permissive extra capacity, not dropped schema data. Multiple-label vendor emission was not established. The example's `sztetordering` “double” annotation conflicts with XSD int; code follows int.

### Bounded historical account evidence to preserve

The behavior notes explicitly concern one TEST account and dates, with raw probe logs outside the repository (`docs/szamlazz-hu-behaviour.md:3–28`). They were read before classifying deviations.

| Historical observation | Citation | Query consequence |
|---|---|---|
| Query by order is exact/case-sensitive; newest document may be any kind | `:40–45` | Preserve selector bytes; do not adopt worker key normalization in the client |
| Shared external id resolves newest holder; PDF external-id retrieval worked; external id not echoed | `:63–71` | Keep independent external selector; do not infer uniqueness or invent returned id |
| Reversal marker absent on live and storno, true on reversed original; credit entries removed on reversal | `:77–80` | Preserve `Option<bool>` and empty credit-entry vector |
| `telj` present on observed documents, mandatory in XSD | `:96` | Optional model is leniency, not evidence of a live missing-date case |
| Paper 1 and electronic 3 observed; 2 only documented | `:97–98,264–267` | Correct numeric appearance semantics; retain code 2 and unknowns |
| A later create changed queried buyer data | `:111` | Qualified mutability warning at `ops/query_xml.rs:360–365`; no immutable-at-issuance promise |
| Five returned credit entries had non-submission ordering | `:134` | Retain server order without assuming temporal/submission order |
| Query code 7 body-only; consumed proforma also absent from query surface | `:141–142` | Parse body errors; 7 does not mean never existed |
| Vendor monetary storage rounds values independently; VAT returned as `27.0` | `:160–162` | Report stored values exactly; normalize only helper interpretation, not raw returned fields |

Not every permissive default has live evidence. Empty lists, unknown extensions, i64 widening, absence tolerance and legacy date spellings include existing library compatibility choices. None establishes a new vendor guarantee.

## 9. Earlier-report closure

Compared only after the independent review with `docs/review/2026-09-10-agent-api-queries.md` (its baseline was `382cf761…`, not this revision):

| Earlier statement/finding | Current assessment |
|---|---|
| Q-D1: missing internal-outgoing retrieval restriction | **Closed.** `ops/query_xml.rs:51–53` states the limit, and `:239–241` qualifies shared `source`. Fresh EN/HU request wording still supports it. |
| Q-V1: HU PDF schema | **Still open as vendor ambiguity**, CV-1. Same conflict freshly reproduced. No Rust writer change justified. |
| Complete returned-field coverage | **Reconfirmed independently:** 125 declarations/19 structures and all PDF payload fields. |
| XSD timezone date/padding fix | **Closed for those cases:** all eleven positions pass. Does not imply complete XSD date-domain conformance. |
| “a datetime in place of a date” is refused (`old :258`) | **Incorrect as a blanket statement.** Unzoned `2026-01-09T12:34:56` parses to Date; zoned form tested fails. Retained Jiff policy, not a newly introduced regression. |
| Optional business text loss fixed | **Confirmed for full `<szamla>` fields/reference numbers.** Do not extend that conclusion to PDF `vevoifiokurl`; CQ-1 reproduces its remaining trim. |
| Missing PDF outstanding/customer URL | **Closed as capability gaps.** Fields, body/header precedence and old JSON defaults exist. URL fidelity is the narrower CQ-1. |
| Numeric rounding/exponent issues fixed | **Reconfirmed:** exact finite parser and regression cases pass. Wider XSD double domain remains a stated limit. |
| Trailing/incomplete XML and namespace-local-name issues fixed | **Closed for the specific tested cases.** CQ-2 is a separate residual well-formedness limitation, not a reassertion that those fixes failed. |
| Bank-account direction, buyer mutability, afalevon meaning, appearance | **Closed/reconfirmed:** current field docs match source or qualify historical observations (`ops/query_xml.rs:149–175,360–365,491–494,525–528`). |

Thus the previous report's documentation finding closes, but an unqualified “no remaining issues” or general XML/date-validation guarantee would overstate the current evidence.

## 10. Checks, reproducibility and limits

### Existing tests executed

```text
cargo test -p szamlazz-agent --lib ops::query
  29 passed
cargo test -p szamlazz-agent --lib xml::tests
  27 passed (substring also selects the 22 query_xml tests above)
cargo test -p szamlazz-agent --test numeric_fidelity --test response_namespaces --test response_completion --test business_text --test response_headers --test upstream
  37 passed: 6 + 6 + 2 + 2 + 10 + 11
```

**71 distinct selected tests passed; 93 test executions including overlap; zero failures.** Default crate features. Shared suites include other-operation controls; their pass is not a review of those operations. No ignored live test ran. Corpus fixtures were available; “passed” is not a claim that the abbreviated vendor PDFs were decoded successfully.

### Scratch reproductions executed

```text
cargo run --offline --manifest-path /tmp/opencode/query-current-fbda137/Cargo.toml
python3 /tmp/opencode/query-current-fbda137/check.py
```

The Rust executable also accepts `xml` or `pdf` as its first argument and reads a body from stdin. `check.py` fetches schemas/examples, runs the probe, compares current declarations, and checks malformed XML against Python ElementTree. The probe depends on the current local crate. Relevant resolved versions match workspace `Cargo.lock`: quick-xml 0.42.0, Jiff 0.2.35, rust_decimal 1.43.0, serde 1.0.229, base64 0.23.1.

Results beyond existing tests:

- Twelve actual writer combinations: two credential variants × three selectors × two operations. Correct namespace, ordered recognized children, required presence, maximum-one occurrences, one selector, and one credential form. Selectors/credentials included characters requiring XML escaping. This was **structural comparison**, not full XSD validation.
- Independent current count and complete manual mapping of the 125 schema declarations; EN/HU shared inline complex structures agreed with download.
- Fresh success samples for all four language/operation combinations failed as expected on placeholder/abbreviated PDF content and parsed after explicitly synthetic PDF replacement; two PDF error samples produced typed code 3.
- CQ-1/CQ-2 reproduced through public operation parsing; Python's independent XML parser refused the listed malformed lexical controls.
- Valid date offsets, invalid offset/junk controls, Jiff datetime/annotation behavior, integer lexical forms, exact numeric domain limits, Unicode numeric/base64 whitespace, and inherited numbered-56 PDF behavior recorded above.

Limits: no full workspace/client-feature matrix, live account exchange, browser behavior, full XSD engine validation, exhaustive XML test suite, fuzzing campaign, PDF content validation, or proof of all vendor-emitted values. The two guessed documentation paths `/agent/basics/response` and `/agent/basics/response-format` returned 403; actual linked query pages and supplementary invoice-response/error pages were fetched successfully. No inference rests on those inaccessible guessed paths. Sources are current unversioned documentation, not contractual evidence that every deployment emits every declared field.

[xc]: https://docs.szamlazz.hu/agent/category/query-document-xml
[pc]: https://docs.szamlazz.hu/agent/category/query-document-pdf
[xr]: https://docs.szamlazz.hu/agent/querying_xml/request
[xhr]: https://docs.szamlazz.hu/hu/agent/querying_xml/request
[xx]: https://docs.szamlazz.hu/agent/querying_xml/xml
[xhx]: https://docs.szamlazz.hu/hu/agent/querying_xml/xml
[xs]: https://docs.szamlazz.hu/agent/querying_xml/response
[xhs]: https://docs.szamlazz.hu/hu/agent/querying_xml/response
[xd]: https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd
[pr]: https://docs.szamlazz.hu/agent/querying_pdf/request
[phr]: https://docs.szamlazz.hu/hu/agent/querying_pdf/request
[px]: https://docs.szamlazz.hu/agent/querying_pdf/xml
[phx]: https://docs.szamlazz.hu/hu/agent/querying_pdf/xml
[ps]: https://docs.szamlazz.hu/agent/querying_pdf/response
[phs]: https://docs.szamlazz.hu/hu/agent/querying_pdf/response
[pd]: https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd
[sx]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[sd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[ae]: https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ah]: https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ix]: https://docs.szamlazz.hu/agent/generating_invoice/xml
[ir]: https://docs.szamlazz.hu/agent/generating_invoice/response
[eh]: https://docs.szamlazz.hu/agent/basics/error-handling
[xmlspec]: https://www.w3.org/TR/xml/
[datatypes]: https://www.w3.org/TR/xmlschema-2/
