# Számla Agent XML/PDF query review — 2026-09-11

## Verdict

**No current, ordinary-document interoperability defect or missing documented query-response field was confirmed.** Both query writers implement all three selectors. The queried-invoice model covers all **125 child-element declarations in 19 complex structures** of the freshly downloaded `szamla.xsd` (126 element declarations including the root; reused types counted once). The PDF projection exposes all six successful payload fields in its operation-specific response schema.

The actionable results are **one low-priority identity-hardening recommendation** and **a concrete vendor schema conflict**. They must not be presented as demonstrated failures of normal Számla Agent responses. Previously reported timezone, monetary-header, optional-text, namespace, trailing-XML and PDF metadata problems do not reproduce in the current implementation.

| Rank | ID | Category | Result |
|---|---|---|---|
| 1 | Q-H1 · P3 | Malformed/business-invalid response hardening | XML query accepts a blank invoice number as successful document identity; PDF query already refuses it. Consider a query-local nonblank identity check. |
| 2 | Q-S1 · P3 | Vendor-source conflict | Hungarian PDF request XSD is malformed XML and disagrees with the English/download schemas on invoice-number requiredness and order-selector position. Preserve the current writer pending vendor clarification. |
| — | Q-S2 | Source/example defects | Query examples contain placeholder PDF data; XML example also violates requiredness and totals consistency. These are not parser regressions. |
| — | Q-B1 | Deliberate representation boundary | Civil dates and Decimal are not complete implementations of the XSD date/double value spaces. Modern zoned dates work; unsupported ancient dates and very large/small amounts remain explicit boundaries. |
| — | Q-O1 | Unsupported feature inventory | Legacy version-1 decoding and auxiliary PDF header projections are absent by design; no missing query-XSD field. Waybill fields are creation-only in the inspected sources. |

Priority describes follow-up urgency, not severity of a proven production incident. No P0/P1/P2 implementation finding was established.

## Review boundary and method

- Starting HEAD: `61c334f9508e8b63df2f3db182d6ca83f4feb8f0`, with existing user edits. Read the working-tree versions of `crates/szamlazz-agent/src/ops/query_xml.rs`, `query_pdf.rs`, all their wire structs/conversions, and relevant `xml.rs`, `types.rs`, `number.rs`, `ops/envelope.rs`, `ops/waybill.rs`, `wire.rs`, tests and fixture provenance.
- During the review, external activity advanced HEAD to `5c6d5ead33a3587c4ea29cc973bedaefc3ddcb1f` and committed the existing edits. **This review performed no commit.** `git diff 61c334f HEAD -- crates/szamlazz-agent` showed only the pre-existing `wire.rs` extraction of the unchanged XML-character check into public `validate_xml_text` (10 insertions, 1 deletion). Query code, model code, shared response helpers and agent tests were unchanged. Test results therefore apply to the requested query source plus the starting user edit, not a claimed pristine-HEAD checkout.
- Code references below are working-tree line numbers. Unless otherwise qualified, paths are relative to `crates/szamlazz-agent/src/`.
- Official pages and schemas were fetched afresh using unauthenticated documentation GETs on 2026-09-11. Site build: `v202608271632`; this is not a verification date for every vendor statement. Primary-source content, not previous review conclusions, determined the findings.
- No secrets, credentials from the environment, private raw live logs, or operational endpoints were read/called. No live tests were run. This review added only this report to the repository; other review reports appeared through concurrent external activity. An offline throwaway executable was compiled from stdin into `/tmp/opencode/queries-61c334f-probe`; it uses synthetic fixture data and literal dummy credentials.
- `docs/szamlazz-hu-behaviour.md` was read in full. It explicitly says the original raw probe logs are outside the repository (`:11–24`). The repository fixture tree contains `upstream` and `synthetic`, not a raw vendor-live query corpus. Live-derived inline regression cases and the current ignored live-test source were inspected; synthetic records are not mislabeled as captured live responses.

## Sources fetched and acquisition checks

Every request/response/XML page under both requested operation categories was fetched, including the Hungarian counterparts. The HTML code blocks were also independently extracted in memory with Python's `HTMLParser`; all tabbed blocks were included. None of those twelve operation pages contained a separate `.xml`, `.xsd`, `.zip` or `.pdf` download anchor. Request download URLs occur in sample `xsi:schemaLocation` attributes instead. The XML response's relative `szamla.xsd` reference was resolved through the shared outgoing-invoice documentation, not guessed to be the creation schema.

