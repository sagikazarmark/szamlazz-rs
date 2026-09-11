# Számla Agent taxpayer and operation-surface review — 61c334f

**Date:** 2026-09-11. **Requested baseline:** `61c334f9508e8b63df2f3db182d6ca83f4feb8f0`.

**Conclusion:** no confirmed actionable P0–P3 defect in the taxpayer implementation against the current Számla Agent contract. All eleven operations in both the current navigation and the official form-action table are implemented and advertised. All business fields and detailed-address components declared for NAV 2.0/3.0 taxpayer responses are exposed. The taxpayer result is a business-data projection, not a lossless NAV envelope; its omissions and unresolved interoperability questions are ranked below.

## Scope and worktree provenance

- Reviewed the current worktree, including the entire `crates/szamlazz-agent/src/ops/taxpayer.rs`, its tests, the shared credential/XML/response handling it uses, `src/lib.rs`, `src/ops.rs`, the crate README and workspace README.
- Read all of `docs/szamlazz-hu-behaviour.md`. Read the ignored live taxpayer smoke test, without executing it. Its presence is not evidence that its current assertions have been exercised against the vendor.
- At entry HEAD was `61c334f`; the worktree already contained worker changes and an extraction of XML character validation into `wire::validate_xml_text`. Another actor committed those changes during this review, advancing HEAD to `5c6d5ead33a3587c4ea29cc973bedaefc3ddcb1f`. `git diff 61c334f --stat -- crates/szamlazz-agent` then showed only `src/wire.rs` (10 insertions, 1 deletion). Taxpayer source/tests, module exports and advertised coverage remained unchanged. This report describes the inspected worktree, not an assertion that HEAD stayed fixed.
- The earlier `837dad0` taxpayer report supplied discovery leads for NAV schema/download URLs only. Every substantive source claim below was independently re-fetched and inspected. No earlier verdict or test result is counted as proof.
- The only repository file authored by this review is this report. No source/test edits, credentials, live Számla Agent or NAV operation calls, or commits. Public documentation/schema GETs and offline tests were used. Other operations receive an operation-presence inventory here; their detailed request/response audits belong to the other reviewers.

**Code notation:** `T` = `crates/szamlazz-agent/src/ops/taxpayer.rs`; `src/…`, `tests/…` and `README.md` otherwise mean paths within `crates/szamlazz-agent`, except where explicitly labelled workspace.

## 1. Ranked findings and follow-ups

There are **no confirmed functional defects to rank P0–P3**. The following are ranked by potential impact, with evidence limits explicit; they are not assertions that current vendor responses fail.

### 1. Generic NAV error roots are not decoded — conditional interoperability gap

- **Code:** T:281–284,404–416 accepts only `QueryTaxpayerResponse` in the two NAV API namespaces. `src/wire.rs:291–307` checks a known non-2xx status before body interpretation. T:592–622 maps an error only after the taxpayer envelope has parsed.
- **Exact sources/claims:** [Számla Agent response][S3] says the response “always matches” NAV's taxpayer response and demonstrates code **57 inside `QueryTaxpayerResponse/result`**. In contrast, [NAV specification][N1] §3.1–3.2, printed pp.162–166, declares `GeneralExceptionResponse` and `GeneralErrorResponse` for native NAV technical/authentication failures, with codes such as `INVALID_REQUEST`, `INVALID_SECURITY_USER`, `FORBIDDEN` and `MAINTENANCE_MODE`. [Common 1.0 XSD][NC] declares `GeneralExceptionResponse` as an extension of `BasicResultType` (**direct** `funcCode/errorCode/message`), and `GeneralErrorHeaderResponse` with header/result; [NAV 3 API XSD][N3A] declares API `GeneralErrorResponse` with `software` and repeated `technicalValidationMessages`.
- **Impact if forwarded unchanged:** at HTTP 200 or without supplied status, a generic root becomes `ResponseError::Parse` instead of a structured `ApiError`; with NAV's non-2xx status forwarded, it becomes `HttpStatus`. The diagnostic code/validation list is not exposed as typed error data. It never becomes a successful or invalid-taxpayer answer.
- **Why not a confirmed bug:** native NAV behavior does not establish how szamlazz.hu wraps errors. The Agent page explicitly promises its taxpayer wrapper; no current vendor capture here contradicts that. The code-57 Agent example is not a native-NAV generic error example.
- **Correction/next evidence:** obtain vendor confirmation or a sanitized forwarded-error capture. If generic roots are supported, add exact namespace/root-specific error readers, including the direct-result exception shape, and decide the HTTP-status policy explicitly. Do not loosen extraction to arbitrary descendants or infer invalid taxpayer from an error.

