# Számla Agent XML/PDF queries — independent official-definition review

**Date:** 2026-09-11. **Starting revision and reviewed HEAD:** `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

## Conclusions

**No definite ordinary-document interoperability bug or omitted current query-schema field was confirmed.** Both requests implement all three selectors. The XML result maps all **125 child-element declarations across 19 complex structures** in the freshly downloaded `szamla.xsd` (126 declarations including the root; reusable types counted once). The PDF result represents all **six successful XML payload fields** in its operation-specific schema.

| Classification | Conclusion / disposition |
|---|---|
| Definite implementation bug | None established in this scope. No P0/P1/P2 defect demonstrated. |
| Current capability omission | No missing declared query field or selector. Legacy PDF response-mode selection, raw-XML preservation and auxiliary shared headers are narrower interfaces, inventoried below rather than asserted to be missing query XML fields. |
| Source disagreement, P3 | **S1:** Hungarian PDF-request schema is malformed and conflicts with EN/download on selector optionality and order. Keep the source-backed writer; request corrected vendor documentation. |
| Unconfirmed valid-response assumption, P3 | **A1:** successful PDF parsing requires a reported nonblank number, although the common success/error XSD makes it optional. The rejection is reproducible; actual vendor emission of such a successful PDF remains unconfirmed. |
| Optional identity hardening, P3 | **H1:** XML query accepts blank invoice numbers. They satisfy the published unrestricted `xs:string`, but do not supply usable number identity. No vendor occurrence established. |
| Source/example limitations | **S2:** illustrative PDFs are not valid base64; sparse examples disagree with their XSD; historical/current schema differences must remain distinct. |
| Unconfirmed operation-specific semantics | **A2:** PDF parsing inherits the numbered-56 issuance exception and drops its notification flag. Reproduced, but no query-specific vendor emission or notification behavior established. |

Priority describes follow-up urgency, not an observed production incident. Representation limits are explicit below; the conclusion does **not** claim acceptance of every value in the full XSD `double` or `date` domain.

## Method, scope and evidence discipline

- This is a whole-current-implementation audit, not merely a diff review. The supplied starting commit equals HEAD, so `git diff 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2...HEAD` is empty. Read all production models, wire structs and conversions in `ops/query_xml.rs`, `ops/query_pdf.rs`, and the necessary `ops/envelope.rs`, `xml.rs`, `number.rs`, `types.rs`, `wire.rs` pieces.
- Code locations below are relative to **`crates/szamlazz-agent/src/`**, unless otherwise stated. They describe the current files at the reviewed revision.
- Independently fetched all twelve EN/HU operation request, response and XML/XSD pages, their current schema downloads, shared outgoing-document annotations, and the linked historical Agent archive. Read the current schemas themselves, not just cached fixtures or existing assertions.
- Consulted the untracked `2026-09-11-agent-api-77d53c5-queries.md` as a lead after inspecting the implementation and primary definitions. Its conclusions/test counts are not verification results for this review. Existing reports, including all untracked `77d53c5` reports, were preserved.
- Checked `docs/szamlazz-hu-behaviour.md` and the actual dated research evidence before any exemption. Unsent vendor questions are not vendor answers; executable probe source is not execution evidence.
- Only unauthenticated public documentation GETs and local parser/writer checks were used. No credentials, authenticated requests, live tests or additional agents. No product edits. Broad Cargo suites were left to the parent as requested; this review ran a separate, targeted scratch public-API executable.

## Fresh primary-source register

Fetched on the review date; site footer **`v202608271632`** is a documentation build identifier, not a live verification date.

| Ref | Official URL | Use |
|---|---|---|
| XR | https://docs.szamlazz.hu/agent/querying_xml/request | POST endpoint, multipart action, three selectors, internal-outgoing retrieval boundary |
| XX | https://docs.szamlazz.hu/agent/querying_xml/xml | Request example, complete inline schema, fixed field order |
| XA | https://docs.szamlazz.hu/agent/querying_xml/response | Full `szamla` success, error envelope, code 7, example |
| HXR | https://docs.szamlazz.hu/hu/agent/querying_xml/request | Hungarian retrieval/selector requirements |
| HXX | https://docs.szamlazz.hu/hu/agent/querying_xml/xml | Hungarian example and inline schema |
| HXA | https://docs.szamlazz.hu/hu/agent/querying_xml/response | Hungarian response and example |
| PR | https://docs.szamlazz.hu/agent/querying_pdf/request | PDF multipart action and alternative selectors |
| PX | https://docs.szamlazz.hu/agent/querying_pdf/xml | PDF request example and inline schema |
| PA | https://docs.szamlazz.hu/agent/querying_pdf/response | Version 1/2, headers, success/error examples and schema |
| HPR | https://docs.szamlazz.hu/hu/agent/querying_pdf/request | Hungarian alternative selectors |
| HPX | https://docs.szamlazz.hu/hu/agent/querying_pdf/xml | Conflicting Hungarian inline schema |
| HPA | https://docs.szamlazz.hu/hu/agent/querying_pdf/response | Hungarian optionality and response definitions |
| XD | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Download from XX sample's `xsi:schemaLocation` |
| PD | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Download from PX sample's `xsi:schemaLocation` |
| SD | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Entire queried-document schema; no imports/includes |
| ED | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Shared envelope, independently identical in structure to PA/HPA |
| OUT | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak | Shared document annotations/example and inline SD |
| HOUT | https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak | Original-language semantics and inline SD |
| CX | https://docs.szamlazz.hu/agent/generating_invoice/xml | Actual destination of XA's schema link; creation request, not query response |
| CA | https://docs.szamlazz.hu/agent/generating_invoice/response | Shared envelope and named header encoding conventions |
| ERR | https://docs.szamlazz.hu/agent/basics/error-handling | Linked error guidance and legacy text format |
| HOME | https://docs.szamlazz.hu/agent/ | Navigation and historical archive link, labelled last updated 2019-03-13 |
| ZIP | https://docs.szamlazz.hu/assets/files/SzamlaAgent-eab03f119308ff908bbf46c7cde5d1bb.zip | Historical XML/PDF query schemas/examples/forms and `xml/szamla/szamla.xsd` |

**Navigation qualification:** XA/HXA names `szamla.xsd` but links to CX's `xmlszamla` creation schema. OUT/HOUT's sample supplies SD's actual absolute URL. Adatkapcsolat annotations help interpret the same document structure; they do not enlarge XR/HXR's “only … internal outgoing invoices (issued in Számlázz.hu)” boundary. Its invoice **Ack** is a different schema. Two guessed response-schema routes (`/xsds/szamla.xsd`, `/xsds/agentxml/szamla.xsd`) returned 404; guessed `/agent/basics/response` returned 403. The actual registered sources above were obtained successfully.

Fresh `<pre>` extraction includes hidden/tabbed schemas. Structural comparison (element names/attributes/children, excluding comments/formatting) established **XX = HXX = XD**, **PX = PD**, **OUT = HOUT = SD**, **PA = HPA = ED**. HPX cannot be parsed as XML.

| Download | Bytes | SHA-256 |
|---|---:|---|
| XD | 1,039 | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| PD | 1,035 | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| SD | 20,049 | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| ED | 1,242 | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |
| ZIP | 1,739,697 | `b70bc43e7dfd7960b6234e8df6916d7a3aa5e10b841d20ae7d26b84e992e420a` |

Full fresh HTML hashes, URLs, extracted blocks and archive-member list are retained locally in `/tmp/opencode/queries-28dcec1/manifest.json`. These scratch files are reproduction material, not permanent repository attachments.

## Findings and actionable follow-up

### A1 — Unconfirmed valid-response assumption: PDF success requires a number

**Location:** `ops/query_pdf.rs:39–41,83–93`; `ops/envelope.rs:120–132,211–217,275–279,326–329`.

PA/HPA: version 2 returns structured `xmlszamlavalasz` with base64 PDF; headers “may also arrive”; `szamlaszam` is `string minOccurs="0"`. Only `sikeres` is mandatory in the schema covering **both success and failure**. The parser instead requires a nonblank number from the body or decoded header before it can construct `InvoicePdf`.

Targeted reproduction, HTTP 200, no headers:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

Fresh ED validation: **valid**. Current `QueryInvoicePdf::parse`: **`Parse(Missing("szamlaszam"))`**. `JVBERi0=` encodes `%PDF-`, deliberately an artifact-decoding control, **not a complete PDF or proof of a legitimate successful vendor response**. The code's identity check does not inspect PDF syntax, so replacing it with a complete PDF does not remove the number requirement. Adding a body number, or header `szlahu_szamlaszam=I%2B1`, succeeds; the latter reports `I+1`.

**Impact if the vendor permits this shape with a real PDF:** an otherwise usable download is lost to a parse failure. Order/external-id requests cannot infer a reported number from their selector. **Not established:** that the vendor emits this success, or guarantees a number on every success. Existing external-id PDF observations establish downloads on the tested account, not universal reply cardinality. Research `2026-09-11-agent-vendor-clarification.md:3,52–60` explicitly remains unsent/unanswered.

**Recommendation:** obtain a success-specific guarantee for all three selectors. If legitimately absent, expose an honestly unnumbered fetched artifact; do not substitute request identity and label it a vendor echo. If guaranteed, document the semantic requirement separately from the shared XSD. Missing PDF remains an appropriate failure for this PDF-fetch result.

### S1 — Source disagreement: Hungarian PDF-request XSD

**Location:** `ops/query_pdf.rs:62–79`.

- EN PX and download PD: `szamlaszam? → rendelesSzam? → valaszVerzio → szamlaKulsoAzon?` after credentials.
- HU HPX: **required** `szamlaszam → valaszVerzio → rendelesSzam? → szamlaKulsoAzon?`.
- HU HPX also has `targetNamespace="…xmlszamlapdf"xmlns:tns=…` without separating whitespace. Parsing the untouched extracted schema fails at **line 1, column 140**.
- Both PR and HPR expressly say invoice number, order number **or** external identifier. Merely fixing the missing whitespace leaves the semantic conflict.

All twelve current writer combinations (XML/PDF × number/order/external × key/password) validate against the fresh downloadable schemas. The writer's exact sequence agrees with PD/PX. Historical ZIP also contains the older query definitions; it is not authority to override the current download.

**Recommendation:** retain the writer and ask the vendor to reconcile HPX with PR/HPR/PX/PD. Do not “fix” an order-only request by adding an invented invoice number or changing order to match the broken HU source. `docs/research/2026-09-10-agent-vendor-questions.md:74–87` already records an unsent question. Live external-id PDF evidence supports that selector's usability, not a fresh comparison of alternate element orders.

### H1 — Optional hardening: blank XML-query number

**Location:** `ops/query_xml.rs:714–715,775–776`; transparent wire `InvoiceNumber`, `types.rs:22–32`.

SD declares `alap/szamlaszam` as required unrestricted `xs:string`, without `minLength`. OUT/HOUT describes the invoice's unique number. Replacing the all-fields specimen's number with empty text succeeds and preserves `""`; a padded `" I-1 "` remains padded. Missing element fails, but empty presence does not.

This is **not a literal schema violation**, and no live empty-number response was found. A consumer that assumes parse success establishes usable number identity could nevertheless store an empty key or make an unusable subsequent request; numeric `alap/id` remains present.

**Recommendation:** choose and document whether query success guarantees nonblank number identity. A response-local non-XML-whitespace check can enforce that without importing the worker's stricter input alphabet or normalizing nonblank source text. Otherwise explicitly state the weaker presence/type guarantee.

### A2 — Unconfirmed PDF-specific numbered-56 semantics

**Location:** `ops/query_pdf.rs:83–93`; `ops/envelope.rs:179–248,285–315`.

The shared issuance parser tolerates `sikeres=false`, code 56, a number and a decoded PDF. The current PDF projection then discards `notification_delivery_failed`. Fresh local control with `<hibakod>56</hibakod><szamlaszam>I-1</szamlaszam><pdf>JVBERi0=</pdf>` returns `Ok(InvoicePdf)`. False/code 7 plus invalid PDF correctly returns code 7 instead.

PA/HPA documents false/code/message errors but no retrieval notification action or code-56 query emission. The behavior record says even attempted creation email failures did **not** produce 56 (`docs/szamlazz-hu-behaviour.md:173,200–201`). Therefore this is a confirmed shared-code behavior, **not a demonstrated valid PDF-query refusal swallowed in production**.

**Recommendation:** decide explicitly whether retrieval should use the issuance exception; if retained, document its metadata policy. Seek query-specific evidence before assigning creation semantics to a reported 56. No broad issuance-parser change is justified by this query-only synthetic control.

### S2 — Source disagreements and illustrative artifacts

1. **Invalid artifact placeholders:** XA/HXA `<pdf>` contains prose (“The receipt .pdf can be found here in BASE64 encoding”); PA/HPA embeds `....` in the base64. All four fresh verbatim success examples fail base64 decoding. Replacing only the PDF text with `JVBERi0=` lets each parse. These are documentation artifacts, not evidence that a valid PDF fails. Removing dots would not reconstruct the missing file.
2. **Sparse/inconsistent XML example:** XA/HXA omits SD-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio` and `privatePersonIndicator`, and item `sztetordering`. `fizmodunified=other` is outside SD's Hungarian enumeration. Item amounts are `380/76/456`, totals `464/93/557`. Current optional/open fields admit the example, and the parser preserves reported amounts rather than recomputing them. This is direct official-example evidence for leniency, not a claim those omissions were all captured live.
3. **HU request placeholder:** HXX shows empty `<pdf>` despite declaring boolean; the comment instructs true/false. Rust emits explicit booleans.
4. **Legacy version optionality:** PA/HPA describes omitted version as legacy mode while PX/PD requires `valaszVerzio`. Always sending 2 satisfies the supported structured path.
5. **Annotation/type conflicts:** OUT/HOUT calls `eszamla` a string and `sztetordering` a double in comments; SD declares both `int`. Rust reads integers. Appearance 1=paper and 3=electronic is additionally supported by P73 observations; 2 is documented but not observed.
6. **Historical external-id echo:** freshly downloaded ZIP member `xml/szamla/szamla.xsd` contains optional `alap/szamlaKulsoAzon`; current SD and OUT/HOUT do not. `docs/szamlazz-hu-behaviour.md:87` records no external-id echo. This is **not** a missing current response field. Historical byte-preserving archival would require a different interface.

