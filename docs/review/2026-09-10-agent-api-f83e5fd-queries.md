# Számla Agent XML/PDF query review — f83e5fd

**Reviewed HEAD:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`

**Source acquisition and verification:** 2026-09-10

**Scope:** `szamlazz-agent` invoice XML query, invoice PDF query, every queried field/nested type, selectors, response/header handling, and the shared XML/namespace/lexical/date/number/PDF machinery on these paths.

## 1. Result

**Complete declared field coverage; three confirmed parser findings.** The freshly fetched invoice schema has **125 child-element declarations in 19 complex structures**. Every declaration is represented. The PDF response exposes all **six successful payload fields**, with verdict/error fields handled separately. All twelve generated operation × selector × credential combinations pass the freshly downloaded request XSDs.

| ID | Severity | Confirmed finding | Evidence boundary |
|---|---|---|---|
| FQ-1 | **P2 — interoperability** | Repeated rows with different prefixes for the same namespace are rejected as duplicate fields. Ignored extensions between rows also break list parsing. | Four mixed-prefix list cases independently pass the official XSD but fail the public parser. Vendor emission/frequency not established. |
| FQ-2 | **P3 — uncommon valid XML rejected** | An explicitly declared `xml` prefix with a character-reference-equivalent reserved URI is rejected before namespace normalization. | Both query parsers fail; the full invoice control passes the official XSD. |
| FQ-3 | **P3 — XML namespace conformance** | Expanded-name duplicate attributes and several forbidden namespace declarations pass whole-document checking. | Synthetic namespace-invalid documents accepted by both query paths; no identity/verdict spoofing or live incident demonstrated. |

**The older CQ-2 lexical specimens are closed:** literal NUL, illegal character references, `]]>` in ordinary text, undefined entities in ignored content, adjacent attributes, literal `<` in attributes and illegal names are now rejected. The new `xmlparser` layer is effective on those cases. FQ-3 is a different, narrower namespace-layer gap; it is not a reassertion of the old character-injection finding.

**PDF URL trimming remains observable, but is classified as a policy/fidelity note**, not a counted defect: the current README explicitly excludes URLs from its business-text preservation promise. The HU PDF request schema conflict remains a vendor ambiguity. No P0/P1 finding, missing declared business field, panic, normal-example regression, or live-account failure was established.

P2 here means a reproducible failure to consume a conforming supported response; it does not mean the vendor currently emits the unusual prefix arrangement. P3 concerns are bounded low-priority interoperability/conformance improvements. They are not claims of exploitation or release-blocking incidents.

## 2. Baseline, independence and source acquisition

- `git rev-parse HEAD` matched the requested full SHA before and after verification. The reviewed crate, workspace dependency manifests/lock, and `docs/szamlazz-hu-behaviour.md` had no working-tree changes. Unrelated Restate work and older untracked reports existed and changed concurrently; they were left intact.
- This is a directly conducted full-surface audit at the requested commit, including unchanged code.
- Current source, schemas, specimens and public parser behavior control conclusions. The older `2026-09-10-agent-api-current-queries.md` and `…current-adjudication.md` were read as leads and counterarguments, not as primary evidence. Current README policies were checked independently.
- Read `docs/szamlazz-hu-behaviour.md` in full. Its observations are bounded to one TEST account/dates, with raw logs outside this repository. None of those evidenced deviations is called a bug here.
- Only unauthenticated documentation GETs were made. No Számla Agent POST, credentials, live-account query, source/test/fixture edit, or repository dependency change. This report is the only repository file written by this review. New offline tooling and downloaded evidence live under `/tmp/opencode/query-f83e5fd-review/`, authored using `apply_patch`.

### Fresh primary-source register

All URLs below were fetched during this review, including code blocks hidden behind documentation tabs. EN/HU pages displayed site build `v202608271632`; that is not a publication date for each statement.

| Sources | Controlling statement / use |
|---|---|
| XML request [EN][xr], [HU][xhr] | “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved via this interface”; HU: “csak belső (Számlázz.hu-ban kiállított) kimenő számlák adatait lehet lekérni.” POST/multipart/action; three alternative selectors. |
| XML request XML/XSD [EN][xx], [HU][xhx], [download][xd] | “the order of the fields is fixed, they cannot be interchanged.” Seven direct-root elements including credentials, selectors and `pdf`. |
| XML response [EN][xs], [HU][xhs] | Success is “Full `szamla` XML document”; HU “Teljes `szamla` XML dokumentum”. Failure is `xmlszamlavalasz` with false verdict/code/message; unknown number/order/external id is code **7**. |
| PDF request [EN][pr], [HU][phr] | `action-szamla_agent_pdf`; number, order **or** external identifier. “if multiple documents share the same order number, the last one is returned”; external id must have been set at creation. |
| PDF request XML/XSD [EN][px], [HU][phx], [download][pd] | Direct-root credentials, selectors, `valaszVerzio`; HU source disagreement detailed in §5. |
| PDF response [EN][ps], [HU][phs], [envelope download][sd] | Version 2: “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; version 1/omitted is PDF/text. “In both cases, additional parameters may also arrive in the HTTP response header.” Nine schema declarations: three verdict/error and six payload fields. |
| [Invoice response XSD][sx] | `targetNamespace="http://www.szamlazz.hu/szamla"`, `elementFormDefault="qualified"`; full field inventory in §6. No imports/includes requiring expansion. |
| Shared outgoing annotations [EN][ae], [HU][ah] | Same `<szamla>` model and inline schema; used for field meaning, not to expand the Agent retrieval boundary. Appearance annotation: “0: nem számla, 1: papír számla, 2: e-számla, 3: e-számla”. |
| [Invoice-create XML][ix], [create response][ir], [error handling][eh] | Followed the XML-response schema navigation; inspected shared response/header vocabulary and error conventions. Create request fields are not presumed queried fields. |
| Categories [XML][xc], [PDF][pc] | Checked operation navigation and linked request/response/schema pages. |
| [W3C XML 1.0][xmlspec], [Namespaces 1.0][nsspec], [XSD datatypes][datatypes] | Independent lexical/expanded-name/date/number/string rules. No assertion that the crate is a general XSD validator. |

### Download fingerprints

SHA-256 calculated from freshly acquired bytes:

| File | SHA-256 |
|---|---|
| `xmlszamlaxml.xsd` | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| `xmlszamlapdf.xsd` | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| `szamla.xsd` | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| `xmlszamlavalasz.xsd` | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |

Fresh EN/HU outgoing inline schemas match the download's child declarations, types, presence and multiplicity; their named simple-type enumerations match too. Both XML request inline schemas, EN PDF request inline schema and both PDF response inline schemas match their downloads' child declarations. The HU PDF inline schema does not (§5).

The scratch `acquisition.json` records all 26 URL/hash acquisitions. In particular, HU PDF XML/XSD HTML was `04fb76c65a80106a45df9bf8d8b3bfc05dd0cece62e7a2c067627b47d60ebc95`; EN was `bc1f0711108ab37ec15f58003b6e53789e3d27874ce3de5197e581c2391df666`. Current XML response HTML hashes were EN `9b149388ab398fc3bfb157cb91f30482c9361beaaaa551c897d1e7d7d995c3ba` and HU `4f51ff9a2951d7848d30942cce93b20b40d2fbc9468c91dedd8dee37e7458c62`. HTML hash changes alone are not semantic changes.

## 3. Confirmed findings

Code locations are relative to `crates/szamlazz-agent/src/` unless another path is given. All refer to the pinned revision.

### FQ-1 — List grouping depends on prefix spelling and adjacency

**P2; high confidence in behavior and conformance counterexample.** This loses availability of the entire queried document, not silently one row.

**Locations:** `ops/query_xml.rs:588` deserializes the namespace-filtered text directly; list fields are `:915–919` (`tetel`), `:1000–1004` (`qutet`), `:1045–1056` (`cimke`, `kifizetes`), and `xml.rs:689–697` (`afakulcsossz`). `xml.rs:196–255`, especially `:199–200,221–227`, preserves protocol prefixes and inserts a placeholder for foreign subtrees. Workspace `Cargo.toml:29` enables quick-xml's `serialize`, not `overlapped-lists`.

**Source:** [fresh `szamla.xsd`][sx] lines 219, 250, 269, 283 declare `tetel`, `afakulcsossz`, `kifizetes`, `qutet` with `maxOccurs="unbounded"`. [Namespaces §6][nsapply] defines the expanded name from namespace URI plus local name, independently of prefix spelling. The crate README `:253–259` expressly says “sparse content and well-formed unknown extensions remain supported.”

**Reproduction:** start with a successfully parsed document containing one complete `tetel`. Duplicate that row immediately after itself, changing only the duplicate's opening/closing QName:

```xml
<tetelek>
  <tetel><!-- complete original row --></tetel>
  <p:tetel xmlns:p="http://www.szamlazz.hu/szamla">
    <!-- identical row children, still in the inherited protocol namespace -->
  </p:tetel>