### 2. NAV envelope metadata and notifications are discarded — confirmed projection limitation

- **Code:** T:188–225,287–304,340–397,592–622 has no public header/software/notification fields. T:348 recognizes only the three basic result leaves. A successful result drops any parsed result `message`/`errorCode`; an error keeps only code and message, not the envelope.
- **Exact sources/claims:** the [published success example][S3] contains `header` and `software`. [NAV 2 API XSD][N2A], `BasicHeaderType`, `BasicResponseType`, `SoftwareType`, and [NAV 3 API XSD][N3A] plus [Common 1.0][NC] declare the full envelope. Common's `BasicResultType` additionally permits `notifications/notification` (unbounded), each with `notificationCode` and `notificationText`, annotated “Miscellaneous notifications”.
- **Impact:** users of `Client::send` cannot inspect the NAV request identifier, response timestamp/version, software provenance or miscellaneous notices through `TaxpayerInfo`. The framework-free caller can retain `RawResponse::body()` (`src/wire.rs:221–225`). This does not drop a taxpayer name, tax-number part or address field.
- **Correction if lossless diagnostics are a requirement:** expose optional response metadata/notifications separately, or provide a supported raw-response hook. Document the projection boundary on the public taxpayer type. Do not turn a notification into failure merely because it exists. The current API promises a taxpayer lookup, not every NAV envelope field, so this is a nonblocking feature limitation.

### 3. `OK` requires validity despite XSD optionality — deliberate, conservative policy

- **Code:** T:594–600 returns `ParseError::Missing("taxpayerValidity")` for `OK` without a nonblank validity value; `README.md:287–292` explicitly states this policy.
- **Exact sources/claims:** [NAV 2 XSD][N2A], `QueryTaxpayerResponseType:1682`, and [NAV 3 XSD][N3A], `QueryTaxpayerResponseType:1566`, declare `taxpayerValidity` with `minOccurs="0"`. [NAV specification][N1] §1.8.9.2, printed p.68, nevertheless says invalid or nonexistent numbers return **false**; the [Agent invalid-number example][S3] is `OK` plus explicit false.
- **Impact:** an XSD-permitted sparse `OK` response without validity cannot be returned through the current `bool` model. It is refused rather than incorrectly converted to false. No source examined establishes such a response as a normal successful Agent result.
- **Correction only if that case is established:** model unknown validity explicitly, preserving the distinction from false. Do not default a missing verdict to false merely to satisfy XSD cardinality. No change is justified by cardinality alone.

### 4. Coverage wording needs its scope understood — no missing in-scope operation

- **Code:** crate `README.md:352–362` accurately lists the implemented operations. Workspace `README.md:19` says “Complete integration surface” and immediately names Számla Agent, IPN and Adatkapcsolat. `src/lib.rs:75–84` exports `ops`, whose modules are at `src/ops.rs:34–43`.
- **Exact sources/claims:** [current Agent navigation][S0] and [official action table][S9] enumerate the same eleven actions. [Third-party invoicing][X1] separately documents **`action-agent_ceg_mb`** for account creation/join requests, with a licence requirement; no corresponding request type exists in the Agent module inventory.
- **Impact/correction:** there is no operation-level implementation gap within the requested ordinary Agent scope. If “complete” is meant to cover every product in the vendor's top navigation, it overstates coverage: qualify it as the three supported protocol surfaces and explicitly exclude delegated onboarding. Third-party onboarding is an actual unimplemented vendor operation, but intentionally outside this review's in-scope completeness claim.

## 2. Fresh primary-source ledger

All sources below were fetched on the review date. Current operation pages display **`v202608271632`**; legacy standalone XSD routes display **`v202606031507`**. The EN taxpayer response page explicitly dates its examples to **2020-11-04**. A recent site build is not a recent live response capture.