**Recommendation:** correct vendor examples/navigation and keep current, historical, inline and downloadable definitions distinguishable. Never infer response fields from the creation request schema to fill perceived gaps.

## Complete request coverage

`O` = schema 0..1, `R` = 1..1. Schema order below is XD/PD order. Credential and selector elements are individually optional in XSD; useful authentication/selection is a semantic requirement. No schema default/fixed values affect these writers.

| Field/surface | XML query | PDF query | Current implementation |
|---|---|---|---|
| Endpoint/method/encoding | HTTPS POST multipart XML file | Same | `wire.rs:14,66–100`; exact XR/PR action selection |
| Action | `action-szamla_agent_xml` | `action-szamla_agent_pdf` | `query_xml.rs:540–548`; `query_pdf.rs:58–65` |
| Root / namespace | `xmlszamlaxml` / `http://www.szamlazz.hu/xmlszamlaxml` | `xmlszamlapdf` / `http://www.szamlazz.hu/xmlszamlapdf` | Exact; no settings wrapper |
| `felhasznalo`, `jelszo` | O string each | O string each | Password branch directly under root, in order; `xml.rs:628–637` |
| `szamlaagentkulcs` | O string | O string | Alternative key branch at root |
| `szamlaszam` | O string | O string in PD/PX | Only invoice-number selector; `query_xml.rs:550–556`, `query_pdf.rs:68–74` |
| `rendelesSzam` | O string | O string before version | Only order selector, correct case; queried response is lowercase `rendelesszam` |
| `pdf` | O boolean | Not declared | Always explicit `include_pdf`; constructor false, `query_xml.rs:65–73,557` |
| `valaszVerzio` | Not declared | R int | Shared `RESPONSE_VERSION=2`, `query_pdf.rs:75` |
| `szamlaKulsoAzon` | O string after PDF flag | O string after version | Only external-id selector; `query_xml.rs:558–560`, `query_pdf.rs:76–78` |
| Escaping/character gate | Shared | Shared | `xml.rs:586–596`, `wire.rs:405–409,415–440`; generated `<`/`&` selector/credential controls validate |