</tetelek>
```

Actual public `QueryInvoiceXml::parse` result:

```text
Parse(Xml(XmlError(Custom("duplicate field `tetel`"))))
```

The comment placeholders above describe the transformation, not the executed test data. `/tmp/opencode/query-f83e5fd-review/xsd.py` generated a document containing every declared field, validated it against the fresh XSD using **libxml2's XSD engine**, and performed this transformation with complete rows. Baseline and two same-QName rows both validate and parse. Two mixed-QName rows validate but fail parsing. The complete failing body is `alias-rows.xml` in that directory.

The same XSD-valid mixed-prefix experiment fails for **`qutet`, `kifizetes` and `afakulcsossz`** with their respective duplicate-field errors. Mixed-prefix repeated `cimke` also fails, but the current schema permits only one label; that label case is a permissive-library control, not an XSD-conforming counterexample.

**Related extension reproduction:** insert `<ext/>` or `<x:ext xmlns:x="urn:future"/>` between two otherwise identical rows. All five vector fields fail. Adjacent rows, comments and processing instructions between rows succeed. The extension specimens are well-formed, but the current XSD does not declare the added element; their relevance is the explicit library extension policy, not a claim that the XSD permits arbitrary children.

**Mechanism checked:** quick-xml 0.42.0 `src/de/map.rs:863–867` uses `n.name() == start.name()` for sequence membership (raw QName); `:913–919,967–970` stops a sequence on a different tag without overlapped-list support. Serde then encounters another field whose *local* name is the same and refuses a duplicate. The foreign placeholder intentionally prevents scalar concatenation but also interrupts a vector.

**Bounded correction direction:** group list entries using protocol expanded names and tolerate ignored children at the list/container boundary, retaining parent-path checks and duplicate-singleton refusal. Enabling overlapped lists alone should not be assumed to solve raw-QName alias grouping. Preserve the scalar protection against `tr<foreign/>ue` becoming `true`.

**Limits:** no evidence that szamlazz.hu's current serializer varies prefixes between rows, and no failure in the ordinary published examples after their explicit PDF-placeholder substitution. Severity is based on rejection of a supported conforming shape, not measured frequency.

### FQ-2 — Equivalent reserved `xml` namespace declaration is rejected

**P3; high confidence.** Both query paths can reject otherwise valid data for an unused, legal namespace declaration.

**Locations:** `xml.rs:73,81` invokes `NsReader` before `namespace_uri` can normalize references (`:258–268`). The latter normalizes ordinary resolved URIs, but cannot recover an error already raised by the reader. The second `NsReader` in `protocol_text` (`:205,211–213`) has the same dependency behavior. PDF reaches this via `ops/envelope.rs:292–294` and `ops/query_pdf.rs:83–92`.

**Source:** [Namespaces §2.3][nscompare]: “replacement of XML character and entity references has already been done before any comparison.” [§3][nsdecl] says the `xml` prefix “MAY, but need not, be declared” and is bound to `http://www.w3.org/XML/1998/namespace`.

**Complete PDF repro:**

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"
 xmlns:xml="http://www.w3.org/XML/1998/n&#97;mespace">
  <sikeres>true</sikeres><szamlaszam>I</szamlaszam><pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

Result through `QueryInvoicePdf::parse`:

```text
Parse(Xml(XmlError(InvalidXml(Namespace(InvalidXmlPrefixBind(
  "http://www.w3.org/XML/1998/n&#97;mespace"
))))))
```

Replacing `n&#97;mespace` with `namespace` succeeds. The XML query behaves identically, including when the declaration is on an ignored foreign child. ElementTree accepts the equivalent declaration; the generated full invoice with it also passes the fresh `szamla.xsd` in libxml2.

**Mechanism checked:** quick-xml `src/name.rs:679–685` compares the raw namespace value with its reserved constant. This is a dependency behavior exposed by the crate's parser, not a new bug in numeric entity decoding at `namespace_uri`.

**Bounded correction direction:** reserved-name validation must use normalized namespace values before the reader's raw comparison, or use a namespace reader with that property. Keep ordinary aliases and reserved-prefix prohibitions. No observed vendor use of this unusual spelling; hence P3.

### FQ-3 — Namespace-invalid documents still pass XML checks

**P3; high confidence in conformance gap, low demonstrated operational impact.** This concerns namespace well-formedness, not the old XML-character failures.

