# Számla Agent taxpayer API review — 2026-09-11

**Reviewed HEAD:** `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`.

**Result: one P3 shared XML namespace-conformance finding; no confirmed taxpayer-specific request, field-extraction or verdict defect.** Every taxpayer business/address field declared by the examined NAV 2.0 and 3.0 schemas is exposed. Several transport/diagnostic fields are intentionally projected out. Those omissions, the documented sparse-content policy and questions about current vendor forwarding are distinguished below from the finding.

## 1. Scope and method

- Read `crates/szamlazz-agent/src/ops/taxpayer.rs`, its shared XML, wire, credential and error boundaries, public README, taxpayer/path/upstream tests and fixture provenance, and `docs/szamlazz-hu-behaviour.md`.
- This is a review of the **current implementation against primary sources**, not a diff review against an earlier report. Previous `docs/review` conclusions and line numbers are historical; recent fixes were checked afresh.
- Two independent reviewers ran **in parallel**, one on schema/contract coverage and one on implementation/parser behavior. Neither delegated further. Their results: schema reviewer found no actionable contract defect; parser reviewer found the P3 below. The lead independently reproduced it and ran the full crate suite.
- The reviewed agent sources, tests, fixtures, README and behavior document matched HEAD; unrelated working-tree work was present. Only this report was added to the repository. Acquisition, schema-validation and reproduction artifacts are under `/tmp/opencode/taxpayer-2ba5-review/`.
- Public documentation/schema GETs only; no authenticated Számla Agent requests or live-account tests. “Full suite” below means the full **`szamlazz-agent`** suite, including its reqwest feature and doctests.

**Line notation:** `T` = `crates/szamlazz-agent/src/ops/taxpayer.rs`; `xml.rs`, `wire.rs`, `error.rs` = files in that crate's `src/`; `README` = its README. All implementation lines refer to the pinned HEAD.

## 2. Fresh primary sources and navigation

Retrieved **2026-09-11**. Navigation was discovered from `https://docs.szamlazz.hu/` → `/agent/` → **`/agent/category/querying-taxpayer`** → **`/agent/querying_taxpayer/request`**, **`/response`**, **`/xml`**. The category uses a hyphen; operation pages use an underscore. The current navigation calls the combined page **XML + XSD**; a guessed standalone `/xsd` path is not the current navigation contract.

The vendor pages display build **`v202608271632`**. The response page expressly dates its examples to **2020-11-04**. Neither today's retrieval nor the site build turns those examples into a current account observation.

