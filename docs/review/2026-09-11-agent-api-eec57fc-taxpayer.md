# Fresh full-source Számla Agent taxpayer review — eec57fc

**Reviewed HEAD:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd`.
**Public sources fetched:** 2026-09-11.
**Result: no confirmed actionable P0–P3 defect in this scope.** The request matches the current Számla Agent request contract. The response projection covers every taxpayer business field and detailed-address component declared by the examined NAV 2.0/3.0 schemas, with the correct version-specific expanded names and parent paths. It is not a lossless NAV response model: inherited exchange/software metadata and notifications are omitted, as inventoried below.

The principal unresolved questions remain **OK without validity**, **generic NAV error forwarding**, and **the version/optional fields currently forwarded by Számla Agent**. None is established as a failure of a supported wrapper response by the available evidence. Fixed historical findings are suppressed, rather than carried forward from earlier reports.

## 1. Scope, method and evidence boundary

- Reviewed all 842 lines of `crates/szamlazz-agent/src/ops/taxpayer.rs`: prefix construction/serde, public types, request writer, both response layouts, pull-parser frames, text processing, verdict conversion and unit tests.
- Traced the operation's shared credential writer, multipart/checking boundary, XML completion/namespace/lexical checks and error-code classification. Inspected the associated tests, fixtures, provenance and README.
- Started from the requested live category, followed all taxpayer request/response/XML/XSD routes and their embedded examples, followed the response's NAV specification link, and supplemented the delegated response definition with first-party NAV schemas. Also checked the corresponding Hungarian pages and older standalone XSD routes. No direct-NAV request feature was promoted into the Számla Agent request contract.
- This is a **full-source review at the pinned HEAD**, not a diff review. Fresh source and vendor checks supplied the conclusions. A broad early evidence search also returned historical review excerpts; they were not treated as current evidence. The previous `837dad0` taxpayer report and adjudication history were subsequently consulted for suppression leads, then checked against current source and freshly fetched definitions.
- Read all of `docs/szamlazz-hu-behaviour.md` and the relevant research/question records. Public documentation examples, synthetic tests and executable live probes are kept distinct from recorded vendor execution.
- **No Rust test suite, vendor-live test, probe, credential access, account request or production call was performed.** The parent runs the suite. No test pass count is asserted. Concrete parser examples below are source-traced reproductions, not newly executed Rust tests. Public GETs and in-memory PDF/schema extraction were performed.
- Only this report was authored by this review. The initial tree was clean; unrelated concurrent changes later appeared in `crates/restate-szamlazz/tests/e2e/`. They were not used as review evidence or modified.

**Line notation:** `T` = `crates/szamlazz-agent/src/ops/taxpayer.rs`; `paths` = `crates/szamlazz-agent/tests/taxpayer_paths.rs`. Other `src/…`, `tests/…` and `README.md` citations are relative to `crates/szamlazz-agent/`. All implementation line references are for `eec57fc`, not a prior report.

## 2. Fresh public-source ledger

Current taxpayer pages identify the site build as **`v202608271632`**. The older standalone XSD routes identify **`v202606031507`**. These are documentation build identifiers, not dates of observed service behavior. The English response page explicitly dates its examples **2020-11-04**.

| ID | URL(s) fetched | Coverage and relevant quotation/result |
|---|---|---|
| S0 | [EN category](https://docs.szamlazz.hu/agent/category/querying-taxpayer), [HU category](https://docs.szamlazz.hu/hu/agent/category/querying-taxpayer) | Operation purpose and its three current descendants: request, response, XML + XSD. |
| S1 | [EN request](https://docs.szamlazz.hu/agent/querying_taxpayer/request), [HU request](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/request) | “send a single XML file in an HTTP POST request”; endpoint, multipart action, HTML form example; data is “from the Online Invoice Platform of NAV”. |
| S2 | [EN XML + XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml), [HU XML + XSD](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xml) | Entire request example and inline schema, both tabs. HU: “Az XML-ben a mezők sorrendje kötött, **nem felcserélhetők**” — field order is fixed, not interchangeable. |
| S3 | [EN response](https://docs.szamlazz.hu/agent/querying_taxpayer/response), [HU response](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response) | All three examples: success, code-57 error, invalid number. “The response always matches the `QueryTaxPayerResponse` type”; “Last update for example responses: 2020-11-04.” Schema tab delegates to N1 §1.8.9. |
| S4 | [EN standalone XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xsd), [HU standalone XSD](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xsd) | Older route, same relevant request fields/order/cardinality/prefix facets. |
| S5 | <https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd> | Working downloadable XML schema; independently fetched, not assumed from the repository copy. |
| S6 | <https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd> | Request example's HTTP schema-location hint fetched as HTTPS: **404**. No schema evidence obtained from this URL. |
| S7 | <https://docs.szamlazz.hu/agent/basics/authentication> | “either an Agent key (recommended) or a username and password”; key lowercase/case sensitivity; same key allowed in both legacy fields; user must have exactly one account. |
| S8 | [EN error handling](https://docs.szamlazz.hu/agent/basics/error-handling), [HU error handling](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Numeric Agent code meanings and same-request retry limit. Taxpayer is absent from the enumerated operations using legacy plain-text errors. |
| N1 | [Vendor-linked NAV 3.0 specification PDF](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf) | §1.8.9, **printed pp.63–69 / physical PDF pp.73–79**; generic errors §3.1–3.2, **printed pp.162–166 / physical pp.172–176**. Fresh PDF GET succeeded; sections extracted and read. |
| N2A | [NAV 2.0 invoiceApi.xsd][n2a] | Header/result inheritance 596–705; taxpayer response 1668–1697; software/address/business types 1866–1993. |
| N2D | [NAV 2.0 invoiceData.xsd][n2d] | Identifier facets, DetailedAddressType 965–1044, separate SimpleAddressType 1987–2024, TaxNumberType 2225–2250. |
| N3A | [NAV 3.0 invoiceApi.xsd][n3a] | Incorporation tokens 43–68; inheritance 548–565; generic error 638–661; request/response 1534–1581; software/address/business types 1756–1889. |
| N3B | [NAV 3.0 invoiceBase.xsd][n3b] | DetailedAddressType 185–264, separate SimpleAddressType 265–302, TaxNumberType 303–328. |
| NC | [NAV NTCA 1.0 common.xsd][nc] | Correct imported **1.0** namespace: string facets, header/result 544–647, notifications 668–701, generic fault types. A newer Common namespace is not interchangeable. |

The request/example links on S1 resolve to S2; the response examples are inline on S3, not separate downloadable example links. S6 is embedded in the request example. Site-wide navigation, support forms and unrelated operation links are not additional taxpayer request/response definitions.

### Acquisition identity and limitations

Fresh GitHub commit lookups resolved Online-Invoice `master` to `cc7a775d6dce361311e409abb9934eb755f2749c`, `API-2.0` to `84442e64bc2cd7feb368fedb8199645188962b23`, and Common `common-1.0.0` to `ab8d7887967492e5f6d6e25447be853fd767add8`.

| Fresh downloaded source | SHA-256 |
|---|---|
| N1, 6,496,304 bytes | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N3B | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| NC | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |

The web-fetch tool rejected N1 for exceeding 5 MB. Python 3 fetched it in memory; `pdftotext` from the installed Nix store extracted it through stdin/stdout. No older PDF or earlier extraction was reused. An initial physical-page selection was ten pages early; the actual taxpayer/error sections were then extracted at the physical page ranges above. Failed discovery guesses (`Online-Invoice/master/src/schemas/invoiceApi.xsd`, Common's nonexistent `master` tree) supplied no evidence. Hashes identify downloaded bytes, not an execution of either service.

## 3. Request coverage: every field and prefix rule

| Contract / exact source | Current implementation | Assessment |
|---|---|---|
| S1: POST to `https://www.szamlazz.hu/szamla/`, `multipart/form-data`, file field `action-szamla_agent_taxpayer` | T:264–279; `src/wire.rs:14,66–99,405–411` | Correct action, endpoint constant and one XML file part. Taxpayer contributes no extra attachments. |
| S2/S5: root `xmltaxpayer`, `targetNamespace="http://www.szamlazz.hu/xmltaxpayer"`, `elementFormDefault="qualified"` | T:269–278; `src/xml.rs:157–179` | Correct namespace, qualified descendants via default binding, UTF-8 XML 1.0 declaration. |
| Root sequence: `beallitasok`, then `torzsszam`; each `minOccurs="1"`, `maxOccurs="1"` | T:273–276 | Both always present, in the declared order. |
| `beallitasok/felhasznalo`: optional `xs:string`, first | `src/xml.rs:630–636`, `src/credentials.rs:53–81` | Emitted for username/password authentication, XML-escaped. |
| `beallitasok/jelszo`: optional `xs:string`, second | Same | Emitted with username, in order; not added to taxpayer request data. |
| `beallitasok/szamlaagentkulcs`: optional `xs:string`, third | T:274; `src/xml.rs:632` | Key variant emits this alone. S7 recommends this form. |
| `torzsszam`: `xs:string`, `<length value="8"/>`, `<pattern value="[0-9]{8}"/>` | T:15–73,85–109,276 | Exactly eight ASCII digits; string representation retains leading zeroes. |