**Locations:** `xml.rs:96–109` iterates attributes and rejects unknown prefixes, but does not check duplicate **expanded** attribute names. `:173–194` tokenizes XML grammar without enforcing namespace constraints. Reserved namespace/default/empty-prefix checks are delegated to quick-xml and incomplete; `namespace_uri` at `:260–274` unescapes already-resolved URIs without validating bindings.

**Sources/quotes:** [Namespaces §6.3][nsunique]: “no element [may] have two attributes with the same expanded name.” [§3][nsdecl]: other prefixes “MUST NOT be bound” to the reserved XML namespace; the XML and XMLNS namespace names “MUST NOT be declared as the default namespace.” [Using Qualified Names][nsqual]: “the attribute value in a prefixed namespace declaration MUST NOT be empty.”

**Reproduction:** insert each independent specimen before the closing root of an otherwise successful XML or PDF response:

| Specimen | Crate result | Independent namespace check |
|---|---|---|
| `<extension xmlns:a="urn:same" xmlns:b="urn:same" a:x="1" b:x="2"/>` | Success | ElementTree: duplicate attribute |
| Same, with second URI `urn:s&#97;me` | Success | Duplicate attribute after entity expansion |
| `<extension xmlns:x="http://www.w3.org/XML/1998/n&#97;mespace"/>` | Success | Forbidden binding to reserved namespace |
| `<extension xmlns="http://www.w3.org/XML/1998/namespace"/>` | Success | Forbidden reserved default namespace |
| `<extension xmlns="http://www.w3.org/2000/xmlns/"/>` | Success | Forbidden reserved default namespace |
| `<extension xmlns:x=""/>` | Success if unused | Prefix undeclaration prohibited by Namespaces 1.0 |

These also reproduce as unused attributes on the accepted root where applicable. Literal `xmlns:x="http://www.w3.org/XML/1998/namespace"` is correctly refused; escaping one URI character bypasses that raw-string check. Ordinary same-QName duplicate attributes are refused. Distinct namespace URIs with the same local attribute name are accepted correctly. An undeclared prefix actually used on an element/attribute is still refused.

**Impact:** `parse` success is not proof of complete Namespaces 1.0 conformance. No specimen here supplied a foreign invoice number, reversal or success verdict, and no illegal control character reached a business field. The attributes are ignored metadata, which bounds practical severity. Prefix undeclaration is allowed by Namespaces 1.1, but the reviewed documents and XML 1.0 boundary do not declare that alternative; it is specifically a 1.0 comparison.

**Bounded correction direction:** validate each declaration and attribute's normalized expanded name, including ignored subtrees, while keeping unknown well-formed fields. Do not impose the invoice XSD's sparse-content requirements as a proxy for namespace correctness.

## 4. Operation, selector and response coverage

### Request coverage

| Contract element | XML query | PDF query / common behavior |
|---|---|---|
| Endpoint / HTTP | `https://www.szamlazz.hu/szamla/`, POST | Same; `wire.rs:7–14,395–408` |
| Multipart field | `action-szamla_agent_xml`, `query_xml.rs:535–537` | `action-szamla_agent_pdf`, `query_pdf.rs:58–60`; file multipart construction `wire.rs:66–99` |
| Root / namespace | `xmlszamlaxml` / `http://www.szamlazz.hu/xmlszamlaxml`, `:539–557` | `xmlszamlapdf` / `http://www.szamlazz.hu/xmlszamlapdf`, `:62–80` |
| Credentials | `felhasznalo`, `jelszo`, `szamlaagentkulcs` directly under root | Key or username/password; ordered writer `xml.rs:484–494`; no `beallitasok` |
| `szamlaszam` | `InvoiceSelector::InvoiceNumber` | Same; exact caller string XML-escaped |
| `rendelesSzam` | `InvoiceSelector::OrderNumber` | Same; exact caller string, not trimmed/case-folded |
| `szamlaKulsoAzon` | `InvoiceSelector::ExternalId`, emitted last | Same; no uniqueness/idempotency assumption |
| `pdf` | `include_pdf: bool`, default false, always explicitly emitted | No request `pdf` flag: the operation itself retrieves PDF |
| `valaszVerzio` | Not a declared XML-query request field | Always shared `ops::RESPONSE_VERSION` = 2, `query_pdf.rs:75` |

`InvoiceSelector` (`types.rs:1034–1053`) represents exactly one selector. Unrestricted strings may be empty: neither schema supplies a nonblank facet, and the client leaves unusable-selector rejection to szamlazz.hu. `write_xml` escapes text; `to_wire` additionally refuses forbidden XML 1.0 characters (`wire.rs:402–419`). `xsi:schemaLocation` is not necessary for namespace identity or a missing business field. The generated credential/selector strings contained `<`, `&` and non-ASCII characters and passed XSD validation.

The internal-outgoing retrieval boundary is explicitly documented at `query_xml.rs:51–53`; `source` at `:239–241` is qualified as shared-schema metadata. Do not infer incoming/imported-document support from a field in this shared model.

### All response fields and branches

| Wire/result | Current behavior and location |
|---|---|
| XML `<szamla>` | Required root/namespace; full document projection, `query_xml.rs:563–609` |
| XML `<xmlszamlavalasz>` failure | Body-only error parsed, including code 7; `:580–586`, `:1784–1798` |
| XML generic success envelope | Refused as wrong operation shape even if numbered; `:583–586` |
| PDF `sikeres` | Required field, flexible boolean helper; `xml.rs:347–378`; empty is leniently false, never success |
| PDF `hibakod`, `hibauzenet` | Typed API error, unknown token retained, absent code not invented; same verdict code |
| PDF `szamlaszam` | `InvoicePdf.invoice_number`; body then decoded header; blank missing, boundary whitespace trimmed; `envelope.rs:120–133,278–282,329–331` |
| PDF `szamlanetto`, `szamlabrutto` | `net_total`, `gross_total: Option<Decimal>`; body then header; `query_pdf.rs:42–45,88–89`, `envelope.rs:229–240` |
| PDF `kintlevoseg` | `outstanding: Option<Decimal>`, absence not zero; `query_pdf.rs:46–49,90`, `envelope.rs:241–246` |
| PDF `vevoifiokurl` | `customer_account_url: Option<String>`; body then decoded header; `query_pdf.rs:50–53,91`, `envelope.rs:135–143` |
| PDF `pdf` | Required `Pdf`; missing/blank is `Missing("pdf")`, invalid base64 fails; `query_pdf.rs:92` |
| XML `pdf` | Optional `Pdf`, missing/blank → None; malformed nonblank base64 fails whole query; `query_xml.rs:604–607` |

The envelope schema makes payload fields optional to accommodate failures. Requiring a usable number and PDF for dedicated PDF success is consistent with its published example/operation. No supported numberless PDF success was established.