| ID | Primary source | Evidence used |
|---|---|---|
| S0 | [Querying taxpayer category](https://docs.szamlazz.hu/agent/category/querying-taxpayer) | Actual operation navigation. |
| S1 | [Request](https://docs.szamlazz.hu/agent/querying_taxpayer/request) | Multipart POST target, file action, NAV as the information source. |
| S2 | [XML + XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml) | Official request example and inline request XSD, required order and eight-digit restriction. |
| S3 | [Response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | Success, code-57 failure and invalid-number examples; links the NAV PDF, §1.8.9. Prose spells the type `QueryTaxPayerResponse`; actual examples and schemas use `QueryTaxpayerResponse`. |
| S4 | [Downloadable request XSD](https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd) | Fresh HTTP 200, same request structure/facets as S2. |
| S5 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Agent key or username/password, key preferred; user restricted to one billing account. |
| S6 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Agent code meanings and retry advice; taxpayer is not in its listed plain-text-error operations. |
| N1 | [Vendor-linked NAV 3.0 specification](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf) | §1.8.9, printed pp.63–69 (PDF pp.73–79), taxpayer semantics; inherited metadata pp.10–12; generic errors §3, pp.162–168. |
| N2A | [NAV 2.0 invoiceApi.xsd][n2a] | `BasicResponseType` 654–705; `QueryTaxpayerResponseType` 1668–1697; taxpayer/address types 1922–1993. |
| N2D | [NAV 2.0 invoiceData.xsd][n2d] | `DetailedAddressType` 965–1044; `SimpleAddressType` 1987 onward; `TaxNumberType` 2225 onward. |
| N3A | [NAV 3.0 invoiceApi.xsd][n3a] | Inheritance 548–564; response 1552–1581; software and taxpayer/address types 1756–1889. |
| N3B | [NAV 3.0 invoiceBase.xsd][n3b] | Detailed address 185–264; separate simple address 265–302; tax number 303–328. |
| NC | [NAV Common 1.0 common.xsd][nc] | Function code 449–468; header/response/result 544–647; notifications 668–701. |
| X1 | [Namespaces in XML 1.0, Third Edition](https://www.w3.org/TR/2009/REC-xml-names-20091208/) | Normalized URI identity §2.3; reserved prefixes §3; NCName constraints §7; processor requirements §8. |

N1 identifies NAV's official [Online-Invoice](https://github.com/nav-gov-hu/Online-Invoice) and [Common](https://github.com/nav-gov-hu/Common) repositories. GitHub branches/tags/trees were freshly queried:

- Online-Invoice `master`: **`cc7a775d6dce361311e409abb9934eb755f2749c`**, NAV 3.0.
- Online-Invoice tag `API-2.0`: **`84442e64bc2cd7feb368fedb8199645188962b23`**.
- Common tag `common-1.0.0`: **`ab8d7887967492e5f6d6e25447be853fd767add8`**. Current Common `main` contains **NTCA 2.0**, which is not the namespace imported by NAV 3.0. Using current Common indiscriminately would audit the wrong result layout.

The request example's schema-location URL, `https://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd`, freshly returned **404**. S4 works. Omission of `xsi:schemaLocation` by the crate is not a request defect: it is a schema-processing hint, not a required business element.

### Evidence identity

`sources.json` in the scratch directory records retrieval time, URLs, status, sizes and SHA-256. Selected downloaded-byte hashes:

| Artifact | SHA-256 |
|---|---|
| N1 PDF, 6,496,304 bytes | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N2A | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N2D | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |
| N3A | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N3B | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| NC | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| S4 | `51fe8565301b0f3a67199b3b3d666fd6abb5acebd5db4cd81c3a479cae816ed7` |

## 3. Ranked actionable finding

### TQ-01 — P3: shared XML validation accepts namespace-forbidden markup

**Current locations:** `xml.rs:58–68`, `xml.rs:417–427`; reached by taxpayer at **T:404–418** and again at **T:423–424**. README **253–268** promises structural/namespace checking and tolerance of *well-formed* unknown extensions.

Two namespace-malformed inputs nevertheless return successful taxpayer data:

```xml
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
  <result><funcCode>OK</funcCode></result>
  <taxpayerValidity>true</taxpayerValidity>
  <xmlns:extension/>
</QueryTaxpayerResponse>
```

Replace `<xmlns:extension/>` with `<?p:target data?>` for the second case. Both were reproduced under **both NAV layouts**, using Common `result` for 3.0.

Minimal public entry point:

```rust
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, RawResponse};

// `body` is either XML above, as bytes.
let raw = RawResponse::new::<&str, &str>([], body.to_vec());
let info = QueryTaxpayer::new("12345678").unwrap().parse(&raw).unwrap();
assert!(info.valid); // Current behavior: malformed namespace markup was accepted.
```

**Why it is a defect:** X1 §3 says “Element names MUST NOT have the prefix `xmlns`.” The namespace resolver supplies the implicit binding, and `read_resolved_event` checks only `ResolveResult::Unknown`; it never rejects this forbidden element prefix. X1 §7 requires PI targets to be NCNames, with no colon. `valid_pi_target` deliberately checks the broader XML `Name`, and `xml_name_start` includes `:`. X1 §8 requires namespace-well-formedness violations to be reported. This is distinct from declining to enforce XSD business constraints.

**Impact/rank:** low-priority conformance gap in a shared helper, observable through this operation. The reproduced markup is ignored and does **not** inject a verdict or taxpayer field. No vendor occurrence, wrong taxpayer identity or authentication bypass is established; this is not P1/P2. A conforming XML consumer may reject the same body the crate reports as successful.

**Action:** reject element QNames whose prefix is exactly `xmlns`; validate PI targets as NCNames while retaining the existing reserved `xml` target rule. Add focused shared-validator regressions, including legal non-ASCII PI names, ordinary `xml:*` elements and legal bound prefixes merely beginning with `xml` (X1 expressly says the latter must not be fatal solely for that spelling). Keep namespace normalization before binding/attribute checks.

**Executed reproduction:**

```sh
cargo run --manifest-path /tmp/opencode/taxpayer-2ba5-review/probe/Cargo.toml \
  --offline --bin namespace_gaps
```

Output: NAV 2 and NAV 3 `reserved-element` and `colon-pi` each yielded `Ok(TaxpayerInfo { valid: true, … })`. The independent parser review also reproduced the paired `xmlns:` element case. The local ID TQ-01 belongs to this report; it is not a status update to an identically numbered historical finding.

## 4. Request inventory

| Contract | Current implementation | Assessment |
|---|---|---|
| POST `https://www.szamlazz.hu/szamla/`, multipart XML file named `action-szamla_agent_taxpayer` (S1) | T:264–266; `wire.rs:14,66–99` | Matches. No direct NAV HTTP request is constructed. |
| Root `{http://www.szamlazz.hu/xmltaxpayer}xmltaxpayer`, qualified children, UTF-8 XML | T:268–278; `xml.rs:132–154` | Matches. |
| Required `beallitasok`, then required `torzsszam` | T:273–276 | Correct order. |
| Credentials: `felhasznalo`, `jelszo`, `szamlaagentkulcs`, each optional in XSD, authentication required by prose | T:274; `xml.rs:603–612`; `credentials.rs:53–79` | Emits key or username/password in schema order. Both generated request forms passed fresh S4 XSD validation. |
| `torzsszam`: string, length 8, `[0-9]{8}` | T:27–32,36–67,97–108 | Exactly eight ASCII digits on constructor, conversion and serde paths; no public unchecked string field. |
| Leading zeroes/checksum | String retained, T:18,39–41,57–60 | `01234567` and `00000000` accepted syntactically. No check-digit validation is required by S2/S4. Existence is the query's answer. |
| Full Hungarian tax number or padded input | Refused, including `12345678-2-02`, leading/trailing spaces, Unicode digits, signs | Correct for this operation. The worker's full-number convenience input is a separate contract. |

No wrapper field selects NAV version, response version, country, pagination or an as-of date. Direct NAV `header`, `user`, `software`, hashes/signature and `user/predecessorTaxNumber` belong to the intermediary's NAV request, not `xmltaxpayer`. N1 says predecessor tax number has no further effect on this operation after validation; that does not create a wrapper input.

## 5. Root, namespaces and path-specific extraction

Namespace identity follows **the element declaration**, not the namespace of its imported type. All namespace names below retain `http://`; downloading an XSD over HTTPS does not change XML identity.

| Recognized path | NAV 2.0 namespace | NAV 3.0 namespace | Implementation |
|---|---|---|---|
| `QueryTaxpayerResponse` | `http://schemas.nav.gov.hu/OSA/2.0/api` | `http://schemas.nav.gov.hu/OSA/3.0/api` | T:404–416 selects exact root/URI pair. |
| `result`, `result/{funcCode,errorCode,message}` | 2.0 API | `http://schemas.nav.gov.hu/NTCA/1.0/common` | T:323–348. |
| Root `infoDate`, `taxpayerValidity`, `taxpayerData` | 2.0 API | 3.0 API | T:343–347. |
| `taxpayerData/{taxpayerName,taxpayerShortName,incorporation,vatGroupMembership,taxNumberDetail,taxpayerAddressList}` | 2.0 API; incorporation tolerated extension | 3.0 API | T:349–358. |
| `taxNumberDetail/{taxpayerId,vatCode,countyCode}` | `http://schemas.nav.gov.hu/OSA/2.0/data` | `http://schemas.nav.gov.hu/OSA/3.0/base` | T:359–363. |
| `taxpayerAddressList/taxpayerAddressItem/{taxpayerAddressType,taxpayerAddress}` | 2.0 API | 3.0 API | T:364–365. |
| Direct `taxpayerAddress/*` components | 2.0 Data | 3.0 Base | T:366–383. |

NAV 2.0 response inheritance is API `QueryTaxpayerResponseType` → API `BasicResponseType`; NAV 3.0 is API `QueryTaxpayerResponseType` → API `BasicOnlineInvoiceResponseType` → Common `BasicResponseType`. Consequently 3.0 `header/result` are Common, but `software` and taxpayer business containers are API. The current layout correctly implements that split.

The parser is **not a global local-name search**. `Layout::child` returns unknown for an unrecognized parent/namespace; unknown frames stay unknown down the subtree (T:306–307,340–397,435–439). Thus a foreign `result`, a familiar field under an extension, an API-namespace `taxpayerId`, or a name at the wrong parent cannot populate output. Prefix aliases, redeclarations and XML-character-reference-normalized URI spellings work. Case changes, HTTPS substitutions and percent-encoding changes are not namespace aliases (X1 §2.3).

Per-parent singleton tracking rejects recognized duplicate leaves and containers, including empty-first forms and aliases; only address items repeat (T:440–447). Scalar child markup is refused rather than flattened (T:428–433). Each recognized address item starts a fresh row; document order and field isolation are retained (T:457–459,535–559). Unknown interleaved elements do not reorder or create rows.

Whole-document validation precedes extraction: UTF-8, expected root, completion, balanced closes, legal prolog/epilog, no second root, undefined entities and XML lexical errors even in ignored content (`xml.rs:176–275`). Namespace bindings are normalized and expanded attribute uniqueness checked (`xml.rs:73–127`). TQ-01 is the identified residual gap, not evidence that these recently fixed checks are absent.

## 6. Returned business-field inventory

Cardinality describes the XSD **when the enclosing container exists**, not the crate's sparse-content policy. Business text uses `Option<String>` and preserves nonblank decoded source characters. No integer/decimal coercion loses leading zeroes.

| Wire field | Source contract | Public field / current mapping |
|---|---|---|
| Root `taxpayerValidity` | Optional `xs:boolean` in both schemas; N1 requires false for invalid/nonexistent numbers | `valid: bool`, T:189–190,567–577,598–600; required locally for OK. |
| Root `infoDate` | Optional `xs:dateTime`; last data change, not request time | `info_date`, T:209–216,584. |
| `taxpayerData/taxpayerName` | Required full name | `name`, T:191–192,579. |
| `taxpayerData/taxpayerShortName` | Optional short name | `short_name`, T:193–196,580. |
| `taxpayerData/taxNumberDetail/taxpayerId` | Required eight-digit stem; group identifier in group taxation | `tax_number`, T:217–219,585; not an assembled hyphenated number. |
| `…/vatCode` | Optional one-digit taxation code | `vat_code`, T:220–222,586. |
| `…/countyCode` | Optional two-digit county code | `county_code`, T:197–200,581. |
| `taxpayerData/incorporation` | Required in NAV **3.0 only**; ORGANIZATION / SELF_EMPLOYED / TAXABLE_PERSON | `incorporation`, T:111–181,206–208,583; open `Other(String)`. |
| `taxpayerData/vatGroupMembership` | Optional eight-digit VAT group stem | `vat_group_membership`, T:201–205,582; string, not boolean. |
| `taxpayerData/taxpayerAddressList` | Optional; one or more items if present | `addresses`, T:223–224,457–459; absent/empty list becomes empty vector. |

Every address is `taxpayerData/taxpayerAddressList/taxpayerAddressItem`; its components are under its direct `taxpayerAddress` child:

| Wire field | XSD | Public field; mapping |
|---|---|---|
| `taxpayerAddressType` | Required HQ / SITE / BRANCH | `kind`; T:231–232,541; open string. |
| `countryCode` | Required ISO alpha-2; NAV 2.0 alone declares default HU | `country_code`; T:233–234,542. |
| `region` | Optional province/region code | `region`; T:235–236,543. |
| `postalCode` | Required; string | `postal_code`; T:237–238,544. |
| `city` | Required | `city`; T:239–240,545. |
| `streetName` | Required | `street_name`; T:241–242,546. |
| `publicPlaceCategory` | Required | `public_place_category`; T:243–244,547–549. |
| `number` | Optional house number | `number`; T:245–246,550. |
| `building` | Optional | `building`; T:247–248,551. |
| `staircase` | Optional | `staircase`; T:249–250,552. |
| `floor` | Optional | `floor`; T:251–252,553. |
| `door` | Optional | `door`; T:253–254,554. |
| `lotNumber` | Optional | `lot_number`; T:255–256,555. |
| `additionalAddressDetail` | **Not a member of taxpayer DetailedAddressType** | `additional_address_detail`; T:257–261,556–558; explicitly tolerated extension. |

**No declared taxpayer business/address field is missing.** The complete XSD-valid scratch examples cover every declared field and three independently populated addresses (HQ, SITE, BRANCH) in each version. NAV 2.0 correctly omits incorporation. The parser does not confuse taxpayer `DetailedAddressType` with the separate `AddressType` choice of simple/detailed address.

### Optional and inherited metadata inventory

| Returned metadata | Declaration | Projection and justification |
|---|---|---|
| `header/{requestId,timestamp,requestVersion,headerVersion?}` | N2A:596–626; NC:544–575; header required by base response | Omitted. Intermediary exchange metadata, not taxpayer identity or validity. Timestamp is not `infoDate`. |
| `software/{softwareId,softwareName,softwareOperation,softwareMainVersion,softwareDevName,softwareDevContact}` | N2A:1866 onward; N3A:1756–1797; required fields/container | Omitted. Describes the billing software, not the taxpayer. |
| `software/{softwareDevCountryCode?,softwareDevTaxNumber?}` | N3A:1798–1809, corresponding N2A fields | Omitted optional developer metadata; not the queried taxpayer's country/tax number. |
| `result/funcCode` | Required OK/ERROR | Consumed as verdict, not exposed separately on `TaxpayerInfo`. |
| `result/errorCode?`, `result/message?` | Optional strings, N2A:680–705; NC:616–647 | Used for non-OK errors; not retained on successful output. Success human-readable message is diagnostic. |
| NAV 3.0 `result/notifications?/notification+/{notificationCode,notificationText}` | NC:640–645,668–701; both notification leaves required | Omitted on success and failure. Informational messages, not a second validity or error verdict. No declared 2.0 equivalent. |
| Generic-error `technicalValidationMessages` | Separate NAV error response, N1 §3.1.2; N3A generic-error type | Not a `QueryTaxpayerResponse` business field; wrapper forwarding unresolved. |

The projection is explicit at T:287–304 and in the public output T:188–225. Omitted diagnostics are a capability boundary, not unimplemented taxpayer data. A caller driving the sans-I/O API can retain `RawResponse::body()` (`wire.rs:221–225`); `TaxpayerInfo` has no raw XML/notification channel, and `Client::send` does not return a raw exchange beside it. Exposing diagnostic metadata should be a consumer-driven API decision. N1 §1.8.9.2 explicitly leaves how much returned information to use to the client.

## 7. Verdict and error semantics

`QueryTaxpayer::parse` first calls `RawResponse::check` (T:281–283). The documented priority is nonblank `szlahu_down` → nonblank error-code header → known non-2xx HTTP status → body (`wire.rs:262–310`; README:310). A caller-created raw response without status delegates to header/body interpretation. Body parsing does not override a known HTTP failure.

| Body / exchange | Current result | Assessment |
|---|---|---|
| `funcCode=OK`, validity true/1 | `Ok`, `valid=true` | Matches existing/valid taxpayer semantics. |
| `funcCode=OK`, validity false/0 | `Ok`, `valid=false` | Matches S3 and N1; not a missing-document or API error. |
| `OK`, absent/empty/XML-blank validity | `ParseError::Missing("taxpayerValidity")` | Documented stronger result contract; see unresolved question U1. |
| Missing/blank `funcCode` | Missing-field parse error | Does not invent success. |
| Invalid nonblank boolean, e.g. `TRUE`, `yes`, `2` | `ParseError::Invalid` | No permissive false fallback. |
| `ERROR`, code 57 | `ApiError`, `ErrorCode::MalformedXml` | Correctly parses the official malformed-prefix example. |
| `ERROR`, symbolic/unknown numeric code | `ApiError`, `ErrorCode::Unknown(token)` | Keeps open NAV codes. |
| Non-OK function code not yet known | `ApiError`, using supplied code/message or fallbacks | Not silently treated as success; original funcCode survives only in the final fallback message when both diagnostics are absent. |
| Non-OK, absent code | `ErrorCode::Absent` | Not invented code 0. |
| Non-OK, absent message | Raw code as message, or `NAV funcCode {code}` if code also absent | T:612–622. |
| `szlahu_down` | `ResponseError::ServiceUnavailable` | Not a NAV code or false validity. |
| Non-2xx, no prior error header | `ResponseError::HttpStatus` | Keeps transport fact; does not invent Agent credentials rejection. |

Codes are separately trimmed (T:521–522; `error.rs:455–469`): known numeric forms such as `003` classify as code 3; unknown/oversized/symbolic codes retain trimmed spelling. Nonblank business/error-message text is decoded and preserved. Verdict trimming uses Rust Unicode whitespace, a broader tolerance than XML Schema's whitespace facet; it is not strict XSD lexical validation. An OK result with contradictory optional error text/code still follows funcCode; no primary source establishes that such a contradiction is a supported second verdict.

Agent codes **3/135/136/164** are credential-related (`error.rs:333–349`), confirmed by S5/S6, and were exercised as body errors in both layouts plus header precedence. Direct NAV `INVALID_SECURITY_USER` remains an unknown NAV token, **not** an Agent-key rejection: those credentials are the intermediary's, not necessarily the caller's. N1's direct-NAV generic-error roots/statuses do not establish how the wrapper relays them.

The body parser validates the entire recognized content before converting the verdict. Nested diagnostic markup, duplicate message elements or an invalid boolean can therefore make even an ERROR body a parse failure. This follows the documented taxpayer scalar/lexical policy (README:265–270); no supported vendor exception was established. Do not apply the invoice issuance envelope's numbered-evidence rules to taxpayer lookup.

`OutcomeClass` describes operation-dependent exchange recovery, not taxpayer validity. A symbolic NAV failure being classified Unknown does not mean a taxpayer document might have been issued: this operation is read-only. No client retry loop is introduced here; S6's repeated-request limits still apply to callers.

## 8. Supported deviations and forward compatibility

1. **Sparse business content is deliberate.** README:244–270 distinguishes XML checking from XSD business validation. Required-in-XSD name, tax-number and address children remain optional in output; empty address rows and lists are tolerated. The fresh vendor code-57 and invalid-number examples themselves omit required `software`: full XSD validation fails them, while the crate correctly reads their intended verdicts. Adding full XSD validation to production would regress documented examples.
2. **Source text, not normalization.** Optional business strings preserve decoded padding and NBSP; absent/empty/XML-whitespace-only values become `None` (T:519–529). Text, CDATA, entities and line endings are decoded (T:472–504). Identifiers and open tokens are not numerically parsed or trimmed as business data. The NAV 2.0 `countryCode` default HU is not injected for an empty element; this reader does not perform schema-default augmentation.
3. **`infoDate` is advisory source text.** T:209–216 and README:409 explicitly retain malformed nonblank text, arbitrary precision, offsets and absence of timezone. This field is `xs:dateTime`, not the stricter NAV header timestamp type. It is neither lookup time nor TTL, and cannot establish cache freshness.
4. **Extensions are not alternate schemas.** `additionalAddressDetail` is documented as tolerated, not a taxpayer simple-address branch. Incorporation in a 2.0 response is likewise an extension; tests now say so (`tests/taxpayer_paths.rs:77–88`). The sparse 3.0 fixture is labeled as sparse rather than a complete conformance example.
5. **Open business tokens.** `Incorporation::Other(String)` round-trips as the exact string; address kinds remain strings. Unknown business fields/subtrees are ignored, not retained for re-emission. New optional fields default to `None` when reading old JSON (README:409; T:195–216); serde accepts additional response JSON fields. `#[non_exhaustive]` alone is not wire openness.
6. **Version support is bounded.** Genuine 2.0 and 3.0 expanded-name layouts work; arbitrary future namespaces do not. A new root/version or an unknown funcCode cannot become a successful old result by local-name matching. That conservative behavior is appropriate; no version-negotiation field exists in the wrapper request.
7. **Behavior notes do not establish taxpayer exceptions.** `docs/szamlazz-hu-behaviour.md:3–28,137–145,255–261` describes invoice-related observations and explicitly unobserved credential cases, not taxpayer captures. It supplies no support for plain-text taxpayer failures, alternate roots, a relaxed prefix, or current optional-field forwarding. README:409 correctly labels the five newer fields as not captured live through the current wrapper.

## 9. Unresolved questions and optional follow-ups

These are ranked **follow-up priorities, not confirmed defects**.

| ID / priority | Question and reproducible boundary | Needed evidence / action |
|---|---|---|
| U1 / medium | Both XSDs make validity optional, but T:598–600 requires it on OK. Removing it from a supported envelope produces `Missing("taxpayerValidity")`; N1 prose and S3 instead define false for invalid/nonexistent numbers. | Ask whether the wrapper intentionally emits OK without validity and what it means. If supported, decide on an explicit indeterminate result; never default missing validity to false. Current README documents the refusal. |
| U2 / medium | Does the wrapper ever forward `GeneralErrorResponse` or `GeneralExceptionResponse` unchanged? At 200 with no error header these fail the current taxpayer-root check; at non-2xx status takes precedence. | Vendor clarification or separately authorized captured failure. Direct NAV schemas alone do not contradict S3's wrapper promise. Do not add generic roots based only on a direct-NAV example. |
| U3 / low | Which NAV version and optional fields does szamlazz.hu forward today? S3 retains 2020 examples; N1 is 3.0; parser supports both. | Request updated official examples/captures, including incorporation, county, short name, group stem, infoDate and notifications. Current source compatibility is established; contemporary forwarding is not. |
| U4 / low, optional enhancement | Do consumers need exchange IDs, successful result messages or notifications? They are omitted, including from a notification-bearing XSD-valid 3.0 response. | Identify an actual diagnostic consumer before adding a response wrapper/raw channel. No missing business-field fix is required. |
| U5 / low, vendor documentation | Request example schema-location link returns 404; failure/invalid examples omit software despite “always matches” schema wording. | Ask vendor to fix the link and qualify/update examples. Keep sparse parsing and the working schema source. |

## 10. Verification and reproducibility

### Full crate suite — lead

```sh
cargo test -p szamlazz-agent --all-features
```

**Passed:** 265 unit/integration tests plus 8 doctests (**273 passed, 0 failed**). **Four live tests ignored**, including `taxpayer_query`; none was enabled. The all-features run used the repository lockfile. It includes 15 taxpayer unit tests, all 10 `taxpayer_paths` tests, upstream fixtures, shared namespace/completion tests and loopback HTTP-client tests.

### Fresh source-shaped checks — lead

Scratch files:

- `fetch.py`, `sources.json`: fresh vendor HTML, extracted three response examples/request example, request XSD, linked NAV PDF and commit-pinned schemas.
- `prepare.py`: PDF text extraction and full XSD validation using scratch-installed pypdf/lxml. Imports without schemaLocation were wired to the fetched local schemas for validation; original downloaded schemas remain intact. `complete-v2.xml` and `complete-v3.xml` are synthetic **XSD-valid** full envelopes, not vendor captures.
- `probe/src/main.rs`: public request/response entry-point checks against current agent source. Both credential writers, prefix constructors/serde refusals, leading zeroes, multipart action, all freshly extracted vendor responses, every taxpayer/address field, three address items, metadata projection, normalized namespaces, lexical verdicts, open/absent errors, credential classification, duplicate/completion refusals and header precedence passed.
- `validate-written.py`: both emitted credential request forms passed the fresh downloadable request XSD.
- `probe/src/bin/namespace_gaps.rs`: independent lead reproduction of TQ-01 under both layouts.
- `schema-findings.md`, `parser-findings.md`, `parser-probe/`: independent reviewer conclusions and additional targeted controls, including unknown-subtree re-entry, alias duplicates and decoded text.

Commands after source acquisition:

```sh
python3 /tmp/opencode/taxpayer-2ba5-review/prepare.py
cargo run --manifest-path /tmp/opencode/taxpayer-2ba5-review/probe/Cargo.toml \
  --offline --bin taxpayer-head-review-probe
python3 /tmp/opencode/taxpayer-2ba5-review/validate-written.py
cargo run --manifest-path /tmp/opencode/taxpayer-2ba5-review/probe/Cargo.toml \
  --offline --bin namespace_gaps
```

The scratch crate has its own dependency resolution; it uses the same quick-xml **0.42.0** and xmlparser **0.13.6** as the workspace. An initial scratch negative control accidentally replaced a `c:` QName in a generated 2.0 document rendered with `a:`; that unchanged-input assertion was corrected before drawing conclusions. Final semantic controls passed; TQ-01 controls deliberately assert the currently observed malformed acceptance. The suite's green result does not negate the reproduced conformance gap.

**Disposition:** schedule TQ-01 as a small shared XML hardening fix. No request widening, taxpayer business-field addition, mandatory business-content validation or error-root expansion is justified by this review's evidence.

[n2a]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n2d]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceData.xsd
[n3a]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[n3b]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd
[nc]: https://github.com/nav-gov-hu/Common/blob/ab8d7887967492e5f6d6e25447be853fd767add8/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
