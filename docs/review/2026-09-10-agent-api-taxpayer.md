# Számla Agent taxpayer review — 2026-09-10

## Verdict

**No confirmed runtime defect remains in the documented taxpayer request, business-field extraction, versioned namespace paths, or fault/success classification at `382cf7615aca1d64a05c7c3f77110248dde51950`.** One **P3, high-confidence test/documentation gap** remains: some permissiveness tests are presented as ordinary NAV-version coverage even though their fields or complete shapes are not declared by those schemas. This is not evidence that real taxpayer responses fail.

The current code contains the earlier five-field exposure, complete-document, expanded-name/path, entity and business-text fixes. They are verified below, not revived as historical findings. Optional NAV diagnostics, the optional-validity schema/prose distinction, and unverified direct NAV error forwarding are separate from confirmed gaps.

### Scope and method

- Compared the current implementation, not a historical diff, with independently fetched current official sources. `git rev-parse HEAD` returned the supplied SHA. `git diff HEAD -- crates/szamlazz-agent fixtures/synthetic/agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md docs/research/2026-09-10-agent-vendor-questions.md` was empty.
- During the review, unrelated shared-workspace activity advanced HEAD to `7da23b44cd006783eb47e60aa54ab8969bdd1c2c`. A final diff of the same scoped paths against the **explicit supplied SHA** remained empty; the findings still describe the requested source revision.
- Primary implementation: `crates/szamlazz-agent/src/ops/taxpayer.rs`; associated README, unit/integration tests, synthetic fixtures and official fixture provenance. Line references below are to this checkout. `taxpayer.rs` means that source file; `tests/…` means `crates/szamlazz-agent/tests/…`.
- Read the relevant `CONTEXT.md` vocabulary/decisions, the behavior notes, `fixtures/SOURCES.md`, and the vendor-question draft. The behavior notes contain no taxpayer-specific live result qualifying this comparison. The draft's two questions concern invoice preview/layout, not taxpayer behavior.
- Public documentation/schema/PDF GETs and local parser/writer calls only. No Számla Agent account calls, production/test edits, or use of the untracked historical `FINAL.md` as present-tense evidence. All reproduction code, downloaded sources, dependencies and build products are under `/tmp/opencode/taxpayer-audit-382cf76/`.
- Inspected shared XML/credential boundaries only as needed to establish what this operation calls. Shared XML implementation and general HTTP/error-catalogue review remain separate. No full XML validator or full workspace test run was needed.

## 1. Primary evidence and acquisition

All sources below were fetched during this review on **2026-09-10**. Main Számla Agent pages display build `v202608271632`; the still-accessible standalone `/xsd` page displays `v202606031507`. A build label is not a field's publication date.

| ID | Official URL | Evidence used |
|---|---|---|
| S1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/request> | POST, multipart XML, `action-szamla_agent_taxpayer`; data comes from NAV's Online Invoice Platform. |
| S2 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xml> | Current request example and inline request XSD. “The sent XML file must comply with the following XSD schema.” |
| S3 | <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | All three current examples, error guidance, link to NAV PDF §1.8.9. “The response always matches the `QueryTaxPayerResponse` type … Last update for example responses: 2020-11-04.” Actual XML spells `QueryTaxpayerResponse`. |
| S4 | <https://docs.szamlazz.hu/agent/querying_taxpayer/xsd> | Older standalone request XSD; same structural contract as S2. |
| S5 | <https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd> | Working downloadable request XSD; same structural contract. |
| N1 | <https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf> | Linked first-party NAV specification, §1.8.9, printed pp.63–69; generic fault discussion §3.1.2. Downloaded PDF has 382 pages; printed pp.64–69 are PDF pages 74–79. |
| N3A | [NAV 3.0 invoiceApi.xsd][n3a] | `QueryTaxpayerResponseType` lines 1552–1581; `TaxpayerAddressItemType`/list/data lines 1812–1889; base response lines 548–565. |
| N3B | [NAV 3.0 invoiceBase.xsd][n3b] | Detailed address lines 185–264, simple address 265–302, tax number 303–328. |
| NC | [NAV NTCA 1.0 common.xsd][nc] | `BasicHeaderType`, `BasicResponseType`, `BasicResultType`, notification types, lines 544–700. |
| N2A | [NAV API-2.0 invoiceApi.xsd][n2a] | Response lines 1668–1697; header/result 596–705; taxpayer address/list/data 1922–1993. |
| N2D | [NAV API-2.0 invoiceData.xsd][n2d] | Detailed address lines 965–1044; tax number 2225–2250; separate simple-address type. |

