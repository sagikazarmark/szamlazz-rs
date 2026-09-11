# Current Számla Agent taxpayer API review — 2026-09-10

## Verdict

**No confirmed runtime defect found in the taxpayer operation at revision `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`.** The request matches the current Számla Agent contract; the parser recognizes genuine NAV 2.0 and 3.0 expanded-name paths, exposes every declared taxpayer business/address field, and distinguishes `valid=false` from failure.

The previous taxpayer report's **TQ-01, P3 test/documentation finding is closed in this revision**: the previously misleading samples and field documentation now explicitly identify sparse responses and tolerated extensions. A complete source-shaped response fixture for each version remains an optional regression improvement, not an outstanding runtime fix or a condition for that labeling closure.

Remaining qualifications are **vendor ambiguities or deliberate projections**, not missing business functionality: optional validity in the XSD versus the crate's required successful verdict, unverified forwarding of NAV's generic error roots, and omitted transport/diagnostic metadata. No P0–P3 production defect or required source change is established here.

### Scope and evidence discipline

- Read-only review of the supplied revision, not a review of only its diff. `HEAD` was the supplied SHA at initial and final source checks; the scoped source/fixture/behavior-note diff against it was empty. Unrelated work appeared in `restate-szamlazz` during the review and was left untouched.
- Read `crates/szamlazz-agent/src/ops/taxpayer.rs`, associated tests, public documentation, shared XML/response/error boundaries used by this operation, fixture provenance, and `docs/szamlazz-hu-behaviour.md`.
- Fresh public documentation/schema/PDF GETs; no live Számla Agent account calls, credentials, source/test edits, or delegation. The only repository artifact written is this report. Acquisition scripts, downloaded evidence and scratch checks are under `/tmp/opencode/taxpayer-current-fbda137/`.
- The older `docs/review/2026-09-10-agent-api-taxpayer.md` supplied leads only. Its conclusions, hashes, test results and line numbers were not accepted as current evidence.
- Below, **`T`** means `crates/szamlazz-agent/src/ops/taxpayer.rs`; **`paths`** means `crates/szamlazz-agent/tests/taxpayer_paths.rs`; other `src/…`, `tests/…` and `README.md` references are relative to `crates/szamlazz-agent/`. All code line citations describe the supplied revision.

## 1. Fresh primary sources

Fetched on **2026-09-10**. Current Számla Agent pages display build **`v202608271632`**; the older standalone taxpayer `/xsd` page displays **`v202606031507`**. A site build is neither an acquisition date nor a guarantee of when a statement changed.

| ID | Fresh URL | Exact evidence used |
|---|---|---|
| S0 | <https://docs.szamlazz.hu/agent/category/querying-taxpayer> | Operation purpose and links to the current request, response and XML/XSD pages. |
| S1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/request> | POST to `https://www.szamlazz.hu/szamla/`, `multipart/form-data`, one XML file in `action-szamla_agent_taxpayer`; data comes from NAV's Online Invoice Platform. |
| S2 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xml> | Request example and inline XSD; “The sent XML file must comply with the following XSD schema.” |
| S3 | <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | All three response examples, error guidance and linked NAV PDF. “The response always matches the `QueryTaxPayerResponse` type”; examples explicitly last updated **2020-11-04** and actually spell the root `QueryTaxpayerResponse`. |
| S4 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xsd> | Older standalone request XSD, same relevant structure/cardinalities/facets as S2. |
| S5 | <https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd> | Working downloadable request XSD, freshly downloaded and read. |
| S6 | <https://docs.szamlazz.hu/agent/basics/authentication> | Agent key or username/password; recommended key; legacy key-in-both-fields option; username access to exactly one billing account. |
| S7 | <https://docs.szamlazz.hu/agent/basics/error-handling> | Meanings of codes 3, 57, 135, 136, 164; taxpayer response links here. The listed operations returning legacy plain-text errors do not include taxpayer lookup. |
| N1 | <https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf> | Linked NAV **v3.0** specification, §1.8.9, printed pp.63–69; §3.1.2–3.2, printed pp.162–165, for direct NAV generic/authentication errors. |
| N3A | [NAV 3.0 invoiceApi.xsd][n3a] | Imports, inheritance, response/business fields and their declaring namespaces; `QueryTaxpayerResponseType` lines 1552–1581; address/data types 1812–1889. |
| N3B | [NAV 3.0 invoiceBase.xsd][n3b] | `DetailedAddressType` 185–264, separate `SimpleAddressType` 265–302, `TaxNumberType` 303–328. |
| NC | [NAV NTCA **1.0** common.xsd][nc] | Imported simple types and header/result/notifications, especially lines 544–647, 668–701. |
| N2A | [NAV API-2.0 invoiceApi.xsd][n2a] | Header/result 596–705; response 1668–1697; address/data types 1922–1993. |
| N2D | [NAV API-2.0 invoiceData.xsd][n2d] | Detailed address 965–1044; tax-number components 2225–2250 and their string facets. |

