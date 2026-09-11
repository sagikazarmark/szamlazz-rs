# Számla Agent taxpayer audit — 837dad0

**Reviewed commit:** `837dad024300e2a202c2b6351fcba73df82a7744` (HEAD).

**Sources retrieved:** 2026-09-11.

**Result:** **no confirmed actionable P0–P3 defect** in the reviewed taxpayer request, response projection or NAV namespace/path parsing. Every taxpayer business field and detailed-address component declared by the examined NAV 2.0 and 3.0 schemas is exposed. This is a business-data projection, **not a lossless NAV response model**: the omitted envelope/diagnostic fields are enumerated below.

The earlier namespace-conformance defects are fixed at this commit. Missing-success-validity semantics, generic NAV error forwarding and the current forwarded NAV version remain vendor questions, not demonstrated failures on a supported Számla Agent response.

## 1. Scope and method

- Audited the complete `crates/szamlazz-agent/src/ops/taxpayer.rs`, including `TaxpayerPrefix`, request serialization, `TaxpayerInfo`, `TaxpayerAddress`, `Incorporation`, layout selection, pull-parser frames and verdict conversion.
- Traced the shared credential writer, multipart construction, response header/status checks, XML completion/namespace/lexical checks and error-code mapping used by this operation. Read the relevant public README policy.
- Read `fixtures/SOURCES.md`, all 308 lines of `docs/szamlazz-hu-behaviour.md`, taxpayer fixtures, taxpayer unit/path/upstream tests and the relevant shared namespace/completion and HTTP-client test code. The live taxpayer test was read, not executed.
- Followed the taxpayer category and every operation descendant in both EN/HU: request, response, combined XML/XSD, plus the older standalone XSD pages. Read all three published response examples and the request example/inline schemas; fetched the working XSD download and attempted the example's broken schema-location URL.
- Independently compared the implementation with fresh official sources, including the delegated NAV PDF and first-party 2.0/3.0 XSD inheritance. Deliberate historical-report comparison came after that audit. One earlier overly broad content search inadvertently returned historical report excerpts; no historical test result, source quotation or line number is counted as fresh evidence here.
- No delegation, credentials, live Számla Agent/NAV execution, source/test/fixture edits or existing-report edits. The only repository addition authored by this audit is this report. Public documentation GETs and local offline tests are the evidence boundary.

**Line notation:** `T` means `crates/szamlazz-agent/src/ops/taxpayer.rs`. Other `src/…`, `tests/…` and `README.md` references are relative to `crates/szamlazz-agent/`. Line ranges describe the pinned commit, not older reports. Working tree was clean at the initial and pre-report checks.

At final verification, other concurrent reviews had added untracked `837dad0` invoice, mutation, query and transport reports. Those files were not edited or used by this audit; tracked files remained unchanged and HEAD remained pinned.

## 2. Fresh-source ledger and coverage

The current operation pages display **`v202608271632`**; the older standalone XSD pages display **`v202606031507`**. These are site-build identifiers, not dates of vendor observations. The EN response page dates its examples to **2020-11-04**.

Every URL in the following table was actually fetched, not inferred from a cached fixture or historical report.