NAV Online-Invoice `master` resolved to `cc7a775d6dce361311e409abb9934eb755f2749c`; the first-party `API-2.0` tag resolves to `84442e64bc2cd7feb368fedb8199645188962b23`. Common's `common-1.0.0` tag resolves to `ab8d7887967492e5f6d6e25447be853fd767add8`. The Common repository's current default branch is now NTCA **2.0**; using that instead of the **1.0 import declared by OSA 3.0** would compare the wrong schema. Citations here are immutable.

The request example's `https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd` location still returns **404**. S5 works; the emitted request need not include `xsi:schemaLocation`. S2/S4/S5 agree on element order, cardinalities and prefix facets; no taxpayer request schema conflict was found.

Selected SHA-256 acquisition identifiers:

| Source bytes | SHA-256 |
|---|---|
| S3 HTML | `f082de46513598a03a07be20c1bc750db9d4fa91917e29a147f037c45c4d1ee6` |
| N1 PDF | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N3B | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| NC | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |

Hashes identify fetched bytes, not semantic truth or immutable website content. `acquire.py` saves HTML and extracts `<pre>` text without inventing response values. The three freshly extracted S3 bodies were parsed directly in scratch tests, independently of the July fixture corpus.

## 2. Complete request comparison

Sources: S1/S2/S4/S5; N1 §1.8.9.1. Request code: `taxpayer.rs:16–109,263–283`.

| Wire field or surface | Official contract | Implementation / verdict |
|---|---|---|
| Multipart operation | `action-szamla_agent_taxpayer` | Exact `ACTION`, line 264. Transport itself is outside this audit. |
| Root | `{http://www.szamlazz.hu/xmltaxpayer}xmltaxpayer` | Exact root and default namespace, lines 267–277. UTF-8 XML declaration supplied by writer. |
| `beallitasok` | One required container, before `torzsszam` | Always present in order, lines 272–275. |
| `beallitasok/felhasznalo` | Optional string, first credential element | User/password credentials emit it first. |
| `beallitasok/jelszo` | Optional string, second | Emitted after username. |
| `beallitasok/szamlaagentkulcs` | Optional string, third | Agent-key credentials emit this alternative. Delegated writer at `src/xml.rs:456–465` matches the sequence. |
| `torzsszam` | Required string, `<length value="8"/>`, `<pattern value="[0-9]{8}"/>` | Exactly eight ASCII digits at all constructors and serde entry points, lines 28–69, 92–109. Leading zeroes retained. |
| `xmlns:xsi`, `xsi:schemaLocation` | Example tooling hints | Deliberately absent; neither is a request business field or required schema attribute. |

There is **no** taxpayer request `valaszVerzio`, NAV-version selector, as-of date, pagination, or NAV direct-authentication block to add. NAV `header`, `user/passwordHash/requestSignature`, `software`, `taxNumber` and `predecessorTaxNumber` belong to the **direct NAV request**, not to the Számla Agent wrapper. The worker's acceptance of a full `NNNNNNNN-N-NN` number does not change this crate's eight-digit request contract (`CONTEXT.md:280–282`). No extra check-digit algorithm is required by S2/S5.