`InvoiceSelector` (`types.rs:1035–1054`) selects exactly one branch; empty strings remain constructible. Local empty-selector refusal would be convenience validation, not a missing selector. No documented batch query or combined-selector precedence is omitted. The request does not trim/case-fold the selector: recorded padded order queries return 7 (`behaviour.md:59–64`). “Last” is the vendor's single returned match, not uniqueness or proof of ownership. `xsi:schemaLocation` is a validation hint, not a required request payload field.

## Complete XML response field inventory

Source for **every** row: fresh SD, independently matched against OUT/HOUT. `s`=string, `i`=int, `d`=double, `b`=boolean; `?` in the Rust column means `Option`. All singleton schema maxima are 1, including root PDF's implicit maximum. Containers/lists are separately inventoried. Reused types are counted once.

### Root, addresses and bank (9 + 4 + 5 + 2 declarations)

Code: `ops/query_xml.rs:80–147,328–343,595–613,623–675,807–830`.

| Wire declaration | Schema | Rust mapping / cardinality reading |
|---|---|---|
| `szamla/szallito` | R container | `supplier: Supplier` |
| `szamla/alap` | R container | `info: InvoiceInfo` |
| `szamla/vevo` | R container | `buyer: BuyerInfo` |
| `szamla/tetelek` | R container | Required wrapper → `items` |
| `szamla/qutetek` | O container | Absent → `financial_items=[]` |
| `szamla/cimkek` | O container | Absent → `labels=[]` |
| `szamla/osszegek` | R container | `totals: Totals` |
| `szamla/kifizetesek` | O container | Absent → `credit_entries=[]` |
| `szamla/pdf` | O s | Nonblank base64 → `?Pdf`; absence/blank → `None` |
| `cimTipus/orszag` | O s | `Address.country: ?String` |
| `cimTipus/{irsz,telepules,cim}` | R s each | `Address.{zip,city,address}: String` |
| `cimpostaTipus/{nev,orszag,irsz,telepules,cim}` | O s each | `BuyerPostalAddress.{name,country,zip,city,address}: ?String` |
| `bankTipus/{nev,bankszamla}` | O s each | `Bank.{name,account}: ?String` |