**Header coverage:** `RawResponse` retains raw values, performs case-insensitive first-match name lookup and decodes textual values once (`wire.rs:182–199,227–249,343–350`). `+` becomes space, `%2B` becomes plus. Codes/numbers are not percent-decoded. Priority is nonblank `szlahu_down`, error header, known non-2xx status, then body (`:273–311`), with the shared numbered-56 exception on the PDF path. Body-only code 7 at HTTP 200 is an API error; a body-only code at HTTP 500 is `HttpStatus` by explicit policy, not a claim about the origin of that status.

PDF net/gross/outstanding use body-before-header, with absent/blank body permitting fallback. Malformed nonblank body values fail rather than fall back. Monetary headers allow ungrouped dot/comma finite numbers; comma `100,01` is evidenced in behavior notes `:160`. Missing headers are absent, present blank monetary headers are malformed. Header customer URLs decode once; XML URLs receive entity decoding only. Metadata parsed by `CreatedInvoice` but not projected to `InvoicePdf` (`document_id`, payment method, notification flag) is not declared in the PDF-specific body schema. Generic mention of additional headers does not prove a missing promised PDF result field. Raw headers remain accessible through `RawResponse`.

The XML result does not merge success-header balances into `<szamla>`; its XSD contains no outstanding amount, customer URL or external-id field. This is not a missing declared body capability.

## 5. Distinct ambiguities and policies

### A1 — HU PDF request schema disagrees with EN/download

Fresh [HU inline schema][phx] has malformed `...xmlszamlapdf"xmlns:tns=...` (no attribute separator). ElementTree fails at **line 1, column 140**. Even after that syntax is repaired, it requires `szamlaszam` (`minOccurs="1"`) and orders the tail `szamlaszam`, `valaszVerzio`, `rendelesSzam`, `szamlaKulsoAzon`.

Fresh [EN inline][px] and [download][pd] make `szamlaszam` optional and order `szamlaszam`, `rendelesSzam`, `valaszVerzio`, `szamlaKulsoAzon`. HU request prose says number/order/external id are alternatives (“vagy”). The crate agrees with EN/download and prose. **Vendor clarification, not a confirmed writer defect.** Historical external-id PDF success supports that capability but does not settle every possible request-order alternative.

### P1 — PDF URL boundary trimming: remaining fidelity loss, explicitly separate policy

`envelope.rs:114–115` uses `xml::de::empty_as_none`; `xml.rs:593–598` applies Unicode `trim` before parsing `String`. A successful PDF envelope containing:

```xml
<vevoifiokurl>&#160;opaque:x&#160;</vevoifiokurl>
```

returns `customer_account_url == Some("opaque:x")`; surrounding ordinary spaces are removed too. [Envelope XSD][sd] line 12 declares `type="string"`, not a whitespace-collapsing token. `query_pdf.rs:50` calls the URL opaque. Nevertheless, README `:244–251` explicitly excludes URLs from business-text preservation. A percent-encoded URL such as `https://example.test/?x=%20%2B` remains unchanged; no usable vendor link with a changed target was demonstrated. Body/header boundary-whitespace handling differs. **Keep as an explicit normalization/fidelity policy, at most P3 if exact URL text is desired**, rather than counting the old CQ-1 as an unqualified defect. The concern is shared through the envelope, not one finding per operation.

### P2 — Finite exact numbers, lenient dates, sparse content and artifacts

- Monetary `double` values are projected into finite exact Decimal, a deliberately smaller domain; no silent rounding. `INF`, `NaN`, `1e-29` and excessive precision fail. This is a representation policy, not complete XSD double support (§7).
- Civil dates preserve printed day and discard accepted timezones. Legacy Jiff syntax remains broader than XSD in some directions and narrower in year range. The README explicitly disclaims strict XSD lexical validation.
- Required-in-XSD fields may be absent in actual examples. Defaults/options are deliberate leniency; malformed nonempty optional dates still fail the **Agent** query. Adatkapcsolat's separate content-tolerance rule must not be imported into this crate.
- `Pdf` validates base64, not PDF document structure. `AA==` returns one zero byte; Unicode whitespace inside base64 is tolerated. An optional XML-query PDF may be absent even when requested, while dedicated PDF query requires it. No raw-PDF/version-1 parser is necessary when the request pins version 2.
- Unknown XML extensions are discarded, not retained in `InvoiceDocument`; raw body is available only while the caller retains `RawResponse`. FQ-1 limits the advertised extension tolerance inside repeated lists. JSON round trips preserve modeled values, not unmodeled XML data or arbitrary future JSON fields.

### P3 — Shared numbered-56 handling on PDF reads

`query_pdf.rs:84` inherits `envelope::parse_issued`. A synthetic false-verdict code-56 envelope with a number and valid PDF returns `InvoicePdf`; the notification-warning bit is not projected. This exception is documented for issuance, but no freshly fetched PDF source establishes notification failure on a PDF read. It is **an inherited unverified operation policy**, not a demonstrated query incident. The new header-56 fallback no longer turns malformed XML into success (`envelope.rs:188–196,254–257`); existing checks and direct PDF controls confirm this. Plain header-only 56 cannot produce a successful PDF query without PDF bytes.

### Source/example defects kept separate

1. EN/HU XML success samples contain prose in `<pdf>` (“The receipt .pdf can be found here in BASE64 encoding”); PDF success samples contain `....` in base64. All four unmodified samples fail base64, as they should. Replacing **only PDF text** with synthetic `JVBERi0=` allows all four to parse. This proves remaining model compatibility, not a valid vendor PDF.
2. The XML example omits schema-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, `lokacio`, `privatePersonIndicator`, `sztetordering`. It also has empty tax/bank/ledger values. Strict presence enforcement would reject the vendor's own example.
3. Its item values `380/76/456` disagree with totals `464/93/557`. The parser correctly returns both without arithmetic repair.
4. HU XML request sample uses empty `<pdf>` despite boolean type; current writer emits a valid explicit boolean.
5. PDF response prose permits version omitted/1, while request XSD requires `valaszVerzio`. Explicit 2 avoids the conflict.
6. XML response navigation points to invoice **generation** XML/XSD. That request schema is not `szamla.xsd`; waybill/layout/create-only fields cannot be inferred as missing response fields.
7. Shared example annotations call `eszamla` “string” and item ordering “double”, whereas XSD says int. Code follows integer semantics and the documented appearance codes. EN nesting prose is less precise than HU/schema; the actual shape is `alap/tipus`.
8. `cimke` has `maxOccurs="1"`; a vector is permissive extra capacity. The schema gives `afalevon` no percentage/unit/range semantics. The code correctly avoids inventing them.

