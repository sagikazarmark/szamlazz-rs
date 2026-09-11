# Independent taxpayer-operation review — f83e5fd

Reviewed **2026-09-10**, against **`f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`** (`HEAD` throughout the review). Scope: `crates/szamlazz-agent/src/ops/taxpayer.rs`, its request/response plumbing, focused existing tests, and current documentation. This is a snapshot review, not a diff review.

## Conclusion

**The request matches the official EN/HU contract, and the current reader covers every taxpayer business field and detailed-address component declared by NAV 2.0 and 3.0.** All six freshly fetched vendor response examples parse with the expected classifications. The version-specific expanded-name paths are correct; neither namespace prefixes nor matching local names in foreign subtrees establish business data or success.

**Two confirmed low-severity malformed-input findings remain:** Unicode whitespace is accepted around the validity boolean beyond the XML Schema lexical rules, and the shared XML preflight misses duplicate attributes with identical expanded names. Neither was observed on an actual vendor response, and neither demonstrates an incorrect interpretation of a schema-conforming taxpayer record. No high/medium-severity defect was established.

NAV 3.0 notifications and response transaction/software metadata are not exposed. This is a projection limitation, not a missing taxpayer business field. Sparse records, raw advisory `infoDate`, non-XSD extension tolerance, and fail-closed handling of malformed errors are adjudicated separately below.

### Method and boundaries

- Read the current implementation and tests before consulting primary sources. **No earlier untracked review was read or used as evidence.** No earlier scratch probe was reused.
- Freshly fetched the six EN/HU operation pages, the EN/HU error pages, the linked NAV PDF, NAV's 3.0 source, its 2.0 release source, and the applicable Common 1.0 schema. Verified schema inheritance rather than inferring namespaces from prefixes in examples.
- Read all of `docs/szamlazz-hu-behaviour.md` (308 lines). It records invoice/receipt/account observations; it supplies **no taxpayer-response probe evidence**. Its account-specific facts do not establish current NAV forwarding, validity semantics, or taxpayer caching.
- Existing tests and new scratch probes were offline or loopback-only. **No live account calls.** All authored scratch files were made with `apply_patch` under `/tmp/opencode`.
- Only this new report was authored in the repository. Unrelated Restate edits were already present and continued changing during the review; `git diff -- crates/szamlazz-agent Cargo.lock` remained empty, and `HEAD` remained pinned. No implementation, source test, or fixture was edited.

## 1. Fresh primary-source ledger

The vendor site identifies itself as **v202608271632**. URLs below were fetched on the review date. HTML copies and extracted code blocks are in `/tmp/opencode/f83e5fd-taxpayer-evidence/`; those are temporary evidence, not repository fixtures.