Seller postal address deliberately uses `cimTipus`, **not** the buyer's all-optional postal type. Its ZIP/city/address tags remain necessary if that wrapper is supplied, though their string values can be empty.

### Seller (8 declarations)

Code: `ops/query_xml.rs:129–147,677–707`.

| `szallito/…` | Schema | `Supplier` mapping |
|---|---|---|
| `id` | R i | `id: ?i64`, relaxed omission/empty |
| `nev` | R s | `name: String` |
| `cim` | R `cimTipus` | `address: Address` |
| `postacim` | O `cimTipus` | `postal_address: ?Address` |
| `adoszam` | R s | `tax_number: ?String`, relaxed omission/empty |
| `csoportazonosito` | O s | `group_id: ?String` |
| `adoszameu` | O s | `eu_tax_number: ?String` |
| `bank` | O `bankTipus` | `bank: ?Bank` |

This is the document's seller block; neither current docs nor historical observations establish `id` as an immutable account identity.

### Core data (28 declarations)

Code: `ops/query_xml.rs:149–325,713–804`.

| `alap/…` | Schema | `InvoiceInfo` mapping |
|---|---|---|
| `id` | R i | `id: i64` |
| `szamlaszam` | R s | `invoice_number: InvoiceNumber`; H1 |
| `gazdEsemAzon` | R i | `economic_event_id: ?i64`, relaxed |
| `forras` | O i | `source: ?i64`; shared-schema external-source field, no expanded Agent retrieval claim |
| `iktatoszam` | O s | `registration_number: ?String` |
| `tipus` | R s | `document_type: DocumentType`, unknown string retained |
| `eszamla` | R i | `appearance: InvoiceAppearance`: 0 not invoice, 1 paper, 2/3 electronic, else `Unknown(code)` |
| `hivszamlaszam` | O s | `referenced_invoice_number: ?InvoiceNumber` |
| `hivdijbekszam` | O s | `referenced_proforma_number: ?InvoiceNumber` |
| `kelt` | R date | `issue_date: ?Date`, relaxed missing/empty |
| `telj` | R date | `fulfillment_date: ?Date`, relaxed missing/empty |
| `fizh` | R date | `due_date: ?Date`, relaxed missing/empty |
| `fizmod` | R s | `payment_method: ?PaymentMethod`, open |
| `fizmodunified` | R restricted s | `unified_payment_method: ?String`, open |
| `keszpenz` | R b | `cash_payment: ?bool`, no false default |
| `rendelesszam` | O s | `order_number: ?String` |
| `nyelv` | R restricted s | `language: ?String`, open |
| `devizanem` | R s | `currency: ?Currency`, open text |
| `devizabank` | O s | `exchange_bank: ?String` |
| `devizaarf` | O d | `exchange_rate: ?Decimal` |
| `megjegyzes` | O s | `comment: ?String` |
| `afatipus` | O s | `vat_type: ?String`; invoice-level foreign VAT annotation |
| `penzforg` | R b | `cash_accounting: ?bool` |
| `kata` | R b | `kata: ?bool` |
| `katafokonyv` | R b | `kata_ledger: ?bool` |
| `email` | O s | `email: ?String`, document-associated address |
| `teszt` | R b | `test: ?bool`; absence not interpreted as live account |
| `sztornozott` | O b | `reversed: ?bool`; absence/false/true distinguishable |