The golden writer assertion (`taxpayer.rs:635–642`, `tests/golden/xmltaxpayer.xml:1`) and upstream request comparison (`tests/upstream.rs:1185–1187`) cover the key-auth example. Scratch checks additionally exercised leading zeroes, serde construction, escaping, bad-length/full/non-ASCII/padded prefixes, exact action and absence of response-version fields. The username/password path was checked by inspection of the delegated writer, not claimed as a taxpayer-specific golden test.

## 3. Response namespaces and every field

### Expanded-name paths

An element's declaring schema determines its namespace, **not the schema containing its type**. In particular `taxNumberDetail` and `taxpayerAddress` stay in the API namespace although their children come from imported component types.

| Path/component | NAV 2.0 | NAV 3.0 | Current code |
|---|---|---|---|
| `QueryTaxpayerResponse` | `OSA/2.0/api` | `OSA/3.0/api` | Exact expanded root selection, lines 403–416. |
| `header` and its children | `OSA/2.0/api` | `NTCA/1.0/common` | Ignored as metadata, not searched for taxpayer values. |
| `result/{funcCode,errorCode,message}` | `OSA/2.0/api` throughout | `NTCA/1.0/common` throughout | Correct root-child result and leaves, lines 323–347. |
| `software` and children | `OSA/2.0/api` | `OSA/3.0/api` | Ignored as metadata. |
| `infoDate`, `taxpayerValidity`, `taxpayerData` and its immediate fields/containers | `OSA/2.0/api` | `OSA/3.0/api` | Correct, lines 342–357. |
| `taxNumberDetail/{taxpayerId,vatCode,countyCode}` | `OSA/2.0/data` | `OSA/3.0/base` | Correct, lines 358–362. |
| `taxpayerAddressList/taxpayerAddressItem/{taxpayerAddressType,taxpayerAddress}` | `OSA/2.0/api` | `OSA/3.0/api` | Correct, lines 363–364. |
| `taxpayerAddress/*` | `OSA/2.0/data` | `OSA/3.0/base` | Correct, lines 365–383. |

Prefixes are arbitrary. Default namespaces, explicit prefixes, misleading prefix spellings and character references inside namespace declarations work. Unknown frames remain unknown for their entire descendants (`taxpayer.rs:305–314,384–395,429–454`); a familiar local name at the wrong parent/namespace does not supply a verdict or overwrite business data.

### Business fields

“Required” below describes the XSD **when its containing structure is present**, not an instruction to make sparse parsing strict. The implementation intentionally reads optional business content leniently.

| Official path below root | NAV declaration / meaning | Public field; implementation lines | Result |
|---|---|---|---|
| `infoDate` | Optional `xs:dateTime`; last data change | `info_date`; 210–217, 585 | Present in S3's success example. Decoded advisory text preserved, including offsets/precision; no lookup-time or TTL inference. |
| `taxpayerValidity` | Optional `xs:boolean`; existing/valid taxpayer | `valid`; 190–191, 568–578, 597–601 | All four XML forms accepted; required for a successful crate result. See §6. |
| `taxpayerData/taxpayerName` | Required name | `name`; 192–193, 580 | Correct; optional when absent/blank. |
| `taxpayerData/taxpayerShortName` | Optional short name in both versions | `short_name`; 194–197, 581 | Correct. |
| `taxpayerData/taxNumberDetail/taxpayerId` | Required eight-digit stem, group identifier for group taxation | `tax_number`; 218–220, 586 | Correct; not incorrectly presented as the complete hyphenated number. |
| `…/taxNumberDetail/vatCode` | Optional one-digit VAT code | `vat_code`; 221–223, 587 | Correct string preservation. |
| `…/taxNumberDetail/countyCode` | Optional two-digit county code | `county_code`; 198–201, 582 | Correct, including `02`. |
| `taxpayerData/incorporation` | **3.0 only**, required when data exists; `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON` | `incorporation`; 112–183, 207–209, 584 | All known strings map correctly; unknown string stays `Other`. Accepted as an extension in 2.0 too; §4. |
| `taxpayerData/vatGroupMembership` | Optional eight-digit group stem | `vat_group_membership`; 202–206, 583 | Correct string, not boolean/full-number synthesis. |
| `taxpayerData/taxpayerAddressList` | Optional; one or more items if present | `addresses`; 224–225, 458–460 | All items retained in order, no cross-item field leakage. Empty/sparse forms tolerated. |