| ID | Source | Relevant quotation / fact |
|---|---|---|
| S1 | [EN request](https://docs.szamlazz.hu/agent/querying_taxpayer/request), [HU request](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/request) | “Base URL: `https://www.szamlazz.hu/szamla/`”; “Method: POST”; “Content type: `multipart/form-data`”; “Form field name: `action-szamla_agent_taxpayer`”. HU: “Az **adatok közvetlenül a NAV Online Számla rendszeréből** származnak.” |
| S2 | [EN XML/XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml), [HU XML/XSD](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xml) | `targetNamespace="http://www.szamlazz.hu/xmltaxpayer"`, `elementFormDefault="qualified"`; sequence `beallitasok`, `torzsszam`; prefix facets `<length value="8"/>`, `<pattern value="[0-9]{8}"/>`. HU: “Az XML-ben a mezők sorrendje kötött, **nem felcserélhetők**.” Settings sequence: optional `felhasznalo`, `jelszo`, `szamlaagentkulcs`. |
| S3 | [EN response](https://docs.szamlazz.hu/agent/querying_taxpayer/response), [HU response](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response) | EN: “The response always matches the `QueryTaxPayerResponse` type … Last update for example responses: **2020-11-04**.” HU: “Sikertelenségnél hibakód és hibaüzenet is tartozik a válaszhoz.” Both show actual root `QueryTaxpayerResponse`, NAV 2.0 API/data namespaces, success, error 57, and `OK` plus `taxpayerValidity=false`. |
| S4 | [Linked NAV HU v3.0 specification](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf), §1.8.9, printed pp. 63–69 (PDF pp. 73–79) | “A szolgáltatás csak magyar adószámok vizsgálatát támogatja”; request is the first eight digits. “Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra.” “Az infoDate az adózó adatainak utolsó változását mutatja.” “Az adózói címadatok listaként szerepelnek, mivel egy adózóhoz több címadat is tartozhat.” |
| S5 | [NAV 3.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd), [invoiceBase.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd) | API lines 1552–1581: `infoDate` is optional `xs:dateTime`, validity optional `xs:boolean`; 1812–1889 enumerate all taxpayer data. `taxpayerAddress` has `type="base:DetailedAddressType"` (1824), `taxNumberDetail` has `type="base:TaxNumberType"` (1864). Base lines 185–263 and 303–328 enumerate address and tax-number leaves. |
| S6 | [NAV 2.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd), [invoiceData.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceData.xsd) | Release tag `API-2.0`/`2.0`. API 654–704 defines API-namespace `result` and its leaves; 1668–1697 defines the response; 1922–1992 defines address list and business data. `taxpayerAddress` uses `data:DetailedAddressType`; `taxNumberDetail` uses `data:TaxNumberType`. No `incorporation` in 2.0. Data 965–1043 and 2225 onward define the components. |
| S7 | [NAV Common 1.0 common.xsd](https://github.com/nav-gov-hu/Common/blob/ab8d7887967492e5f6d6e25447be853fd767add8/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd) | Tag `common-1.0.0`. Lines 596–647 define **Common-namespace** `header`, `result`, `funcCode`, `errorCode`, `message`, `notifications`. Lines 668–700: repeated `notification` containing `notificationCode`, `notificationText` (“Miscellaneous notifications”). `FunctionCodeType` has `OK`/`ERROR`. |
| S8 | [NAV 3.0 changelog](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/CHANGELOG_3.0.md) | Lines 339, 420, 441 discuss incorporation and `TAXABLE_PERSON`; line 454: base namespace changes apply to **child tags**, “the parent tag in this case is not allowed to get a new namespace value.” Links the official [Common repository](https://github.com/nav-gov-hu/Common). |
| S9 | [EN errors](https://docs.szamlazz.hu/agent/basics/error-handling), [HU errors](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | 57: “XML reading error … The response body contains more information.” 3/135/136/164 are authentication/access failures. “You may send the same request … at most five times”; HU “legfeljebb ötször”. Taxpayer is not in the listed operations using response-version-1 plain-text errors. |
| S10 | [W3C Namespaces in XML 1.0, §§2.3, 6.3, 8](https://www.w3.org/TR/xml-names/#uniqAttrs) | “no element [may] have two attributes with the same expanded name”; processors must report namespace-well-formedness violations. Namespace comparison uses normalized attribute values, with XML character/entity references already replaced; it is not URL percent-decoding. |

Source qualifications:

- S3's prose spelling `QueryTaxPayerResponse` is inconsistent with its examples, S4 and both NAV XSDs. The code correctly accepts **`QueryTaxpayerResponse`**, not that capitalization typo.
- The `http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd` schema-location hint from S2, fetched as HTTPS, returned **404**. The two fresh inline schemas were available and were both used for actual request validation. No claim is made that a separate downloadable XSD was verified.
- S3's dated 2.0 examples and link to 3.0 do not prove which version a particular account receives today. NAV's source establishes schema structure, not szamlazz.hu's current forwarding behavior.
- NAV 3.0 source was freshly cloned at `cc7a775d6dce361311e409abb9934eb755f2749c`; 2.0 at `84442e64bc2cd7feb368fedb8199645188962b23`. Common's current master was also fetched (`72d8dfe2bacf7b4e115b52a84eca8df1314864eb`), but it is a **2.0 Common** source and was not substituted for the imported **1.0** schema. The OSA catalog's [Common-1.0.RC3 link](https://raw.githubusercontent.com/nav-gov-hu/Common/Common-1.0.RC3/src/schemas/nav/gov/hu/NTCA/common.xsd) was fetched too; the relevant inheritance agrees with released Common 1.0.
- The linked PDF was freshly downloaded (382 pages), SHA-256 `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2`. It was text-extracted with scratch-only pypdf 6.9.2 after the web fetcher's 5 MB limit and missing `pdftotext` prevented the first extraction route.

Fresh HTML SHA-256 (EN/HU, respectively):

| Page | EN | HU |
|---|---|---|
| request | `60b6a4c5b487ca6b9f3eb1f30ccddfc01fab798c1b3b6cb4e71524b4de4b708b` | `46bcaf5ff071d4b991755f97b113a34edd17a096de914243e4981f70dcdd3352` |
| response | `5a15595e367ec7e508ec732efac4b5b785d3e037b178fd05bea1fa6421e5244a` | `4968b91000a24707318d8a44436a39b5c2c68817de0d605de0b0d56ae57382a8` |
| xml | `ef7fd867a0e43201cdf5a6b9ac5066abf489e3be056cedd7473f2f0efeac8ec7` | `9a72657235a6c5e427f1b4becdab82ba776a507ebc93bdf500a6b79fc962b033` |

## 2. Complete coverage inventory

All implementation line references below are at the pinned commit. `taxpayer.rs` means `crates/szamlazz-agent/src/ops/taxpayer.rs`; `xml.rs`, `wire.rs`, and `error.rs` are in the same crate's `src/`.

### 2.1 Request

| Surface | Current behavior and verification | Verdict |
|---|---|---|
| Endpoint/method/form | `wire.rs:14,67–99`; `taxpayer.rs:265–279`: one XML file, correct multipart action. Existing blocking-client loopback test verifies POST/form and response. | Matches S1. |
| Root, namespace, encoding | `xmltaxpayer`, exact namespace, XML 1.0 UTF-8 declaration (`xml.rs:21–40`). | Matches S2; no unnecessary NAV request envelope. |
| Field sequence | `beallitasok` then `torzsszam`; credentials are agent key or username/password in schema order (`xml.rs:484–492`). | Matches S2. Both modes validated against both freshly fetched inline XSDs, including XML escaping and non-ASCII credential text. |
| Prefix | `taxpayer.rs:16–74,86–109`: private validated string; exact eight ASCII digits via constructors, `FromStr`, `TryFrom`, serde. Preserves leading zeros. | Matches S2/S4. Full formatted numbers, padding, Unicode digits and nondigits are refused. No checksum is asserted by the schema or enforced locally. |
| Other knobs | No response-version field, date, country, NAV user hash/signature, request id, or software block in this Számla Agent request. | Correct: none appears in S2. Native NAV request requirements must not be imposed on this adapter's request. |
| Schema-location hint | Writer omits `xsi:schemaLocation` shown in sample. | Not required data; emitted requests pass XSD validation without it. |

### 2.2 Expanded-name paths

Abbreviations are **namespace URIs**, not literal required prefixes:

| Layout | A (operation/API) | R (result) | C (components) |
|---|---|---|---|
| NAV 2.0 | `http://schemas.nav.gov.hu/OSA/2.0/api` | same as A | `http://schemas.nav.gov.hu/OSA/2.0/data` |
| NAV 3.0 | `http://schemas.nav.gov.hu/OSA/3.0/api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

`taxpayer.rs:324–398,405–418` correctly selects these from `{A}QueryTaxpayerResponse`. S5/S6/S7 establish them through **element declarations**, not through the namespace of a simple type.

| Recognized path from root | Interpretation |
|---|---|
| `{R}result/{R}funcCode`, `{R}errorCode`, `{R}message` | Result container and leaves both R. In NAV 3.0 an A-namespace `result` is not equivalent. |
| `{A}infoDate`, `{A}taxpayerValidity` | Root children, never nested in `taxpayerData`. |
| `{A}taxpayerData/{A}taxpayerName`, `{A}taxpayerShortName`, `{A}incorporation`, `{A}vatGroupMembership` | Simple types imported from Common/data do not relocate the declaring API element. |
| `{A}taxpayerData/{A}taxNumberDetail/{C}taxpayerId`, `{C}vatCode`, `{C}countyCode` | The parent remains A; its complex type's leaves are C. |
| `{A}taxpayerData/{A}taxpayerAddressList/{A}taxpayerAddressItem/{A}taxpayerAddressType` | Address kind belongs to A. |
| Same item / `{A}taxpayerAddress/{C}…` | Detailed-address leaves belong to C; the address parent remains A. |

Unknown frames remain unknown throughout their descendants (`307–316,386–389,438–455`). Foreign/default-namespace-reset elements cannot supply a field, and re-entering a recognized namespace under an unknown wrapper does not restore a recognized path. A wrong-namespace optional field is ignored, not reported as malformed; wrong-namespace verdict/validity causes a missing required fact. That behavior is intentional and documented in README 260–265.

### 2.3 Every taxpayer business field

S4/S5/S6 were enumerated in full. “Required” below means the XSD requires it **when its containing optional structure exists**, not that this projection enforces it.

| Wire field | XSD status / meaning | Rust field and current lines | Assessment |
|---|---|---|---|
| `taxpayerValidity` | Optional in both XSD envelopes; successful negative result is false. | `valid: bool`, 190–191, 570–581, 599–603 | All four XML boolean tokens supported; `OK` requires a nonblank fact. F1 below covers over-broad padding. |
| `infoDate` | Optional `xs:dateTime`; last data change, not lookup timestamp. | `info_date`, 210–217, 587 | Nonblank decoded source text, including offsets/precision, retained. Explicit advisory policy; no temporal validation/TTL. |
| `taxpayerName` | Required; registered full name. | `name`, 192–193, 582 | Retained; sparse/blank becomes `None`, as documented. |
| `taxpayerShortName` | Optional in both versions. | `short_name`, 194–197, 583 | Retained, correct A path. |
| `taxNumberDetail/taxpayerId` | Required eight-digit core number, group id for group taxation. | `tax_number`, 218–220, 588 | String retains zeros and source characters; not an assembled full number. |
| `taxNumberDetail/vatCode` | Optional one-digit VAT code (schema 1–5). | `vat_code`, 221–223, 589 | String retained, no closed code or numeric conversion. |
| `taxNumberDetail/countyCode` | Optional two-digit county code. | `county_code`, 198–201, 584 | Leading zero preserved (`02` verified). |
| `vatGroupMembership` | Optional eight-digit VAT-group identifier, not boolean. | `vat_group_membership`, 202–206, 585 | Correct scalar and A path. |
| `incorporation` | Required in 3.0 taxpayer data; absent from 2.0 schema. | `incorporation`, 112–183, 207–209, 586 | `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`, and verbatim `Other`; tolerated in 2.0 as an extension. |
| `taxpayerAddressList/taxpayerAddressItem` | Optional list container; one-or-more items if present, unbounded max. | `addresses`, 224–225, 443–461 | Preserves order and every recognized item, including sparse/empty items; no first-address-only loss. |

All five newly exposed fields have serde defaults; existing tests read old JSON lacking them and round-trip new data. No schema-declared business field is left unrepresented.

### 2.4 Every address field

S5 `invoiceBase.xsd:185–263` and S6 `invoiceData.xsd:965–1043` declare the same twelve detailed-address leaves. `taxpayerAddressType` is the separate API-level sibling.

| Wire field | Rust field | Assignment line | XSD status / meaning |
|---|---|---|---|
| `taxpayerAddressType` | `kind` | 544 | Required: `HQ` headquarters, `SITE` site, `BRANCH` branch; unknown text retained. |
| `countryCode` | `country_code` | 545 | Required ISO alpha-2; NAV 2.0 also has schema default `HU`. |
| `region` | `region` | 546 | Optional province/region code. |
| `postalCode` | `postal_code` | 547 | Required; preserves leading zeros and supplied `0000`. |
| `city` | `city` | 548 | Required settlement. |
| `streetName` | `street_name` | 549 | Required public-place name. |
| `publicPlaceCategory` | `public_place_category` | 550–552 | Required public-place category. |
| `number` | `number` | 553 | Optional house number. |
| `building` | `building` | 554 | Optional building. |
| `staircase` | `staircase` | 555 | Optional staircase. |
| `floor` | `floor` | 556 | Optional floor; `0` preserved. |
| `door` | `door` | 557 | Optional door; `02` preserved. |
| `lotNumber` | `lot_number` | 558 | Optional lot number, e.g. `123/4`. |
| `additionalAddressDetail` | `additional_address_detail` | 559–561 | **Not in either taxpayer detailed-address schema**. Declared on `SimpleAddressType`; tolerated direct C-namespace extension only. Correctly qualified in rustdoc 258–261. |

All address fields are `Option<String>` (`231–262`). Independent scratch probes filled every leaf in **both** layouts, checked all corresponding JSON values, removed each leaf through a namespace mutation, and duplicated each leaf to verify refusal. Multiple addresses, an empty third item, independent per-item fields, and `BRANCH` were exercised. Existing tests cover `SITE`, `HQ`, text decoding, empty content, and per-item duplicate containers/kinds.

There is **no documented taxpayer `simpleAddress`/`detailedAddress` choice wrapper**: `taxpayerAddress` directly has `DetailedAddressType`. Supporting an extra `additionalAddressDetail` leaf is not evidence for such an alternative. Omitting XSD defaults (e.g. NAV 2.0 `<countryCode/>` becoming `None`, not `HU`) is consistent with the documented non-validating source-text projection.

### 2.5 Result, error, namespace, and loss matrix

| Input / case | Actual behavior | Assessment / evidence |
|---|---|---|
| `OK`, validity true/1 | `Ok(TaxpayerInfo { valid: true, … })`. | Official success and both-version probes pass. |
| `OK`, validity false/0 | Successful data with `valid: false`. | Official negative examples and both-version probes pass; never inferred from transport or error. False means invalid **or** nonexistent; not a distinction the bool can make. |
| `OK`, missing/empty/XML-blank validity | `ParseError::Missing("taxpayerValidity")`. | Deliberate useful-answer requirement, stronger than XSD `minOccurs=0`; documented README 264–265. |
| Malformed boolean (`TRUE`, `False`, `2`, `true false`, `junk`) | Parse failure. | Existing/scratch checks pass except F1's Unicode-padding case. |
| Missing/empty/foreign/wrong-path `funcCode` | Missing `funcCode` parse failure. | No default success; existing/scratch checks pass. |
| `ERROR`, numeric 57 | `ApiError { code: MalformedXml, message: … }`. | Both fresh official error examples pass, including full diagnostic text. |
| `ERROR`, credential codes | Normal typed `ApiError`; shared `ErrorCode` classifier identifies credential codes. | Body 3 tested under both layouts; shared header checks cover down/code/status order. 135/136/164 classification is source/code review, not live evidence. |
| `ERROR`, textual NAV/future code | `ErrorCode::Unknown` retains token after documented code trimming. | `INVALID_REQUEST` probed in both layouts; never numeric-parse loss. `Unknown`/absent have generic `OutcomeClass::Unknown`; that is not a taxpayer validity value. |
| Non-`OK` without code | `ErrorCode::Absent`; message if supplied, else raw code, else `NAV funcCode {token}`. | `615–625`; scratch covers absent/empty code, code-only, and future funcCode with message. No invented 0/57. |
| Unknown non-`OK` funcCode | Error branch, not success. | Conservative local policy; only `OK`/`ERROR` declared by NAV. Raw funcCode is not separately exposed if a message/code supplies the error. |
| `OK` plus code/message | Success branch ignores code/message if validity exists (`599–613`). | FuncCode-led policy; no current official contradictory sample establishes that a code must override `OK`. Header errors still override body. |
| False plus populated data | Keeps false **and** supplied fields. | Probed under both layouts; avoids inventing validity from name presence or discarding delivered content. |
| `ERROR` plus malformed business validity | Parse failure before `into_info`, losing typed API classification. | Probed: `ERROR/INVALID_REQUEST` + validity `junk` becomes `Parse(Invalid { field: "taxpayerValidity", … })`. Schema-invalid payload; fail-closed policy, not a confirmed valid-response defect. |
| Down / error header / non-2xx | In order: `ServiceUnavailable`, header `Api`, then `HttpStatus`; body only after all three (`taxpayer.rs:283`, `wire.rs:278–308`). | Both-version scratch checks and existing wire tests pass. HTTP 500 with only body error is `HttpStatus`, documented README 301. |
| Truncated download | Native client returns `IncompleteResponse` retaining status, headers and source, never supplies an invented complete empty body. | Existing shared client loopback regression passed; its request is invoice creation, so this verifies transport plumbing, not a taxpayer-specific wire capture. |
| Empty, non-UTF-8, wrong root/version, extra root, truncation, malformed tail | Parse failure, not false. | Shared `response_root` and completion tests. Wrong version/capitalization independently probed. |
| Foreign or unknown subtree, nested recognized local names, namespace rebinding/reset | Ignored as whole path; no recognized descendant injection. | Existing and scratch probes pass. Aliases and XML-reference-escaped namespace URIs work; no URL percent-decoding. |
| Duplicate recognized singleton/container | Parse error, including empty-first duplicates; address items alone repeat. | `443–450`; existing and all-address-leaf scratch checks pass. |
| Child inside scalar | Parse error even if foreign, rather than concatenating around child. | `432–436`; tested. |
| Text, entities, CDATA, comments/PIs, line endings | Decoded scalar text concatenated; comments/PIs add no content, literal CR/CRLF normalize, numeric CR retained. | `475–507`; existing mixed-text test checks Unicode/NBSP/CR and padding. Blank business text becomes `None`. |
| Unknown entity, illegal XML syntax/character in ignored extension | Preflight refuses whole body. | `xml.rs:63–193`; existing completion/namespace tests pass. DTDs are refused, so no DTD/entity expansion or XSD defaults are applied. |
| Duplicate expanded-name attributes | **Accepted** despite namespace violation. | F2 below. |

### 2.6 Explicitly unrepresented envelope data

This inventory distinguishes complete **business coverage** from a lossless response model:

- NAV header: `requestId`, `timestamp`, `requestVersion`, optional `headerVersion`; all ignored. NAV 2.0 declares these under A, NAV 3.0 under Common R. The root namespace, not header text, selects layout.
- Software: `softwareId`, `softwareName`, `softwareOperation`, `softwareMainVersion`, `softwareDevName`, `softwareDevContact`, optional `softwareDevCountryCode`, `softwareDevTaxNumber`; ignored. This describes the intermediary software, not the queried taxpayer.
- NAV 3.0 `result/notifications/notification[]/{notificationCode,notificationText}`: ignored. Scratch `NOTICE`/`Keep me` produced a normal successful projection with no notification field. S7 establishes that this is a real optional envelope structure, not merely an invented extension. No source reviewed establishes current szamlazz.hu forwarding of it or a mandatory taxpayer business action it carries. **Projection limitation / documentation opportunity**, no severity assigned as a functional defect.
- `funcCode` is reduced to success/error; `errorCode` and `message` appear only on errors. Unknown content/attributes are not retained on `TaxpayerInfo`, and the type has no raw XML accessor. A caller needing all evidence must retain `RawResponse` before parsing (or use the lower-level transport). This is not a lossless archival model.

## 3. Confirmed findings

Severity scale: high = material incorrect business result on a documented normal path; medium = meaningful supported-data loss or misclassification; low = demonstrated edge-case conformance/hardening defect with no established normal-vendor impact.

### F1 — Low: non-XML Unicode padding turns an invalid validity lexical form into a boolean

**Exact current lines:** `taxpayer.rs:524–525` uses `frame.text.trim()` for `taxpayerValidity`, before `570–581` accepts `true`/`false`/`1`/`0`. Rust `trim()` removes NBSP and other Unicode whitespace, not just XML's space/tab/CR/LF. The crate already has the narrower `xml::is_xml_space` (`xml.rs:676–677`) and uses it elsewhere for required booleans (`615–622`).

Repro (no headers; parse through `QueryTaxpayer::new("12345678")`):

```xml
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
  <result><funcCode>OK</funcCode></result>
  <taxpayerValidity>&#160;true&#160;</taxpayerValidity>
</QueryTaxpayerResponse>
```

**Observed:** `Ok(TaxpayerInfo { valid: true, … })`. The corresponding NAV 3.0 document, with Common-namespace result, behaves identically. The independent host-libxml2 XSD oracle rejects the same scalar as `xs:boolean` (`validation_code=1824`), while accepting `true`, `false`, `1`, `0` and XML-whitespace padding.

**Expected boundary:** refuse this malformed verdict fact rather than erase non-XML characters to obtain a valid token. S5/S6 declare `xs:boolean`; this is distinct from intentionally preserving unvalidated optional business strings and advisory dates. It does not change a legitimate true to false, and no actual NAV emission of NBSP-padded validity was observed. Thus low severity, not a business-path blocker. If this broader boolean normalization is intended, it should be an explicit documented exception rather than presented as XML lexical handling.

### F2 — Low: shared XML preflight misses duplicate attributes with the same expanded name

**Exact current lines:** `taxpayer.rs:405–417` delegates preflight to `xml::response_root`; `xml.rs:96–109` checks attribute syntax and undeclared prefixes but never checks a set of **resolved** attribute names. Lexical token validation at `xml.rs:173–193` does not provide that namespace-level uniqueness check. `taxpayer.rs:438–450` checks duplicate recognized **elements**, not attributes.

Repro:

```xml
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api"
                      xmlns:f="urn:foreign">
  <result><funcCode>OK</funcCode></result>
  <taxpayerValidity>true</taxpayerValidity>
  <f:extension xmlns:a="urn:x" xmlns:b="urn:x" a:id="1" b:id="2"/>
</QueryTaxpayerResponse>
```

**Observed:** `Ok(TaxpayerInfo { valid: true, … })`, both NAV layouts. Independent libxml2 emits `Namespaced Attribute id in 'urn:x' redefined` (it may still return a tree; the namespace diagnostic, not null-tree behavior, is the oracle).

**Expected boundary:** S10 prohibits two attributes with identical expanded names, irrespective of differing prefixes. This is a namespace-well-formedness failure, not business XSD validation. The omission falls short of the shared strict-XML boundary described in README 253–259, particularly checking ignored extensions. This is a shared-parser defect reached by taxpayer parsing, not a wrong NAV field path. These attributes are ignored by this projection, so the probe establishes inconsistent malformed-input acceptance, **not** a verdict-injection exploit or taxpayer-data corruption. No live vendor impact established.

## 4. Policy choices, source ambiguities, and documented limits

These are not additional confirmed defects:

1. **No strict response-XSD validation.** Required taxpayer names/ids/incorporation/address components may be absent, blank or wrong-namespace and project as absent. Length, country/VAT/county patterns, element ordering, and consistency with the requested prefix are not verified. Unknown business tokens survive. This matches README 244–265 and the `Option` model. A caller's definition of a sufficiently complete taxpayer record is a separate validation decision.
2. **Missing validity is not false.** Both XSDs make it optional at envelope level, allowing error replies. S4 says invalid/nonexistent returns false and S3 shows it explicitly. Requiring validity on `OK` is documented conservative policy; no source example proves that `OK` without validity is a valid negative answer.
3. **False is not a precise nonexistence classification.** S4 includes invalid as well as nonexistent numbers. The public `valid` wording is appropriate; consumers should not read it as “this identifier never existed” or as an EU/VIES check.
4. **`infoDate` is source text.** Rustdoc 210–217 and README 400 explicitly reject datetime/TTL inference. Existing tests exercise offsets, no zone, long fractions, empty/blank, malformed dates and NBSP. Keeping malformed advisory text is policy, not an accidental temporal parse success. No authoritative taxpayer cache TTL was established.
5. **Full data on false and malformed errors.** Keeping data alongside false avoids unnecessary information loss. Conversely, parsing all recognized scalars before interpreting funcCode can make malformed content hide the typed code (matrix above). This is the documented whole-response parse boundary. Source/schema-invalid error combinations do not establish a bug in valid-error handling.
6. **Extension tolerance is asymmetric with validation.** Unknown elements are ignored, while a recognized optional field with a child element or duplicate fails the entire response. `incorporation` in NAV 2.0 and direct `additionalAddressDetail` are tolerated extensions. These must not be advertised as schema declarations.
7. **Default values are not injected.** NAV 2.0's `countryCode` XSD default `HU` is not applied to an empty element. The parser retains source presence/content policy rather than constructing a schema-augmented record.
8. **Other native NAV error roots.** Common defines `GeneralErrorHeaderResponse` and `GeneralExceptionResponse`; this operation accepts only the two versioned `QueryTaxpayerResponse` roots. S3 says szamlazz.hu returns that type and demonstrates its own error 57 wrapped accordingly. Direct NAV root support is not proven necessary on this adapter; unexpected roots fail closed. Likewise the native NAV request's authentication/signature fields are not missing Számla Agent request fields.
9. **Current forwarding remains unverified.** README 400 accurately says synthetic NAV 3.0 tests cover the five new fields and that their current forwarding was not captured live. Freshly fetching a 2020 example in 2026 is not a fresh account observation. County/VAT-group/incorporation exposure in downstream worker/CLI-specific projections is outside this review.
10. **No hidden retry implementation.** The core performs no I/O; the client has no application-level retry/cache loop. S9's five-send/operator rule does not imply `valid=false`, parse failure or NAV error should be retried until valid. Generic error outcome classifications describe an exchange, not taxpayer existence, and unknown textual NAV errors are intentionally not forced into numeric Agent error meanings.
11. **Documentation minor gaps.** Public rustdoc for incorporation does not itself say “3.0 only; tolerated in 2.0”, though the existing test does. Notifications/header/software omissions are not individually listed in the README. These are opportunities to make the projection limits clearer, not evidence of an omitted business field.

## 5. Tests and probes actually run

### Existing tests

All commands completed successfully; no live test was selected. **54 existing tests passed**, counting each test once:

```sh
cargo test -p szamlazz-agent --lib ops::taxpayer::tests
# 15 passed

cargo test -p szamlazz-agent --test taxpayer_paths --test response_completion --test response_namespaces --test error_classification
# taxpayer_paths 10, response_completion 4, response_namespaces 6,
# error_classification 3: 23 passed

cargo test -p szamlazz-agent --test upstream every_response_example_parses_through_its_operation -- --nocapture
# 1 passed: corpus present, 18 response examples checked,
# including all three taxpayer examples; no skip

cargo test -p szamlazz-agent --test custom_http_client
# 1 passed: taxpayer POST/multipart and parsing on loopback

cargo test -p szamlazz-agent --lib wire::tests
# 13 passed

cargo test -p szamlazz-agent --features client-reqwest --test client interrupted_response_retains_headers_without_promoting_them_to_a_verdict
# 1 passed: shared incomplete-download evidence, four header combinations
```

The general error-classification suite is a shared control, not a claim that every case in it is taxpayer-specific. The selected incomplete-download test uses an invoice request and shared client machinery. Other native transport failures, browser transport, full workspace tests and live account tests were not run.

### Fresh-source/offline probes

Authored scratch sources:

- `/tmp/opencode/f83e5fd-taxpayer-sources.py`: fetches operation HTML, extracts code blocks, and extracts the freshly downloaded NAV PDF. Uses only public documentation/package-download GETs.
- `/tmp/opencode/f83e5fd-taxpayer-probe/{Cargo.toml,src/main.rs}`: path-depends on the current unmodified crate. The pertinent dependency versions match the workspace: quick-xml 0.42.0, xmlparser 0.13.6, serde 1.0.229, rust_decimal 1.43.0, jiff 0.2.35.
- `/tmp/opencode/f83e5fd-taxpayer-validate.py`: independent host-libxml2 oracle via Python ctypes, no repository dependencies or schema edits.

Executed:

```sh
python3 /tmp/opencode/f83e5fd-taxpayer-sources.py
cargo run --offline --manifest-path /tmp/opencode/f83e5fd-taxpayer-probe/Cargo.toml
python3 /tmp/opencode/f83e5fd-taxpayer-validate.py
```

Results:

- Six fresh extracted EN/HU XML response examples: success, code 57, and successful false exactly as expected. The docs site's email masking is retained; that software field is ignored by the taxpayer projection.
- Prefix constructors and serde: eight ASCII digits including leading/all zeros accepted; seven/nine digits, full formatted number, padding, Unicode digits and nondigits refused.
- Both credential modes: valid against **both** freshly extracted inline XSDs (**four schema validations passed**); UTF-8, XML escaping, sequence, leading-zero prefix, and multipart action checked.
- NAV 2/3: all business/address components populated; every address leaf checked after namespace mutation and after duplication; multiple/sparse/empty items isolated. Four boolean tokens and XML padding accepted; empty/blank/bad lexical forms refused except the explicitly observed F1 control.
- NAV 2/3: numeric/NAV textual/absent/future error combinations; missing/empty/foreign funcCode; wrong root/version; scalar children; repeated validity; namespace reset/rebinding and hidden re-entry; header/down/status precedence all met assertions.
- F1 and F2 reproduced and then asserted as current behavior under both layouts. Independent libxml2 rejects the NBSP boolean and reports the duplicate-attribute namespace error.
- Notifications omitted, data retained with false, and malformed validity on an error classified as parse failure: observed policy/projection controls, not promoted to confirmed valid-input defects.
- Final Rust probe output: **`ALL ASSERTIONS PASSED`**. It ran once initially and again after adding the remaining classification/rebinding/header controls; the second run passed too.

Tooling limits and unsuccessful routes: the schema-location URL returned 404; webfetch refused the PDF over 5 MB; `pdftotext` and bare `python` were unavailable; `python3 -m venv` could not bootstrap pip because ensurepip was absent. These were acquisition/tooling failures, not failed crate tests. Scratch-only wheel extraction supplied the PDF reader; system libxml2 supplied XSD validation. No full NAV response was XSD-validated: the all-fields probes deliberately contain sparse records and tolerated extensions, and the vendor error example itself omits normally required NAV envelope data. The XSD oracle was used for the two emitted requests and the boolean lexical control, while source enumeration established NAV path/field coverage.

**Overall:** normal documented taxpayer interoperability is supported by fresh-source comparison and focused execution. Remaining confirmed issues are narrow malformed-input conformance gaps; current live forwarding and deliberately reduced envelope evidence remain explicitly outside that conclusion.