All public prefix entry points preserve the invariant: `FromStr`, `TryFrom<&str>`, `TryFrom<String>`, custom `Deserialize`, and `QueryTaxpayer::new` validate; `From<TaxpayerPrefix>` accepts an already validated value. The tuple field is private. Request JSON cannot introduce an unchecked prefix. No full-number-to-prefix truncation or trimming occurs.

Source-traced examples: `01234567`, `00000000`, `12345678` are structurally accepted; seven/nine digits, `12345678-2-42`, outer spaces, internal whitespace, signs and non-ASCII digit characters are refused by T:27–32. This checks **format**, not existence or a local checksum. S3's seven-digit example reports code 57 and explicitly quotes the same pattern; no source requires a further local check-digit algorithm.

N1 printed p.64 describes “A lekérdezni kívánt magyar adószám első 8 jegye” — the first eight digits of the Hungarian tax number. The inline schema comment's “adóazonosító jel” wording is misleading: it is not permission to send a ten-digit personal tax identifier. NC even declares that different identifier separately.

S7's alternative of putting the same key in both legacy fields is representable with `Credentials::user_password`. Emitting both key and password modes simultaneously is unnecessary. Credentials remain unchanged rather than silently lowercased; the lowercase rule is documented at `src/credentials.rs:5–9`.