### Buyer and buyer ledger (12 + 6 declarations)

Code: `ops/query_xml.rs:345–403,833–918`.

| `vevo/…` | Schema | `BuyerInfo` / ledger mapping |
|---|---|---|
| `id` | O i | `id: ?i64` |
| `nev` | R s | `name: String` |
| `azonosito` | O s | `identifier: ?String`, separate from numeric id |
| `cim` | R `cimTipus` | `address: ?Address`, wrapper relaxed |
| `postacim` | O `cimpostaTipus` | `postal_address: ?BuyerPostalAddress` |
| `email` | O s | `email: ?String` |
| `adoszam` | R s | `tax_number: ?String`, relaxed |
| `csoportazonosito` | O s | `group_id: ?String` |
| `adoszameu` | O s | `eu_tax_number: ?String` |
| `lokacio` | R i | `location: ?i64`; documented 1/2/3/−1, unknown integers retained |
| `privatePersonIndicator` | R b | `private_person: ?bool`, relaxed |
| `fokonyv` | O container | `ledger: ?BuyerLedgerInfo` |
| `fokonyv/vevo` | O s | `account: ?String` |
| `fokonyv/vevoazon` | O s | `buyer_id: ?String` |
| `fokonyv/datum` | O date | `date: ?Date` |
| `fokonyv/folyamatostelj` | O b | `continuous_fulfillment: ?bool` |
| `fokonyv/elszDatTol` | O date | `settlement_from: ?Date` |
| `fokonyv/elszDatIg` | O date | `settlement_to: ?Date` |

Case-sensitive buyer date names have explicit serde renames. Buyer data is not promised immutable at issuance (`query_xml.rs:364–369`, behavior evidence below).

### Printed items and item ledger (14 + 6 declarations, plus list)

Code: `ops/query_xml.rs:405–467,920–1003`.

| `tetelek/tetel/…` | Schema | `DocumentItem` / ledger mapping |
|---|---|---|
| `nev` | R s | `name: String` |
| `azonosito` | O s | `id: ?String` |
| `mennyiseg` | R d | `quantity: Decimal` |
| `mennyisegiegyseg` | R s | `unit: String` |
| `nettoegysegar` | R d | `unit_price: Decimal` |
| `afatipus` | O restricted s | `vat_type: ?String` |
| `afakulcs` | R d, ≥0 | `vat_rate_code: String`, raw token |
| `netto` | R d | `net_value: Decimal` |
| `arresafaalap` | O d | `margin_vat_base: ?Decimal` |
| `afa` | R d | `vat_value: Decimal` |
| `brutto` | R d | `gross_value: Decimal` |
| `megjegyzes` | O s | `comment: ?String` |
| `sztetordering` | R i | `ordering: ?i64`, relaxed |
| `fokonyv` | O container | `ledger: ?DocumentItemLedger` |
| `fokonyv/arbevetel` | O s | `revenue_account: ?String` |
| `fokonyv/afa` | O s | `vat_account: ?String`, not numeric VAT |
| `fokonyv/gazdasagiesemeny` | O s | `economic_event: ?String` |
| `fokonyv/gazdasagiesemenyafa` | O s | `vat_economic_event: ?String` |
| `fokonyv/elszdattol` | O date | `settlement_from: ?Date` |
| `fokonyv/elszdatig` | O date | `settlement_to: ?Date` |

`tetelek/tetel` is 1..unbounded in SD; a present empty wrapper yields `[]` in Rust. Item-ledger date names are lowercase, unlike buyer settlement dates. `vat_rate()` prefers a nonblank `afatipus`, otherwise interprets `afakulcs`; unknown VAT codes remain open and numeric exponent spelling is understood. It does not validate the raw rate field's XSD restriction during document parsing.

### Financial items and labels (10 + 1 + 1 declarations)

Code: `ops/query_xml.rs:470–510,1005–1055`.

| Declaration | Schema | Mapping |
|---|---|---|
| `qutetek/qutet` | 0..unbounded | `financial_items: Vec<FinancialItem>` |
| `qutet/nev` | R s | `name: String` |
| `qutet/afatipus` | O restricted s | `vat_type: ?String` |
| `qutet/afakulcs` | R d, ≥0 | `vat_rate_code: String` |
| `qutet/{netto,afa,brutto}` | R d each | `{net,vat,gross}: Decimal` |
| `qutet/{elszdattol,elszdatig}` | O date each | `{settlement_from,settlement_to}: ?Date` |
| `qutet/afalevon` | R i | `deductible_vat: i64`, no invented percentage/unit constraint |
| `qutet/cimkek` | O container | `labels: Vec<String>` |
| `cimkek/cimke`, shared by root/qutet | 0..1 s | `Vec<String>` accepts more than one; deliberate widening, no lost schema-supported value |