**Version pinning independently checked through GitHub's API:** Online-Invoice `master` resolved to `cc7a775d6dce361311e409abb9934eb755f2749c`; `API-2.0` resolves to commit `84442e64bc2cd7feb368fedb8199645188962b23`. Common's annotated `common-1.0.0` tag object `7f32094a8275cf91c8dc29213a36f9ac2653a4ca` resolves to commit `ab8d7887967492e5f6d6e25447be853fd767add8`. All schema links below pin those commits. N3A lines 8–10 import **NTCA/1.0/common**, so substituting a newer Common namespace would be the wrong comparison. N1's URL is version-named but mutable; the acquired hash pins the bytes reviewed.

The example's schema-location URL, <https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd>, freshly returned **404**. S5 works. That broken documentation link does not make the crate's schema-location-free request incorrect.

### Acquisition identifiers

`acquire.py` downloaded sources afresh, extracted HTML `<pre>` text with HTML entity decoding and saved `acquisition.json`. No source XML values were repaired. The fresh success example includes the website's rendered `[email protected]` in software metadata; this is not a live-response capture.

| Acquired source bytes | SHA-256 |
|---|---|
| S3 response HTML | `29c7a7d0acf81288af72117d830e5d593c137d45f03db0ee271965d48c57b878` |
| S2 XML/XSD HTML | `ef7fd867a0e43201cdf5a6b9ac5066abf489e3be056cedd7473f2f0efeac8ec7` |
| S5 request XSD | `51fe8565301b0f3a67199b3b3d666fd6abb5acebd5db4cd81c3a479cae816ed7` |
| N1 PDF | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N3B | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| NC | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |

HTML hashes can change with page rendering; equality/difference is not by itself evidence of a protocol change. The relevant examples and declarations were read, not inferred from hashes.

## 2. Complete request audit

Sources S1/S2/S4/S5/S6. Implementation: **T:16–109,265–285**, `src/xml.rs:456–465`, `src/wire.rs:395–408`.

| Surface | Contract and current implementation | Verdict |
|---|---|---|
| Transport/action | POST, one multipart XML file, `action-szamla_agent_taxpayer`; exact `AgentRequest::ACTION` at T:266 and shared `to_wire` multipart construction. | Matches. |
| Root | `{http://www.szamlazz.hu/xmltaxpayer}xmltaxpayer`, qualified children, UTF-8 declaration. | Matches T:269–279 and shared document writer. |
| `beallitasok` | Required once, before `torzsszam`. | Always emitted, T:274–277. |
| Credential sequence | Optional strings `felhasznalo`, `jelszo`, `szamlaagentkulcs`, in that order. Authentication prose requires usable credentials even though the XSD makes each element optional. | `Credentials` emits either username then password or the key, through `src/xml.rs:458–465`. Values are XML-escaped, not lowercased or replaced. |
| `torzsszam` | Required string, exactly eight ASCII digits (`length=8`, `[0-9]{8}`). | All construction/serde paths validate, T:28–68,92–109. Leading zeroes retained. Full hyphenated tax numbers, padding, signs, Unicode digits and bad lengths refused before sending. |
| Schema hints | Example has `xmlns:xsi` and `xsi:schemaLocation`. | Tooling hints, not required attributes/business inputs; deliberately omitted. |

There is **no taxpayer `valaszVerzio`, NAV-version selector, country selector, pagination or as-of date** in the wrapper's request schema. NAV's direct `header`, `user`, `passwordHash`, `requestSignature`, `software` and `predecessorTaxNumber` belong to another request surface; N1 §1.8.9.1 and N3A:1534–1551 do not justify adding them to `xmltaxpayer`. The eight-digit stem is not a ten-digit personal tax identification number. No extra check-digit validation is demanded by S2/S5.