| ID | Exact URL / inspected content |
|---|---|
| S0 | <https://docs.szamlazz.hu/agent/>, <https://docs.szamlazz.hu/agent/category/querying-taxpayer>, <https://docs.szamlazz.hu/agent/category/basics> — current navigation and taxpayer purpose. |
| S1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/request>, <https://docs.szamlazz.hu/hu/agent/querying_taxpayer/request> — endpoint, method, content type, action and NAV provenance. |
| S2 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xml>, <https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xml> — request example and complete inline XSD, including both tab contents. |
| S3 | <https://docs.szamlazz.hu/agent/querying_taxpayer/response>, <https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response> — all three response examples and delegated response-schema link. |
| S4 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xsd>, <https://docs.szamlazz.hu/hu/agent/querying_taxpayer/xsd> — legacy routes, same relevant request sequence/facets. |
| S5 | <https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd> — working downloadable request XSD. |
| S6 | <https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd> — example's schema-location hint fetched over HTTPS: **404**. Also tried <https://www.szamlazz.hu/szamla/docs/xsds/agent/xmltaxpayer.xsd>: **404**. |
| S7 | <https://docs.szamlazz.hu/agent/basics/authentication> — key or username/password, dedicated single-account user, legacy key in both fields. |
| S8 | <https://docs.szamlazz.hu/agent/basics/error-handling> — code 57 and credential codes; plain-text response-version-1 operations listed separately, not taxpayer. |
| S9 | <https://docs.szamlazz.hu/agent/basics/sending-requests> — all eleven action names, same endpoint, one XML per document. |
| S10 | <https://docs.szamlazz.hu/agent/category/generating-invoice>, <https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules>, <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types>, <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification> — distinguish document variants and email settings from separate operations. |
| N1 | <https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf> — delegated source, §1.8.9 pp.63–69 and §3.1–3.2 pp.162–166. Web fetch exceeded 5 MB; fresh curl GET and `pdftotext -layout` through `nix shell nixpkgs#poppler-utils` succeeded. |
| N2A | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd> — 2.0 header/result 596–705, response 1668–1697, software/taxpayer/address types 1866–1993. |
| N2D | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceData.xsd> — 2.0 detailed address 965–1044 and tax number 2225–2250. |
| N3A | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd> — 3.0 inheritance 548–565, response 1552–1581, software/taxpayer/address types 1756–1889, incorporation tokens 43–68. |
| N3B | <https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd> — detailed address, separate simple address, tax number. |
| NC | <https://raw.githubusercontent.com/nav-gov-hu/Common/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd> — imported NTCA **1.0**, including header/result, notifications, generic roots, and token/string facets. |
| X1 | <https://docs.szamlazz.hu/third-party-invoicing/> — separate delegated/self-billing product and account onboarding action. |
| X2 | <https://docs.szamlazz.hu/penzugyi-adatkapcsolat/> — separate PUSH product: outgoing/incoming invoices, bank transactions, receipts. |

GitHub commit queries resolved N3 to `cc7a775d6dce361311e409abb9934eb755f2749c`, N2 to `84442e64bc2cd7feb368fedb8199645188962b23`, and Common 1.0 to `ab8d7887967492e5f6d6e25447be853fd767add8`. Fresh download SHA-256:

| Source | SHA-256 |
|---|---|
| N1 PDF | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |

Scratch acquisition files use `/tmp/opencode/taxpayer-61c334f-*`. Initial guessed NAV paths without `nav/gov/hu/OSA` returned 404 and supplied no evidence. No old scratch download was reused.

## 3. Complete taxpayer request comparison

| Requirement | Current code | Assessment |
|---|---|---|
| S1/S9: POST to `https://www.szamlazz.hu/szamla/`, XML file in `multipart/form-data`, field `action-szamla_agent_taxpayer` | T:264–279; `src/wire.rs:14,359–408`; `src/client.rs:374–379` | Correct endpoint/action/multipart path. |
| S2/S5: root `xmltaxpayer`, namespace `http://www.szamlazz.hu/xmltaxpayer`, qualified elements | T:269–278; `src/xml.rs:139–160` | Correct root, default namespace and UTF-8 declaration. HTTP namespace identity is not changed to HTTPS. |
| S2 HU: fixed order; XSD requires `beallitasok` followed by `torzsszam` | T:273–276 | Both present in sequence. |
| Settings sequence: optional `felhasznalo`, `jelszo`, `szamlaagentkulcs`; S7 authentication alternatives | T:274; `src/xml.rs:610–619`; `src/credentials.rs:45–81` | Key-only and ordered username/password forms supported. The legacy same-key-in-both-fields form is representable. |
| Prefix: `length=8`, `[0-9]{8}` | T:15–73,85–109 | All public construction/deserialization routes validate eight ASCII digits, preserving leading zeroes. No undocumented checksum gate. |
| Request example has `xsi:schemaLocation` | Writer omits it | Harmless omission of a schema hint, not business data; the example's download target is broken. |

