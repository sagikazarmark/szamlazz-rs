# Számla Agent audit: taxpayer query and public operation inventory

Date: **2026-09-11**. Starting and inspected HEAD:
`77d53c553c9ecdc86d5fa72ca932c636256ae807`.

## Result and scope

**No confirmed taxpayer interoperability bug and no missing documented Agent
operation found.** All **11** independently enumerated multipart operations have
public request implementations, are reachable through `ops`, and can be sent by
the generic `Client::send`. All taxpayer business fields declared by the inspected
NAV 2.0/3.0 types have a projection in the current crate.

This is a whole-implementation review of `crates/szamlazz-agent`'s taxpayer
operation and its shared request/response path, plus a crate-wide public-operation
inventory; it is not a diff review or a field-by-field audit of other operations.
The latter belong to the other review scopes. The agent crate had no working-tree
modifications when inspected. Existing unrelated worker changes were present.
No source fixes, credential reads, `.env` access, live calls or vendor probes were
performed. This session had no subagent tool; this assigned scope was completed
directly, without subdelegation.

During the final check, concurrent work advanced HEAD to
`370ff2e5398e9ff4a3aff3b6008ab82e38b9d058`. A comparison with the starting commit
showed no changes to `crates/szamlazz-agent`, `Cargo.toml`, `Cargo.lock`,
`docs/szamlazz-hu-behaviour.md` or `docs/testing.md`; the audited code and cited
line numbers therefore still describe the requested baseline.

The request, XML, response, authentication, error-handling and operation-inventory
sources below were fetched afresh. Prior reviews were not used as evidence.
Read local policy first: `crates/szamlazz-agent/README.md`, especially lines
288–316, 354–358 and 480, and `docs/szamlazz-hu-behaviour.md`. The latter's
invoice/receipt observations do not establish taxpayer forwarding behavior.

### Findings classification

| ID | Classification | Result / impact |
|---|---|---|
| T-S1 | Source uncertainty | NAV defines generic error roots, but fresh Agent documentation promises `QueryTaxpayerResponse`; Agent forwarding of the generic roots is unestablished. They are not parsed as typed API errors. |
| T-M1 | Unsupported diagnostic metadata | Header/software metadata, successful result messages and NAV notifications are not exposed by `TaxpayerInfo`; no declared taxpayer business field is missing. |
| T-H1 | Optional lexical hardening | Unicode whitespace around verdict/code/validity is trimmed more broadly than XML Schema whitespace. An NBSP-padded boolean is accepted. |
| T-S2 | Documentation/source limitation | Current Agent examples remain NAV 2.0, dated 2020; NAV 3.0 schema compatibility is not a captured current Agent response. The example's request schema-location URL returns 404. |

None of these is promoted to a severity-ranked production bug without the missing
evidence. Reproductions below establish parser behavior, not vendor behavior.

## 1. Primary sources and what each establishes

All accessed on the review date. Agent pages displayed footer version
`v202608271632`.