Financial items are ledger-side rows: no quantity/unit-price fields are declared. “QUiCK tétel” remains a plausible origin, not a vendor-defined English expansion.

### Totals and credit entries (5 + 3 + 2 + 7 + 1 declarations)

Code: `xml.rs:811–894`, `types.rs:1056–1108`, `ops/query_xml.rs:512–538,1057–1092`.

| Declaration | Schema | Mapping |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded | `Totals.by_vat_rate`; absence → `[]` |
| `afakulcsossz/afatipus` | O restricted s | `VatTotal.vat_type: ?String` |
| `afakulcsossz/afakulcs` | R d, ≥0 | `vat_rate_code: String`, same helper precedence |
| `afakulcsossz/{netto,afa,brutto}` | R d each | `VatTotal.{net,vat,gross}: Decimal` |
| `osszegek/totalossz` | R container | `Totals.total: GrandTotal` |
| `totalossz/{netto,afa,brutto}` | R d each | `GrandTotal.{net,vat,gross}: Decimal` |
| `kifizetesek/kifizetes` | 1..unbounded | `credit_entries: Vec<RecordedCreditEntry>`; empty wrapper accepted |
| `kifizetes/datum` | R date | `date: Date`, missing/blank refused |
| `kifizetes/jogcim` | R s | `title: PaymentMethod`, unknown text retained |
| `kifizetes/osszeg` | R d | `amount: Decimal` |
| `kifizetes/megjegyzes` | O s | `comment: ?String` |
| `kifizetes/bankszamlaszam` | O s | `bank_account: ?String` |
| `kifizetes/banktranzid` | O i | `bank_transaction_id: ?i64` |
| `kifizetes/devizaarf` | O d | `exchange_rate: ?Decimal` |

OUT/HOUT says the bank-account value may be the sender's account **or**, when unknown, the account printed on the invoice. The model accurately preserves that ambiguity. The SD tree has no outstanding field; gross minus credits is a derived value, not an omitted declared field.

**Count check:** root 9 + address 4 + buyer-postal 5 + bank 2 + seller 8 + core 28 + buyer-ledger 6 + buyer 12 + item-ledger 6 + item 14 + item-list 1 + VAT subtotal 5 + grand total 3 + totals container 2 + credit entry 7 + credit-list 1 + labels 1 + financial-list 1 + financial item 10 = **125**.

## PDF response, headers and errors

Code: `ops/query_pdf.rs:36–95`; `ops/envelope.rs:100–248,275–315,318–370`; `xml.rs:461–553`; `wire.rs:227–311,343–351`.

| Envelope field | PA/HPA/ED | Current reading |
|---|---|---|
| `sikeres` | R boolean | Required scalar; `true/1/false/0`, with XML padding. Missing/empty/invalid does not establish success. |
| `hibakod` | O string | On false: known `ErrorCode`, unknown preserved, missing/blank → `Absent`. |
| `hibauzenet` | O string | XML-decoded diagnostic; unavailable diagnostic does not erase readable refusal. |
| `szamlaszam` | O string | Nonblank body before decoded `szlahu_szamlaszam`; both trimmed; A1. |
| `szamlanetto` | O double | `net_total: ?Decimal`; body before raw `szlahu_nettovegosszeg`. |
| `szamlabrutto` | O double | `gross_total: ?Decimal`; body before raw `szlahu_bruttovegosszeg`. |
| `kintlevoseg` | O double | `outstanding: ?Decimal`; body before raw `szlahu_kintlevoseg`; absent is not zero. |
| `vevoifiokurl` | O string | `customer_account_url: ?String`; XML entity decoding only, decoded header fallback. |
| `pdf` | O base64Binary | Required artifact in `InvoicePdf`; missing/blank → `Missing("pdf")`, malformed nonblank → base64 error. |

- **Transport/header precedence:** nonblank `szlahu_down` → nonblank error-code header → known non-2xx HTTP status → body. Headerless false/code 7 at HTTP 200 is `Api`; at HTTP 500 it is `HttpStatus`. This is explicitly documented library policy (`wire.rs:133–142`), not a claim all vendor errors use one channel. Error headers take priority over success metadata. Unknown status on a manually built `RawResponse` skips only the HTTP-status check.
- **XML query branch:** `query_xml.rs:568–593` accepts `szamla` under its namespace, or `xmlszamlavalasz` under the error-envelope namespace. A successful envelope is rejected because it is not full document data. Body-only code 7 is handled. Header values cannot fill absent core XML identity/totals or manufacture a full document.
- **Metadata precedence:** a readable body value wins. Blank/absent envelope metadata permits fallback; malformed nonblank body money fails rather than falling back. Numeric headers use raw text and comma/dot conversion, optional sign/exponent, HTTP SP/HTAB padding. Text headers decode form-style once (`+` → space, `%2B` → literal plus). The XML URL `https://x/?q=a+b&amp;x=%2B` becomes `https://x/?q=a+b&x=%2B`, not form-decoded again. The local header/body conflict controls confirmed this.
- **Auxiliary headers:** `szlahu_id`, `szlahu_fizetesmod` and the numbered-56 flag are read by `CreatedInvoice` then omitted from `InvoicePdf`; PA/HPA declares none as a successful XML element. CA's creation header table and recorded create/storno headers do not guarantee their presence on every PDF query. Exposing them is a possible convenience extension, not a missing declared query field. XML query likewise is a document-body projection, not a raw-header archive.
- **Legacy mode:** PDF version 1 raw bytes / `[ERR]…` is neither selectable nor parsed here. Always requesting 2 offers the documented PDF capability through one structured interface. Creation's `xmlagentresponse=DONE;…` is not a PDF/XML-query success format.