Full hyphenated tax numbers, whitespace and Unicode digit lookalikes are deliberately refused by this **Agent** request type. Native NAV `header`, `user`, hashes/signature, software and `predecessorTaxNumber` are not fields of `xmltaxpayer`; the intermediary owns those. N1 says predecessor input has no further effect on this native operation. There is no documented missing Agent parameter for country, pagination, historical date, response version or NAV version.

## 4. Response roots, namespaces and verdicts

The parser correctly distinguishes namespace declarations from the namespace of an imported field **type**:

| Layout | Root/business namespace A | Result/header namespace R | Tax-number/address components C |
|---|---|---|---|
| NAV 2.0 | `http://schemas.nav.gov.hu/OSA/2.0/api` | A | `http://schemas.nav.gov.hu/OSA/2.0/data` |
| NAV 3.0 | `http://schemas.nav.gov.hu/OSA/3.0/api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

N2's response extends API `BasicResponseType`; N3's extends API `BasicOnlineInvoiceResponseType`, which extends Common `BasicResponseType`. T:323–397 implements that distinction. Prefix spelling is arbitrary; the expanded name and parent path are what count.

- `{A}QueryTaxpayerResponse/{R}result/{R}{funcCode,errorCode,message}`: T:342,348.
- Root-child `{A}{infoDate,taxpayerValidity,taxpayerData}`: T:343–347.
- `{A}taxpayerData/{A}{taxpayerName,taxpayerShortName,incorporation,vatGroupMembership,taxNumberDetail,taxpayerAddressList}`: T:349–358.
- `{A}taxNumberDetail/{C}{taxpayerId,vatCode,countyCode}`: T:359–363.
- `{A}taxpayerAddressList/{A}taxpayerAddressItem/{A}{taxpayerAddressType,taxpayerAddress}`, then direct C components: T:364–383.

The response page's prose `QueryTaxPayerResponse` capitalization differs from its actual examples and both NAV XSDs; the implementation correctly uses **`QueryTaxpayerResponse`**.

| Response structure / condition | Behavior and evidence |
|---|---|
| Published NAV 2 success | `OK`, validity true, taxpayer name/stem/VAT, HQ address and root infoDate are extracted. `tests/upstream.rs:964–992` exercises all of those supplied values. |
| Published invalid number | `OK` with false is successful data, no fabricated name or address; T:706–715, `tests/upstream.rs:994–1006`. Matches S3 and N1 p.68. |
| Published Agent code-57 failure | `ERROR`, `errorCode=57`, message, no validity required; T:592–622,794–805 and `tests/upstream.rs:1008–1018`. Maps to `ErrorCode::MalformedXml`. |
| Native NAV textual code inside the supported wrapper | Retained as `ErrorCode::Unknown(String)`, not parsed into a numeric-only code; T:612–622,824–840 and `src/error.rs:459–470`. N1 §3 says error codes intentionally have no XSD enumeration to avoid client implementation dependencies. |
| Missing error code/message | Non-OK still errors; `ErrorCode::Absent`, message falls back to raw code or `NAV funcCode …`. No invented numeric zero; T:612–622. |
| Unknown non-OK `funcCode` | Conservative error, never success. If a future code also has a message, the raw function token is not separately exposed; current XSD function tokens are only `OK` and `ERROR`. |
| XML boolean | `true/false/1/0` supported; invalid nonblank lexical value refused, empty/missing fails an OK verdict; T:519–528,567–577,594–600. |
| Header/status/down | Operation begins with `RawResponse::check`; nonblank down, then header error, then known non-2xx precede the body. Ordinary code 56 has no taxpayer success exception. This is documented shared policy, not evidence of which taxpayer headers the vendor actually emits. |
| Generic NAV exception/error roots | Not supported as taxpayer replies; see ranked follow-up 1. Do not confuse `technicalValidationMessages` on `GeneralErrorResponse` with a missing normal taxpayer business field. |

Foreign/unknown containers cannot inject a verdict or data using familiar descendant names. Duplicate recognized singleton fields/containers, including empty-first duplicates, and children within recognized scalars are refused. `taxpayerAddressItem` is the repeating exception; each starts a fresh row (T:428–459,535–559). Text and CDATA are accumulated with references decoded, rather than overwritten per event (T:472–504). Whole-document validation runs first through `xml::response_root`: expected root, UTF-8, completion, matching closes, XML lexical and namespace constraints through EOF. This is not an XSD business-content validator.

## 5. Every taxpayer business field and address component

“Required” below means the XSD requires it **when its enclosing optional container exists**. Code deliberately represents sparse content rather than rejecting every incomplete business record.

| Wire field | Declaration/meaning (N1/N2A/N2D/N3A/N3B/NC) | Public projection / code |
|---|---|---|
| Root `taxpayerValidity` | Optional boolean; exists and valid | `valid: bool`, locally required on OK; T:189–190,598–600. |
| Root `infoDate` | Optional `xs:dateTime`, last data change | `info_date: Option<String>`; T:209–216,584. No lookup-time or TTL claim. |
| `taxpayerData/taxpayerName` | Required full name | `name`; T:191–192,579. |
| `taxpayerData/taxpayerShortName` | Optional short name in both versions | `short_name`; T:193–196,580. |
| `taxNumberDetail/taxpayerId` | Required eight-digit core number; group identifier for group taxation | `tax_number` string; T:217–219,585. |
| `taxNumberDetail/vatCode` | Optional one-digit VAT code, schema `[1-5]` | `vat_code` string; T:220–222,586. |
| `taxNumberDetail/countyCode` | Optional two-digit county code | `county_code` string; T:197–200,581. Leading zeroes retained. |
| `taxpayerData/vatGroupMembership` | Optional eight-digit VAT-group identifier | `vat_group_membership` string; T:201–205,582. Not a boolean/full assembled tax number. |
| `taxpayerData/incorporation` | Required in NAV 3 data, undeclared in NAV 2 | `incorporation`; all three tokens `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`, plus `Other(String)`; T:111–182,206–208,583. |
| `taxpayerAddressList/taxpayerAddressItem` | Optional list; items 1..unbounded when present | `addresses: Vec<TaxpayerAddress>` in wire order; T:223–224,457–459. |
| Item `taxpayerAddressType` | Required `HQ`, `SITE`, `BRANCH` | `kind: Option<String>`, open; T:231–232,541. |
| Address `countryCode` | Required alpha-2 country; 2.0 has schema default HU | `country_code`; T:233–234,542. |
| `region` | Optional province/region code | `region`; T:235–236,543. |
| `postalCode` | Required string | `postal_code`; T:237–238,544. |
| `city` | Required | `city`; T:239–240,545. |
| `streetName` | Required | `street_name`; T:241–242,546. |
| `publicPlaceCategory` | Required | `public_place_category`; T:243–244,547–549. |
| `number` | Optional house number | `number`; T:245–246,550. |
| `building` | Optional | `building`; T:247–248,551. |
| `staircase` | Optional | `staircase`; T:249–250,552. |
| `floor` | Optional | `floor`; T:251–252,553. |
| `door` | Optional | `door`; T:253–254,554. |
| `lotNumber` | Optional | `lot_number`; T:255–256,555. |

Both taxpayer schemas use **`DetailedAddressType` directly**, not the separate `AddressType` simple/detailed choice. No missing `simpleAddress` or `detailedAddress` wrapper support is established. `additional_address_detail` (T:257–261,556–558) deliberately retains a tolerated `additionalAddressDetail` extension; its XSD home is **SimpleAddressType**, not taxpayer detailed addresses. Its test is explicitly named extension tolerance (T:768–782). Likewise incorporation under NAV 2 is tolerated extension data, not asserted 2.0 schema conformance (`tests/taxpayer_paths.rs:76–114`).

### Entire envelope inventory outside that projection

- `header`: **requestId, timestamp, requestVersion, headerVersion** — omitted. API namespace in 2.0, Common in 3.0. The timestamp is response/request time, distinct from root `infoDate`.
- `software`: **softwareId, softwareName, softwareOperation, softwareMainVersion, softwareDevName, softwareDevContact, softwareDevCountryCode, softwareDevTaxNumber** — omitted. All declared in the API namespace in both versions; the last two are optional. The Agent examples omit software entirely on error/invalid-number, despite NAV base-schema requirements, so requiring it would break the published examples.
- `result`: **funcCode, errorCode, message** — consumed for the verdict, code/message returned on error only. Common 1.0 adds **notifications/notification/{notificationCode,notificationText}** — omitted (ranked follow-up 2).
- Native generic error **technicalValidationMessages/{validationResultCode,validationErrorCode,message}** — omitted with its unsupported root. Generic Common **GeneralErrorHeaderResponse** is not an additional accepted root either.
- Unknown extensions: structurally checked but not preserved in the typed projection. No raw XML member on `TaxpayerInfo`.

No contact email, telephone, bank account, company-registration number, EU VAT validation result or pagination block is declared in the examined normal taxpayer response. Software developer contact is not taxpayer contact.

## 6. Intentional differences, observations and uncertainty

1. **Sparse data:** required business strings are optional Rust fields. Absent/empty/XML-blank values become `None`; nonblank decoded text, including padding/NBSP, is preserved. No trimming of taxpayer identifiers into another value; no fabricated country HU or postal 0000. The NAV 2 empty-country schema default is therefore not materialized. This is explicit character/projection policy (`README.md:264–292`, T:519–528), not a claim to return the XSD post-validation information set.
2. **Advisory date:** malformed, zoneless or high-precision `infoDate` remains source text (T:209–216; `tests/taxpayer_paths.rs:6–34`). The declared source type is dateTime, but a bad advisory date does not destroy an otherwise useful lookup. No live deviation was needed to justify this documented local policy.
3. **No XSD value/cardinality enforcement for business content:** string length/pattern/enumeration violations and an empty address list can remain data. Verdict booleans and XML shape are checked. Success validity is the explicit stricter case discussed above.
4. **No taxpayer live deviation in the behavior ledger:** `docs/szamlazz-hu-behaviour.md` documents invoice/proforma/storno/credit-entry and amount behavior on a test account, not taxpayer forwarding layouts. It supplies no evidence for current NAV 3 forwarding, generic-root forwarding, extra address fields or missing OK validity.
5. **New fields already present:** short name, county code, VAT-group identifier, incorporation and infoDate are implemented and documented (`README.md:447`). Historical missing-field findings must not be repeated against this worktree.
6. **Live smoke is limited:** `tests/live.rs:25–40` checks true validity and nonblank name/stem only. It is ignored and was not run. Even a passing historical smoke would not establish all error/header/namespace/address/metadata cases. Current NAV version forwarded by szamlazz.hu remains unverified here; the response example is NAV 2, and NAV 3 support is schema-derived and synthetically tested.
7. **Official documentation defects:** the request schema-location URL returns 404; the inline “adóazonosító jel” wording conflicts with the eight-digit taxpayer-stem facets (personal tax ID is a different type); response prose has a capitalization typo. None warrants changing the correct request/root names to match prose mistakes.

## 7. Independent official operation inventory

The current sidebar at S0 and the explicit **“Base URL and form field” table at S9** independently agree. The following exact action values match the implementation constants; there is **no missing ordinary Agent action**.

| Current navigation operation | Official action (S9) | Implementation location | Advertised location |
|---|---|---|---|
| Generating invoice | `action-xmlagentxmlfile` | `ops/invoice.rs:678–679`, `CreateInvoice` | README:356 |
| Reversing invoice | `action-szamla_agent_st` | `ops/storno.rs:162–163`, `StornoInvoice` | README:357 |
| Registering credit entry | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:274–275`, `RegisterCreditEntry`; `ClearCreditEntries:237–238` reuses it | README:358 |
| Query document (PDF) | `action-szamla_agent_pdf` | `ops/query_pdf.rs:58–59`, `QueryInvoicePdf` | README:359 |
| Query document (XML) | `action-szamla_agent_xml` | `ops/query_xml.rs:535–536`, `QueryInvoiceXml` | README:359 |
| Deleting a pro forma invoice | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:60–61`, `DeleteProforma` | README:360 |
| Generating a receipt | `action-szamla_agent_nyugta_create` | `ops/receipt.rs:189–190`, `CreateReceipt` | README:361 |
| Reversing a receipt | `action-szamla_agent_nyugta_storno` | `ops/receipt.rs:340–341`, `StornoReceipt` | README:361 |
| Querying a receipt | `action-szamla_agent_nyugta_get` | `ops/receipt.rs:420–421`, `QueryReceipt` | README:361 |
| Sending a receipt | `action-szamla_agent_nyugta_send` | `ops/receipt.rs:502–503`, `SendReceipt` | README:361 |
| Querying taxpayer | `action-szamla_agent_taxpayer` | T:264–265, `QueryTaxpayer` | README:362 |

`src/lib.rs:81` publicly exposes `ops`; `src/ops.rs:34–43` publicly exposes all eight operation modules. Four receipt operations sharing one module explains why module count is lower than action count. The internal envelope/waybill modules are shared implementation vocabulary, not missing public operations. `ClearCreditEntries` is an explicit caller-intent variant of the existing credit-entry action, not a twelfth vendor action.

### Variants and apparent omissions

- Invoice, proforma, prepayment, final, corrective and delivery-note variants are represented by `InvoiceKind` (`ops/invoice.rs:34–85`); storno is separate, as S10 specifies. The delivery-note template description in the current document-types page is a per-invoice semantics question, not evidence of a missing action; the detailed invoice audit owns that comparison.
- Paper/e-invoice, preview, invoice notification/email attachments, discounts, tour-operator simple items and data-erasure codes are settings on existing operations. The S10 navigation lists those settings, not separate endpoints. A standalone **send/resend-invoice** action, invoice-list/search action, customer-master CRUD and prefix-management action do not appear in the examined Agent action table/current navigation. Their absence from the crate cannot be labelled an unimplemented documented Agent operation. Receipt send is explicitly a separate action and is present.
- Response version 1 is intentionally not offered where the crate selects version 2 (`src/ops.rs:28–32`); this is a wire-format choice, not a missing operation. Taxpayer has no response-version selector.
- **Third-party invoicing/self-billing:** X1's `action-agent_ceg_mb` onboarding is unimplemented here and outside scope. Legacy standalone XSD pages still display self-billing in an older Agent sidebar; the current site separates that product. Existing username/password credentials may serve a manually arranged delegated account, but do not constitute an implemented onboarding API.
- **IPN:** separate inbound payment-status receiver, implemented by `crates/szamlazz-ipn` (`src/lib.rs:1–24`); no outbound action in S9. Its status amounts are not the Agent's individual credit entries. This report did not re-audit the IPN protocol.
- **Adatkapcsolat:** X2 explicitly describes vendor-initiated PUSH of outgoing/incoming invoices, bank transactions and receipts. It is the separate `szamlazz-adatkapcsolat` receiver, not unimplemented Agent bulk querying or bank operations.
- Native NAV operations in N1 (token exchange, invoice-data/reporting/transaction queries, etc.) are not additional Agent actions. The taxpayer request is an intermediary wrapper, not a direct NAV client.

## 8. Offline verification and remaining evidence gaps

Executed successfully against the inspected code, without enabling or selecting ignored live tests:

```text
cargo test --offline -p szamlazz-agent --lib ops::taxpayer
  15 passed; 0 failed