The key-based golden assertion is T:637–644 / `tests/golden/xmltaxpayer.xml:1`. The upstream outline comparison is `tests/upstream.rs:1185–1187`; it is not an XSD validator and intentionally loses empty-container distinctions (`fixtures/SOURCES.md:230–234`). Fresh scratch checks additionally verified username/password escaping/order, prefix serde refusal, leading zeroes, multipart action and absence of a response-version field.

## 3. Versioned response paths

**Namespaces come from element declarations, not the namespace of an imported element type.** Thus `taxNumberDetail` and `taxpayerAddress` are API elements whose children are component elements. Actual namespace URIs retain `http://`; the fact that documentation downloads use HTTPS does not change XML identity.

| Element path/component | NAV 2.0 namespace | NAV 3.0 namespace | Code |
|---|---|---|---|
| `QueryTaxpayerResponse` root | `http://schemas.nav.gov.hu/OSA/2.0/api` | `http://schemas.nav.gov.hu/OSA/3.0/api` | T:405–418, exact root/local-name pairs. |
| `header` and its children | 2.0 API | `http://schemas.nav.gov.hu/NTCA/1.0/common` | Deliberately omitted metadata. |
| `result` and `funcCode/errorCode/message` | 2.0 API | NTCA 1.0 Common | T:324–349. |
| `software` and children | 2.0 API | 3.0 API | Deliberately omitted metadata. |
| `infoDate`, `taxpayerValidity`, `taxpayerData` and its direct children | 2.0 API | 3.0 API | T:344–359. |
| `taxNumberDetail/{taxpayerId,vatCode,countyCode}` | `http://schemas.nav.gov.hu/OSA/2.0/data` | `http://schemas.nav.gov.hu/OSA/3.0/base` | T:360–364. |
| Address list/item/type/address containers | 2.0 API | 3.0 API | T:365–366. |
| `taxpayerAddress/*` | 2.0 Data | 3.0 Base | T:367–384. |

Sources: N2A:654–705,1668–1697,1922–1993; N3A:548–565,1552–1581,1812–1889; NC:596–647; N2D/N3B component declarations.

Prefixes are arbitrary, including misleading spellings; XML character references in namespace declarations resolve before comparison (`src/xml.rs:231–246`). Unknown/foreign frames stay unknown through descendants; a recognized local name under the wrong parent or namespace cannot supply or overwrite data (**T:341–398,430–473**). Duplicate recognized singleton containers/scalars fail, even when the first is empty; repeated address items remain valid and independent (**T:443–461**).

The shared root reader checks UTF-8, expected root, undeclared element/attribute prefixes, matching structure and completion through EOF before field extraction (**T:405–417; `src/xml.rs:63–166`**). Existing completion controls reject truncated bodies, second roots and malformed prolog/epilog; this is not a claim of complete XSD validation. Recognized scalars cannot contain child elements (**T:432–436**); undefined entities fail even in ignored subtrees (**T:485–507**).

## 4. Full business-data projection

“Required” below is the XSD cardinality **when the containing structure is present**, not a proposal to require it in the crate's lenient business-content reader. Sources N2A/N3A response/data types, N2D/N3B components, and N1 §1.8.9.2.

| Wire path below root | Declared meaning/type | Public projection / assignment |
|---|---|---|
| `infoDate` | Optional `xs:dateTime`, last change to taxpayer data. | `info_date`, T:210–217,587; nonblank decoded source text. |
| `taxpayerValidity` | Optional `xs:boolean` in both XSDs; existing/valid taxpayer verdict. | `valid: bool`, T:190–191,570–580,599–603; mandatory to construct a successful result. See §7. |
| `taxpayerData/taxpayerName` | Required full registered name. | `name`, T:192–193,582. |
| `taxpayerData/taxpayerShortName` | Optional short name, both versions. | `short_name`, T:194–197,583. |
| `taxpayerData/taxNumberDetail/taxpayerId` | Required eight-digit stem; a group identifier in group taxation. | `tax_number`, T:218–220,588; not an assembled hyphenated number. |
| `…/taxNumberDetail/vatCode` | Optional VAT status digit, schema `[1-5]`; not an invoice VAT rate. | `vat_code`, T:221–223,589. |
| `…/taxNumberDetail/countyCode` | Optional two-digit county code. | `county_code`, T:198–201,584; preserves `02`. |
| `taxpayerData/incorporation` | Required **in NAV 3.0 only** when data exists: `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`. | `incorporation`, T:112–183,207–209,586; known enum values and `Other(String)`. Accepted in 2.0 as an explicitly tested extension. |
| `taxpayerData/vatGroupMembership` | Optional eight-digit group stem. | `vat_group_membership`, T:202–206,585; string, neither boolean nor full-number synthesis. |
| `taxpayerData/taxpayerAddressList` | Optional list, one or more items when present in schema. | `addresses`, T:224–225,460–461; all items in order; empty/sparse lists tolerated. |