## Content parsing, namespaces and representation boundaries

### Namespace and document completion

`xml.rs:37–155,201–300` validates UTF-8, one complete allowed expanded root name, namespace bindings and lexical XML through EOF. It rejects undeclared/reserved bindings, duplicate expanded attributes, malformed/truncated/trailing bodies and DTDs. `xml.rs:303–377` canonicalizes protocol element names for serde, omits complete foreign subtrees, and preserves a placeholder so mixed scalar text cannot join across a foreign child. Scalar parent paths and duplicate known fields remain serde's responsibility.

Fresh controls rejected wrong root namespace, a second root, truncation, foreign-only verdict, `tr<foreign/>ue`, and duplicate PDF invoice number even with header fallback. A foreign `sztornozott` does not set reversal; mixed-prefix repeated `tetel` rows under the correct namespace both survive. Equivalent prefixes are supported, not compared by textual prefix spelling. Unknown well-formed protocol fields are ignored; malformed unknown XML still fails full-document validation. This is **not full XSD validation**: sequence rearrangements, wider lists and open vocabularies can be accepted.

### Scalar content and artifacts

| Domain | Current behavior / limit | Assessment |
|---|---|---|
| Integer fields | Every current SD integer declaration is `xs:int`; all ten expanded occurrences map to signed `i64`. `query_xml.rs:10–25,713–727,949–950,1027–1028` and the optional-id helpers. Fresh all-integer −2147483648/+2147483647 controls parse. | Complete current integer value range; intentionally wider. No inferred percentage range for `afalevon`. |
| Dates | Eleven expanded date positions use `xml.rs:667–727`. Preserve civil date without UTC conversion; modern leap day with `Z` or `+14:00` works across all positions. Checked slicing avoids multibyte panic. Optional missing/empty → `None`; invalid nonblank date fails the Agent query. Required credit date stays required. | Finite Jiff date-domain limit, not full XSD `date` support. `10000-01-01` is refused; no claim this is a modern invoice defect. README `:264–268` explicitly describes the civil/finite reader. Do not import Adatkapcsolat's invalid-date-as-content policy into this Agent review. |
| Money/quantity/rates as Decimal | `number.rs:61–146`, `xml.rs:647–665`, `envelope.rs:344–370`: finite exact conversion with sign/exponent; out-of-range/precision/nonfinite values refused rather than rounded. Fresh `+1.25e+2` works across every declared double field; `1e-29` fails. | Explicit narrower representation than unrestricted XSD doubles; not live-exempted by account behavior. If full mathematical XSD range is required, a broader representation is needed. |
| Numeric VAT tokens | All three `afakulcs` positions retain `String`; helper prefers special `afatipus`, else parses numeric token (`query_xml.rs:461–467,504–509`; `types.rs:296–324,1088–1094`). | No implicit decimal precision loss; raw unknown/non-numeric/negative tokens can survive because this is not schema validation. `TEHK` remains `Other`, not discarded. |
| Business strings | `xml.rs:745–757`, `query_xml.rs:1094–1101`: optional text consisting solely of XML whitespace → `None`; otherwise decoded characters/padding/NBSP preserved. Required strings and labels are raw decoded strings. Envelope number/URL use the older trimming helper. | Typed reading, not byte-for-byte archival. Number whitespace treatment differs between XML/PDF, explicitly recorded in H1/A1. |
| Booleans | `xml.rs:759–788`: four XSD forms, blank optional → `None`, invalid nonblank → error. Fresh all-boolean 0/1/false controls passed. | No fabricated false when unreported. Some helpers accept Unicode outer whitespace more broadly than XML whitespace; permissive lexical boundary, not refusal of a valid response. |
| PDF artifact | `types.rs:96–117`: standard base64 after whitespace removal; decoded bytes, no PDF structure/render/signature validation. XML `pdf` is SD `string`, semantically base64 in XA; invalid nonblank content fails entire query (`query_xml.rs:609–612`). Blank/absent remains `None`, even when requested. | Base64 `not a pdf` bytes are accepted. This is an artifact-validation capability boundary, not proof a valid PDF is mishandled. Unicode whitespace is stripped more broadly than XML base64 whitespace. |
| Raw archival | `InvoiceDocument` holds typed fields, no raw XML or unknown-field bag; `RawResponse::body` is available at wire level. | No current declared field missing. A byte-preserving archival interface is an additional capability, not the current contract. |

No response `fuvarlevel`, carrier-specific waybill, creation settings, seller notification template or data-erasure-code element is declared by current SD. Those are creation-request fields in CX; treating their absence here as an omitted query field would compare different protocols. Likewise imported/incoming retrieval is expressly outside XR's boundary, despite the shared response schema carrying `forras`.

## Targeted verification: executed results and reproducibility

Scratch implementation created independently for this review:

```sh
python3 /tmp/opencode/queries-28dcec1-acquire.py
cargo build --manifest-path /tmp/opencode/queries-28dcec1-check/Cargo.toml --offline
python3 /tmp/opencode/queries-28dcec1-verify.py
```