| Ref | Source URL | Inspected material |
|---|---|---|
| XC | https://docs.szamlazz.hu/agent/category/query-document-xml | Category and complete operation-page navigation |
| PC | https://docs.szamlazz.hu/agent/category/query-document-pdf | Category and complete operation-page navigation |
| XR | https://docs.szamlazz.hu/agent/querying_xml/request | Endpoint, multipart action, selectors, internal-outgoing boundary, HTML form |
| XA | https://docs.szamlazz.hu/agent/querying_xml/response | Success example, error envelope/code 7, schema reference |
| XX | https://docs.szamlazz.hu/agent/querying_xml/xml | Request example and inline schema |
| PR | https://docs.szamlazz.hu/agent/querying_pdf/request | Endpoint, multipart action, selectors, HTML form |
| PA | https://docs.szamlazz.hu/agent/querying_pdf/response | Legacy error example, structured success/error examples, complete reply schema |
| PX | https://docs.szamlazz.hu/agent/querying_pdf/xml | Request example and inline schema |
| HXR | https://docs.szamlazz.hu/hu/agent/querying_xml/request | Hungarian retrieval/selector claims |
| HXA | https://docs.szamlazz.hu/hu/agent/querying_xml/response | Hungarian success example and schema reference |
| HXX | https://docs.szamlazz.hu/hu/agent/querying_xml/xml | Hungarian request example and inline schema |
| HPR | https://docs.szamlazz.hu/hu/agent/querying_pdf/request | Hungarian selector claims |
| HPA | https://docs.szamlazz.hu/hu/agent/querying_pdf/response | Hungarian examples and reply schema |
| HPX | https://docs.szamlazz.hu/hu/agent/querying_pdf/xml | Conflicting Hungarian request schema |
| XD | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Complete downloaded XML-query request schema |
| PD | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Complete downloaded PDF-query request schema |
| ED | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Complete downloaded shared reply schema |
| SD | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Complete queried-invoice schema |
| OUT | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak | Shared document example/annotations, full inline XSD and download URL |
| HOUT | https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak | Original Hungarian semantics, especially bank account, appearance and ledger |
| CX | https://docs.szamlazz.hu/agent/generating_invoice/xml | Destination of XA's schema link; creation-only ledger/waybill schemas |
| CD | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd | Downloaded creation schema; confirms waybill placement and fields |
| CA | https://docs.szamlazz.hu/agent/generating_invoice/response | Shared envelope/header definitions; creation-only legacy success delimiter |
| ERR | https://docs.szamlazz.hu/agent/basics/error-handling | Linked error format and error-code guidance |
| SEND | https://docs.szamlazz.hu/agent/basics/sending-requests | Action names, POST/file shape, case-sensitive tags |
| FLOW | https://docs.szamlazz.hu/agent/basics/how-does | Shared transport/document flow |
| XSD | https://www.w3.org/TR/xmlschema-2/ | Date (§3.2.9), boolean (§3.2.2), double (§3.2.5), base64Binary (§3.2.16) lexical/value-space reference |

Two exploratory, unlinked URLs, `/agent/basics/response` and `/agent/basics/receiving-response`, returned HTTP 403. No claim depends on them. All operation pages and the four query-related schemas above were successfully acquired. Adatkapcsolat Ack schemas are a different protocol and were not substituted for the Agent reply.

Fresh download SHA-256 values (actual response bytes, no reformatting):

| Schema | SHA-256 |
|---|---|
| XD | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| PD | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| ED | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |
| SD | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |

Python `ElementTree` parsed the fresh downloads and enumerated their structures. That is XML well-formedness/structural inspection, **not full XSD validation**. Neither `lxml` nor `xmllint` was available; no schema-validation result is claimed. The initial `python` invocation failed because that executable is absent; the equivalent `python3` acquisition/count check succeeded.

## Ranked findings

### Q-H1 · P3 — Successful XML-query identity may be blank

**Category:** malformed/business-invalid response hardening; not a demonstrated vendor interoperability defect.

**Code:** `ops/query_xml.rs:708–722,767–776` deserializes `alap/szamlaszam` directly as `InvoiceNumber` and projects it unchanged. `types.rs:22–32` is a transparent, unvalidated string wrapper. The counterpart PDF path uses `ops/envelope.rs:123–132,326–329` to reject a blank number, and `:275–279` requires one.

**Document claim:** XA calls success a full invoice document; OUT/HOUT describes `alap/szamlaszam` as the invoice's unique number. SD declares it required `xs:string` (`fixtures/upstream/agent/xsd/szamla.xsd:123`). **The XSD does not impose `minLength`: an empty string is schema-valid.** Therefore a nonblank check would enforce useful document identity, not repair a literal XSD mismatch.

**Independent reproduction:** substitute the synthetic fixture's `E-TST-2026-66` with `""` or `" \t "`, then call `QueryInvoiceXml::parse`. Both return `Ok`, carrying exactly `InvoiceNumber("")` / `InvoiceNumber(" \t ")`. A normal `INV-1` remains successful. The PDF path with a valid PDF and no number returns `Parse(Missing("szamlaszam"))`.

**Impact:** callers treating `Ok(InvoiceDocument)` as a usable identity can persist an empty lookup key or attempt a subsequent by-number operation with it. A record still has `alap/id`, so this is not evidence that every identity is lost. No inspected vendor example or recorded live observation supplies a blank invoice number; no duplicate issuance claim follows from this probe.

**Correction:** if successful queried documents are intended to guarantee usable number identity, add a query-local deserializer rejecting empty/XML-whitespace-only `alap/szamlaszam`, while preserving every character of nonblank numbers. Keep the general Agent `InvoiceNumber` wire wrapper lenient and do not import the worker's 40-byte alphabet into it. Test omitted, paired-empty, self-closing, whitespace-only and nonblank padded values. Otherwise explicitly document that a successful parse guarantees presence/type, not nonblank identity.

### Q-S1 · P3 — Hungarian PDF request schema cannot be a common conformance target

**Category:** source conflict; no current writer correction established.

**Code:** `ops/query_pdf.rs:62–79` writes credentials, the selected invoice/order number, `valaszVerzio=2`, then an external id only when selected. `types.rs:1034–1052` represents exactly one selector.

**Conflicting document claims:**