### Every address member

Both NAV versions declare **`DetailedAddressType` directly**, N2A:1934 and N3A:1824. Neither declares the simple/detailed `AddressType` choice here.

| Wire field | Schema requirement | Public field; code declaration / assignment |
|---|---|---|
| `taxpayerAddressType` | Required `HQ`, `SITE`, `BRANCH`. | `kind`; T:232–233 / 544, open string. |
| `countryCode` | Required ISO alpha-2 code; NAV 2.0 alone declares default `HU`. | `country_code`; T:234–235 / 545. |
| `region` | Optional region/province code. | `region`; T:236–237 / 546. |
| `postalCode` | Required postal code, a string. | `postal_code`; T:238–239 / 547. |
| `city` | Required settlement. | `city`; T:240–241 / 548. |
| `streetName` | Required public-place name. | `street_name`; T:242–243 / 549. |
| `publicPlaceCategory` | Required public-place category. | `public_place_category`; T:244–245 / 550–552. |
| `number` | Optional house number. | `number`; T:246–247 / 553. |
| `building` | Optional. | `building`; T:248–249 / 554. |
| `staircase` | Optional. | `staircase`; T:250–251 / 555. |
| `floor` | Optional; textual, not an integer. | `floor`; T:252–253 / 556. |
| `door` | Optional. | `door`; T:254–255 / 557. |
| `lotNumber` | Optional. | `lot_number`; T:256–257 / 558. |
| `additionalAddressDetail` | **Not a taxpayer DetailedAddressType member**; declared on separate SimpleAddressType. | `additional_address_detail`; T:258–262 / 559–561; explicitly documented tolerated extension. |

**No declared business/address field is omitted.** The scratch positive controls separately built complete source-shaped NAV 2.0/3.0 envelopes, included every declared business field, all address details, and three address kinds. NAV 2.0 omitted incorporation; NAV 3.0 included it. Each also included addresses with only optional detail fields omitted. These were manually traced against declarations, not certified by an XSD validator.

## 5. Success, failure, authentication and metadata

### Verdict and error extraction

Every parse first applies `RawResponse::check` (**T:282–285; `src/wire.rs:262–310`**): nonblank `szlahu_down` → `ServiceUnavailable`; otherwise a nonblank error-code header → `ApiError`; otherwise known non-2xx HTTP status → `HttpStatus`; only then does the body decide. Missing status on a caller-built `RawResponse` leaves the body to the parser. This is a deliberate shared policy, not proof of the vendor's status/header behavior for every taxpayer fault.

| Input | Current result | Assessment / evidence |
|---|---|---|
| `funcCode=OK`, validity `true` / `1` | `Ok`, `valid=true`. | Correct, T:570–580,599–613. |
| `OK`, validity `false` / `0`, no data | `Ok`, `valid=false`, absent business fields and empty addresses. | Correct; fresh S3 negative example and N1 p.68 point 1 explicitly return false for invalid/nonexistent tax numbers. |
| `OK`, missing/blank validity | `ParseError::Missing("taxpayerValidity")`. | Intentional stronger result contract, not fabricated false; §7. |
| Malformed boolean | Parse error. | Does not turn unusable verdict text into valid/invalid data. |
| `ERROR`, code 57 and message | `ResponseError::Api`, `ErrorCode::MalformedXml`, decoded message. | Exact S3 failed example, including seven-digit input refused by the wrapper's schema check. |
| Non-OK with numeric Agent code | Typed known code where recognized; unknown numeric spelling preserved otherwise. | T:615–625; `src/error.rs:455–469`. |
| Non-OK with symbolic NAV code | `ErrorCode::Unknown(trimmed_token)` and decoded message. | Token preserved; no conversion to code zero or `valid=false`. |
| Non-OK with code only | Code plus original trimmed code as fallback message. | T:619–622. |
| Non-OK with message only | `ErrorCode::Absent`, supplied message. | No invented numeric code. |
| Non-OK with neither | `ErrorCode::Absent`, `NAV funcCode {value}`. | Sparse failure remains failure; validity not required. |
| Missing/foreign/nested `funcCode` | Parse error for absent recognized verdict. | T:597 and path recognition. |
| Duplicate verdict/result/data scalar | Parse error. | Cannot overwrite an error with success; per-parent singleton checks. |
| Unknown nonblank `funcCode` | Non-OK error, not success. | Known schema tokens are `OK`/`ERROR`; future token conservatively fails. |