## 6. Complete queried-field inventory

Source: freshly acquired [`szamla.xsd`][sx], corroborated with both outgoing inline schemas. **R/O** denote XSD required/optional, not the parser's gate. `?` denotes `Option`; unqualified strings below are `String`, numbers are Decimal unless marked i64. Grouped fields map in the listed order. Optional business strings use `business_text`: XML-whitespace-only is None; otherwise decoded text is preserved, including padding/NBSP.

### Root and reusable structures (9 + 4 + 5 + 2 declarations)

Public `query_xml.rs:76–147,324–339`; wire `:618–703,802–826`.

| Wire declaration | XSD | Public mapping / presence behavior |
|---|---|---|
| `szamla/szallito`, `alap`, `vevo` | R complex | `supplier: Supplier`, `info: InvoiceInfo`, `buyer: BuyerInfo`, required wrappers |
| `szamla/tetelek` | R complex | `items: Vec<DocumentItem>`, required wrapper |
| `szamla/qutetek` | O complex | `financial_items: Vec<FinancialItem>`, absent → empty |
| `szamla/cimkek` | O complex | `labels: Vec<String>`, absent → empty |
| `szamla/osszegek` | R complex | `totals: Totals`, required |
| `szamla/kifizetesek` | O complex | `credit_entries: Vec<RecordedCreditEntry>`, absent → empty |
| `szamla/pdf` | O string | `pdf: Pdf?`, base64 semantics supplied by operation docs |
| `cimTipus/orszag` | O string | `Address.country?` |
| `cimTipus/irsz`, `telepules`, `cim` | R strings | `Address.zip`, `city`, `address`; empty accepted, missing refused |
| `cimpostaTipus/nev`, `orszag`, `irsz`, `telepules`, `cim` | O strings | `BuyerPostalAddress.name?`, `country?`, `zip?`, `city?`, `address?` |
| `bankTipus/nev`, `bankszamla` | O strings | `Bank.name?`, `account?` |

`cimTipus` serves supplier billing/postal and buyer billing; buyer postal is the separate `cimpostaTipus` with recipient name. No supplier-postal recipient name is declared.

### Supplier (8 declarations)

Public `query_xml.rs:125–147`; wire/conversion `:672–703`; XSD lines 105–116.

| `szallito/…` | XSD | `Supplier` |
|---|---|---|
| `id` | R int | `id: i64?`, omission/empty tolerated |
| `nev` | R string | `name` |
| `cim` | R complex | `address: Address` |
| `postacim` | O complex | `postal_address: Address?` |
| `adoszam` | R string | `tax_number?` |
| `csoportazonosito`, `adoszameu` | O strings | `group_id?`, `eu_tax_number?` |
| `bank` | O complex | `bank: Bank?` |

This is the printed seller party, not a verified account pin. The schema gives no stability guarantee for its internal id.

### Core invoice data (28 declarations)

Public `query_xml.rs:225–322`; wire/conversion `:705–800`; XSD lines 120–152.

| `alap/…` | XSD | `InvoiceInfo` |
|---|---|---|
| `id` | R int | `id: i64`, required |
| `szamlaszam` | R string | `invoice_number: InvoiceNumber`, required element; unrestricted wire string |
| `gazdEsemAzon` | R int | `economic_event_id: i64?` |
| `forras` | O int | `source: i64?`, unknown code retained |
| `iktatoszam` | O string | `registration_number?`, receiver-assigned registration number |
| `tipus` | R string | `document_type: DocumentType`, open token |
| `eszamla` | R int | `appearance: InvoiceAppearance`, required open i64 code |
| `hivszamlaszam`, `hivdijbekszam` | O strings | `referenced_invoice_number?`, `referenced_proforma_number?`; nonblank text preserved by `:1089–1096` |
| `kelt`, `telj`, `fizh` | R dates | `issue_date?`, `fulfillment_date?`, `due_date?` |
| `fizmod` | R string | `payment_method: PaymentMethod?`, open |
| `fizmodunified` | R restricted string | `unified_payment_method?`, open |
| `keszpenz` | R boolean | `cash_payment: bool`, absent/empty → false |
| `rendelesszam` | O string | `order_number?`; lowercase response name, unlike request `rendelesSzam` |
| `nyelv` | R restricted string | `language?`, open |
| `devizanem` | R string | `currency: Currency?`, preserves token |
| `devizabank`, `devizaarf` | O string/double | `exchange_bank?`, `exchange_rate: Decimal?` |
| `megjegyzes`, `afatipus` | O strings | `comment?`, `vat_type?` |
| `penzforg`, `kata`, `katafokonyv` | R booleans | `cash_accounting`, `kata`, `kata_ledger`, absent/empty → false |
| `email` | O string | `email?`, per-document email |
| `teszt` | R boolean | `test: bool?`, absent/empty → unknown |
| `sztornozott` | O boolean | `reversed: bool?`, absent distinct from false |

Appearance `:149–223` maps 0 → NotInvoice, 1 → Paper, 2/3 → Electronic(exact code), other → Unknown(exact code); integer survives JSON. This agrees with vendor annotation and bounded P73 observations. `Electronic` built by a caller reports e-invoice by variant, irrespective of contained integer; wire construction is through `From<i64>`.

### Buyer and buyer ledger (12 + 6 declarations)

Public `query_xml.rs:341–398`; wire/conversion `:828–913`; XSD lines 155–180.

| Path | XSD | Public mapping |
|---|---|---|
| `vevo/id` | O int | `BuyerInfo.id: i64?` |
| `vevo/nev` | R string | `name` |
| `vevo/azonosito` | O string | `identifier?`, account-local partner identifier, distinct from numeric id |
| `vevo/cim` | R complex | `address: Address?`, omission tolerated |
| `vevo/postacim` | O complex | `postal_address: BuyerPostalAddress?` |
| `vevo/email` | O string | `email?`, not `alap/email` |
| `vevo/adoszam` | R string | `tax_number?` |
| `vevo/csoportazonosito`, `adoszameu` | O strings | `group_id?`, `eu_tax_number?` |
| `vevo/lokacio` | R int | `location: i64?`; 1 domestic, 2 EU, 3 outside EU, -1 unknown; other integers preserved |
| `vevo/privatePersonIndicator` | R boolean | `private_person: bool`, absent/empty → false |
| `vevo/fokonyv` | O complex | `ledger: BuyerLedgerInfo?` |
| `fokonyv/vevo`, `vevoazon` | O strings | `account?`, `buyer_id?` |
| `fokonyv/datum` | O date | `date?` |
| `fokonyv/folyamatostelj` | O boolean | `continuous_fulfillment: bool?` |
| `fokonyv/elszDatTol`, `elszDatIg` | O dates | `settlement_from?`, `settlement_to?`; mixed-case wire names explicitly renamed |