- Fresh acquisition uses Python `urllib`, HTMLParser for tabbed code blocks and ZIP inspection. XML shape extraction uses ElementTree; **actual schema validation uses installed libxml2 through ctypes**, not ElementTree pretending to validate XSD.
- The public-API Rust executable has a path dependency on current `szamlazz-agent`, no HTTP feature/client, and consumes JSON lines with synthetic body/header data. It serializes results so complete field mappings can be inspected. Initial scratch build failed because the harness imported `AgentRequest`/`RawResponse` from crate root; corrected to `szamlazz_agent::wire`. This was a harness mistake, not a product issue.
- Generated an all-fields document directly from fresh SD: **135 expanded element nodes**, all 125 declarations represented, with reused address structures expanded. Fresh SD validates it; Rust parses it and its JSON projection was inspected against the field inventory.
- **65 independent schema-optional element occurrences removed:** every resulting body parsed, including optional containers. The all-fields baseline's required strings were synthetic meaningful nonempty text; empty/absent and identity limits were separate controls.
- **12 writer combinations:** all validated against XD/PD, with `<`/`&` and selector padding included. No request was sent.
- Whole-type controls changed all ten integer positions, all eleven date positions, all boolean positions and all declared double positions simultaneously. Signed int extrema, two timezone forms, boolean 0/1/false and exact exponent numbers succeeded. The attempted long trailing-zero double specimen `0.000000000000000000000000000010` equals **1e-29**, still outside Decimal's domain: it failed as expected after correcting the initial harness expectation. It is not evidence that an exactly representable trailing-zero number fails.
- Four verbatim EN/HU success examples failed on placeholder base64; four otherwise identical placeholder-replaced controls parsed.
- Numberless PDF envelope passed fresh ED validation and failed Rust's identity requirement (A1). Header fallback, body precedence, invalid body money, missing PDF, error-before-artifact, numbered 56, HTTP status and down-header precedence behaved as recorded above.
- Namespace/completion, blank/padded XML identity, invalid multibyte/wide dates, small decimals, scientific VAT text and decoded non-PDF bytes were exercised. **119 public-API calls** in the completed matrix; detailed input/result pairs in `/tmp/opencode/queries-28dcec1/verification.json`, full field projection in `all-fields.json`, generated specimen in `all-fields.xml`.
- Only baseline SD, 12 requests and numberless ED were XSD-validated in this targeted run; no claim that every permissive/error specimen is schema-valid. No PDF renderer or signature verifier was used. Five-byte PDF controls isolate decoding/presence, not document validity.
- Scratch Cargo resolved its own cached lock (quick-xml 0.42.0, Jiff 0.2.35, rust_decimal 1.43.0, base64 0.23.1); this is targeted public-API evidence, **not a replacement for the parent's workspace-locked broad tests**. No broad repository Cargo test command was run here.

## Existing live evidence checked before disposition

| Repository source / lines | What it actually establishes | What it does not establish |
|---|---|---|
| `docs/szamlazz-hu-behaviour.md:59–64` | Tested order-query case sensitivity, exact padding behavior, newest document of different types | All possible order alphabets or definition of “last” by id versus date |
| Same `:82–90` | Nonunique external ids; XML/PDF newest holder; tested external-id retrieval; no echoed external id; removed proforma yields 7 | Universal response-number cardinality or cross-account ownership |
| Same `:96–99,115–117` | Absent reversal marker before storno, true after; credit removal; observed fulfillment date; appearance 1=paper/3=electronic | Live use of appearance 2; full XSD cardinality compliance |
| Same `:130,154,161–165` | Mutable buyer data, credit-entry ordering, body-only query code 7, documented operation-specific header observations | Immutable buyer snapshot; identical header sets on all query successes |
| `docs/research/2026-09-11-credit-clearing-live.md:14–32,64–74` | XML-query entries before/after two clear operations; parsed numbers/outstanding; verified cleanup | Raw-channel provenance (not archived), any PDF-query number guarantee |
| `docs/research/2026-09-11-receipts-live.md:12–16,44–73` | Receipt-specific query/lifecycle/PDF signature and MNB observations | Invoice XML/PDF contract exemptions; full PDF rendering; raw HTTP corpus |
| `docs/research/2026-09-11-agent-vendor-clarification.md:3,52–60,93–98` | Explicit pending PDF-success guarantee question and evidence limits | Vendor answer or proof a numberless successful PDF is emitted |
| `docs/research/2026-09-10-agent-vendor-questions.md:74–87` | Pending HU PDF schema clarification | A live alternate-order acceptance test |

Historical raw invoice logs are explicitly not in this repository (`behaviour.md:11–12`); no full captured live-response corpus is claimed. Dates/Decimal range limits are justified as documented representation policy, **not** exempted by the absence of extreme-value live observations.

## Recommended disposition

1. Retain the current selector writers and full document field mapping: freshly fetched primary definitions and targeted checks support them.
2. Resolve **A1** and **S1** through the existing vendor-clarification work; preserve their unconfirmed/source-conflict classification until a success-specific answer or appropriate direct evidence exists.
3. Make **H1** and **A2** explicit contract decisions rather than silently importing assumptions from other operations.
4. Keep numeric/date domain limits, artifact byte decoding and typed/raw archival boundaries visible. Expand them only for a deliberate additional capability; no demonstrated ordinary query failure here requires an immediate product change.