cargo test --offline -p szamlazz-agent --test taxpayer_paths --test upstream --test response_namespaces --test response_completion
  taxpayer_paths:       10 passed
  upstream:             11 passed
  response_namespaces:  11 passed
  response_completion:   4 passed
```

**Total: 51 passing tests.** Upstream tests cover the repository's published-example copies; fresh page inspection confirmed the relevant taxpayer structures/values independently. Synthetic tests establish parser behavior, not vendor forwarding. No full schema-validation run or new test fixture is claimed.

Meaningful remaining gaps: a genuine current NAV 3 Agent capture, native generic-error forwarding behavior/status, real error header combinations, notifications/metadata forwarding, and any legitimate OK-without-validity case. NAV 3 error handling follows the same layout-driven extraction as NAV 2, but the dedicated tests predominantly exercise NAV 3 success/namespaces and NAV 2 error cases; that is a test-coverage opportunity, not a demonstrated implementation fault.

[S0]: https://docs.szamlazz.hu/agent/category/querying-taxpayer
[S3]: https://docs.szamlazz.hu/agent/querying_taxpayer/response
[S9]: https://docs.szamlazz.hu/agent/basics/sending-requests
[N1]: https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf
[N2A]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[N3A]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[NC]: https://raw.githubusercontent.com/nav-gov-hu/Common/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
[X1]: https://docs.szamlazz.hu/third-party-invoicing/