### Each address field

Sources: N2A/N2D, N3A/N3B and N1 pp.67–69. Both versions use **DetailedAddressType**, not the simple/detailed `AddressType` choice used elsewhere in NAV.

| Wire field | XSD | Public field; declaration / assignment lines |
|---|---|---|
| `taxpayerAddressType` | Required `HQ`, `SITE`, `BRANCH` | `kind`; 232–233 / 542; open string retains future values. |
| `countryCode` | Required country code; 2.0 additionally declares default `HU` | `country_code`; 234–235 / 543. Empty-default distinction in §6. |
| `region` | Optional province/region code | `region`; 236–237 / 544. |
| `postalCode` | Required postal code | `postal_code`; 238–239 / 545. |
| `city` | Required settlement | `city`; 240–241 / 546. |
| `streetName` | Required public-place name | `street_name`; 242–243 / 547. |
| `publicPlaceCategory` | Required public-place category | `public_place_category`; 244–245 / 548–550. |
| `number` | Optional house number | `number`; 246–247 / 551. |
| `building` | Optional building | `building`; 248–249 / 552. |
| `staircase` | Optional staircase | `staircase`; 250–251 / 553. |
| `floor` | Optional floor | `floor`; 252–253 / 554. |
| `door` | Optional door | `door`; 254–255 / 555. |
| `lotNumber` | Optional lot number | `lot_number`; 256–257 / 556. |
| `additionalAddressDetail` | **Not declared on taxpayer DetailedAddressType**; belongs to separate SimpleAddressType | `additional_address_detail`; 258–260 / 557–559. Tolerated extension, not a missing standard alternative; §4. |

No declared taxpayer business/address field is silently omitted. `name`, number components and all address values preserve decoded nonblank text rather than normalizing it. This includes surrounding padding and NBSP; XML-whitespace-only values become `None` (`taxpayer.rs:520–531`, README:235–242).

### Complete inherited/diagnostic field accounting

These fields are not in `TaxpayerInfo`, by projection choice:

- `header/{requestId,timestamp,requestVersion,headerVersion}` — required first three, optional last; NAV 2 API / NAV 3 Common namespaces. Root namespace selects parsing, not the advisory header version. The wrapper owns the NAV request id.
- `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact,softwareDevCountryCode,softwareDevTaxNumber}` — first six required, last two optional; API namespace in each version. S3's failed/invalid examples omit this nominally required block, supporting non-validating extraction.
- `result/funcCode` — required; drives success/error, not exposed as a success field.
- `result/errorCode`, `result/message` — optional; surfaced through `ApiError` on non-OK, ignored on OK. N1's operation semantics distinguish success from failure; mere schema co-occurrence of `OK` and an error-code element is not evidence that an OK taxpayer result should become an error.
- NAV 3 Common `result/notifications/notification[]/{notificationCode,notificationText}` — optional notifications container, one or more notifications, both leaves required. Entirely omitted from the public result/error projection; optional capability in §6. No such result field exists in N2A.
- XML namespace declarations/schema-location hints are not taxpayer data.
- `GeneralErrorResponse/technicalValidationMessages[]` is a **different root**, not another child missing from `QueryTaxpayerResponse`. Its forwarding by Számla Agent is unestablished; §6.

## 4. Confirmed gap

### TQ-01 — Separate schema conformance samples from tolerated extensions

**Priority:** P3 (low). **Confidence:** high. **Category:** tests/documentation; no demonstrated runtime failure on a documented response.

**Exact locations:**