| Ref | Source | Relevant quote / authority |
|---|---|---|
| A1 | [Taxpayer request](https://docs.szamlazz.hu/agent/querying_taxpayer/request) | “Base URL: `https://www.szamlazz.hu/szamla/`”, “Method: POST”, “Content type: `multipart/form-data`”, “Form field name: `action-szamla_agent_taxpayer`”. Owns the Agent transport contract. |
| A2 | [Taxpayer XML + XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml) | Root `xmltaxpayer`, namespace `http://www.szamlazz.hu/xmltaxpayer`; `beallitasok` then `torzsszam`; `length value="8"`, `pattern value="[0-9]{8}"`. Owns the Agent request shape, not NAV's direct request shape. |
| A3 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | “The response always matches the `QueryTaxPayerResponse` type … Last update for example responses: 2020-11-04.” Actual XML spells the root `QueryTaxpayerResponse`. Examples show success, `ERROR/57`, and `OK` with `taxpayerValidity=false`. |
| A4 | [Hungarian taxpayer response](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response) | “Sikertelenségnél hibakód és hibaüzenet is tartozik a válaszhoz.” (A failed response also has an error code and message.) Same 2.0 XML examples and linked NAV 3.0 PDF. |
| A5 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | “either an Agent key (recommended) or a username and password”; use `<szamlaagentkulcs>`. Legacy key-in-both-username-and-password is also documented. |
| A6 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | 57: “XML reading error”; XSD validation can cause it. Plain-text error format explicitly lists invoice create, invoice storno, credit entry and PDF query, not taxpayer query. “same request … at most five times.” |
| A7 | [Working taxpayer XSD download](https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd) | Same request namespace, sequence, three optional credential elements and eight-digit facets as A2. |
| A8 | [Agent index](https://docs.szamlazz.hu/agent/) and [taxpayer category](https://docs.szamlazz.hu/agent/category/querying-taxpayer) | Official sidebar enumerates the 11 operation categories. Each request page was followed independently (table below). |
| N1 | [NAV 3.0 interface PDF, linked by A3/A4](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf) | §1.8.9, printed pp. 63–69: domestic eight-digit lookup, returned business/address data and validity; §1.4.1, pp. 11–12: result meanings; §§3.1–3.2, pp. 162–166: NAV generic error contracts. Owns NAV semantics, not Agent forwarding. |
| N2 | [NAV 3.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | `QueryTaxpayerResponseType` lines 1552–1581; `TaxpayerAddressItemType`, list and data lines 1812–1889; `GeneralErrorResponse` lines 2046–2057. Fresh master resolved to this commit. |
| N3 | [NAV 3.0 invoiceBase.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd) | `DetailedAddressType` lines 185–264 and `TaxNumberType` lines 303–328. |
| N4 | [NAV 2.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | Result lines 654–705; taxpayer response lines 1668–1697; address/data lines 1922–1993; generic roots lines 2196 onward. |
| N5 | [NAV 2.0 invoiceData.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/API-2.0/src/schemas/nav/gov/hu/OSA/invoiceData.xsd) | Detailed address lines 965–1044; tax number lines 2225 onward. |
| N6 | [NAV Common 1.0 schema](https://github.com/nav-gov-hu/Common/blob/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd) | `BasicResponseType` lines 596–615, `BasicResultType` lines 616–647, notifications lines 668 onward, `GeneralExceptionResponse` lines 778–788. Common 2.0 on current main is not substituted for the imported NTCA 1.0 namespace. |
| W1 | [W3C XML Schema Datatypes](https://www.w3.org/TR/xmlschema-2/#boolean), [whiteSpace facet](https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace) | Boolean legal literals: “{true, false, 1, 0}”; whitespace replacement/collapse concerns `#x9`, `#xA`, `#xD`, `#x20`, not NBSP. |

The requested directory URL
`https://docs.szamlazz.hu/agent/querying_taxpayer/` returned 403 twice. The actual
category and all three requested child pages were successfully fetched. This did
not prevent reviewing their contents.

## 2. Independent operation inventory and public coverage

The following list starts from A8's navigation, not the crate's module list. Each
linked official **request** page's quoted “Form field name” matches the action in
the implementation. Code paths below are relative to
`crates/szamlazz-agent/src/`.

| Official operation / source | Form field name | Public request; implementation lines |
|---|---|---|
| [Generating invoice](https://docs.szamlazz.hu/agent/generating_invoice/request) | `action-xmlagentxmlfile` | `ops::invoice::CreateInvoice`; `ops/invoice.rs:678–680` |
| [Reversing invoice](https://docs.szamlazz.hu/agent/reversing_invoice/request) | `action-szamla_agent_st` | `ops::storno::StornoInvoice`; `ops/storno.rs:162–164` |
| [Registering credit entry](https://docs.szamlazz.hu/agent/credit_entry/request) | `action-szamla_agent_kifiz` | `ops::credit_entry::RegisterCreditEntry`; `ops/credit_entry.rs:274–276`; `ClearCreditEntries` shares it at 237–239 |
| [Query document PDF](https://docs.szamlazz.hu/agent/querying_pdf/request) | `action-szamla_agent_pdf` | `ops::query_pdf::QueryInvoicePdf`; `ops/query_pdf.rs:58–60` |
| [Query document XML](https://docs.szamlazz.hu/agent/querying_xml/request) | `action-szamla_agent_xml` | `ops::query_xml::QueryInvoiceXml`; `ops/query_xml.rs:540–542` |
| [Deleting proforma](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request) | `action-szamla_agent_dijbekero_torlese` | `ops::proforma::DeleteProforma`; `ops/proforma.rs:60–62` |
| [Generating receipt](https://docs.szamlazz.hu/agent/generating_receipt/request) | `action-szamla_agent_nyugta_create` | `ops::receipt::CreateReceipt`; `ops/receipt.rs:193–195` |
| [Reversing receipt](https://docs.szamlazz.hu/agent/reversing_receipt/request) | `action-szamla_agent_nyugta_storno` | `ops::receipt::StornoReceipt`; `ops/receipt.rs:344–346` |
| [Querying receipt](https://docs.szamlazz.hu/agent/querying_receipt/request) | `action-szamla_agent_nyugta_get` | `ops::receipt::QueryReceipt`; `ops/receipt.rs:424–426` |
| [Sending receipt](https://docs.szamlazz.hu/agent/sending_receipt/request) | `action-szamla_agent_nyugta_send` | `ops::receipt::SendReceipt`; `ops/receipt.rs:506–508` |
| [Querying taxpayer](https://docs.szamlazz.hu/agent/querying_taxpayer/request) | `action-szamla_agent_taxpayer` | `ops::taxpayer::QueryTaxpayer`; `ops/taxpayer.rs:264–266` |

Public reachability is complete:

- `ops.rs:34–42` exports all eight operation modules; receipt's four operations
  share a module. `envelope` and `waybill` are helpers, not omitted operations.
- `lib.rs:75–87` exposes `ops`, `wire`, and feature-gated `client`/`Client`.
  Requests do not need crate-root re-exports to be public.
- `client.rs:365–405` implements `send<R: AgentRequest>` without a closed
  operation dispatch list. Every row above participates through that trait.
- `wire.rs:14,353–411` supplies the endpoint, multipart file construction,
  validation and parser contract for all requests.
- A generating-invoice request explicitly covers the document variants;
  `InvoiceKind` contains invoice, proforma, delivery note, prepayment, final and
  corrective (`ops/invoice.rs:34–85`). They are not six missing endpoint methods.
  Preview and explicit credit-entry clearing are request modes, not additional
  vendor action names.
- No separate invoice-email-resend, incoming-invoice import, NAV token exchange
  or NAV invoice-reporting Agent operation appeared in this inventory. NAV's own
  operations, IPN, Adatkapcsolat and the PHP wrapper are distinct surfaces; their
  existence is not evidence of missing Agent operations.

This establishes operation coverage, not full conformance of every field in the
other ten operations.

## 3. Taxpayer request audit

| Concern | Code | Assessment against A1/A2/A5/A7 |
|---|---|---|
| Root / namespace | `ops/taxpayer.rs:268–278` | Correct `xmltaxpayer` and `http://www.szamlazz.hu/xmltaxpayer`. No NAV request root is sent. |
| Authentication location | `ops/taxpayer.rs:273–275`, `xml.rs:628–638` | Credentials inside `beallitasok`. Agent key emits only `szamlaagentkulcs`; username/password emits `felhasznalo`, then `jelszo`, as the XSD sequence specifies. |
| Credential values / escaping | `xml.rs:586–597,630–638` | Values are XML-escaped, not lowercased or normalized. A5's lowercase-key rule is caller credential correctness, not a missing request field. Legacy key-in-both fields is expressible with `Credentials::user_password`. |
| Tax number | `ops/taxpayer.rs:15–68,85–108,276` | Private-string `TaxpayerPrefix` enforces exactly eight ASCII digits on construction and deserialization. Leading zeroes survive. Full `NNNNNNNN-N-NN`, padding, non-ASCII digits and alphabetic input are refused. This matches the Agent schema; accepting a full number is not an Agent requirement. |
| Field order / request completeness | `ops/taxpayer.rs:273–276` | Required `beallitasok` then required `torzsszam`; all declared request paths are expressible. No `valaszVerzio`, NAV software/user/signature/header or NAV version selector belongs in this request. |
| Transport | `ops/taxpayer.rs:265`, `wire.rs:14,405–411`, `client.rs:374–405` | Correct action, POST, multipart file, endpoint and typed parsing. |
| `xsi:schemaLocation` | Writer vs A2 example | Omission is not a missing business/request field: both independently fetched XSDs validate the generated request without it. The schema location is a hint, not an authentication or routing selector. |

Offline execution generated both credential forms using escaped dummy strings and
the leading-zero stem `01234567`. Both requests validated independently against
the fresh inline XSD and the fresh download: **4/4 validations passed**. The
checked `to_wire` path also accepted both. No vendor acceptance claim follows
from XSD validation.

## 4. Response namespaces, result and error semantics

### Expanded names and parent paths

`ops/taxpayer.rs:323–397,400–517` selects a layout from the root namespace and
recognizes only its direct expected child paths. Prefix spellings are irrelevant.

| Element role | NAV 2.0 | NAV 3.0 | Evidence |
|---|---|---|---|
| `QueryTaxpayerResponse`, taxpayer business containers/leaves | `http://schemas.nav.gov.hu/OSA/2.0/api` | `http://schemas.nav.gov.hu/OSA/3.0/api` | A3; N2/N4 |
| Root child `result`, its `funcCode/errorCode/message` | OSA 2.0 `api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` | N4 basic response; N2 extends common response; N6 |
| Children of `taxNumberDetail` and `taxpayerAddress` | `http://schemas.nav.gov.hu/OSA/2.0/data` | `http://schemas.nav.gov.hu/OSA/3.0/base` | N2–N5 |

The component **container** remains in the API namespace even though its typed
children belong to data/base. The current parser gets this distinction right.
Foreign subtrees cannot re-enter a recognized path to provide business data or a
verdict. Unknown roots, undeclared prefixes, duplicate recognized singletons and
child elements in scalar fields are refused. Repeated address items are allowed
and kept independently (`ops/taxpayer.rs:427–470`).

### Verdict / validity / codes

- A3's `funcCode=OK` means the **query succeeded**, not that the tax number is
  valid. `into_info` requires a readable `taxpayerValidity` and returns it as
  `TaxpayerInfo.valid` (`ops/taxpayer.rs:592–610`). Both false and true remain data.
- N1 §1.8.9.2 says “Nem érvényes vagy nem létező adószámra false érték kerül
  visszaadásra” (an invalid or nonexistent tax number returns false). Therefore
  `valid=false` does not distinguish invalid from nonexistent. The implementation
  does not manufacture that distinction or discard accompanying business fields.
- N1 §1.4.1 says `errorCode` is returned when `funcCode` is `ERROR`, and describes
  `message` as optional explanatory text. N4/N6 define only `OK` and `ERROR`
  function-code tokens. The parser accepts only exact `OK` as success after its
  trimming policy; any other nonblank function code is an API error. Missing or
  blank `funcCode` fails parsing. This is conservative for new tokens, although
  the public error does not separately retain the function code when a message
  is supplied.
- A3's numeric `ERROR/57` becomes `ErrorCode::MalformedXml`; nonnumeric NAV codes
  are retained as `ErrorCode::Unknown(String)`. Missing/blank error code becomes
  `ErrorCode::Absent`, not a fabricated number. Missing message falls back to the
  raw code or `NAV funcCode …` (`ops/taxpayer.rs:612–622`). Known numeric spelling
  normalization is the shared `ErrorCode` policy (`error.rs:459–487`).
- N1 §3 explicitly keeps error codes out of schema enumerations so additions do
  not create client implementation dependencies. An open string fallback is
  appropriate. NAV code meanings must not be inferred from an Agent numeric-code
  table. Unknown codes remain unknown to the shared outcome classifier
  (`error.rs:376–382`); there is no automatic client retry loop.
- N2/N4 mark validity optional across the response type, which includes failure
  shapes. Refusing `OK` without validity is documented local policy
  (`README.md:315–316`), not proof of a missing successful variant. A3's successful
  and invalid-number examples both supply validity; N1's narrative specifies
  false for invalid/nonexistent numbers. No sourced successful omission requiring
  a different public representation was found.

### Generic transport/error precedence

`QueryTaxpayer::parse` calls `RawResponse::check` before body interpretation
(`ops/taxpayer.rs:281–284`). `wire.rs:251–310` applies:

1. nonblank `szlahu_down` → `ServiceUnavailable`;
2. nonblank `szlahu_error_code` → `Api` (including 56; taxpayer has no issuance
   exception);
3. known non-2xx status → `HttpStatus`;
4. otherwise the taxpayer body parser decides.

This matches the explicit README policy, rather than guessing who generated a
non-2xx body. The client supplies status and preserves incomplete-transfer
evidence separately (`client.rs:385–405`). A3 establishes body-only errors as
essential; the parser reads them without headers. A6 does not require adding
invoice `xmlszamlavalasz`, `sikeres/hibakod` or v1 plain-text parsing to taxpayer
responses.

### T-S1 — NAV generic errors: conditional interoperability gap, not proven Agent bug

N1 §§3.1–3.2 defines generic errors for direct NAV calls; its table includes
`GeneralExceptionResponse` / `INVALID_REQUEST` and `GeneralErrorResponse` /
`INVALID_SECURITY_USER`, `MAINTENANCE_MODE`, etc. N2 defines the latter in OSA
3.0 `api`; N6 defines the former in NTCA 1.0 `common` with `funcCode`, `errorCode`
and `message` directly under the root. N4 also defines both under OSA 2.0 `api`.

The crate accepts only `QueryTaxpayerResponse` roots
(`ops/taxpayer.rs:404–416`). A synthetic, well-formed common-namespace generic
exception with `ERROR/INVALID_REQUEST` produces:

```xml
<GeneralExceptionResponse xmlns="http://schemas.nav.gov.hu/NTCA/1.0/common">
  <funcCode>ERROR</funcCode>
  <errorCode>INVALID_REQUEST</errorCode>
  <message>bad request</message>
</GeneralExceptionResponse>
```

- no error/down header and HTTP 200: `ResponseError::Parse(UnexpectedBody(...))`;
- no error/down header and HTTP 400: `ResponseError::HttpStatus { status: 400, … }`.

Both outcomes were reproduced offline. If Agent forwards such a root, callers
lose typed code/message classification and receive only bounded diagnostic body
text. **But A3/A4 promise the operation-specific response and show Agent's own
57 wrapped in that root.** The NAV contract alone cannot establish that Agent
forwards direct NAV roots, headers or status codes. Obtain an Agent response
capture or explicit vendor confirmation before severity-ranking this as a defect.
The names `GeneralTechnicalException` in PDF prose and
`GeneralExceptionResponse` in the schema/table should also not be conflated into
an invented additional XML root.

## 5. Complete taxpayer business/address field coverage

All code references in this section are to `src/ops/taxpayer.rs`. Paths start
under the expected `QueryTaxpayerResponse`. Business strings intentionally remain
optional and are not revalidated against NAV length/pattern facets.

| Wire field / path | Public projection | Code lines | Source |
|---|---|---|---|
| `taxpayerValidity` | `valid: bool` | 189–190,567–578,598–600 | A3; N1/N2/N4 |
| `infoDate` | `info_date: Option<String>` | 209–216,584 | A3; N2:1560–1564; N4:1676–1680 |
| `taxpayerData/taxpayerName` | `name` | 191–192,579 | A3; N2/N4 taxpayer data |
| `taxpayerData/taxpayerShortName` | `short_name` | 193–196,580 | N2/N4 taxpayer data |
| `taxpayerData/taxNumberDetail/taxpayerId` | `tax_number` (stem, not full tax number) | 217–219,585 | A3; N3/N5 tax number |
| `…/taxNumberDetail/vatCode` | `vat_code` | 220–222,586 | A3; N3/N5 |
| `…/taxNumberDetail/countyCode` | `county_code` | 197–200,581 | N3/N5; retains leading zeroes |
| `taxpayerData/vatGroupMembership` | `vat_group_membership` | 201–205,582 | N2/N4; eight-digit group identifier, not a boolean |
| `taxpayerData/incorporation` | `incorporation: Option<Incorporation>` | 111–182,206–208,583 | N2; absent from N4's 2.0 declaration |
| `taxpayerData/taxpayerAddressList/taxpayerAddressItem` | `addresses: Vec<TaxpayerAddress>` | 223–224,457–459 | N2/N4; repeated list |
| `…/taxpayerAddressItem/taxpayerAddressType` | address `kind` | 231–232,541 | N1: HQ/SITE/BRANCH; open text |
| `…/taxpayerAddress/countryCode` | `country_code` | 233–234,542 | N3/N5 |
| `…/taxpayerAddress/region` | `region` | 235–236,543 | N3/N5 |
| `…/taxpayerAddress/postalCode` | `postal_code` | 237–238,544 | N3/N5 |
| `…/taxpayerAddress/city` | `city` | 239–240,545 | N3/N5 |
| `…/taxpayerAddress/streetName` | `street_name` | 241–242,546 | N3/N5 |
| `…/taxpayerAddress/publicPlaceCategory` | `public_place_category` | 243–244,547–549 | N3/N5 |
| `…/taxpayerAddress/number` | `number` | 245–246,550 | N3/N5 |
| `…/taxpayerAddress/building` | `building` | 247–248,551 | N3/N5 |
| `…/taxpayerAddress/staircase` | `staircase` | 249–250,552 | N3/N5 |
| `…/taxpayerAddress/floor` | `floor` | 251–252,553 | N3/N5 |
| `…/taxpayerAddress/door` | `door` | 253–254,554 | N3/N5 |
| `…/taxpayerAddress/lotNumber` | `lot_number` | 255–256,555 | N3/N5 |
| `…/taxpayerAddress/additionalAddressDetail` | `additional_address_detail` | 257–261,556–558 | Tolerated extension only: declared on NAV **simple** addresses, not taxpayer's **detailed** address type |

`incorporation` knows `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON` and preserves
new strings as `Other`. Accepting it in a 2.0 body is extension tolerance, not
proof that 2.0 declared it. Both address schemas use `DetailedAddressType`, so a
missing taxpayer `simpleAddress` alternative is not a feature gap.

`infoDate` is “Last date on which the data was changed” in N2/N4, not lookup time
or expiry. Its unvalidated source-text representation is explicitly documented
at lines 209–216 and README line 480. Retaining malformed advisory text, arbitrary
fraction precision or an absent timezone is deliberate; no datetime conversion
bug follows. Likewise missing business fields and empty lists are supported
sparse data, not invented defaults. The 2.0 XSD's `countryCode` default of `HU`
is not injected by this non-schema-validating reader.

### T-M1 — Unsupported metadata / projection choices

`TaxpayerResponse` explicitly reduces the upstream document to exposed fields
(`ops/taxpayer.rs:287–304`); the recognized-path table omits the following:

| Upstream metadata | Current behavior | Source |
|---|---|---|
| `header/requestId`, `timestamp`, `requestVersion`, optional `headerVersion` | Not exposed | A3 examples; N4 `BasicHeaderType`; N6:544 onward |
| `softwareId`, `softwareName`, `softwareOperation`, `softwareMainVersion`, `softwareDevName`, `softwareDevContact`, optional `softwareDevCountryCode`, `softwareDevTaxNumber` | Not exposed | A3 example; N2:1756–1811; N4 software type |
| `result/message` on success | Parsed internally, discarded by `into_info` | N1 §1.4.1 allows a message accompanying the function code; code 592–610 |
| `result/notifications/notification/{notificationCode,notificationText}` | Ignored | N6:640–645,668 onward; N1 calls these informational messages for future API use |
| Generic-error `technicalValidationMessages` | No typed projection; those roots are unsupported as discussed in T-S1 | N2:638–660; N1 §3.1.2 |

These can matter for support correlation and informational diagnostics, but no
evidence makes them necessary to compute the returned taxpayer validity or
business record. They are feature/projection choices, not dropped business data.
The caller can retain `RawResponse::body()` with a custom transport; `Client::send`
returns only the typed result. Exposing metadata through the worker or CLI is a
separate contract decision, as README line 480 already notes.

## 6. Lexical handling and hardening

- Shared `xml::response_root` checks UTF-8, expected expanded root name, completed
  structure through EOF, legal surrounding content and XML lexical validity
  (`xml.rs:201–300`). It refuses DTDs, extra roots, truncation, undefined entities,
  malformed ignored extensions and illegal XML characters.
- Taxpayer scalar accumulation decodes XML entities, CDATA and XML 1.0 line
  endings, keeping text across comments/processing instructions
  (`ops/taxpayer.rs:472–505`). No numeric parsing or byte-offset date slicing
  occurs in business fields.
- Optional business strings preserve nonblank decoded characters and padding.
  Only absent/empty/XML-whitespace-only content becomes `None`; NBSP-only text
  remains present (`ops/taxpayer.rs:519–529`). This matches README lines 288–295.
- Boolean validity supports `true`, `false`, `1`, `0`; other nonblank tokens fail.
  Missing/empty validity cannot become an invented false on `OK`.
- Duplicate recognized singletons, even empty-then-populated, and nested scalar
  elements fail; address item repetition remains valid.
- Schema sequence, required business fields, string length/patterns and unknown
  tokens are intentionally not fully XSD-validated on response. Sparse records
  remain usable. This is not proof that malformed business values satisfy NAV's
  schema.

### T-H1 — Unicode trimming is broader than XML whitespace

`ops/taxpayer.rs:521–522` uses `str::trim()` for `funcCode`, `errorCode` and
`taxpayerValidity`. Consequently this minimal synthetic body parses as valid:

```xml
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
  <result><funcCode>OK</funcCode></result>
  <taxpayerValidity>&#160;true&#160;</taxpayerValidity>
</QueryTaxpayerResponse>
```

The offline check confirmed `Ok(info)` with `info.valid == true`. W1's XML Schema
whitespace collapse does not strip NBSP. The parser also accepts padded `OK`
although function-code schema restrictions derive from strings. The README
describes a tolerant reader, not a full XSD validator, and no observed/documented
Agent response was incorrectly interpreted here. Classify as **optional
hardening**, not a demonstrated interoperability bug. If a stricter verdict
lexical contract is desired, it should be explicit and accompanied by tests for
XML whitespace versus other Unicode whitespace. Business-text preservation
should remain a separate decision.

## 7. T-S2 — Fresh-source limitations

1. A3 explicitly dates examples to 2020-11-04; all three remain OSA 2.0. The linked
   NAV 3.0 PDF and N2/N3 establish 3.0 names/types. The current parser supports
   both correctly. They do not establish which version Agent presently returns
   for each account/error path. Synthetic 3.0 coverage is labeled as such.
2. A3's success example includes `infoDate`, but not short name, county code, VAT
   group membership or incorporation. N2–N5 declare the applicable fields and
   current code projects them. Their frequency/current forwarding by Agent is
   not established by the examples or by offline checks.
3. A2's sample `schemaLocation` points at
   `http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd`; its HTTPS counterpart
   returned **404**. The analogous `/szamla/docs/xsds/agent/` path also returned
   404. The independently fetched working download is A7 under `/taxpayer/`.
   This is a documentation link limitation; the crate emits no such broken hint.
4. N1's prose spells a notification path without the plural wrapper in its table;
   N6 declares `notifications/notification`. Neither proves Agent currently
   forwards those messages. Schema paths are the authority for NAV XML structure.

## 8. Verification record

Executed offline against current crate code:

```sh
cargo test -p szamlazz-agent --locked --offline --lib ops::taxpayer::tests
cargo test -p szamlazz-agent --locked --offline --test taxpayer_paths
cargo test -p szamlazz-agent --locked --offline --test response_completion --test response_namespaces --test response_headers --test business_text
cargo test -p szamlazz-agent --locked --offline --test upstream responses::every_response_example_parses_through_its_operation
```

**57 tests passed**: 15 taxpayer unit, 10 taxpayer path, 4 response completion,
11 namespace, 14 header, 2 business-text, 1 upstream corpus test. These are
targeted checks; some shared tests exercise other operations too. No ignored
live/probe test was selected.

Additional scratch verification under `/tmp/opencode`:

- `taxpayer-77d53c5-extract.py` uses Python's HTML parser to extract `<pre>` text
  from the freshly downloaded A2/A3 HTML. No response XML repairs were made.
- `taxpayer-77d53c5-check.rs`, compiled against the library reported by fresh
  `cargo build -p szamlazz-agent --locked --offline --message-format=json`, parses
  the three **fresh** A3 examples: valid record (including stem and `infoDate`),
  `ERROR/57`, and `OK/false`: **3/3 passed**. This is distinct from merely passing
  checked-in fixtures.
- The same scratch executable exercises invalid prefix boundaries, generates
  escaped dummy credentials in both forms with a leading-zero stem, reproduces
  generic exception behavior at 200/400 and the NBSP boolean behavior.
- `xmllint --nonet --noout --schema` via `nix shell nixpkgs#libxml2` validated both
  generated requests against both independent fresh request schemas: **4/4**.
  Response schemas were inspected, not claimed to have been compiled/validated.
- The fresh N1 PDF was converted with `pdftotext -layout` via
  `nix shell nixpkgs#poppler-utils`; quoted page numbers are printed PDF pages.

### Acquisition fingerprints

SHA-256 of downloaded/decoded artifacts (temporary files use prefix
`/tmp/opencode/taxpayer-77d53c5-`):

| Artifact | SHA-256 |
|---|---|
| N1 `nav3.pdf` | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| N2 `nav3-api.xsd` | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| N3 `nav3-base.xsd` | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| N4 `nav2-api.xsd` | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| N5 `nav2-data.xsd` | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |
| N6 `common1.xsd` | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| A7 `request-download.xsd` | `51fe8565301b0f3a67199b3b3d666fd6abb5acebd5db4cd81c3a479cae816ed7` |
| Fresh A3 success `response-0.xml` | `20ba9328e2047b0918e9a8e612bff84f5c3a2379c55bf23cddb18da54aaf1919` |
| Fresh A3 failure `response-1.xml` | `fe65bc9b4af7276aa5b2c753589bd6184225db4ba08619f01046add000fd3bc0` |
| Fresh A3 invalid number `response-2.xml` | `9fa150d457a5b53071befca3c618f3f631bdeaddf49ed95c3bf48a622f767400` |

Temporary artifacts support reproducibility during this session; this report's
URLs, source excerpts and code references are the durable review record.