- PX and PD: `szamlaszam` is optional; order is `szamlaszam? → rendelesSzam? → valaszVerzio → szamlaKulsoAzon?`.
- HPX: `szamlaszam` is mandatory; order is `szamlaszam → valaszVerzio → rendelesSzam? → szamlaKulsoAzon?`.
- PR and HPR expressly permit identification by **invoice number, order number or external id**.
- HPX additionally renders `targetNamespace="...xmlszamlapdf"xmlns:tns=...` without a separating space. Parsing its untouched, HTML-decoded schema block fails at line 1, column 140. This defect was independently checked, not inferred solely from Markdown formatting.

**Impact:** validating the crate's order/external-id requests against HPX either fails to load the schema or, after fixing only its syntax, rejects otherwise documented selector forms. Following its order would instead disagree with both PX and PD.

**Correction:** vendor should repair and reconcile HPX with its own request prose and the English/download schema. Preserve the current writer and the independently supported order/external-id capabilities. If source-drift tests are expanded, keep the disagreeing originals separate; a merged schema cannot establish server acceptance. Existing live observations support external-id PDF retrieval (`docs/szamlazz-hu-behaviour.md:64–66`), but this review made no new server check of either ordering.

## Source conflicts and deliberate boundaries, not new interoperability bugs

### Q-S2 — Published examples are illustrative, not valid acceptance fixtures

- XA/HXA puts English prose inside `<pdf>`; PA/HPA abbreviates base64 with `....`. Both are correctly refused by `Pdf::from_base64` (`types.rs:110–116`). `tests/upstream.rs:689–699,710–725` separately asserts the rejection and a synthetic substitution/abbreviation-removal control. Removing dots does not reconstruct an original PDF or establish rendering validity.
- XA/HXA omits SD-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio`/`privatePersonIndicator`, and item `sztetordering`. Its `fizmodunified=other` is outside SD's Hungarian enum. Its lone row totals `380/76/456`, while the per-rate and grand totals are `464/93/557`. Current parsing intentionally retains reported values without enforcing arithmetic or the closed enum.
- HXX's example writes empty `<pdf></pdf>` although its schema types it boolean; its comment instructs true/false. The crate emits an explicit valid boolean, not the empty placeholder.
- PA/HPA describes absent `valaszVerzio` as legacy version 1; PX/PD requires the tag. The crate always sends 2, satisfying both descriptions on its supported path.
- XA/HXA links to creation's `xmlszamla` schema page while naming the response `szamla.xsd`. These are different roots, namespaces and models. SD/OUT provides the actual response tree. Inferring response fields from the creation request would create false omissions.
- OUT/HOUT example annotations call `eszamla` a string and `sztetordering` a double; SD types both `int`. Current signed integer representations fit the values and schema. The appearance code interpretation is supported by the annotation and live observations, not the annotation's misleading type label.

### Q-B1 — Exact boundaries of dates, money and base64

**Dates.** All eleven positions use the shared civil-date adapter (`xml.rs:649–709`): three invoice dates; three buyer-ledger dates; two item-ledger dates; two financial-item dates; one required credit-entry date. `Z`, positive/negative offsets through `14:00`, XML padding and leap days work. The printed date is retained without UTC conversion. Empty optional dates become `None`; invalid nonblank dates fail the entire Agent query. This differs deliberately from the Adatkapcsolat content-tolerant parser.

The adapter first accepts Jiff's legacy date domain (`:653–655`). The probe also accepted `2024-02-29T12:00:00` as February 29, `20240229`, and year zero; it refused XSD-1.0-valid BCE spelling `-0001-02-28`. Tests explicitly retain extended-year `-000001-02-28` and year zero (`ops/query_xml.rs:1333–1387`). Thus “every `xs:date`” or “strict XSD lexical validation” would be false. `README.md:240–244` already disclaims strict XSD conformance and retains the finite legacy domain. No inspected modern invoice is affected; broadening ancient-year support or narrowing legacy accepted forms requires a deliberate contract decision, not automatic remediation. Checked slicing (`xml.rs:659–663`) removes the previous multibyte panic.

**Money.** All represented quantities, amounts and rates use `Decimal`, via `xml.rs:629–647`, `number.rs:61–141`, and the envelope helpers. Finite representable decimal/exponent spellings are exact; excess precision/range is refused rather than silently rounded. XSD `double` also permits infinity/NaN and values outside Decimal's domain, so this is a conscious financial representation boundary. `1e-29` and `1e100` do not become plausible rounded amounts. VAT rate *text* is separately retained even when it cannot become `Percent`; unknown codes remain `Other`.

**Base64.** Standard base64 with XML whitespace, CDATA/entity decoding and wrapping works. `types.rs:113` strips Unicode whitespace, wider than XSD's four whitespace characters: the probe accepted an embedded NBSP. Valid base64 encoding `not a pdf` also becomes `Pdf(9 bytes)`; this type decodes bytes but does not validate PDF syntax, signatures or rendering. These are artifact/lexical-hardening choices, not evidence of failure on a valid encoded PDF. XML query treats absent/blank PDF as `None` even if requested; malformed nonblank base64 fails the query. PDF query requires a nonblank decoded artifact and reports missing PDF when absent/empty (`ops/query_pdf.rs:83–93`).

### Q-O1 — Unsupported feature omissions versus documented payload coverage

| Capability | Current boundary and adjudication |
|---|---|
| PDF response version 1 | Not selectable or decoded by the typed PDF operation. It deliberately requests 2 and supplies decoded PDF bytes. No loss of the requested PDF-download capability. |
| Legacy `[ERR]…----------` extraction | Not implemented; unexpected version-1 body is refused (or a typed error comes from headers). ERR itself says the important text ends at a hyphen delimiter, with differing hyphen counts in prose/examples. Structured mode does not need that delimiter parser. |
| `xmlagentresponse=DONE;{number}` | Creation's legacy text success in CA, not a documented PDF-query/XML-query success shape. No query delimiter field is missing. |
| Batch/multiple selectors or selector precedence | Not offered; enum permits one selector. Documentation names alternative selectors and returns one latest document, not a delimited list or combined-selector contract. Empty selector strings remain constructible; local rejection would be convenience validation, not a missing wire field. |
| PDF auxiliary header metadata | `parse_issued` reads `document_id`, `payment_method`, notification flag, but `InvoicePdf` intentionally does not project them (`query_pdf.rs:86–93`; `tests/response_headers.rs:385–387`). These are not missing elements of PA's reply XSD. Query-specific emission and code-56 notification behavior are not established by the fetched PDF page. |
| XML-query outstanding amount | No `kintlevoseg` in SD/XA; do not fabricate it in the full document from gross minus entries. PDF result supports it where the envelope provides it. Reversal can remove entries. |
| External id echoed in full XML | Neither SD nor the live observations supplies it. Query selector knowledge is not a response field (`behaviour.md:68`). |
| Incoming/NAV-imported query support | XR/HXR explicitly limits XML query to internally issued outgoing documents. `InvoiceInfo::source` retains shared-schema `forras`, not a promise of broader retrieval (`query_xml.rs:51–53,239–242`). |
| Waybill/carrier response data | No `fuvarlevel` or carrier sub-block in SD, XA or OUT's full schema. Creation-only models are covered below. Adding guessed response nesting would not fix a documented omission. |
| Raw XML / unknown-field preservation | `InvoiceDocument` is a typed projection, not a raw archive. Unknown elements are tolerated but not exposed. No inspected query document field is lost; future-extension archival is a separate feature. |

## Complete request and transport coverage

`R` = exactly one required schema element; `O` = optional 0..1. Table order is wire order. An authentication alternative is chosen; the XSD makes credential elements optional but does not make an unauthenticated query meaningful.

| Element/concern | XML query | PDF query | Code / assessment |
|---|---|---|---|
| Action | `action-szamla_agent_xml` | `action-szamla_agent_pdf` | `query_xml.rs:535–542`; `query_pdf.rs:58–65`; XR/PR/SEND agree |
| Root/namespace | `xmlszamlaxml`, `http://www.szamlazz.hu/xmlszamlaxml` | `xmlszamlapdf`, `http://www.szamlazz.hu/xmlszamlapdf` | Exact match |
| `felhasznalo`, `jelszo` | O string each | O string each | Root credentials, in that order, `xml.rs:610–619` |
| `szamlaagentkulcs` | O string | O string | Alternative to username/password; no `beallitasok` wrapper |
| `szamlaszam` | O string | O string in PX/PD | Invoice-number selector; HPX conflict Q-S1 |
| `rendelesSzam` | O string | O string before version in PX/PD | Order selector, spelling differs from response `rendelesszam` |
| `pdf` | O boolean, always explicit false/true | Not a request field | `query_xml.rs:552`; constructor defaults false |
| `valaszVerzio` | Not a request field | R int, always shared `RESPONSE_VERSION=2` | `query_pdf.rs:75`; no missing XML-query version setting |
| `szamlaKulsoAzon` | O string after `pdf` | O string after version | `query_xml.rs:553–555`; `query_pdf.rs:76–78` |
| XML escaping | All selector text and credentials escaped | Same | `xml.rs:568–578`; independent probe observed `&amp;` / `&lt;` in each selector |
| Schema location | Not emitted | Not emitted | A sample's schema-location hint is not a required business element |
| Endpoint/file shape | HTTPS POST to `https://www.szamlazz.hu/szamla/`, one multipart XML file | Same | `wire.rs:7–14,66–100,395–408`; action is file field name; no HTML submit control needed |
| Multipart delimiter | CRLF framing, closing `--boundary--`, collision-avoiding boundary | Same | `wire.rs:68–99,109–130`; existing offline collision/framing tests passed |