S6/S7 define Agent authentication errors: **3** invalid login, **135** active browser session, **136** blocked login/account condition, **164** user with access to multiple billing accounts. When supplied in the supported taxpayer result shape or error header, all become `ApiError` with `is_credential_error() == true` (`src/error.rs:333–349`). Fresh scratch checks exercised all four under both NAV layouts and a header-level 135 over an otherwise successful body.

**Direct NAV credentials are different credentials.** N1 §3.2 lists `INVALID_SECURITY_USER`, `INVALID_REQUEST_SIGNATURE`, etc. The crate preserves such tokens if relayed inside a taxpayer result; it does **not** classify them as rejection of the caller's Számla Agent key. For example `INVALID_SECURITY_USER` remains `Unknown("INVALID_SECURITY_USER")`, `is_credential_error()==false`. This is an appropriate boundary without evidence that the caller can fix NAV's credentials through the wrapper. An HTTP 401 without a recognized error header becomes `HttpStatus`, not an invented Agent code 3.

N1's direct NAV generic errors and their HTTP statuses do not prove how Számla Agent forwards them; see §7. Likewise, no taxpayer-specific evidence establishes a legacy `[ERR]` plain-text body or `xmlszamlavalasz` body-only authentication fallback. S3 promises the taxpayer root, and S7's plain-text-operation list excludes it. Broadening accepted roots without wrapper evidence is not a required fix.

### Full accounting of intentionally omitted protocol/diagnostic fields

| Field(s) | Source | Current projection and impact |
|---|---|---|
| `header/{requestId,timestamp,requestVersion,headerVersion}` | N2A:596–627; NC:544–575. First three required, last optional. | Omitted. These identify the wrapper-to-NAV exchange. Root namespace selects the layout; advisory header version is not searched for business values. |
| `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact,softwareDevCountryCode,softwareDevTaxNumber}` | N3A:1756–1811; N2A:1866–1921. Last two optional. | Omitted. Billing-software metadata, not taxpayer identity. S3's invalid/error examples even omit the nominally required software container. |
| `result/funcCode` | N2A:680–705; NC:616–647. | Consumed as verdict, not exposed on successful `TaxpayerInfo`. |
| `result/errorCode`, `result/message` | Same. | Exposed on non-OK via `ApiError`; omitted on OK. Mere XSD permission to co-occur with OK is not evidence of an additional failure verdict. |
| NAV 3.0 `result/notifications/notification[]/{notificationCode,notificationText}` | NC:640–645,668–701; optional container, repeating notification, both leaves required. Not declared on N2A's result. | Omitted on success and error. A notification-bearing success stays success; an error retains its error/fallback message, not notification text. Diagnostic exposure limitation, not missing taxpayer business functionality. |
| Namespace/schema-location attributes | XML tooling metadata. | Not business fields. |
| Generic fault `technicalValidationMessages[]` | N1 §3.1.2; N3A `GeneralErrorResponseType`. | Belongs to a different root, not a missing `QueryTaxpayerResponse` child. Forwarding unverified. |

The projection boundary is explicit in **T:288–305**, implemented by **T:341–398,595–625**. A caller holding its own `RawResponse` can retain `body()` (`src/wire.rs:221–225`); `TaxpayerInfo` itself has no raw XML or notification channel. No evidence found that a notification supplies a missing business result or must be treated as an error. NAV §1.8.9.2 point 5 leaves client use of returned information discretionary; it is not a promise that Számla Agent forwards every diagnostic.

## 6. Numeric and text rules

There are **no monetary values, floating-point business values or integer identifiers to parse numerically in this operation**. The taxpayer stem, VAT digit, county, group stem, postal code and address numbers are strings. The revision's general Decimal/arithmetic fixes must not be projected onto these fields.