- `tests/taxpayer_paths.rs:76–111`, especially the name `incorporation_tokens_are_open_wire_strings_in_both_nav_versions` at line 77 and the 2.0 XML at line 86.
- `taxpayer.rs:258–260,769–783`: `additionalAddressDetail` described/tested as simple-address detail on a taxpayer address.
- `fixtures/synthetic/agent/taxpayer_v3.xml:2–3,15–17`, consumed by `taxpayer.rs:665–670` and `tests/taxpayer_paths.rs:220–257`: genuine mixed namespaces, but missing required header/software, and the second detailed address lacks required street/category.

**Official evidence:**

1. [NAV 2.0 `TaxpayerDataType`][n2a-data] enumerates `taxpayerName`, `taxpayerShortName`, `taxNumberDetail`, `vatGroupMembership`, `taxpayerAddressList`; it has **no `incorporation`**. [NAV 3.0][n3a-data] declares `<xs:element name="incorporation" type="IncorporationType">` at line 1870. N1 §1.8.9.2 point 9 describes that 3.0 field.
2. [NAV 2.0][n2a-address] says `<xs:element name="taxpayerAddress" type="data:DetailedAddressType">`; [NAV 3.0][n3a-address] says the corresponding `type="base:DetailedAddressType"`. [N3B][n3b] declares `additionalAddressDetail` only inside **SimpleAddressType**, lines 265–302, not DetailedAddressType, lines 185–264. The detailed type requires `streetName` and `publicPlaceCategory`.

**Concrete impact:** the suite successfully exercises permissive behavior, but an implementer using these samples as a source-derived version matrix would incorrectly conclude that incorporation is part of NAV 2.0 or that the taxpayer operation has a documented simple-address alternative. The sparse 3.0 fixture is useful, but cannot establish conformance of a complete response. This weakens regression evidence rather than breaking today's taxpayer extraction. `fixtures/SOURCES.md:23–32` correctly calls these project-authored synthetic fixtures; the issue is the more specific test/field framing, not false provenance for the entire corpus.

**Minimal remedy:** keep the parser's tolerated cases, label them explicitly as extensions/sparse compatibility cases, and qualify `additional_address_detail` rustdoc as not declared by the current taxpayer schemas. Add a small complete source-shaped fixture/control for each version: omit incorporation in 2.0, include it in 3.0, use the real result/component namespaces, and include a complete detailed address plus one with only optional details omitted. Preserve separate sparse tests. No full XSD validator or removal of public fields is required.

**Reproduction/control:** scratch `all_fields_both_real_layouts_and_arbitrary_prefixes` passes with complete 2.0/3.0 envelopes, all declared business fields, all three address types and all detailed-address components. `version2_default_country_and_extension_policy_observations` separately proves the 2.0 incorporation extension is accepted. Existing permissive tests also pass; this finding does not misreport them as failing tests.

## 5. Fault extraction, success and sparse results

`QueryTaxpayer::parse` first calls the established shared response check (`taxpayer.rs:280–283`), then parses the root-selected NAV body. This review does not replace shared header/down/status precedence.

| Case | Current behavior | Assessment |
|---|---|---|
| Direct recognized `result/funcCode=OK`, validity true/1 | `Ok(TaxpayerInfo { valid: true, … })` | Correct. |
| `OK`, validity false/0, no data | Successful query with `valid=false`, absent fields/empty addresses | Exactly S3 invalid-number example; N1 point 1: “Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra.” (Invalid/nonexistent tax numbers return false.) |
| Missing/blank validity with OK | `ParseError::Missing("taxpayerValidity")` | Intentional fail-closed policy, not fabricated false. See §6. |
| Nonboolean nonblank validity | Parse error | Correct refusal of an unusable verdict. XML boolean space/tab/CR/LF forms work. Unicode trimming is more permissive than XSD lexical rules; no valid-response loss shown. |
| Non-OK with numeric Agent error | `ResponseError::Api`, typed numeric code, decoded message | S3's 57 example passes. Lines 613–623. |
| Non-OK with symbolic NAV error | Exact symbolic token retained as `ErrorCode::Unknown`, decoded message | Scratch covers real 2.0 and Common 3.0 result paths, not only a 2.0-shaped fixture. |
| Non-OK with code but no message | Raw code used as message | Preserves useful cause. |
| Non-OK with message but no code | `ErrorCode::Absent`, supplied message | No invented numeric code. |
| Non-OK with neither | `ErrorCode::Absent`, `NAV funcCode {value}` | Sparse failure stays failure; validity is not required. |
| Missing/foreign/nested funcCode | Parse error for missing real verdict | No descendant/local-name fallback. |
| Two result/funcCode/validity singletons | Parse error | Cannot overwrite error with success. Singleton checks include empty-first cases. |
| OK with sparse data or empty address item | Optional content/empty address retained | Intentional non-validating extraction. S3 already demonstrates incomplete inherited metadata. |
| Same address fields on different items | Independent addresses | Item repetition is exempt from singleton rejection; singleton checking is per parent. |
| Notifications on success/error | Original verdict preserved, notification content omitted | No promotion of notification text into `ApiError.message`; optional capability below. |
| Malformed whole document or unknown entity | Parse error | Old failure-to-success/truncation findings are fixed. |