Buyer rustdoc correctly warns about historically observed mutability (`:360–365`), rather than promising an immutable issuance snapshot.

### Printed items and item ledger (14 + 6 declarations)

Public `query_xml.rs:400–463`; wire/conversion `:921–998`; XSD lines 183–216.

| `tetelek/tetel/…` | XSD | `DocumentItem` |
|---|---|---|
| `nev` | R string | `name` |
| `azonosito` | O string | `id?` |
| `mennyiseg` | R double | `quantity` |
| `mennyisegiegyseg` | R string | `unit` |
| `nettoegysegar` | R double | `unit_price` |
| `afatipus` | O restricted string | `vat_type?` |
| `afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`, raw token |
| `netto` | R double | `net_value` |
| `arresafaalap` | O double | `margin_vat_base?` |
| `afa`, `brutto` | R doubles | `vat_value`, `gross_value` |
| `megjegyzes` | O string | `comment?` |
| `sztetordering` | R int | `ordering: i64?`, omission tolerated |
| `fokonyv` | O complex | `ledger: DocumentItemLedger?` |
| `fokonyv/arbevetel`, `afa` | O strings | `revenue_account?`, `vat_account?` (ledger account, not amount) |
| `fokonyv/gazdasagiesemeny`, `gazdasagiesemenyafa` | O strings | `economic_event?`, `vat_economic_event?` |
| `fokonyv/elszdattol`, `elszdatig` | O dates | `settlement_from?`, `settlement_to?`; lowercase wire names |

No sorting or recomputation. Negative storno quantities/totals survive. `vat_rate()` uses nonblank `afatipus` first, otherwise the numeric token (`:456–462`).

### Financial items (10 declarations)

Public `query_xml.rs:465–505`; wire/conversion `:1006–1043`; XSD lines 287–306.

| `qutetek/qutet/…` | XSD | `FinancialItem` |
|---|---|---|
| `nev` | R string | `name` |
| `afatipus` | O restricted string | `vat_type?` |
| `afakulcs` | R double, minInclusive 0 | `vat_rate_code: String` |
| `netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross` |
| `elszdattol`, `elszdatig` | O dates | `settlement_from?`, `settlement_to?` |
| `afalevon` | R int | `deductible_vat: i64`, no invented percentage/range |
| `cimkek` | O complex | `labels: Vec<String>` |

Special VAT type takes precedence here too (`:499–504`). Speculative QUiCK expansion in prose is explicitly qualified.

### Totals (2 + 5 + 3 declarations)

Wire/projection `xml.rs:683–763`; public `types.rs:1055–1107`; XSD lines 224–253.

| Path | XSD | Public mapping |
|---|---|---|
| `osszegek/afakulcsossz` | 1..unbounded complex | `Totals.by_vat_rate: Vec<VatTotal>`, absence tolerated |
| `osszegek/totalossz` | R complex | `Totals.total: GrandTotal`, required |
| `afakulcsossz/afatipus` | O restricted string | `VatTotal.vat_type?` |
| `afakulcsossz/afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`, special-code precedence in helper |
| `afakulcsossz/netto`, `afa`, `brutto` | R doubles | `VatTotal.net`, `vat`, `gross` |
| `totalossz/netto`, `afa`, `brutto` | R doubles | `GrandTotal.net`, `vat`, `gross` |

### Credit entries and list declarations (7 + 4 declarations)

Public `query_xml.rs:507–533`; wire `:1052–1087`; XSD lines 217–221,256–285.

| Path | XSD | Public mapping |
|---|---|---|
| `kifizetesek/kifizetes/datum` | R date | `RecordedCreditEntry.date: Date`, required/nonempty |
| `…/jogcim` | R string | `title: PaymentMethod`, open |
| `…/osszeg` | R double | `amount` |
| `…/megjegyzes` | O string | `comment?` |
| `…/bankszamlaszam` | O string | `bank_account?` |
| `…/banktranzid` | O int | `bank_transaction_id: i64?` |
| `…/devizaarf` | O double | `exchange_rate?` |
| `tetelekTipus/tetel` | 1..unbounded | `Vec<DocumentItem>`, empty present wrapper accepted |
| `qutetekTipus/qutet` | 0..unbounded | `Vec<FinancialItem>` |
| `kifizetesekTipus/kifizetes` | 1..unbounded | `Vec<RecordedCreditEntry>`, empty wrapper accepted; no five-entry read cap |
| `cimkekTipus/cimke` | 0..1 | `Vec<String>`, permissive multiplicity |

Credit bank-account doc (`:525–528`) agrees with the HU annotation: “A kifizetés ténylegesen erről a bankszámláról érkezett, vagy a számlán szereplő bankszámlaszám (ha a küldő bankszámlaszám nem ismert)” — sender's account, otherwise the printed account when sender unknown. It is not always a destination account.

**Inventory arithmetic:** 9 + 4 + 5 + 2 + 8 + 28 + 12 + 6 + 14 + 6 + 10 + 2 + 5 + 3 + 7 + 4 = **125**. Shared complex structures are counted once, not once per use. The all-fields synthetic invoice generated from these declarations validates and parses.

**Not declared on the queried response:** `fuvarlevel`/carrier structures, request `arfolyam` object, layout/preview flags, erasure codes, `szamlaKulsoAzon`. Exchange data is in `devizabank`, `devizaarf`, credit-entry `devizaarf`. No missing response-field finding is inferred from request-only capabilities.

## 7. Shared parsing audit

### Numbers and open tokens

- All monetary/quantity positions use `xml::de::decimal` / `optional_decimal` (`xml.rs:503–521`) and exact `number::numeric/parse` (`number.rs:63–141`), without an intermediate float. PDF uses the same parser through `envelope.rs:158–168,347–352`.
- Signs, `.5`, `1.`, `e/E` exponents, leading/trailing zeros are accepted. Scratch confirmed `1e-2 → 0.01`, `100e-30 → 1e-28`; existing fidelity tests cover significant-digit boundaries and preserved representable scale. Insignificant coefficient zeros are canceled before domain checks. Exponent expansion is capped before allocation (`number.rs:109–117`). Zero with an enormous exponent remains zero; nonzero huge exponent fails.
- `1e-29`, `1.00000000000000000000000000001`, overflow, `NaN`, `INF`, underscores and XML commas fail; no rounding into a different finite amount was found. Unicode trim at scalar helpers accepts NBSP around numeric text, broader than XSD whitespace; this is permissive input acceptance, not loss of a valid monetary value.
- Every queried integer is i64 by explicit width policy (`query_xml.rs:10–25`). Required `alap/id` and appearance use serde; optional ids use `empty_as_none`; `afalevon` uses `from_text`. Signed/XML-padded integers and i64::MAX pass; i64 overflow, decimal and exponent integer forms fail. No arithmetic is performed on those metadata values.
- Three `afakulcs` positions retain raw `String`, not a validated nonnegative double. Helpers interpret representable numeric forms or retain an unknown token via `VatRate::Other`; `afatipus` takes precedence. This does not modify the raw field. All 21 XSD VAT tokens are representable, including `TEHK` as Other; all fifteen language tokens and future strings survive. Currency remains open, without `Ft`→`HUF` rewriting.
- `DocumentType` retains `SZ/D/ES/VS/HS/SS/SL` and all unknown tokens (`JS` in the shared annotation is Other). `PaymentMethod` preserves arbitrary `fizmod` and `jogcim`; translated `credit_card` / `transfer` need not be coerced to Hungarian variants. `fizmodunified` remains open despite the XSD enumeration. `forras`, `lokacio`, and appearance retain unknown integer codes.