`InvoiceSelector::OrderNumber` preserves input, as a wire client should. Recorded live create trimming does not justify silently trimming queries: exact padded queries were answered 7 (`behaviour.md:40–45`). A shared external id returns the newest holder, not a unique immutable match (`:63–70`). The parser does not claim that returned data proves caller ownership.

## Complete queried-invoice field/type/nesting coverage

This inventory follows SD's **19 structures and 125 child declarations**, independently enumerated from the fresh download. `s` = string; `i` = XSD int; `d` = double; `b` = boolean; `date` = XSD date. `?` on a Rust mapping means `Option`; `false` means missing/empty is folded to false. Fields grouped on one row share schema type/optionality unless stated otherwise. Presence relaxations are explicit, not omitted from the comparison.

### Root and reusable structures (9 + 4 + 5 + 2 declarations)

| Path / type | Schema | Rust mapping / presence |
|---|---|---|
| `szamla/{szallito,alap,vevo,tetelek,osszegek}` | R containers | `supplier`, `info`, `buyer`, `items`, `totals`; each wrapper required |
| `szamla/qutetek` | O container | `financial_items`, absent → empty vector |
| `szamla/cimkek` | O container | `labels`, absent → empty vector |
| `szamla/kifizetesek` | O container | `credit_entries`, absent → empty vector |
| `szamla/pdf` | O s | `pdf: Option<Pdf>`; decoded, absence/blank → None |
| `cimTipus/orszag` | O s | `Address::country: ?String` |
| `cimTipus/{irsz,telepules,cim}` | R s | `zip`, `city`, `address`: required String, empty string accepted |
| `cimpostaTipus/{nev,orszag,irsz,telepules,cim}` | O s | `BuyerPostalAddress::{name,country,zip,city,address}`: all optional |
| `bankTipus/{nev,bankszamla}` | O s | `Bank::{name,account}`: optional strings |