| Rule | Evidence and current behavior |
|---|---|
| Strict outbound stem | Exactly eight ASCII digits, not trimmed; leading zeroes preserved at every constructor/serde entry point (T:28–68). |
| Returned number components | N2D and NC describe constrained **strings**. NC:391–409 declares eight digits and VAT `[1-5]`; NC:328–336 county `[0-9]{2}`. Code preserves decoded strings, including leading zeroes, without numeric conversion or rejecting unfamiliar content (T:584–589). Scratch deliberately out-of-schema `" 001 "`, VAT `"6"`, county `"+2"` remain text. This is permissive projection, not proof those values are valid NAV data. |
| Boolean | `true`, `false`, `1`, `0`; outer whitespace trimmed (T:524–525,570–580). XML space/tab/CR/LF accepted; `TRUE`, `yes`, `01`, `1.0`, `-0`, `1e0` refused. `str::trim` also accepts outer NBSP, a known permissiveness beyond XSD boolean whitespace, not a demonstrated valid-input failure. |
| Function/error codes | Trimmed separately from business text. Known numeric codes parse as `u16`, so `003` and `+3` classify as 3; unknown/too-large/symbolic tokens retain trimmed spelling (`src/error.rs:455–469`). These are error tokens, not taxpayer numbers. |
| Business text and error message | XML-decoded characters retained, including leading/trailing padding and NBSP. Absent/empty/XML-whitespace-only fields become `None`; NBSP-only business text remains `Some` (T:475–507,522–531; README:237–244). XML `+`/`%2B` are not URL-decoded. |
| CDATA/entities/line endings | Text, CDATA, standard entities and numeric character references concatenate; literal XML line endings normalize, character references preserve their represented characters (T:475–507; paths:162–176). Unknown named entities fail. This is character fidelity, not raw-byte fidelity. |
| `info_date` | Advisory decoded source text, not a validated datetime, lookup timestamp or cache expiry (T:210–217,587; paths:7–34; README:381). Offset, fractional precision and even malformed nonblank text survive; no timezone/TTL inferred. |
| Open categories | Incorporation known values and `Other(String)` serialize as wire strings (T:112–183); address kind is an open string. Padded business tokens stay padded/unknown rather than silently normalized. |
| Sparse business content | Optional fields remain optional; a true validity does not cause invention of missing name/address/incorporation. Added fields default to `None` on older JSON (T:194–217; paths:55–59). |

**Source disagreement found, without runtime impact:** N1 p.68's postal-code table prints `[A-Z0-9]{4,10}`; pinned NC:359–368 instead declares length 3–10 and `[A-Z0-9][A-Z0-9\s\-]{1,8}[A-Z0-9]`, allowing internal spaces/hyphens. The parser preserves either as text. This is evidence against adding a guessed restrictive postal-code validator, not a crate defect.

**NAV 2.0 country default:** N2D:971 declares `countryCode default="HU"`; N3B:191 has no default. The crate treats empty-present country as `None` (T:529,545), matching its documented blank-business-text policy instead of performing schema default augmentation. Scratch confirms this. Callers must not assume absent/blank country is always HU; adding XSD-default augmentation would be a separate policy change, not missing field extraction.

## 7. Ambiguities, severity, reproduction and impact

No concrete defect is assigned severity. The following records distinguish reproducible implementation behavior from unverified vendor behavior; none is a P0–P3 production finding.

### A1 — Optional schema validity versus required success result

- **Classification:** vendor-contract ambiguity plus explicit projection policy; confidence high in the code/schema difference, no observed failing wrapper response.
- **Evidence:** N2A:1682 and N3A:1566 have `minOccurs="0"`. N1's type table also says optional, while p.68 point 1 explicitly says invalid/nonexistent numbers return false. Fresh S3's negative example does so. README:252–254 explicitly requires validity on OK.
- **Reproduction:** a completed supported envelope containing `result/funcCode=OK` but no `taxpayerValidity` yields `ParseError::Missing("taxpayerValidity")`, T:599–603. Scratch checks both versions, with inherited header/software present.
- **Potential impact:** a vendor success omitting validity cannot be returned as `TaxpayerInfo`; it becomes a parse failure. Such forwarding is unestablished. Absence must not silently become `false`.
- **Closure condition if revisited:** captured wrapper response or explicit vendor clarification, followed by an intentional tri-state result decision. No automatic relaxation recommended here.

### A2 — Generic NAV errors/authentication forwarding