The parser reads/validates recognized fields before `into_info`: a malformed boolean or duplicate recognized data on an ERROR body may produce a parse error rather than `ApiError`. That body is outside the declared lexical/singleton contract; no adversarial **valid** failure sample was found that loses its recognized code this way. Do not require failure short-circuiting before body integrity checking on that evidence alone.

## 6. Vendor ambiguities, optional capabilities and preserved deviations

### Vendor/version ambiguities — not confirmed defects

1. **Current forwarded NAV version and optional fields.** S3 still publishes 2.0 examples dated 2020-11-04 but links N1 3.0. Its prose capitalization `QueryTaxPayerResponse` does not match its actual XML/schema spelling. Support both genuine layouts as implemented; do not invent a request version switch or a second root spelling. Today's forwarding of every 3.0 field has not been observed live.
2. **Validity absent on an otherwise successful body.** Both N2A and N3A declare `taxpayerValidity` with `minOccurs="0"`, so a complete XSD-valid `OK` envelope can omit it. N1 point 1 and S3's negative example nevertheless specify explicit false for invalid/nonexistent numbers. README:250–252 explicitly chooses to require validity on OK. Scratch confirms the intentional refusal. This is a stricter public-result contract, not a new regression: retain it unless a deliberate tri-state API decision and vendor evidence justify changing it. Never silently turn absence into false.
3. **Direct NAV generic failures.** N1 §3.1.2 calls `GeneralErrorResponseType` the general fault of every direct NAV operation, and N3A defines that separate root (lines 638–660, 2046–2055). S3 says the Számla Agent response always has the taxpayer response type and illustrates an Agent error inside it. There is no primary forwarding evidence for direct `GeneralErrorResponse`/`GeneralExceptionResponse` or their HTTP status behavior through this wrapper. Current rejection is not a confirmed Agent bug; request a captured wrapper example before broadening roots or fault precedence.

### Optional capabilities — no required production remedy

- **NAV 3 Common notifications:** NC lines 640–700 call these “Miscellaneous notifications,” carrying `notificationCode` and `notificationText`. `taxpayer.rs:347,593–624` and `TaxpayerInfo` expose no channel for them. A valid success with a notification parses successfully but loses that diagnostic in the typed projection; a sparse error keeps its fallback message rather than adopting notification text. This is a real exposure omission, not a wrong verdict. Live forwarding/usefulness through Számla Agent is unverified. If wanted, add optional diagnostic records or an explicit raw-response path; do not turn notices into errors. N1 point 5 expressly leaves the manner/extent of client use discretionary: “A kliens oldalán diszkrecionális … milyen mértékben …”.
- **Header/software metadata and successful result message:** similarly unexposed, with no claim that the crate offers a complete NAV transport DTO. These can aid diagnostics but are not taxpayer identity/business-field gaps. Caller-held `RawResponse` remains available through the custom transport surface.
- **Strict schema/content validation:** not required for this task or current projection. Do not add mandatory names, addresses, incorporation or code-pattern checking merely because the XSD requires them in a complete direct NAV record.