There is **no wrapper request field** for `valaszVerzio`, NAV version, country, pagination, as-of date or predecessor tax number. Native NAV `header`, `user`, `passwordHash`, `requestSignature`, `software` and `predecessorTaxNumber` belong to the intermediary's NAV request, not this request XSD. N1 p.64 additionally says predecessor input has no further effect on this operation after validation.

Omitting `xsi:schemaLocation` is justified: it is a schema hint, not a required business field, and its published target currently fails. Checked `to_wire` also rejects characters XML 1.0 cannot represent (`src/wire.rs:405–431`); low-level `write_xml` is explicitly unchecked.

## 4. Complete versioned response-path comparison

Namespace identity follows **where an element is declared**, not where its imported type is declared. Arbitrary prefixes and default namespaces are equivalent when they resolve to these exact URIs; HTTP namespace URIs do not become HTTPS when schema downloads do.

| Layout | API (`A`) | Result/header (`R`) | Tax/address components (`C`) |
|---|---|---|---|
| 2.0 | `http://schemas.nav.gov.hu/OSA/2.0/api` | Same as A | `http://schemas.nav.gov.hu/OSA/2.0/data` |
| 3.0 | `http://schemas.nav.gov.hu/OSA/3.0/api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

N2A `QueryTaxpayerResponseType` extends its API `BasicResponseType`. N3A extends API `BasicOnlineInvoiceResponseType`, which extends NC `BasicResponseType`. Thus 3.0 has Common header/result but API software and business elements. T:323–337 matches this precisely.

| Path from `{A}QueryTaxpayerResponse` | Declaring namespace(s) | Implementation |
|---|---|---|
| Root | A; exact `QueryTaxpayerResponse` spelling | T:404–416 accepts exactly the two supported root/namespace pairs. S3's prose capital-P spelling is inconsistent with its own examples and NAV schemas; code uses the actual spelling. |
| `result/{funcCode,errorCode,message}` | R for both container and leaves | T:340–348 |
| `infoDate`, `taxpayerValidity`, `taxpayerData` | A, direct root children | T:343–347 |
| `taxpayerData/{taxpayerName,taxpayerShortName,vatGroupMembership,incorporation}` | A; incorporation is declared only in 3.0 | T:349–358 |
| `taxpayerData/taxNumberDetail` | A, even though its type is imported | T:349–351 |
| `taxNumberDetail/{taxpayerId,vatCode,countyCode}` | C | T:359–363 |
| `taxpayerData/taxpayerAddressList/taxpayerAddressItem` | A at each level | T:351,364 |
| `taxpayerAddressItem/{taxpayerAddressType,taxpayerAddress}` | A | T:365 |
| `taxpayerAddress/*` detailed-address leaves | C | T:366–383 |

The remaining declared header/software/notification paths are inventoried in §6; they are deliberately not extraction paths.

### Integrity of extraction

- `response_root` validates the entire UTF-8 document before extraction: one expected root, matching/completed closes, valid prolog/epilog through EOF, no second root, no DTD, valid namespace bindings/attributes and lexical/entity checks (`src/xml.rs:40–154,201–300,396–448`). Malformed ignored extensions do not evade that preflight.
- `Layout::child` only recognizes direct children of a known frame. A foreign or unknown ancestor remains unknown even if a descendant has a familiar name/namespace (T:306–307,340–397,435–439). A hidden `funcCode` cannot manufacture a verdict; a root-level `taxpayerName` cannot manufacture business data.
- Recognized singleton containers and leaves are checked per parent, including empty-first duplicates (T:440–448). A child element inside a recognized scalar is refused even when that child is foreign (T:429–434).
- Only `taxpayerAddressItem` repeats. Each starts a fresh `TaxpayerAddress`; its own singleton tracking and assignments prevent sibling-address leakage (T:441,457–459,535–561). Ignored extensions between items do not change item order.
- Namespace declarations are normalized before reserved-binding and expanded-attribute checks; escaped aliases therefore work. Undeclared prefixes, duplicate expanded attributes, reserved `xmlns:` elements and colon-bearing PI targets are rejected by the shared boundary. Current tests expressly cover these formerly defective cases.

This is not full XSD validation: order of response fields, all required business content, known token restrictions and schema defaults are not enforced. That matches the README's explicit sparse-content policy, not an accidental claim of schema conformance.

## 5. Every taxpayer business field and address component

“Required” below means the XSD requires it **if its enclosing optional container is present**. The public projection reads missing/empty business content as absent. T:184–262 declares the output; T:519–589 assigns it.

| Wire path | NAV definition | Public field / assignment |
|---|---|---|
| Root `taxpayerValidity` | Optional `xs:boolean` in N2A:1682 and N3A:1566; whether existing and valid | `valid: bool`, T:189–190,567–577; mandatory locally for OK at T:598–600 |
| Root `infoDate` | Optional `xs:dateTime`; “Last date on which the data was changed” (N2A:1676–1680, N3A:1560–1564) | `info_date: Option<String>`, T:209–216,584 |
| `taxpayerData/taxpayerName` | Required full name | `name`, T:191–192,579 |
| `taxpayerData/taxpayerShortName` | Optional short name, both versions | `short_name`, T:193–196,580 |
| `taxpayerData/taxNumberDetail/taxpayerId` | Required eight-digit stem; group identifier for group taxation | `tax_number`, T:217–219,585 |
| `…/vatCode` | Optional one-digit string, schema `[1-5]{1}` | `vat_code`, T:220–222,586 |
| `…/countyCode` | Optional two-digit string | `county_code`, T:197–200,581 |
| `taxpayerData/vatGroupMembership` | Optional eight-digit group identifier | `vat_group_membership`, T:201–205,582; neither boolean nor assembled full tax number |
| `taxpayerData/incorporation` | Required in 3.0 taxpayer data; not declared in 2.0 | `incorporation`, T:206–208,583 |
| `taxpayerData/taxpayerAddressList` | Optional container; `taxpayerAddressItem` occurs one-or-more, unbounded when present | `addresses`, T:223–224,457–459; sparse/empty list accepted |

`Incorporation` represents all three N3A:43–68 tokens — `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON` — plus `Other(String)`. Conversions, display and serde retain unknown tokens (T:111–182). New optional fields default absent when decoding older JSON. No schema-defined taxpayer business field is missing.

N2A:1934 and N3A:1824 both declare `taxpayerAddress` as **DetailedAddressType**, not a choice between simple and detailed address wrappers:

| Wire leaf | Schema presence/meaning | Output assignment |
|---|---|---|
| Item `taxpayerAddressType` | Required: `HQ` headquarters, `SITE` site, `BRANCH` branch | `kind`, T:541; open string |
| Address `countryCode` | Required ISO alpha-2; 2.0 has default HU | `country_code`, T:542 |
| `region` | Optional region/province code | `region`, T:543 |
| `postalCode` | Required string | `postal_code`, T:544 |
| `city` | Required | `city`, T:545 |
| `streetName` | Required | `street_name`, T:546 |
| `publicPlaceCategory` | Required | `public_place_category`, T:547–549 |
| `number` | Optional house number | `number`, T:550 |
| `building` | Optional | `building`, T:551 |
| `staircase` | Optional | `staircase`, T:552 |
| `floor` | Optional | `floor`, T:553 |
| `door` | Optional | `door`, T:554 |
| `lotNumber` | Optional lot identifier | `lot_number`, T:555 |
| `additionalAddressDetail` | **Not declared** on taxpayer DetailedAddressType in either version; exists on the separate SimpleAddressType | `additional_address_detail`, T:556–558; correctly labeled a tolerated extension at T:257–261 |

All twelve declared detailed-address leaves and the item kind are represented. Their string representation is appropriate: a house/floor/lot/postal identifier is not an arithmetic value.

### Boolean, numeric, datetime and business-text handling

1. **Boolean:** `true`, `false`, `1`, `0` map exactly after verdict trimming (T:521–522,567–577). Nonblank `yes`, `TRUE`, `2` fail; empty/blank validity cannot become false. This applies even if the eventual function code is ERROR, because extraction precedes verdict conversion.
2. **Wider verdict whitespace:** `str::trim()` also removes Unicode whitespace, so `&#160;true&#160;` is accepted as true. This is wider than XSD boolean whitespace and differs from business-text preservation. It does not change any schema-conforming value; the crate disclaims XSD business validation and explicitly separates verdict policy from business text (`README.md:273–301`). Optional normalization-consistency work, not a newly discovered functional defect or a claim that historical NBSP behavior was fixed.
3. **Identifiers are strings:** no decimal or floating-point taxpayer field exists. Returned tax-number components, group stem and address values preserve leading zeroes. The output does not enforce the request prefix's eight-digit invariant or equality to that prefix, nor invent a hyphenated full number. Returned content is the reported data.
4. **Error tokens:** `ErrorCode::from` trims, matches known `u16` numeric values and retains unknown/symbolic/oversized tokens (`src/error.rs:457–470`). Thus `003` and `+3` classify as code 3; a symbolic NAV code stays a string. This normalization is not applied to taxpayer numbers.
5. **Business text:** text, CDATA and references are accumulated (T:472–504), XML line endings decoded, and nonblank text preserved with padding/NBSP. Empty or XML-space/tab/CR/LF-only content becomes `None` (T:519–529). `&#160;` alone remains `Some` for a name/address/token, and `&#13;` remains a referenced CR rather than being confused with literal CR line-ending normalization. Comments and PIs do not erase neighboring text. Strings are not silently truncated to schema maximum lengths.
6. **infoDate:** an advisory source string, explicitly “not a validated datetime” (T:209–216). Offset, precision, zone absence and malformed nonblank text are retained, rather than shifted or rejected. N1 printed p.68: “Az infoDate az adózó adatainak utolsó változását mutatja” — last change of taxpayer data. It is not lookup time, cache expiry or proof of freshness. `README.md:456` accurately describes this choice.
7. **Default augmentation:** an empty-present NAV 2.0 country could receive HU during schema validation (N2D:971); this non-validating source projection instead returns `None`. NAV 3.0 has no such default. No missing country is assumed Hungarian.

The delegated PDF's printed p.68 postal-code facet is narrower than the imported NC `PostalCodeType` (which permits 3–10 characters and internal spaces/hyphens). The parser preserves the string and accepts either documented form. This source-level facet discrepancy requires no runtime change here.

## 6. Envelope and diagnostic coverage — explicit omissions

| Declared response path | Version / source | Treatment |
|---|---|---|
| `header/{requestId,timestamp,requestVersion,headerVersion?}` | 2.0 API (N2A:596–627); 3.0 Common (NC:544–575) | Ignored exchange metadata. Root namespace, not header version text, selects the layout. Header timestamp is not taxpayer infoDate. |
| `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact}` | API in both; N2A:1866–1907, N3A:1756–1797 | Ignored billing-software metadata, not queried-taxpayer identity. |
| `software/{softwareDevCountryCode?,softwareDevTaxNumber?}` | API in both; N2A:1908–1919, N3A:1798–1809 | Ignored optional developer metadata. |
| `result/funcCode` | R in both | Consumed as verdict. Unknown non-OK token is reflected in fallback text only when neither error code nor message exists. |
| `result/{errorCode?,message?}` | R in both | Exposed as `ApiError` on non-OK; not retained on successful `TaxpayerInfo`. |
| `result/notifications?/notification+/{notificationCode,notificationText}` | 3.0 Common, NC:640–645,668–701: “Miscellaneous notifications” | Ignored on success and error; no analogous field in the examined 2.0 result. |
| Unknown fields and attributes | Outside the projection | Ignored after XML well-formedness checks; not retained for re-emission. |
| `technicalValidationMessages[]/{validationResultCode,validationErrorCode?,message?}` | N3A:638–661, NC technical-validation type, N1 §3.1.2 | Belongs to a **generic error response**, not a missing taxpayer-root business child. Forwarding is unresolved (§8). |

The private parser openly says it is “reduced to the fields this crate surfaces” (T:287–288); public `TaxpayerInfo` has no raw XML or notification field. A caller owning `RawResponse` can retain `body()` (`src/wire.rs:221–225`); the ordinary typed result does not itself preserve this evidence. This is real diagnostic information loss, not lossless response coverage.

No examined source makes these metadata/notifications another taxpayer validity verdict or an obligatory business-data feature. N1 §1.8.9.2 point 5 leaves the use and extent of returned information to the client. Exposure of complete diagnostics would be a capability decision; it is not counted as a required fix without such a requirement.

## 7. Error and verdict matrix

Every taxpayer parse first checks **nonblank `szlahu_down` → nonblank error-code header → known non-2xx HTTP status → body** (T:281–284; `src/wire.rs:262–310`). This is a documented crate precedence policy, not evidence of which channels the vendor uses for each taxpayer fault.

| Input | Result at this HEAD | Evidence / consequence |
|---|---|---|
| OK + true/1 | Successful `TaxpayerInfo { valid: true, … }` | T:567–577,596–610; S3 success. |
| OK + false/0 | Successful `valid: false` | S3 invalid-number example. N1 p.68: “Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra” — invalid **or** nonexistent; not an API error or proof the number never existed. |
| OK + missing/empty/blank validity | `ParseError::Missing("taxpayerValidity")` | T:598–600; absence is never fabricated false. See Q1. |
| Missing/empty/wrong-path/wrong-namespace funcCode | Missing-funcCode parse error | T:340–397,594; unknown subtrees cannot supply it. |
| ERROR + 57 + message | `ApiError`, `ErrorCode::MalformedXml`, decoded message | T:612–622; matches S3's malformed seven-digit request example. |
| Non-OK + symbolic NAV error code | `ApiError`, `ErrorCode::Unknown(token)` | No forced numeric parse. S3 delegates error interpretation; N1 §3 expressly keeps error-code sets open to avoid implementation dependencies. |
| Non-OK + no/blank code | `ErrorCode::Absent` | No invented zero, 57, or success. |
| Non-OK + missing message | Raw code used as message; if neither code nor message, `NAV funcCode {func_code}` | T:616–620. |
| Unknown nonblank non-OK function code | Error branch | Does not guess a future success. |
| OK + optional errorCode/message or notifications | funcCode/validity decide; optional diagnostics not surfaced | No source-backed rule makes these override OK. |
| False validity + supplied business data | Data retained alongside false | T:596–610; avoids discarding content or claiming it is valid. |
| Malformed recognized diagnostic scalar or invalid nonblank boolean, even with ERROR | Parse error before `into_info` | T:429–447,567–577. This follows the stated scalar-integrity contract; no source-valid error demonstrates a defect. |
| Nonblank `szlahu_down` | `ServiceUnavailable` | Never false validity or a fabricated NAV token. |
| Error header plus otherwise successful body | Header `ApiError` wins | No taxpayer numbered-56 exception; that is document-issuance behavior. |
| Non-2xx without prior error/down header | `HttpStatus` with bounded excerpt | A body-only NAV error is not promoted over known HTTP failure; status omission on manually built RawResponse leaves body interpretation available. |

S7/S8 authentication/access codes **3, 135, 136, 164** correspond to invalid login, active browser session, blocked login/account access and multiple-account user. `src/error.rs:334–352` classifies these four as credential errors. If they arrive inside the supported taxpayer result or error header, the current code exposes them correctly. Their exact taxpayer body/header/status occurrence is not established by the behavior notes.

Direct NAV `INVALID_SECURITY_USER` / `INVALID_REQUEST_SIGNATURE` are different credentials/signatures: if wrapped, they remain symbolic unknown codes, not automatically Agent-key rejection. N1's direct-NAV HTTP 401 example cannot establish Számla Agent's forwarding status or envelope.

No automatic retry exists in this operation. S8 permits the same request at most five sends, then operator intervention; README:161 states the limit. `OutcomeClass` is shared exchange-recovery vocabulary, not taxpayer validity: an unknown class on a read does not imply a document was issued. The README's taxpayer transport example correctly supplies HTTP status and disables ureq's early status-as-error behavior (`README.md:313–341`).

## 8. Findings, accepted deviations and unresolved questions

### Confirmed severity-ranked findings

**None: P0 = 0, P1 = 0, P2 = 0, P3 = 0.** No current source-backed request omission, business-field omission, wrong NAV layout, verdict substitution or ordinary supported-response failure was found. This is not an assertion of exhaustive fuzzing or live compatibility.

### Accepted deviations — no defect severity

- **Sparse projection:** omitted XSD-required business fields become `None`; empty address lists/items are tolerated. README:282–301 explicitly separates XML integrity from XSD business validation. S3's own error/invalid examples omit the nominally required software block. Strict response XSD validation would reject published examples.
- **Source text rather than enforced string/date facets:** documented above; preserves useful business values and avoids pretending infoDate is a validated timestamp.
- **Open business tokens:** unknown incorporation and address kinds remain data. 2.0 incorporation and additional-address detail are explicitly tolerated extensions, not claims about the schemas.
- **Wider Unicode verdict trimming:** real behavior, but no newly established requirement to reject every non-XSD lexical variant. It remains an optional consistency question, not a fixed finding or a newly counted defect.
- **Diagnostic projection:** header/software/notifications and successful diagnostics are omitted; lossless response access is not promised.

### Unresolved contract questions with exact boundaries

| ID | Primary evidence and current file:line | Reproduction / possible impact | What would settle it |
|---|---|---|---|
| **Q1 — OK without validity** | [N2A][n2a]:1682 and [N3A][n3a]:1566 both declare `name="taxpayerValidity" type="xs:boolean" minOccurs="0"`. [S3](https://docs.szamlazz.hu/agent/querying_taxpayer/response) and N1 p.68 show explicit false for invalid/nonexistent numbers. `crates/szamlazz-agent/src/ops/taxpayer.rs:598–600` requires it for OK. | Supported root + `<result><funcCode>OK</funcCode></result>` without validity gives a missing-field parse error. A legitimate indeterminate OK response could not be represented. Optionality across the success/error response type alone does not prove that case occurs or define its meaning. | Success-specific vendor semantics/example; if indeterminate success exists, expose it explicitly rather than defaulting to false. Existing vendor question: `docs/research/2026-09-10-agent-vendor-questions.md:121–133`. |
| **Q2 — generic NAV failure forwarding** | [S3](https://docs.szamlazz.hu/agent/querying_taxpayer/response): “always matches” taxpayer type; N1 §3.1–3.2 separately defines generic roots for direct NAV. `crates/szamlazz-agent/src/ops/taxpayer.rs:404–416` accepts only taxpayer roots. | `GeneralErrorResponse`/`GeneralExceptionResponse` at 200 with no error header fails root checking; non-2xx is HttpStatus first (`src/wire.rs:301–307`). If wrapper forwarding is unchanged, typed error/validation detail is lost. Direct-NAV possibilities alone do not demonstrate this. | Wrapper root, namespace, HTTP status and headers for technical/authentication errors. Do not add arbitrary accepted roots based solely on a direct-NAV contract. |
| **Q3 — current version and optional-field forwarding** | [S3](https://docs.szamlazz.hu/agent/querying_taxpayer/response): “Last update … 2020-11-04”; examples use 2.0 while schema link is 3.0. T:323–337 supports both. | Synthetic 3.0 tests prove intended parser paths only. They cannot prove current forwarding of short name, county, group membership, incorporation, infoDate or notifications. | Updated official wrapper samples or separately authorized captures; no wrapper version selector is declared. README:456 accurately preserves this limit. |
| **Q4 — documentation defects/drift** | S6 freshly 404; S3's `QueryTaxPayerResponse` typo; S2's personal-identifier wording; PDF/Common postal facets differ. | Broken schema hint obstructs manual validation. None changes the emitted root/prefix or causes current supported taxpayer data to be lost. | Vendor documentation correction; no crate change required by these discrepancies. |

These are **unclassified questions, not P3 defects**. A clear success-specific contractual requirement could settle Q1 without a production incident; the existing source does not supply one.

### Minimal offline reproduction recipes (not executed here)

Use `QueryTaxpayer::new("12345678")?.parse(&RawResponse::new::<&str, &str>([], body.as_bytes().to_vec()))` through the public `AgentRequest` trait:

```xml
<!-- Q1: source-traced Missing("taxpayerValidity"), not false -->
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
  <result><funcCode>OK</funcCode></result>
</QueryTaxpayerResponse>
```

Insert `<taxpayerValidity>false</taxpayerValidity>` before the root close to obtain the explicit negative-data branch. Replace OK with ERROR to obtain `ApiError` with absent code and `NAV funcCode ERROR` fallback. Add `<errorCode>INVALID_SECURITY_USER</errorCode>` inside result to preserve that symbolic code. For 3.0 change the root's default API URI to `/OSA/3.0/api` and give `result` the default namespace `http://schemas.nav.gov.hu/NTCA/1.0/common`; leave validity in 3.0 API. These are sparse compatibility specimens, not claims of full XSD conformance.

For the normalization observation, use `<taxpayerValidity>&#160;true&#160;</taxpayerValidity>` with OK: T:521–522 trims NBSP, then T:569 returns true. For business-text contrast, a proper `taxpayerData/taxpayerName` containing only `&#160;` is `Some("\u{a0}")`. For path integrity, moving result under an unknown wrapper leaves funcCode missing; duplicating a recognized result/validity or adding a child inside funcCode gives a parse error.

## 9. Historical-finding suppression check

Historical reports/adjudications supplied leads only. Current evidence independently supports these dispositions:

| Earlier concern | Current evidence | Disposition |
|---|---|---|
| Missing short name/county/group/incorporation/infoDate | T:193–216,580–584; `paths:7–114,223–245`; upstream infoDate assertion | Implemented, suppressed. |
| Global local-name extraction / incorrect 3.0 namespace fixture | T:323–397; genuine Common result/Base components in `fixtures/synthetic/agent/taxpayer_v3.xml:5–20`; `paths:223–260` | Fixed, suppressed. |
| Singleton overwrite / nested scalar / multiple-root or truncated response | T:428–448 and `src/xml.rs:201–300`; `paths:120–138,192–220`, `tests/response_completion.rs:12–87,189–192` | Fixed boundaries present, suppressed; tests inspected, not rerun. |
| Entity/text loss | T:472–504,519–529; `paths:163–175` | Fixed, suppressed. |
| Mislabeling sparse fixtures/extensions as schema conformance | T:257–261; `paths:77–88`; 3.0 fixture:2–4 | Labeling corrected, suppressed. Full source-conforming fixtures would improve regression coverage but are not an outstanding runtime fix. |
| Namespace declaration / duplicate expanded-attribute gaps | `src/xml.rs:91–153`; `tests/response_namespaces.rs:281–329` | Fixed, suppressed. |
| Reserved `xmlns:` elements and colon PI targets | `src/xml.rs:95–101,262,444–448`; `tests/response_namespaces.rs:194–237` tests both layouts | Fixed, suppressed. |
| NBSP around validity | T:521–522 still Unicode-trims | Not represented as fixed. Retained as a normalization policy observation, consistent with the independently verified scope and historical adjudication. |

## 10. Tests and recorded evidence inspected

**Inspected, not executed. Parent owns suite execution.**

| File / lines | What was checked |
|---|---|
| `src/ops/taxpayer.rs:626–842` | Canonical request golden; 2.0/3.0 success; prefix constructors; validated-prefix conversion; false validity; all detailed-address leaves; JSON round-trip; boolean lexical forms; tolerated additional-address extension; unrelated root; numeric code 57, CDATA and symbolic NAV errors. |
| `tests/golden/xmltaxpayer.xml:1` | Exact key-based emitted request, no response-version field. |
| `tests/taxpayer_paths.rs:1–260` | Advisory infoDate variants; new-field path isolation; old JSON defaults; all incorporation tokens/unknown token; wrong/foreign paths; duplicate/scalar checks; references/CDATA/line endings/NBSP; independent multiple addresses; sparse errors; extra root; actual 3.0 Common/Base mapping. |
| `tests/response_completion.rs:12–87,172–193` | Taxpayer included in valid prolog/epilog and invalid tail/prefix/truncation matrix. Additional lexical controls use an invoice entry point into the same shared validator; not misreported as taxpayer-specific executions. |
| `tests/response_namespaces.rs:175–258,281–329` | Taxpayer namespace-invalid extensions, reserved names in both versions, escaped URI identity and normalized binding/expanded-attribute checks. |
| `tests/upstream.rs:966–1018,1185–1187` | Three official taxpayer responses: named success/infoDate/address, invalid-number success, seven-digit code-57 rejection; official request outline comparison. |
| `tests/schema_requests.rs:1–3,58–90,559–565` | Synthetic request exporter covers leading-zero and ordinary stems under both escaped key and username/password credentials. It **exports inputs**, not an XSD-validation result by itself. |
| `tests/custom_http_client.rs:13–74` | Loopback POST, multipart/action/key, status transfer, returned taxpayer and session-cookie utility; README transport pattern. |
| `tests/client.rs:183–239` | Taxpayer as the synthetic operation for native injected-cookie-jar/clone/isolation controls. This says nothing about actual wrapper account selection. |
| `tests/live.rs:25–40` | Ignored read-only taxpayer smoke checks valid + nonblank name/number. It does not pin response version, every optional field or raw error envelope; its existence is not run evidence. |

Fixtures read: `fixtures/upstream/agent/requests/xmltaxpayer.xml`, all three upstream taxpayer responses, synthetic `taxpayer.xml`, `taxpayer_v3.xml`, `taxpayer_error.xml`, `taxpayer_invalid_taxnumber.xml`. The freshly fetched examples agree on the reviewed request/response paths and values; no byte-for-byte HTML comparison or fresh example execution is asserted. `fixtures/SOURCES.md:3–39,56,77–88,109` distinguishes historical official downloads from synthetic/golden data. The 3.0 fixture expressly omits header/software and some required address leaves; it is not a complete conforming NAV response.

Relevant README coverage: testing evidence boundary **10–18**; business text/XML/path/verdict policies **273–301**; taxpayer HTTP example and status handling **313–343**; operation entry **371**; new optional fields/infoDate/current-forwarding limitation **456**. These statements match the implementation and inspected evidence. No README-specific correction was identified.

`docs/szamlazz-hu-behaviour.md:1–37` bounds historical document/account probes and later clearing evidence; **147–155** records other operations' header behavior; **266–272** explicitly leaves credential-code/header observation unverified. The file contains no taxpayer response capture. Invoice, storno, deletion, credit-entry and rounding observations cannot establish taxpayer forwarding. The later clearing results are genuine recorded executions for their own operation, not taxpayer evidence.

`docs/research/2026-09-10-agent-vendor-questions.md:121–133` asks Q1–Q3 rather than answering them. The newer `docs/research/2026-09-11-agent-vendor-clarification.md:1–7` is an unsent draft about credit acknowledgements/effective request schemas, not a taxpayer vendor answer. No examined research record resolves the current taxpayer version or generic-error forwarding.

**Verification limits:** no fresh Rust reproduction, full XSD validation, suite pass claim, fuzz/load test, live service observation or exhaustive transport audit. The full workspace, CLI/Restate projections and direct NAV client are outside this taxpayer review. Source inspection and current first-party documentation support the no-actionable-defect result; Q1–Q3 remain explicit evidence gaps.

Final check: HEAD remained the pinned commit; `git diff --name-only eec57fc -- crates/szamlazz-agent` was empty. The report passed `git diff --no-index --check /dev/null docs/review/2026-09-11-agent-api-eec57fc-taxpayer.md`. Concurrent Restate source/README/test and design-document changes were also visible at this check; none was authored or included in this review.

[n2a]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n2d]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceData.xsd
[n3a]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n3b]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd
[nc]: https://raw.githubusercontent.com/nav-gov-hu/Common/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