### Dates — all eleven positions

Ten optional dates (3 core, 3 buyer ledger, 2 item ledger, 2 financial item) and required credit-entry date share `xml.rs:523–583`. Existing `query_xml.rs:1333–1469` tests every position.

| Input class | Observed result |
|---|---|
| Ordinary civil date; `Z`; `±hh:mm` through `±14:00`; XML padding | Same printed date, no UTC/day shift |
| Invalid day, `+14:01`, `+01:60`, trailing junk | Parse error |
| Multibyte malformed date such as `é123456789` | Parse error, no panic |
| Missing/empty optional date | None |
| Missing/empty credit-entry date | Error |
| `2026-01-09T12:34:56` | `2026-01-09`; time discarded by retained Jiff parsing |
| `2026-01-09T12:34:56Z` | Error |
| `2026-01-09[Europe/Budapest]` | `2026-01-09` |
| Compact `20240229`, year zero, six-digit signed legacy year | Accepted by existing controls |
| XSD-style `-0001-01-09`, `10000-01-09` | Error in fresh probes |

`civil_date` tries Jiff first (`:527–530`) before the checked suffix fallback. Therefore “rejects every datetime” and “full xs:date validation” are both inaccurate. These retained finite-domain choices are explicitly documented, and BCE/far-future invoice emission was not established. Required and optional helpers differ in absence handling, not malformed nonempty content.

### XML, whitespace, namespaces and data loss

- The original body is UTF-8 checked and scanned through EOF with `NsReader` (`xml.rs:63–168`): known root/namespace, balanced closes, one root, declaration placement, PI target, comments, no outside nonwhitespace text/CDATA/references, no DTD. The new lexical pass (`:173–194`) checks token grammar and unescapes references in **all text/attributes**, including ignored content. `xmlparser` itself rejects illegal literals inside CDATA/comments/PIs. No external entity resolution/network operation occurs.
- XML query admits `szamla` and the error envelope in their own documented namespaces; PDF admits `xmlszamlavalasz`. Missing/wrong namespace fails. `http` and `https` namespace strings are distinct; no URL canonicalization is appropriate. Ordinary entity-escaped namespace URIs work; FQ-2/FQ-3 bound reserved-name behavior.
- Foreign subtrees, including descendants re-entering the protocol namespace, cannot supply protocol data. Same-namespace unknown wrappers are skipped by parent path, not searched recursively. Duplicate recognized singleton fields fail. Known scalar children fail instead of concatenating text around them. Direct reversal/identity spoofing controls and aliases still pass their intended checks. FQ-1 identifies list-specific alias/extension limits.
- Required strings may be empty; optional business strings preserve nonblank decoded text and NBSP. Entities/CDATA are decoded, literal CRLF normalized, character-reference CR retained. Labels and raw tokens are modeled text, not original XML bytes. Empty/absent options and false defaults are intentionally lossy distinctions; unknown fields are not retained.
- XML declaration policy is UTF-8 bytes plus version 1.0 and syntactically valid encoding-name validation (`:277–340`), **not encoding transcoding or consistency validation**. Scratch UTF-8 bodies declaring ISO-8859-1 or UTF-16 are still treated as UTF-8 and accepted. Official examples use UTF-8; no alternate-encoding response support was established. This is a stated implementation limit, not an additional normal-wire finding.
- No order/recalculation logic changes queried money or credit-entry sequence. Same-version public JSON round trips pass, using Rust names, Decimal strings, wire-token strings, integer appearance, civil dates and base64 PDF. PDF balance/URL serde defaults permit older JSON missing those additions; no general historical/future JSON compatibility is claimed.

### Panic and resource-path assessment

No panic reproduced. Date slicing uses `checked_sub` and `split_at_checked` (`xml.rs:533–542`); six-byte offset indexing is guarded by length. Protocol projection slices at reader byte boundaries; its `expect`s assert positions within an in-memory string and initialized output (`:210,226,229–230,241–242`), not numeric/content validity. Writer `expect`s target infallible Vec-backed writes. Numeric precision/exponent arithmetic uses checked/bounded conversions. A 10,000-level **ignored** well-formed extension parsed successfully without stack failure in an isolated subprocess.

This is not a fuzzing proof or an OOM/stack guarantee. Responses are buffered; namespace projection can copy the body; base64 creates compact text and decoded bytes. The review did not benchmark worst-case namespace resolution, impose a body cap, or test enormous recognized recursive structures. The dependency's internal assertions were not exhaustively proven.

## 8. Historical evidence and prior-finding closure

### Behavior notes applied, not mislabeled as defects

| Evidence in `docs/szamlazz-hu-behaviour.md` | Review consequence |
|---|---|
| `:40–45`: exact/case-sensitive order query, last document may be any kind | Preserve selector bytes; do not substitute worker key normalization |
| `:63–71`: shared external id resolves latest holder; PDF by external id works; id not echoed; deleted/consumed proforma disappears | Independent external selector is supported, not an idempotency guarantee; absent query is not “never existed” |
| `:77–80`: reversal marker absent before reversal/on SS; credit entries disappear after reversal | Option marker and empty vector correct; SS reference carries relationship |
| `:96–98`: fulfillment date observed; paper 1/electronic 3; 2 only documented | Optional date is leniency; appearance mapping correct |
| `:111`: later create changes earlier queried buyer data | Current mutability caveat correct |
| `:134`: returned credit entries not in submission order | Preserve result order; do not infer transaction chronology |
| `:141–142`: body-only query error 7 | Body parsing essential; current implementation handles it |
| `:160–162`: stored money independently rounded, VAT rendered `27.0`, comma monetary header | Report stored values, no consistency repair; numeric VAT helper normalization is separate |