Code: `query_xml.rs:80–147,324–339,590–608,618–670,802–826`. Seller postal address deliberately uses `cimTipus`, not the all-optional buyer postal type.

### Seller (8 declarations)

| `szallito/…` | Schema | `Supplier` mapping |
|---|---|---|
| `id` | R i | `id: ?i64`, relaxed absence/empty |
| `nev` | R s | `name: String` |
| `cim` | R `cimTipus` | `address: Address` |
| `postacim` | O `cimTipus` | `postal_address: ?Address` |
| `adoszam` | R s | `tax_number: ?String`, relaxed absence/empty |
| `csoportazonosito` | O s | `group_id: ?String` |
| `adoszameu` | O s | `eu_tax_number: ?String` |
| `bank` | O `bankTipus` | `bank: ?Bank` |

Code: `query_xml.rs:129–147,672–703`. No seller-id stability/account-identity guarantee inferred; see `behaviour.md:144`.

### Core invoice data (28 declarations)

| `alap/…` | Schema | `InvoiceInfo` mapping |
|---|---|---|
| `id` | R i | required `id: i64` |
| `szamlaszam` | R s | required `invoice_number: InvoiceNumber`; Q-H1 |
| `gazdEsemAzon` | R i | `economic_event_id: ?i64` |
| `forras` | O i | `source: ?i64`; 26/28/34 retained, no enum restriction |
| `iktatoszam` | O s | `registration_number: ?String` |
| `tipus` | R s | `document_type: DocumentType`, unknown string preserved |
| `eszamla` | R i | `appearance: InvoiceAppearance`, integer preserved |
| `hivszamlaszam`, `hivdijbekszam` | O s | `referenced_invoice_number`, `referenced_proforma_number`: optional numbers |
| `kelt`, `telj`, `fizh` | R date | `issue_date`, `fulfillment_date`, `due_date`: optional civil dates |
| `fizmod` | R s | `payment_method: ?PaymentMethod`, open token |
| `fizmodunified` | R restricted s | `unified_payment_method: ?String`, open rather than enum-validated |
| `keszpenz` | R b | `cash_payment: bool`, default/empty false |
| `rendelesszam` | O s | `order_number: ?String`, decoded nonblank text preserved |
| `nyelv` | R restricted s | `language: ?String`, open rather than language-enum-validated |
| `devizanem` | R s | `currency: ?Currency`, open token, no forced HUF normalization |
| `devizabank` | O s | `exchange_bank: ?String` |
| `devizaarf` | O d | `exchange_rate: ?Decimal` |
| `megjegyzes`, `afatipus` | O s | `comment`, `vat_type`: optional strings |
| `penzforg`, `kata`, `katafokonyv` | R b | `cash_accounting`, `kata`, `kata_ledger`: bool, default/empty false |
| `email` | O s | `email: ?String`, document-associated email |
| `teszt` | R b | `test: ?bool`, absent/empty not invented as false |
| `sztornozott` | O b | `reversed: ?bool`, absent/empty distinct from explicit true |

Code: `query_xml.rs:149–322,708–800,1089–1096`. Appearance `0` means not an invoice, `1` paper, `2/3` electronic, otherwise `Unknown(i64)`. OUT/HOUT additionally names `JS`; the open `DocumentType` preserves it even without a named variant. Presence of `forras` in a synthetic all-fields record does not establish retrieval of external invoices.

### Buyer and buyer ledger (12 + 6 declarations)

| `vevo/…` | Schema | `BuyerInfo` mapping |
|---|---|---|
| `id` | O i | `id: ?i64` |
| `nev` | R s | `name: String` |
| `azonosito` | O s | `identifier: ?String`; distinct from internal numeric id |
| `cim` | R `cimTipus` | `address: ?Address`, relaxed omission; present address still has required components |
| `postacim` | O `cimpostaTipus` | `postal_address: ?BuyerPostalAddress` |
| `email` | O s | `email: ?String` |
| `adoszam` | R s | `tax_number: ?String` |
| `csoportazonosito`, `adoszameu` | O s | `group_id`, `eu_tax_number`: optional strings |
| `lokacio` | R i | `location: ?i64`; 1 domestic, 2 EU, 3 outside EU, −1 unknown, others retained |
| `privatePersonIndicator` | R b | `private_person: bool`, default/empty false |
| `fokonyv` | O container | `ledger: ?BuyerLedgerInfo` |
| `fokonyv/{vevo,vevoazon}` | O s | `ledger.account`, `ledger.buyer_id`: optional strings |
| `fokonyv/datum` | O date | `ledger.date: ?Date` |
| `fokonyv/folyamatostelj` | O b | `ledger.continuous_fulfillment: ?bool` |
| `fokonyv/{elszDatTol,elszDatIg}` | O date | `ledger.settlement_from`, `ledger.settlement_to`: optional dates |

Code: `query_xml.rs:341–398,828–913`. The capital `D`/`T`/`I` in buyer settlement elements is preserved. A present empty ledger returns `Some` with empty fields; absent ledger returns `None`.