### Explicitly preserved behavior

- **`info_date` is source text.** `taxpayer.rs:210–217`, README:371 and `tests/taxpayer_paths.rs:7–34` deliberately retain even malformed advisory text; empty/XML-blank becomes absent, NBSP does not. N1 defines last-change time, not lookup time, timezone normalization or cache expiry. Do not resurrect a datetime-validation finding.
- **Business text is decoded but not trimmed.** README:235–242 and the new tests establish this. `funcCode`, `errorCode` and boolean verdict parsing retain their separate trimming policy.
- **Sparse content stays optional.** True validity does not prove every name/address/code field is present. Requiring the XSD's complete inherited metadata would reject S3's own error/invalid examples.
- **NAV 2.0 empty country default is not materialized.** N2D:971 declares `<xs:element name="countryCode" type="CountryCodeType" default="HU">`. A schema-aware processor supplies HU for an empty-present element. The crate instead returns `None` at `taxpayer.rs:520–529`, consistently with its documented blank-business-text policy; scratch reproduces it. NAV 3's corresponding element has no default. This is an intentional non-validating semantic difference, not a reason to default every missing country to HU. If callers want XSD-default augmentation, make that a separate explicit capability/policy decision.
- **Tolerated extension fields remain accepted.** 2.0 incorporation and direct additional-address detail do not need to be removed to fix TQ-01; their claims/tests need qualification.
- **Open categories/errors stay open.** New incorporation values remain `Other(String)`; address kind and number components remain strings; symbolic NAV errors are not forced into the Számla Agent numeric catalogue.
- **No implicit taxpayer cache or worker projection expansion.** `CONTEXT.md:280–282` distinguishes the NAV lookup and worker-owned projection. Added client business fields do not require journal/CLI contract changes in this review.

## 7. Systematic coverage and fixes confirmed

### Existing automated coverage inspected

| Area | Existing evidence | Assessment |
|---|---|---|
| Writer/prefix/public serde | `taxpayer.rs:635–642,673–705`; prefix validation at 28–69 | Writer/golden and basic constructor routes covered. Scratch expands lexical/serde boundaries. |
| Official success, invalid, 57 failure | `tests/upstream.rs:966–1019` | Strong source-example assertions, including infoDate. Workspace-only corpus correctly kept out of published packages (`fixtures/SOURCES.md:3–32`). Scratch independently parses fresh examples. |
| NAV 3 common/base split | `tests/taxpayer_paths.rs:220–257`, synthetic fixture | Genuine namespaces now; the old fake “replace 2.0 with 3.0” concern is fixed. Fixture is sparse, TQ-01. |
| All five added fields/open incorporation/old JSON | `tests/taxpayer_paths.rs:7–111` | Strong path, text and compatibility coverage. 2.0 incorporation framing needs qualification. |
| Parent/namespace isolation | `tests/taxpayer_paths.rs:118–158,189–218`; taxpayer cases in `tests/response_namespaces.rs:78–123` | Unknown/foreign paths, duplicate containers/scalars, undefined entities and escaped namespaces covered. |
| Character fidelity and address isolation | `tests/taxpayer_paths.rs:160–174`; `taxpayer.rs:718–737` | Entities, CDATA, comments/PI, CR/NBSP/padding and optional detailed fields covered. |
| Sparse/errors | `tests/taxpayer_paths.rs:176–187`; `taxpayer.rs:795–840` | Existing focused sparse-error tests mainly 2.0; scratch verifies 3.0 counterparts/fallbacks. |
| Whole-document boundary | Taxpayer case in `tests/response_completion.rs:12–84,104–107` | Existing truncation/prolog/epilog coverage inspected. Scratch confirms truncation/second root in both versions. Shared implementation not re-audited here. |