Not every default has live evidence. Sparse examples, i64 widening, arbitrary future tokens and legacy date acceptance also reflect explicit library policy. Those facts do not expand the vendor contract.

### Closure table against older query/current/adjudication leads

| Earlier lead | Revalidated at f83e5fd |
|---|---|
| CQ-2: illegal characters/attribute syntax/entities/names pass | **Closed for every listed specimen.** Current lexical pass rejects all seven old cases plus CDATA/comment/PI illegal-character controls. |
| CQ-1: opaque PDF URL trimming | **Behavior remains**, but explicit README exception and preserved percent-encoded URL controls support a policy/fidelity classification, not a counted defect. |
| CV-1 / Q-V1: HU PDF request schema | **Still vendor ambiguity**, freshly fetched and independently parsed. Writer passes EN/download XSD. |
| Q-D1: missing internal-outgoing retrieval limit | **Closed:** `query_xml.rs:51–53,239–241`, fresh EN/HU wording agrees. |
| Missing PDF outstanding/customer URL | **Closed:** fields, body/header precedence and defaults exist. |
| All declared queried fields represented | **Reconfirmed independently** against fresh schemas and generated XSD-valid all-fields body. |
| Timezone-date/padding gaps and multibyte panic concern | **Closed for tested offsets/padding/multibyte cases**, across all eleven positions. Legacy Jiff date policy remains as described; blanket datetime-refusal claim stays withdrawn. |
| Numeric exponent/rounding loss | **Closed for selected fidelity cases**; exact finite Decimal domain remains narrower than XSD double. |
| Trailing/incomplete XML; foreign namespace identity/verdict | **Specific old cases remain closed.** Newly demonstrated FQ-1/FQ-2/FQ-3 narrow any blanket namespace/extension-completeness claim. |
| Shared header-56 malformed-XML promotion / optional-payload handling | Current envelope changes and existing tests pass; direct PDF malformed-XML control fails safely. No new PDF notification event inferred. |
| Bank account direction, buyer mutability, afalevon percentage, appearance semantics | **Closed/reconfirmed:** current docs match source or clearly qualify observed policy. |

No conclusion from a different operation's current review was adopted as a query finding without checking its shared path and query-specific consequence.

## 9. Verification performed and limits

### Existing focused tests

Executed without source/test/fixture modifications:

```text
cargo test -p szamlazz-agent --lib ops::query
  29 passed
cargo test -p szamlazz-agent --lib xml::tests
  27 passed (includes 22 already-counted query_xml tests)
cargo test -p szamlazz-agent --test numeric_fidelity --test response_namespaces --test response_completion --test business_text --test response_headers --test upstream
  41 passed: 6 + 6 + 4 + 2 + 12 + 11
```

**75 distinct selected tests, 97 executions including overlap; zero failures.** Some shared tests cover other operations; that pass is not a review of those operations. Default crate features; no ignored/live test ran. Workspace upstream corpus was present. Its tests can otherwise skip at runtime; request-outline normalization also is not a substitute for XSD validation.

### Fresh offline scratch verification

```text
cargo build --offline --manifest-path /tmp/opencode/query-f83e5fd-review/Cargo.toml
python3 /tmp/opencode/query-f83e5fd-review/fetch.py
python3 /tmp/opencode/query-f83e5fd-review/check.py
python3 /tmp/opencode/query-f83e5fd-review/edge.py
python3 /tmp/opencode/query-f83e5fd-review/xsd.py
python3 /tmp/opencode/query-f83e5fd-review/controls.py
```

- Public `AgentRequest::parse`/`write_xml` called from an external temporary Rust consumer; stdin accepts arbitrary offline XML/PDF bodies. Parser panics caught where unwindable; mutation probes run in separate subprocesses with per-call timeouts (the XSD helper's subprocesses rely on the enclosing command timeout).
- Twenty-six fresh documentation/schema/specification GETs; schema inventories generated independently. Complete manual wire-to-public-field comparison.
- Twelve generated requests validated with **libxml2 XSD validation**, after an initial structural check. Python `lxml` and `xmllint` were unavailable; the scratch-only helper called installed libxml2 via Python ctypes. This native helper is verification tooling, not repository code.
- A synthetic invoice containing all declared fields validates and parses. Four mixed-prefix repeated-list variants validate but fail parsing. Reserved `xml` equivalent declaration also passes full-invoice XSD validation.
- Four fresh success examples checked before/after replacing only unusable placeholder PDF text; two fresh PDF failure examples return typed code 3. HTML `<pre>` text extraction preserves code text as served, not hypothetical missing line separators; EN/HU message-spacing differences are not blamed on the crate.
- Reproduced old lexical cases, namespace-invalid/valid controls, five list families with adjacency/comments/PIs/foreign/same-namespace interruptions, scalar/duplicate controls, numeric/date boundaries, PDF absence/base64/percent-URL behavior, and a deep ignored extension.
- Standalone resolved parser versions: quick-xml **0.42.0**, xmlparser **0.13.6**, Jiff **0.2.35**, rust_decimal **1.43.0**, serde **1.0.229**, base64 **0.23.1**. Scratch uses its own offline-resolved lockfile; not every transitive/proc-macro patch is asserted identical to the workspace lock.

### Limits and evidence quality

No live calls, actual downloaded PDF, browser/wasm/client-feature matrix, full workspace suite, exhaustive XML conformance suite, fuzzing campaign or performance benchmark. ElementTree is an independent check, not an infallible oracle: it rejected the XML 1.0 Fifth Edition-valid supplementary-plane name `<𐀀/>`; that discrepancy was checked against the W3C Name production and **not** reported as a crate bug. Namespace findings instead have direct specification support, specific countercontrols, and, for valid-response failures, XSD validation where stated.

No claim that all vendor-emitted values were exercised, that absence settles document existence, or that XML parse success certifies PDF contents. Official docs are mutable; hashes identify this acquisition. Fixture provenance (`fixtures/SOURCES.md:3–39,90–140`) distinguishes workspace vendor copies, synthetic packaged fixtures and a project-modified **create** XSD. No stale fixture or old report was treated as authoritative over freshly acquired evidence.

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
[nsspec]: https://www.w3.org/TR/xml-names/
[nscompare]: https://www.w3.org/TR/xml-names/#NSNameComparison
[nsdecl]: https://www.w3.org/TR/xml-names/#ns-decl
[nsqual]: https://www.w3.org/TR/xml-names/#ns-using
[nsapply]: https://www.w3.org/TR/xml-names/#scoping
[nsunique]: https://www.w3.org/TR/xml-names/#uniqAttrs
[datatypes]: https://www.w3.org/TR/xmlschema-2/