### Printed line items and item ledger (1 + 14 + 6 declarations)

| `tetelek/…` | Schema | Mapping |
|---|---|---|
| `tetel` | 1..unbounded | `Vec<DocumentItem>`, empty wrapper tolerated |
| `tetel/nev` | R s | `name: String` |
| `tetel/azonosito` | O s | `id: ?String` |
| `tetel/mennyiseg` | R d | `quantity: Decimal` |
| `tetel/mennyisegiegyseg` | R s | `unit: String` |
| `tetel/nettoegysegar` | R d | `unit_price: Decimal` |
| `tetel/afatipus` | O restricted s | `vat_type: ?String`, future tokens preserved |
| `tetel/afakulcs` | R d ≥ 0 | `vat_rate_code: String`, raw numeric token preserved |
| `tetel/{netto,afa,brutto}` | R d | `net_value`, `vat_value`, `gross_value`: Decimal |
| `tetel/arresafaalap` | O d | `margin_vat_base: ?Decimal` |
| `tetel/megjegyzes` | O s | `comment: ?String` |
| `tetel/sztetordering` | R i | `ordering: ?i64`, not used to reorder the wire rows |
| `tetel/fokonyv` | O container | `ledger: ?DocumentItemLedger` |
| `fokonyv/{arbevetel,afa}` | O s | `revenue_account`, `vat_account`: optional strings |
| `fokonyv/{gazdasagiesemeny,gazdasagiesemenyafa}` | O s | `economic_event`, `vat_economic_event`: optional strings |
| `fokonyv/{elszdattol,elszdatig}` | O date | `settlement_from`, `settlement_to`: optional dates |

Code: `query_xml.rs:400–463,915–998`. Item settlement elements are lowercase, unlike buyer ledger. Negative storno quantities and amounts remain valid data. `vat_rate()` gives nonblank `afatipus` precedence over numeric text; `types.rs:294–325` recognizes numeric XML padding and scientific notation without changing the stored raw token. No computation replaces reported totals.

### Financial items and labels (1 + 10 + 1 declarations)

| Path | Schema | Mapping |
|---|---|---|
| `qutetek/qutet` | 0..unbounded | `Vec<FinancialItem>` |
| `qutet/nev` | R s | `name: String` |
| `qutet/afatipus` | O restricted s | `vat_type: ?String` |
| `qutet/afakulcs` | R d ≥ 0 | `vat_rate_code: String`; same helper precedence |
| `qutet/{netto,afa,brutto}` | R d | `net`, `vat`, `gross`: Decimal |
| `qutet/{elszdattol,elszdatig}` | O date | `settlement_from`, `settlement_to`: optional dates |
| `qutet/afalevon` | R i | `deductible_vat: i64` |
| `qutet/cimkek` | O container | `labels: Vec<String>`, absent → empty |
| `cimkek/cimke` (invoice and financial item) | 0..1 s | `Vec<String>`, additionally tolerates repeated labels |

Code: `query_xml.rs:465–505,1000–1050`. SD calls these financial items, not printed items. It does not specify the unit/range of `afalevon`; current neutral integer rustdoc is appropriate. Repeated labels are intentional leniency relative to `maxOccurs=1`, not loss of a field.

### Totals and recorded credit entries (2 + 5 + 3 + 1 + 7 declarations)

| Path | Schema | Mapping |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded | `Totals::by_vat_rate: Vec<VatTotal>`, missing subtotals tolerated |
| `osszegek/totalossz` | R container | `Totals::total: GrandTotal`, required |
| `afakulcsossz/afatipus` | O restricted s | `vat_type: ?String` |
| `afakulcsossz/afakulcs` | R d ≥ 0 | `vat_rate_code: String` |
| `afakulcsossz/{netto,afa,brutto}` | R d | `net`, `vat`, `gross`: Decimal |
| `totalossz/{netto,afa,brutto}` | R d | `net`, `vat`, `gross`: Decimal |
| `kifizetesek/kifizetes` | 1..unbounded when wrapper present | `Vec<RecordedCreditEntry>`, empty wrapper tolerated |
| `kifizetes/datum` | R date | `date: Date`, required; empty/malformed fails |
| `kifizetes/jogcim` | R s | `title: PaymentMethod`, open token |
| `kifizetes/osszeg` | R d | `amount: Decimal` |
| `kifizetes/megjegyzes` | O s | `comment: ?String` |
| `kifizetes/bankszamlaszam` | O s | `bank_account: ?String` |
| `kifizetes/banktranzid` | O i | `bank_transaction_id: ?i64` |
| `kifizetes/devizaarf` | O d | `exchange_rate: ?Decimal` |

Code: `xml.rs:806–888`; `types.rs:1055–1107`; `query_xml.rs:507–533,1052–1087`. HOUT says the bank account is the sender's when known, otherwise the invoice's account; current rustdoc (`:525–527`) captures both. Credit-entry order is preserved, not interpreted as submission order. Integer-width widening to `i64` is explicit (`query_xml.rs:10–25`); every integer declaration in this fresh SD is actually `int`.

## Waybill and ledger request/response distinction

CX/CD locates `fuvarlevel` **under creation's `xmlszamla`, between buyer and items**, not under queried `szamla`. `ops/waybill.rs:1–5` says exactly this. All of its structures were compared so that their absence from the query model is not mistaken for incomplete inspection:

| Creation path | Schema fields, in order | Current representation |
|---|---|---|
| `fuvarlevel` | optional strings `uticel`, `futarSzolgalat`, `vonalkod`, `megjegyzes`; optional containers `tof`, `ppp`, `sprinter`, `mpl` | `Waybill` and writer, `ops/waybill.rs:87–113,131–178` |
| `tof` | optional strings `azonosito`, `shipmentID`; optional int `csomagszam`; optional strings `countryCode`, `zip`, `service` | `TransOFlex`, `:9–24,137–148`; parcel count `u32` checked against XSD int maximum by create validation |
| `ppp` | optional strings `vonalkodPrefix`, `vonalkodPostfix` | `PickPackPoint`, `:26–33,149–154` |
| `sprinter` | optional strings `azonosito`, `feladokod`, `iranykod`; optional int `csomagszam`; optional strings `vonalkodPostfix`, `szallitasiIdo` | `Sprinter`, `:35–50,155–166`; checked parcel count |
| `mpl` | required strings `vevokod`, `vonalkod`, `tomeg`; optional string `kulonszolgaltatasok`; optional double `erteknyilvanitas` | `Mpl`, `:52–84,167–177`; weight intentionally string, value Decimal |

`uticel` is documented as unused; carrier string is open (TOF/PPP/SPRINTER/FOXPOST/MPL/GLS/EMPTY); the general barcode is fallback when carrier data is insufficient. There are no separate FOXPOST/GLS child schemas to add. This comparison establishes field/type/order coverage, not carrier rendering.

The creation buyer ledger uses `vevoFokonyv/{konyvelesDatum,vevoAzonosito,vevoFokonyviSzam,folyamatosTelj,elszDatumTol,elszDatumIg}`; response buyer ledger instead uses `fokonyv/{vevo,vevoazon,datum,folyamatostelj,elszDatTol,elszDatIg}`. Creation item ledger uses `tetelFokonyv/{gazdasagiEsem,gazdasagiEsemAfa,arbevetelFokonyviSzam,afaFokonyviSzam,elszDatumTol,elszDatumIg}`; queried item ledger uses the lowercase response names inventoried above. The current query model follows the response schema, not a reused request shape.

Similarly, request `ExchangeRate` (`types.rs:943–980`) serializes invoice `arfolyamBank/arfolyam`; queried values are separate `alap/devizabank` and `alap/devizaarf`, plus credit-entry `devizaarf`. No documented queried `arfolyam` container is missing.

## PDF envelope, errors and malformed-input coverage

PA/HPA/ED has nine child declarations. Successful payload fields and errors are covered separately:

| Element / condition | Implementation |
|---|---|
| `sikeres` R boolean | Required verdict; true/false/1/0 read by `xml::Verdict` (`xml.rs:455–504`). Empty is currently folded to false by `flexible_bool`, so it cannot promote a blank verdict to success. |
| `hibakod`, `hibauzenet` O strings | Typed open code and diagnostic. Body-only code 7 works. Missing code on false verdict stays absent; malformed optional diagnostic does not erase a readable refusal. |
| `szamlaszam` O string | Required for successful `InvoicePdf`; body before decoded header (`envelope.rs:123–132,275–279`). Its schema optionality also serves error envelopes; no observed success without identity. |
| `szamlanetto`, `szamlabrutto`, `kintlevoseg` O doubles | `net_total`, `gross_total`, `outstanding`: optional Decimal, body first then raw monetary header. Missing is not zero. |
| `vevoifiokurl` O string | `customer_account_url`, body first then decoded header; XML receives entity decoding, not percent/plus decoding. Its envelope-specific trimming policy is deliberate (`README.md:269–271`). |
| `pdf` O base64Binary | Required in successful PDF result, base64 decoded; not required by the shared XSD on errors. |
| Header numeric delimiter | Ungrouped dot or comma accepted; `1,234` is 1.234, never a thousands-group heuristic. Mixed/repeated delimiters and grouping spaces refused. `envelope.rs:344–371`; live comma evidence `behaviour.md:160`. |
| Text header encoding | Percent/plus decoded once through `RawResponse::szlahu`; raw numeric signs and codes bypass it (`wire.rs:227–249,343–350`). |
| Header/down/status precedence | `szlahu_down` → error header → known non-2xx status → body (`wire.rs:291–310`). No blanket success from a number header. |
| Code 56 shared-envelope exception | Readable numbered notification failure may preserve optional metadata; PDF query still requires PDF. No new query-specific vendor guarantee inferred from issuing semantics. |
| XML query error alternative | Recognizes only expected `szamla` or `xmlszamlavalasz`; false envelope becomes error, successful envelope becomes `UnexpectedBody` (`query_xml.rs:563–586`). |
| Complete XML and namespaces | `xml.rs:183–283` checks complete root through EOF and lexical validity; `:285–360` filters foreign subtrees and canonicalizes namespace aliases before serde. Unknown nesting cannot supply identity/reversal. |
| Scalar/list shape | Duplicate recognized singleton fields and nested scalar children refused; repeated rows across aliases/interleaved extensions retained (`tests/response_namespaces.rs`). Unknown well-formed extensions tolerated. |

This is not XSD validation: empty lists, open tokens, reordered recognizable elements and sparse optionalized fields intentionally remain usable. Nor is it a full PDF validator or an unbounded-domain numeric/date parser.

## Recorded-live evidence and deliberate deviations