| ID | URL(s) fetched | Coverage / result |
|---|---|---|
| S0 | [EN category](https://docs.szamlazz.hu/agent/category/querying-taxpayer), [HU category](https://docs.szamlazz.hu/hu/agent/category/querying-taxpayer) | Purpose and complete current operation navigation: three descendants. |
| S1 | [EN request](https://docs.szamlazz.hu/agent/querying_taxpayer/request), [HU request](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/request) | Endpoint, POST, multipart file action, NAV provenance; HTML form example. |
| S2 | [EN XML/XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml), [HU XML/XSD](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xml) | Complete request example and inline XSD, both tabs; credential order and prefix facets. |
| S3 | [EN response](https://docs.szamlazz.hu/agent/querying_taxpayer/response), [HU response](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response) | Success, code-57 error, invalid-number examples; schema tab delegates to N1 §1.8.9. |
| S4 | [EN standalone XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xsd), [HU standalone XSD](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xsd) | Older route; same relevant request sequence/cardinality/facets. |
| S5 | <https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd> | Working XML download; same relevant structure as inline XSD. |
| S6 | <https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd> | **404**, fetching the request example's HTTP schema-location hint as HTTPS. |
| S7 | <https://docs.szamlazz.hu/agent/basics/authentication> | Agent key or username/password, key preferred, lowercase/case sensitivity, single-account user restriction, legacy key-in-both-fields form. |
| S8 | <https://docs.szamlazz.hu/agent/basics/error-handling> | Code meanings, repeated-request limit; taxpayer is not among listed legacy plain-text-error operations. |
| N1 | <https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf> | Vendor-delegated PDF: §1.8.9, printed pp.63–69; generic/authentication error sections §3.1–3.2, pp.162–166. Web fetch exceeded 5 MB; a fresh `curl` GET succeeded, and local `pdftotext -layout` extraction was read. |
| N2A | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd> | NAV 2.0: header/result 596–705; taxpayer response 1668–1697; software and business/address types 1866–1993. |
| N2D | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceData.xsd> | NAV 2.0 components: detailed address 965–1044; tax number 2225–2250. |
| N3A | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd> | NAV 3.0: inheritance 548–565; request/response 1534–1581; software/business/address types 1756–1889; incorporation 43–68. |
| N3B | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd> | NAV 3.0: detailed address 185–264; separate simple address 265–302; tax number 303–328. |
| NC | <https://raw.githubusercontent.com/nav-gov-hu/Common/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd> | Imported **NTCA 1.0**, not current Common 2.0: header/result 544–647, notifications 668–701, taxpayer/VAT/county string facets. |
| NE | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/sample/API%20sample/queryTaxpayer.xml> | First-party direct-NAV request example; confirms separate header/user/signature/software request surface. Sample authentication values were not used. |

### Acquisition identifiers and unsuccessful discovery

GitHub API commit lookups resolved Online-Invoice `master` to **`cc7a775d6dce361311e409abb9934eb755f2749c`**, `API-2.0` to **`84442e64bc2cd7feb368fedb8199645188962b23`**, and Common `common-1.0.0` to **`ab8d7887967492e5f6d6e25447be853fd767add8`**. NAV 3.0 imports NTCA **1.0** even though the current Common default branch has 2.0 schemas.

Fresh downloaded-byte SHA-256 values:

| Source | SHA-256 |
|---|---|
| N1 | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| NC | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |

These hashes identify acquisition bytes, not server behavior. Scratch downloads/extracted PDF text are `/tmp/opencode/taxpayer-837dad0-*`; no old scratch corpus was reused. N3B was read through web fetch; no local-byte hash is asserted for it.

For completeness, discovery also fetched these GitHub API URLs (metadata only):

- `https://api.github.com/repos/nav-gov-hu/Online-Invoice/git/trees/master?recursive=1`
- `https://api.github.com/orgs/nav-gov-hu/repos?per_page=100`
- `https://api.github.com/repos/nav-gov-hu/Common`
- `https://api.github.com/repos/nav-gov-hu/Common/git/trees/main?recursive=1`
- `https://api.github.com/repos/nav-gov-hu/Common/branches?per_page=100`
- `https://api.github.com/repos/nav-gov-hu/Common/tags?per_page=100`
- `https://api.github.com/repos/nav-gov-hu/Common/git/trees/common-1.0.0?recursive=1`
- `https://api.github.com/repos/nav-gov-hu/Online-Invoice/commits/master`
- `https://api.github.com/repos/nav-gov-hu/Online-Invoice/commits/API-2.0`
- `https://api.github.com/repos/nav-gov-hu/Common/commits/common-1.0.0`

Initial path guesses returned 404 and supplied no schema evidence: `https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/invoiceApi.xsd`, the corresponding `invoiceBase.xsd`, `https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/invoiceApi.xsd`, and `https://api.github.com/repos/nav-gov-hu/Common/git/trees/master?recursive=1`.

## 3. Complete request and authentication comparison

| Requirement / source quotation | Implementation | Assessment |
|---|---|---|
| S1: “Method: POST”, “Content type: `multipart/form-data`”, “Form field name: `action-szamla_agent_taxpayer`”; endpoint `https://www.szamlazz.hu/szamla/` | T:264–279; `src/wire.rs:14,66–99,402` | Correct action, XML file part and endpoint constant. |
| S2/S5: `targetNamespace="http://www.szamlazz.hu/xmltaxpayer"`, `elementFormDefault="qualified"` | T:269–277; `src/xml.rs:139–161` | Correct root, inherited default namespace and UTF-8 XML declaration. |
| S2 HU: “Az XML-ben a mezők sorrendje kötött, **nem felcserélhetők**.” XSD sequence: `beallitasok`, `torzsszam`, both required | T:273–276 | Correct order and presence. |
| S2/S5 settings sequence: optional `felhasznalo`, `jelszo`, `szamlaagentkulcs`; S7: “You can use either an Agent key … or a username and password.” | T:274; `src/xml.rs:610–619`; `src/credentials.rs:45–81` | Key emits only `szamlaagentkulcs`; user/password emits the two ordered fields. Credentials are supplied at serialization, not taxpayer data. |
| S7 also accepts the same key in username and password | `Credentials::user_password` can represent that pair | Supported without a special request mode. No need to emit both authentication alternatives simultaneously. |
| S2/S5: `<length value="8"/>`, `<pattern value="[0-9]{8}"/>` | T:15–73,85–109 | Constructors, owned/borrowed conversions, `FromStr` and serde validate exactly eight ASCII digits. Leading zeroes remain strings. |

The Agent key is passed unchanged; S7's lowercase/case-sensitive credential rule is documented in `credentials.rs:5–9`. There is no justification for silently case-folding it. Username/password users must resolve to one billing account (S7/S8, code 164); this is an account-access condition, not a taxpayer input field.

Full hyphenated tax numbers, padding, non-ASCII digits and wrong lengths are refused. There is no required local checksum validation: existence/validity is the operation's answer. The inline comment's “adóazonosító jel” wording is misleading; the eight-digit facets and N1 §1.8.9.1 establish the taxpayer **törzsszám**, not a ten-digit personal tax identification number.

No wrapper request element selects NAV version, response version, country, pagination, historical date or predecessor. Native NAV `header`, `user`, `passwordHash`, `requestSignature`, `software` and `user/predecessorTaxNumber` belong to the intermediary's direct NAV request (N1/NE), not `xmltaxpayer`. N1 says predecessor input is validated but has no further effect on `/queryTaxpayer`.

Omitting the example's `xsi:schemaLocation` is not missing business data. The correct namespace is emitted; the linked schema hint currently returns 404 while S5 works.

## 4. Versioned roots and paths

Namespaces follow **element declarations**, not the namespace of an imported simple or complex type. Abbreviations below are URI identities, not required prefix spellings:

| Layout | API (`A`) | Result (`R`) | Components (`C`) |
|---|---|---|---|
| NAV 2.0 | `http://schemas.nav.gov.hu/OSA/2.0/api` | same as A | `http://schemas.nav.gov.hu/OSA/2.0/data` |
| NAV 3.0 | `http://schemas.nav.gov.hu/OSA/3.0/api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

N2A's `QueryTaxpayerResponseType` extends its API `BasicResponseType`. N3A extends API `BasicOnlineInvoiceResponseType`, which extends NC `BasicResponseType`: **3.0 result/header are Common; software and business containers are API**. This explains why a blanket `2.0` → `3.0` string replacement is not a correct NAV 3 fixture.

| Recognized path from `{A}QueryTaxpayerResponse` | Namespace/path mapping | Code |
|---|---|---|
| Root | Exact `QueryTaxpayerResponse` in A | T:404–416 |
| `result/{funcCode,errorCode,message}` | Container and children all R | T:323–348 |
| `infoDate`, `taxpayerValidity`, `taxpayerData` | Direct root children in A | T:343–347 |
| `taxpayerData/{taxpayerName,taxpayerShortName,vatGroupMembership,incorporation,taxNumberDetail,taxpayerAddressList}` | A; incorporation is standard only in 3.0 | T:349–358 |
| `taxNumberDetail/{taxpayerId,vatCode,countyCode}` | Parent A, children C | T:359–363 |
| `taxpayerAddressList/taxpayerAddressItem/{taxpayerAddressType,taxpayerAddress}` | All A | T:364–365 |
| `taxpayerAddress/*` | Direct children C | T:366–383 |

The code matches both declared layouts. HTTP in the **namespace URI** remains HTTP even when the source is downloaded over HTTPS. The prose spelling `QueryTaxPayerResponse` in S3 is a typo relative to its own XML and both NAV schemas; the code uses the correct spelling.

`Layout::child` and unknown frames prevent local-name injection through a foreign/unknown ancestor (T:306–307,340–397). Namespace aliases work; a field in a wrong namespace/path is ignored, and cannot supply missing verdict facts. Duplicate recognized singleton containers/leaves are refused, including empty-first duplicates; address items alone repeat (T:428–459). Each item starts a new row; sibling fields cannot leak between rows (T:535–559).

Whole-document checking precedes extraction: UTF-8, expected root, balanced/completed document, prolog/epilog through EOF, no second root, no DTD, namespace rules and lexical/entity checks (`src/xml.rs:19–137,183–283,378–441`). The new exact `xmlns:` element rejection and NCName PI-target checks are present. Character references in namespace declarations are normalized before reserved-binding and expanded-attribute uniqueness checks. This is XML checking, not full XSD validation.

## 5. Complete returned-data inventory

“Required” describes the XSD **when the enclosing optional container exists**. The public projection deliberately tolerates sparse business content. Assignments are at T:519–589; DTO declarations at T:184–262.

| Wire field | NAV declaration / meaning | Exposed field |
|---|---|---|
| Root `taxpayerValidity` | Optional `xs:boolean`, both versions; existing/valid taxpayer verdict | `valid: bool` (locally required on OK) |
| Root `infoDate` | Optional `xs:dateTime`; “Last date on which the data was changed” (N2A:1676–1680, N3A:1560–1564) | `info_date: Option<String>` |
| `taxpayerData/taxpayerName` | Required full registered name | `name` |
| `taxpayerData/taxpayerShortName` | Optional short name, both versions | `short_name` |
| `taxpayerData/taxNumberDetail/taxpayerId` | Required eight-digit stem; group identifier for group taxation | `tax_number` |
| `…/vatCode` | Optional one-digit VAT code; schema `[1-5]` | `vat_code` |
| `…/countyCode` | Optional two-digit county code | `county_code` |
| `taxpayerData/vatGroupMembership` | Optional eight-digit VAT-group identifier, not boolean | `vat_group_membership` |
| `taxpayerData/incorporation` | Required in **3.0** taxpayer data; absent from 2.0 declaration | `incorporation: Option<Incorporation>` |
| `taxpayerData/taxpayerAddressList/taxpayerAddressItem` | Optional list container; one-or-more, unbounded items when present | `addresses: Vec<TaxpayerAddress>` in document order |

`Incorporation` represents all N3A:43–68 tokens: `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`, plus verbatim `Other(String)` with string serde (T:111–182). The five added optional fields have serde defaults; tests read old JSON without them and round-trip new values (`tests/taxpayer_paths.rs:37–114`). The returned tax-number parts are strings, not monetary/integer fields, so leading zeroes are retained and no full number is fabricated.

### Every address component

N2A:1934 declares `taxpayerAddress` as `data:DetailedAddressType`; N3A:1824 declares `base:DetailedAddressType`. **There is no taxpayer simple/detailed choice wrapper.**

| Wire field | Schema presence | Public field / assignment |
|---|---|---|
| Item `taxpayerAddressType` | Required: HQ headquarters, SITE site, BRANCH branch | `kind`, T:541; open string |
| Address `countryCode` | Required ISO alpha-2; 2.0 additionally has default HU | `country_code`, T:542 |
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
| `lotNumber` | Optional | `lot_number`, T:555 |
| `additionalAddressDetail` | **Not declared** by either taxpayer detailed-address type; belongs to separate SimpleAddressType | `additional_address_detail`, T:556–558; explicitly documented tolerated extension at T:257–261 |

All twelve declared detailed-address leaves and the separate item kind are represented as optional strings. The union of declared 2.0/3.0 business/address fields contains no missing output member.

### Inherited and diagnostic data: what is not exposed

| NAV response data | Source | Projection |
|---|---|---|
| `header/{requestId,timestamp,requestVersion,headerVersion?}` | N2A:596–627; NC:544–575 | Omitted. Exchange metadata; timestamp is distinct from taxpayer `infoDate`. Root namespace selects parsing, not header version text. |
| `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact}` | N2A:1866–1907; N3A:1756–1797 | Omitted. Describes the billing software, not the queried taxpayer. |
| `software/{softwareDevCountryCode?,softwareDevTaxNumber?}` | N2A:1908–1919; N3A:1798–1809 | Omitted optional developer metadata. |
| `result/funcCode` | N2A:680–705; NC:616–647 | Consumed as verdict, not separately returned. Unknown non-OK value only survives in the fallback message when neither error code nor message exists. |
| `result/errorCode?`, `result/message?` | Same | Used on failure; discarded on successful projection. |
| 3.0 `result/notifications?/notification+/{notificationCode,notificationText}` | NC:640–645,668–701: “Miscellaneous notifications” | Omitted on success and failure. No equivalent on the examined 2.0 result type. |
| Unknown fields/attributes | Outside recognized projection | Ignored, not retained for re-emission. |

The public `TaxpayerInfo` has no raw XML, response metadata or notification channel (T:188–225); the private parser explicitly describes itself as “reduced to the fields this crate surfaces” (T:287–288). A caller driving the sans-I/O boundary can retain `RawResponse::body()` (`src/wire.rs:221–225`). This is genuine diagnostic information loss, but no reviewed source makes it a missing taxpayer business capability or a second verdict. N1 §1.8.9.2 point 5 leaves how much returned information to use to the client. A lossless diagnostic API would be a separate capability decision.

## 6. Error, validity and authentication semantics

T:281–284 invokes `RawResponse::check` before reading the body. The shared order is nonblank `szlahu_down`, then nonblank error-code header, then known non-2xx status, then XML (`src/wire.rs:262–310`). Header/status precedence is a documented crate policy; it is not evidence that all taxpayer failures use particular headers.

| Input / boundary | Result at HEAD | Evidence / interpretation |
|---|---|---|
| OK + true / 1 | Success, `valid=true` | T:567–577,596–610; S3 success and N1 |
| OK + false / 0 | Success, `valid=false` | S3 negative example; N1 p.68: “Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra.” Invalid **or** nonexistent; not API failure or proof the identifier never existed. |
| OK + absent/empty validity | Missing-field parse error | T:598–600; never defaults absence to false |
| Missing/empty/foreign-path funcCode | Missing-field parse error | T:594; no invented success |
| Nonblank invalid boolean (`yes`, `TRUE`, `2`) | Invalid-field parse error | T:567–577 |
| ERROR + 57 + message | `ApiError`, `ErrorCode::MalformedXml`, decoded message | Matches published malformed-prefix error, including seven-digit XSD diagnostic |
| ERROR + symbolic NAV code | `ErrorCode::Unknown(token)` and message | T:612–622; `src/error.rs:457–470`; no forced numeric conversion |
| Non-OK + no code | `ErrorCode::Absent` | No invented zero or code 57 |
| Non-OK + no message | Code as message; if neither, `NAV funcCode {value}` | T:616–620 |
| Unknown non-OK funcCode | Error branch | Conservative policy, not a future success assumption |
| OK + optional code/message, or false + business data | Follows funcCode/validity; supplied business data retained | T:596–610; no evidence these optional diagnostics override the verdict |
| Malformed XML / duplicate recognized scalar / nested scalar child | Parse error | Whole-document and field integrity precede verdict conversion |
| `szlahu_down` | `ServiceUnavailable` | Not false validity or a NAV code |
| Known non-2xx without preceding error header | `HttpStatus` | Body error not promoted over status; unknown status on manually built `RawResponse` leaves body parsing available |

Agent codes **3, 135, 136, 164** map respectively to invalid credentials, active browser session, blocked login/account access, and multi-account user; `is_credential_error` recognizes precisely these (`src/error.rs:334–352`), matching S7/S8. Their exact taxpayer wire/header occurrence is not established by this audit. Direct NAV `INVALID_SECURITY_USER` and `INVALID_REQUEST_SIGNATURE` remain symbolic unknown codes if wrapped in a recognized taxpayer result; they are not automatically classified as rejection of the caller's Agent key.

The core does not retry this operation. S8 says at most five sends of the same request, then operator intervention. `OutcomeClass` describes operation-dependent exchange recovery, not taxpayer validity: an unknown error class on this read is not evidence that a document was issued.

## 7. Accepted deviations and bounded ambiguities

### Accepted projection/compatibility policies — no defect severity

1. **Sparse business content.** Required-in-XSD names, tax numbers, incorporation and address leaves may project as absent; empty lists/items are tolerated. README:262–290 explicitly distinguishes XML checks from XSD validation. S3's own negative/error examples omit the nominally required software block. Full production XSD validation would reject those examples.
2. **Decoded source text.** T:472–504,519–529 retains nonblank business characters, including padding/NBSP, with XML text/entity/CDATA and line-ending decoding. XML-whitespace-only business fields become `None`; XSD string facets and returned-number/request-prefix equality are not enforced.
3. **Advisory infoDate.** T:209–216 explicitly says “not a validated datetime”; malformed nonblank text, offsets, long fractions and no timezone are retained. N1: “Az infoDate az adózó adatainak utolsó változását mutatja.” It is last data change, not lookup time, TTL or cache expiry. No datetime-validation defect is reopened.
4. **Default augmentation omitted.** N2D:971 has `countryCode default="HU"`; an empty-present country yields `None`, not schema-supplied HU (T:526–528,542). N3B's country has no default. This is a non-validating source projection, not permission to assume every missing country is HU.
5. **Extensions are explicitly extensions.** 2.0 incorporation is tolerated though not declared; direct additional-address detail does not introduce a simple-address alternative. Tests and fixture comments now label these accurately.
6. **Verdict normalization is wider than XSD whitespace.** T:521–522 calls `frame.text.trim()` for function/error codes and validity. Consequently `&#160;true&#160;` becomes true, while NBSP is outside XML Schema boolean whitespace. This does not alter a schema-conforming boolean; README exempts numeric/verdict parsing from business-text fidelity and disclaims full XSD business validation. Treat XML-only trimming or an explicit normalization note as optional consistency work, not an ordinary-response defect. This agrees with the later adjudication of historical F1; it is not a claim that F1's behavior was fixed.
7. **Malformed recognized diagnostics still fail.** A nested/duplicate `message` or invalid boolean can prevent an ERROR body becoming typed `ApiError` (T:428–447,567–577,592–622). This follows the documented taxpayer scalar policy. No source-valid error reproduces such loss; the issuance-envelope `hibauzenet` exception is a different path.

### Vendor ambiguities — potential impact and reproducible boundary

These have **no confirmed-defect severity**; they distinguish a real code boundary from unverified wrapper behavior. Reproductions below are directly traceable to code/existing tests; no additional scratch parser program was executed in this audit.

| ID | Quote + source and code | Concrete reproduction / possible impact | Resolution needed |
|---|---|---|---|
| A1: OK without validity | N2A:1682 and N3A:1566: `name="taxpayerValidity" type="xs:boolean" minOccurs="0"`; N1 p.68 and S3 instead give explicit false for invalid/nonexistent numbers. T:598–600: `.ok_or(ParseError::Missing("taxpayerValidity"))?` | Remove validity from a recognized OK envelope: parse error, not `TaxpayerInfo`. A wrapper intentionally emitting such a success could not be represented. Current README:289–290 expressly requires validity. | Vendor clarification/example with meaning; then an explicit indeterminate-result decision, never absence-to-false. |
| A2: generic NAV failure forwarding | N1 §3.1.2–3.2 specifies `GeneralErrorResponse` and generic technical failures, including HTTP 401 `INVALID_SECURITY_USER`; S3 says “The response always matches the `QueryTaxPayerResponse` type.” T:404–416 lists only taxpayer roots. | Generic NAV root at HTTP 200 without error header fails expected-root checking; at non-2xx status wins. If forwarded unchanged, typed symbolic code/validation details are unavailable. | Actual wrapper root, namespace, headers and status. Direct-NAV documentation does not establish wrapper forwarding. |
| A3: current version/optional fields | S3: “Last update for example responses: 2020-11-04”; examples 2.0, delegated PDF 3.0 | Both layouts parse, but source compatibility does not prove current forwarding of every optional field/notification. | Updated official wrapper examples or separately authorized captures. No invented version selector. |
| A4: source defects | S6 404; S3 root spelling typo and error/negative examples omitting software | Broken schema hint can obstruct manual validation; strict schema enforcement would reject published sparse examples. Crate's correct namespace and sparse projection remain usable. | Vendor documentation correction, not a source fix here. |

## 8. Historical findings cross-check

The focused comparison covered `2026-09-10-agent-api-taxpayer.md`, `2026-09-10-agent-api-current-taxpayer.md`, `2026-09-10-agent-api-f83e5fd-taxpayer.md`, `2026-09-11-agent-api-taxpayer.md`, and the relevant adjudication entries. These are historical context, not current primary protocol evidence.

| Earlier concern | Current independent evidence | Disposition |
|---|---|---|
| Five omitted business fields | T:193–216,580–584; path tests and upstream infoDate assertion | Implemented; not repeated. |
| Fake NAV 3 namespace coverage / global local-name extraction | T:323–397; `tests/taxpayer_paths.rs:223–260` with genuine Common/Base split | Fixed; not repeated. |
| Duplicate singleton overwrite / truncation / extra root / entity loss | T:428–447,472–504 plus shared complete-document checker; selected tests pass | Fixed; not repeated. |
| Sparse/extension samples presented as schema conformance | `fixtures/synthetic/agent/taxpayer_v3.xml:2–4`; `tests/taxpayer_paths.rs:77–88`; T:257–261 | Labeling fixed. A complete XSD-valid fixture remains optional regression improvement. |
| Duplicate expanded attributes / invalid reserved namespace bindings (f83e5fd F2 and merged shared finding) | `src/xml.rs:89–134`; `tests/response_namespaces.rs:281–329` | Fixed; current tests pass. |
| Reserved `xmlns:` elements / colon PI targets (2026-09-11 TQ-01) | `src/xml.rs:77–83,424–440`; `tests/response_namespaces.rs:194–237` checks both NAV versions and legal controls | Fixed; current tests pass. |
| NBSP around validity (f83e5fd F1) | T:521–522 still Unicode-trims; historical adjudication downgraded this to normalization consistency | Retained policy observation in §7, not counted as an unresolved functional defect. |

No historical severity was copied onto a fixed implementation. No new missing-business-field, authentication, error-root or datetime finding is justified by the fresh evidence.

## 9. Verification, provenance and exclusions

Executed against the workspace lockfile:

```sh
cargo test --locked -p szamlazz-agent --lib ops::taxpayer
cargo test --locked -p szamlazz-agent --test taxpayer_paths --test response_namespaces --test response_completion --test upstream
```

**51 passed, 0 failed:** 15 taxpayer unit tests, 10 taxpayer-path tests, 11 shared namespace tests, 4 completion tests and 11 upstream/corpus tests. No live target or ignored test was selected. Shared/corpus tests also cover other operations incidentally; those passes do not extend this review's scope. These were local protocol tests; the commands did not use Cargo's `--offline` flag, and no claim of a network-isolated build is made.

Coverage inspected includes canonical key request, prefix construction, 2.0/3.0 success, negative validity, code 57/symbolic errors, detailed address leaves, open incorporation/old JSON, source-text infoDate, wrong namespace/parent rejection, singleton/scalar checks, address isolation, escaped namespace URIs and whole-document completion. `tests/custom_http_client.rs` verifies taxpayer multipart POST on loopback but was **read, not run** here. The opt-in live smoke at `tests/live.rs:25–40` was likewise only read.

`fixtures/SOURCES.md:3–32,34–39,56,77–88,109,172–180,230–250` distinguishes historical official workspace fixtures from synthetic/golden samples and records the NAV PDF link without relabeling the dated 2.0 examples. The corpus was present; upstream tests exercised its three taxpayer responses. Fresh web examples were read and compared with those fixtures, **not separately extracted and executed**; no byte-for-byte HTML identity or fresh full-XSD-validation result is asserted. Synthetic 3.0 data is explicitly sparse and cannot prove full conformance.

`docs/szamlazz-hu-behaviour.md` records bounded document/account probes, not taxpayer response captures. In particular lines 137–145 describe other operations' header behavior, while 255–261 leave credential codes/header shapes unobserved. These observations do not establish current taxpayer forwarding, generic-error roots, cache freshness or missing-validity semantics.

**Excluded:** full workspace/API review, CLI/Restate projections and journals, direct NAV client implementation, EU/VIES lookup, cache policy, full XSD validation, exhaustive fuzzing/load tests, browser/TLS/session lifecycle audit, account provisioning and any live service behavior. No test-account invoice facts were transplanted into taxpayer guarantees.

**Final disposition:** no required production change identified for this scope at `837dad0`. Preserve the versioned extraction and established sparse/source-text policies; pursue A1–A3 only with evidence about the Számla Agent wrapper, and expose omitted diagnostics only through a deliberate consumer-facing capability decision.