- **Classification:** integration ambiguity, not a demonstrated unsupported Számla Agent response.
- **Evidence:** N1 §3.1.2–3.2 specifies separate `GeneralErrorResponse` / `GeneralExceptionResponse` forms for direct NAV, including HTTP 401 `INVALID_SECURITY_USER`; S3 says the wrapper always returns the taxpayer response type and illustrates its own numeric 57 inside it.
- **Reproduction by inspection:** no recognized error header + generic root at 200 fails T:405–417; at non-2xx, status wins in `src/wire.rs:301–307`. A symbolic code within a genuine taxpayer result is preserved, as independently tested.
- **Potential impact:** if the wrapper forwards a generic root unchanged, the caller gets parse/status failure rather than its typed symbolic NAV code. No live capture or primary wrapper statement establishes that case.
- **Closure condition:** evidence of the wrapper's actual root, namespace, HTTP status and headers. Direct NAV's contract alone cannot settle this.

### A3 — Current forwarded NAV version/optional fields

- **Classification:** documentation/version ambiguity.
- **Evidence:** current S3 still publishes 2020-11-04 NAV 2.0 examples while linking the v3.0 PDF. Code supports both genuine namespace layouts; it does not infer a version from the document date/header or invent an input version switch.
- **Impact:** schema-level support for all 3.0 fields is established offline; today's forwarding of those fields by Számla Agent is not. `README.md:381` correctly states the live-capture limit.

### D1 — Protocol diagnostics are intentionally not a business DTO

- **Classification:** deliberate projection, no severity assigned.
- **Reproduction:** a NAV 3.0 Common notification on an otherwise successful/false-validity result produces the same business result; serialization contains no notification. An ERROR with only a notification keeps `Absent` / `NAV funcCode ERROR`. Fresh scratch controls pass.
- **Impact:** callers needing NAV exchange IDs/software details/notifications must retain raw responses or seek a separate diagnostic capability. No taxpayer business field or validity verdict is lost. Do not reopen this as a missing-business-functionality defect without evidence of required business semantics.

Parsing checks recognized content before `into_info`: a malformed boolean or duplicate business singleton on an ERROR body can cause a parse error instead of `ApiError` (T:470,511,570–580,595–625). That is outside the declared lexical/cardinality contract. It does not establish that a valid error response is misclassified, and does not justify bypassing body integrity checks.

## 8. Closure and fixture provenance

### Latest closure independently verified

`git show fbda137` confirms these changes, all also present in current files:

| Earlier concern | Current evidence | Closure |
|---|---|---|
| Five missing business fields | T:194–217,583–587; upstream `infoDate` assertion and paths field tests. | Previously implemented; all five freshly verified. |
| Fake NAV 3 coverage using 2.0 result/component layout | T:324–337; `paths:223–260`; `fixtures/synthetic/agent/taxpayer_v3.xml:5–22`. | Genuine Common result/Base children, with API containers. |
| NAV 2 incorporation presented as standard schema coverage | `paths:77–88` explicitly says v3 token support and v2 extension tolerance. | **TQ-01 closed.** |
| `additionalAddressDetail` presented as a supported taxpayer simple-address alternative | T:258–262; renamed test T:771–785. | **TQ-01 closed.** |
| Sparse NAV 3 sample presented as complete conformance | `fixtures/synthetic/agent/taxpayer_v3.xml:2–4` names omitted header/software and second-address street/category. | **TQ-01 closed.** Complete conforming fixtures remain optional. |
| Truncation/second-root/foreign-path/text-overwrite risks | Shared complete-root check, namespace/path frames, singleton guard, entity/text reader; existing focused tests rerun. | Earlier fixes hold; no current reproduction. |