| Evidence read | Consequence for this review |
|---|---|
| `behaviour.md:40–45` | Queries are exact/case-sensitive; order lookup may return a storno/corrective/latest different type. Preserve selector text and returned document type. |
| `:63–71` | External ids are nonunique/latest-holder, not echoed, and consumed/deleted proformas disappear from query. No new response identity field or uniqueness promise. |
| `:77–80` | Reversal marker absent before reversal and on the storno itself; true on original afterwards. Reversal removes original credit entries. No inferred outstanding amount. |
| `:96–98` | Fulfillment date observed on every queried probe; lenient optional date representation is still deliberate. `eszamla=1` is paper, `3` electronic; 2 is annotation-supported but not observed. |
| `:111` | Later create changed earlier queried buyer data on the test account; current buyer rustdoc correctly qualifies this rather than promising an immutable issuance record. |
| `:133–142` | Credit-entry ordering not submission order; query code 7 can be body-only and does not prove a document never existed. |
| `:159–162` | Stored amounts may be independently rounded; `27.0` queried VAT rate is normal; header decimals may contain commas. Do not recompute returned values or reject ordinary decimal formatting. |
| `:168–170,174–278` | Observations are bounded to test-account probes; unverified cases remain unverified. |
| `tests/live.rs:42–133,136–169` | Current ignored invoice/proforma journeys assert XML query by number/external id, included PDF, amounts, appearance, credit entries, reversal references and disappearance. No execution during this review. |

`ops/query_xml.rs:1693–1798` contains live-derived but reduced inline reversal/test/code-7 regression shapes. `fixtures/synthetic/agent/*` are explicitly synthetic (`fixtures/SOURCES.md:23–32`); `fixtures/upstream/agent/*` are official documentation examples, principally historical July acquisitions (`:34–39`). Their passing tests are not evidence of current live emission of every optional block. The current docs were checked independently of that cache.

The behavior note still points to a former `tests/live.rs::eszamla_semantics` entry (`:23,304`); current `docs/testing.md:163–164` places investigative appearance cases in probes and matching cases in core journeys. This is a navigation drift in the evidence index, not a query parser finding.

## Tests executed and limits

### Existing offline suite

Executed successfully:

```sh
cargo test --offline --locked -p szamlazz-agent --lib --test upstream --test business_text --test numeric_fidelity --test response_headers --test response_namespaces --test response_completion --test literals
```

| Target | Passed | Failed / ignored |
|---|---:|---:|
| Library unit tests | 185 | 0 / 0 |
| `business_text` | 2 | 0 / 0 |
| `literals` | 2 | 0 / 0 |
| `numeric_fidelity` | 6 | 0 / 0 |
| `response_completion` | 4 | 0 / 0 |
| `response_headers` | 14 | 0 / 0 |
| `response_namespaces` | 11 | 0 / 0 |
| `upstream` | 11 | 0 / 0 |
| **Total** | **235** | **0 / 0** |

These cover all-section field projection, all eleven date positions, query goldens/selectors, JSON round-trips, optional business text, exact numeric parsing and VAT interpretation, PDF balance/URL precedence, alias/interleaved row retention, namespace boundaries, complete XML, and actual corpus handling. The corpus was available; its successful checks were not missing-corpus skips. It remains a historical example corpus, not newly downloaded live fixtures.

### Independent targeted probes

`cargo build --offline --locked -p szamlazz-agent` succeeded. A standalone Rust executable linked the resulting `target/debug/libszamlazz_agent.rlib` and called public `AgentRequest` methods directly; no network client feature or live target was enabled.

| Probe | Observed result |
|---|---|
| Replace synthetic `alap/szamlaszam` with empty / XML whitespace | Both parse successfully, preserving blank identity (Q-H1) |
| Replace it with `INV-1` | Success, exact number retained |
| Replace invoice dates with `2024-02-29Z`, `2024-02-29+14:00` | Both success, printed February 29 retained |
| Date suffix `2024-02-29junk` | Parse failure |
| `2024-02-29T12:00:00`, `20240229`, `0000-02-29` | Accepted legacy Jiff spellings/domain (Q-B1) |
| `-0001-02-28` | Refused; not full XSD BCE lexical support (Q-B1) |
| PDF envelope with valid PDF but no number | `Parse(Missing("szamlaszam"))` |
| PDF envelope with number and empty PDF | `Parse(Missing("pdf"))` |
| Base64 with embedded NBSP | Decoded to five bytes |
| Valid base64 of `not a pdf` | Accepted as `Pdf(9 bytes)`; no PDF-format validation |
| Each of three selectors, with `&`/`<`, on both writers | Exactly one selector, escaped, in documented EN/download order; dummy agent-key and username/password root placement correct |

Reproduction core for Q-H1 (uses the existing synthetic fixture):

```rust
use szamlazz_agent::{InvoiceSelector, ops::query_xml::QueryInvoiceXml};
use szamlazz_agent::wire::{AgentRequest, RawResponse};

let request = QueryInvoiceXml::new(InvoiceSelector::OrderNumber("O".into()));
let body = include_str!("/home/laborant/szamlazz-rs/fixtures/synthetic/agent/szamla_query.xml")
    .replace("E-TST-2026-66", "");
let document = request.parse(&RawResponse::new::<&str, &str>([], body.into_bytes())).unwrap();
assert_eq!(document.info.invoice_number.as_str(), "");
```

No code fixes, repository test additions, schema updates, full XSD validator installation, live account calls, PDF rendering tests, transport-feature matrix or worker suite execution were performed. Those are not implied by the 235 passing parser/writer-focused tests. Current results support complete documented field coverage and normal lexical interoperability, with the explicitly bounded hardening and source-conflict follow-ups above.