Useful follow-up test coverage, distinct from a runtime finding: persist a source-shaped 3.0 ERROR fixture; assert every address optional in both layouts, all four booleans including `0`, sparse successful data, and all missing-error-code/message combinations. The scratch checks demonstrate those currently work. Avoid tests that merely repeat the implementation's namespace constants without comparing them to the declaring NAV schemas.

### Confirmed fixes at this HEAD

1. **Five business fields exposed:** short name, county code, VAT-group stem, incorporation and infoDate (`taxpayer.rs:194–217,580–587`). The old “all five omitted” finding is obsolete.
2. **Real 3.0 layout:** Common result and Base tax/address leaves (`323–383`), with appropriate API containers. Prefix spelling is not the selector.
3. **Path-scoped extraction:** unknown descendants and foreign names do not overwrite records (`339–395,429–454`).
4. **Duplicate singleton guard:** repeated result/funcCode/validity/data/tax/address scalars rejected per parent; repeated address items supported (`441–460`).
5. **Complete root required:** the operation calls `response_root` before extraction (`403–415`); old truncated/second-root acceptance does not reproduce.
6. **Text fidelity and undefined-entity handling:** decoded text/CDATA/references preserved and undeclared entities rejected (`473–505,520–531`).
7. **False validity, symbolic errors and sparse fault fallbacks preserved:** no absent-to-false fabrication or failure-to-success coercion (`593–624`).
8. **Provenance qualification updated:** `fixtures/SOURCES.md:172–180` now records the NAV 3.0 PDF link without relabeling the dated 2.0 examples. Its earlier empty-schema-heading statement is explicitly historical, not a current documentation defect.

### Execution record

1. `cargo test --manifest-path /tmp/opencode/taxpayer-audit-382cf76/Cargo.toml --tests`: initially **6 scratch checks + 10 existing taxpayer-path tests + 6 existing response-namespace tests passed**. The namespace test file includes other operations; those incidental passes are not an expanded audit claim.
2. After adding explicit both-version historical-defect controls, `cargo test --manifest-path /tmp/opencode/taxpayer-audit-382cf76/Cargo.toml --test audit --locked`: **7 scratch checks passed**.
3. `cargo test --locked -p szamlazz-agent --lib ops::taxpayer::tests --target-dir /tmp/opencode/taxpayer-audit-382cf76/target-workspace`: **15 unit tests passed**; no live tests selected.

The standalone scratch manifest compiles this checkout's source with its own resolved lockfile (quick-xml 0.42.0); the unit command uses the existing workspace lockfile. That workspace lockfile already had unrelated user changes at review start; it was not rewritten. These are offline behavior checks, not a claim to reproduce every historical dependency version at the pinned source commit.

The seven scratch tests cover:

- all freshly fetched S3 examples;
- complete source-shaped 2.0/3.0 envelopes and every business/detail field, three address kinds, optional omissions, leading-zero codes, arbitrary/misleading prefixes and escaped namespaces;
- numeric/symbolic/sparse faults in both versions and valid 3.0 notification-bearing success/error responses;
- all four booleans, XML whitespace, no-data true/false results, intentional missing-validity refusal, deliberately incomplete content separately labeled;
- NAV 2.0's empty-country default policy and extension acceptance;
- prefix constructor/serde/writer controls;
- both-version truncation, second-root, duplicate verdict, scalar child and nested-unknown-subtree controls.

The complete positive samples were constructed by tracing schema types/order/cardinalities, not by claiming the sparse project fixtures are schema-valid or running a full validator. Malformed/extension controls are distinguished from valid samples throughout. **No failing documented-response sample, P0/P1/P2 issue, or required production fix was established.**

[n3a]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n3b]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd
[nc]: https://github.com/nav-gov-hu/Common/blob/ab8d7887967492e5f6d6e25447be853fd767add8/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
[n2a]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n2d]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceData.xsd
[n2a-data]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1956-L1993
[n3a-data]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1846-L1889
[n2a-address]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1922-L1941
[n3a-address]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd#L1812-L1831