The live GitHub issue state was also read: [#199](https://github.com/sagikazarmark/szamlazz-rs/issues/199) is **CLOSED**, with [completion comment](https://github.com/sagikazarmark/szamlazz-rs/issues/199#issuecomment-5615527665) naming `061294804cc3facf613fcc6b3527b853c0d9afe4` for the five fields. That historical closure is consistent with the current implementation; its claimed past workspace/rustdoc runs are not counted as checks performed in this review. The subsequent `fbda137` labeling closure was verified directly, not inferred from the issue's state.

### Provenance and coverage strength

- `fixtures/SOURCES.md:3–21,34–39,56,77–79,109` distinguishes the July-acquired official workspace corpus from redistributable samples. Upstream tests read it at runtime and can skip when absent from a published package. It was present in this checkout; the corpus checks ran.
- `fixtures/SOURCES.md:23–32` correctly identifies synthetic fixtures and golden writer expectations as project-authored. The new NAV 3 fixture comment further bounds what its success proves. No synthetic sample is treated here as a captured vendor exchange.
- `fixtures/SOURCES.md:172–180` records the fresh NAV PDF link while preserving the 2.0 examples' own date/version. Its old empty-schema-heading statement at lines 81–88 is explicitly historical, not a current defect.
- `tests/upstream.rs:966–1019` asserts the real example's name/stem/infoDate/address, false validity, and code 57. Fresh examples were separately extracted and parsed in scratch, rather than merely rerunning cached-corpus checks.
- Existing focused coverage: writer/prefix/serde/basic success/error/address tests (T:637–844); all five fields/advisory date/open token compatibility (paths:7–114); singleton/foreign-path/text/error/address isolation (paths:120–219); genuine 3.0 paths (paths:223–260); escaped/undeclared namespace controls (`tests/response_namespaces.rs:78–123`); complete-document checks (`tests/response_completion.rs:12–84,120–123`).
- Useful optional persistence: complete source-shaped 2.0/3.0 positive records; a 3.0 Common ERROR/notification sample; all four boolean forms and sparse error-code/message combinations. Scratch establishes current behavior but is not a committed regression suite. Lack of those particular fixtures does not imply a runtime failure.

### Behavior-note check

Read **all of `docs/szamlazz-hu-behaviour.md`**. It records document probes on one test account and bounded dates (lines 1–28,164–172), with no taxpayer-specific capture establishing current NAV forwarding, notification content or missing-validity semantics. Lines **255–261 explicitly leave credential codes 3/135/136/164 and their header forms unobserved**. General invoice/credit-entry header observations (137–145) cannot establish taxpayer headers. No note overrides the fresh taxpayer documentation/schema comparison.

## 9. Executed checks and limits

All Cargo commands below were **offline**. Only public-source acquisition used the network. No ignored live test was selected.

1. `cargo test --locked --offline -p szamlazz-agent --lib ops::taxpayer::tests --target-dir /tmp/opencode/taxpayer-current-fbda137/target` — **15 passed**.
2. `cargo test --locked --offline -p szamlazz-agent --test taxpayer_paths --test response_namespaces --test response_completion --test upstream --test error_classification --target-dir /tmp/opencode/taxpayer-current-fbda137/target` — **32 passed**: 10 taxpayer-path, 6 namespace, 2 completion, 11 upstream, 3 error-classification tests. Some shared/corpus tests cover other operations; those incidental passes are not a broader API audit claim.
3. `cargo test --offline --manifest-path /tmp/opencode/taxpayer-current-fbda137/Cargo.toml --test audit --target-dir /tmp/opencode/taxpayer-current-fbda137/target` — **5 scratch tests passed**:
   - today's three freshly extracted S3 examples;
   - complete source-shaped envelopes for both versions, every declared business/address field, three address kinds, optional-detail omissions, arbitrary prefixes and equivalent escaped namespace URIs;
   - numeric/symbolic/sparse errors in both versions, all four Agent authentication codes, 3.0 notification-bearing success/error and header/down/status precedence;
   - all XML boolean forms and invalid lexical controls, explicit missing-validity refusal, source-text/blank/NBSP policies, unvalidated returned codes, NAV 2 empty-country behavior and tolerated extensions;
   - strict prefix constructor/serde behavior and username/password/key request encoding.

**Total: 47 existing tests + 5 new scratch checks, all passing.** Existing tests used the repository lockfile with `--locked`. The standalone scratch crate resolved its own offline lockfile (including different cached proc-macro patch versions), while compiling the same path source and quick-xml 0.42.0. It is not a reproduction of every workspace dependency pin. No workspace lockfile was rewritten.

Tool setup limits were resolved: `python` and `pdftotext` were not on PATH; acquisition used `python3`, and the freshly downloaded PDF was extracted with the installed `/nix/store/g0f2man6jdwimdpz383l8p11r1rzx9hs-poppler-utils-26.06.0/bin/pdftotext`. No PDF conclusions were borrowed from an older extracted copy.

No full XSD validator, full workspace suite, live taxpayer call, packet capture, vendor clarification, load test or browser test was performed. Complete scratch samples were source-shaped by manual type/cardinality/namespace tracing; deliberately sparse/extension samples were labeled separately. General transport/security behavior was inspected only at the boundary this operation invokes. **Within those limits, no current documented-response failure or required production remedy was established.**

[n3a]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n3b]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd
[nc]: https://github.com/nav-gov-hu/Common/blob/ab8d7887967492e5f6d6e25447be853fd767add8/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
[n2a]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n2d]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceData.xsd
