# Számla Agent review: taxpayer query and independent operation inventory

**Date:** 2026-09-11. **Starting and inspected commit:**
`28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

## Summary

**No confirmed taxpayer interoperability defect found.** The current implementation
covers all business fields and detailed-address components declared for taxpayer
responses in the inspected NAV 2.0 and 3.0 schemas. All **11 mainstream Agent
actions** in the current official action table have public implementations.

**The wider documentation inventory contains a twelfth action:**
`action-agent_ceg_mb`, for delegated company-account creation / a join request to
an existing company. It appears on the sitemap-listed `/agent/self_billing` page
and has a current technical reference under third-party invoicing. There is no
built-in request for it. This is an established capability exclusion, consistent
with the project's third-party-invoicing non-goal, rather than a taxpayer bug.
An unqualified claim that the documentation contains only eleven actions would
be incorrect.

| ID | Assessment | Severity / confidence | Impact |
|---|---|---|---|
| O-1 | `action-agent_ceg_mb` is documented but not implemented | Informational scope gap; **high** | A consumer needs its own request implementation or another integration for delegated account onboarding. All eleven mainstream actions are covered. |
| T-1 | Typed taxpayer results omit exchange metadata, successful messages and notifications | Low diagnostic limitation; **high** for projection behavior; current notification forwarding **unestablished** | `Client::send` cannot provide lossless NAV diagnostics or request correlation from a successful typed result. No declared taxpayer business field is missing. |
| T-2 | Direct NAV generic error roots are not interpreted as typed API errors | Conditional interoperability limitation; production severity **not established**; **high** for parser behavior | If Agent forwards such roots, code/message classification is lost to a parse or HTTP-status error. Agent documentation instead promises the taxpayer wrapper. |
| T-3 | Verdict/boolean trimming accepts non-XML Unicode whitespace | Low, optional lexical hardening; **high** offline confidence | An NBSP-padded boolean is accepted although it is not an XML Schema boolean lexical value after XML whitespace collapse. No incorrect vendor response was observed. |
| T-4 | Fresh Agent examples, linked schemas and current forwarding leave evidence gaps | Documentation/evidence limitation; **high** | Examples are still dated 2020 and use 2.0; 3.0 forwarding, successful missing validity, and optional-field frequency cannot be inferred from passing synthetic tests. |

No high/medium-severity production defect is established by this review. The
limitations above are not evidence that a dependency outage means an invalid
tax number, nor permission to retry document writes.

## Scope, method and evidence boundary

This reviews the **current implementation**, not only changes since the starting
commit: `src/ops/taxpayer.rs`, relevant shared XML/wire/client/error handling,
taxpayer tests and local policy, plus an independent inventory of whole Agent
actions. Other operations receive an action-coverage check, not a field-level
conformance review. Code citations below are current line numbers; unless
qualified otherwise, paths are relative to `crates/szamlazz-agent/`.

Fresh acquisition covered the Agent index, official action table, all eleven
request pages, EN/HU taxpayer response pages, request XSD, authentication/error
guidance, and all **72 English `/agent/` URLs in the current sitemap** for action
discovery. The sitemap crawl matters: the self-billing page is absent from the
main operation sidebar. HTML text extraction found **12 unique `action-*`
tokens**, independently of the crate's module list. The current third-party
reference corroborates the extra action.

The source chain for NAV is explicitly bounded: **Agent taxpayer response page
→ linked NAV 3.0 interface PDF → its §7.3.1/§7.3.2 first-party schema repositories**.
The 2.0 repository branch explains the version in Agent's examples. Common 1.0 is
the namespace imported by NAV 3.0; unrelated current Common versions are not
substituted. No unrelated direct NAV operation is counted as an Agent action.

`docs/szamlazz-hu-behaviour.md:1–47` establishes the dates/account limitations of
the invoice, credit-clearing and receipt observations. Its header observations
are operation-specific, not taxpayer captures. The taxpayer smoke at
`tests/live.rs:25–40` checks `valid`, nonblank name and tax number; it does not
assert raw namespaces, optional metadata or failure forwarding. The existence of
this test is not proof of execution. No taxpayer forwarding capture was found in
the consulted behavior record or dated live research notes. No authenticated
execution, credential access or new live observation was performed here.

The existing untracked `2026-09-11-agent-api-77d53c5-taxpayer.md` was consulted as
a lead. Its claims were checked against fresh sources and current code; its
eleven-action conclusion needs the O-1 qualification. Prior reports were
preserved. No subagents or product edits were used. Broad cargo verification is
left to the parent run; this report records only its own targeted checks.

## 1. Primary source register

All sources below were accessed on the review date. Agent pages displayed footer
build **`v202608271632`**. NAV 3.0 repository `master` resolved to
`cc7a775d6dce361311e409abb9934eb755f2749c`; `API-2.0` resolved to
`84442e64bc2cd7feb368fedb8199645188962b23`.

| Ref | Primary URL / location | What it establishes |
|---|---|---|
| A1 | [Taxpayer request](https://docs.szamlazz.hu/agent/querying_taxpayer/request) | Agent endpoint, POST/multipart file and `action-szamla_agent_taxpayer`; data originates from NAV. |
| A2 | [Taxpayer XML + XSD](https://docs.szamlazz.hu/agent/querying_taxpayer/xml) | Agent request namespace, element order, credentials, eight-digit stem. |
| A3 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | “The response always matches the `QueryTaxPayerResponse` type”; examples last updated 2020-11-04. Actual XML root spelling is `QueryTaxpayerResponse`. Shows success, wrapped Agent `ERROR/57`, and successful invalid-number `false`. Links N1. |
| A4 | [Hungarian taxpayer response](https://docs.szamlazz.hu/hu/agent/querying_taxpayer/response) | Same examples and NAV link; failed requests have a code and message. No additional generic-root promise. |
| A5 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Agent key or username/password; legacy key in both username/password fields is also supported. |
| A6 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Agent numeric codes; plain-text v1 error list names create/storno/credit/PDF, not taxpayer. No endless automatic retry. |
| A7 | [Downloadable taxpayer request XSD](https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd) | Independently fetched request schema agrees with A2's request shape. |
| A8 | [Agent index](https://docs.szamlazz.hu/agent/), [what is Agent](https://docs.szamlazz.hu/agent/basics/what-is), [official action table](https://docs.szamlazz.hu/agent/basics/sending-requests), [sitemap](https://docs.szamlazz.hu/sitemap.xml) | Eleven mainstream actions; sitemap additionally exposes `/agent/self_billing`. |
| A9 | [Self-billing under Agent](https://docs.szamlazz.hu/agent/self_billing) | Additional `action-agent_ceg_mb` form, `XmlCegMb` request and XSD; creating an account or sending a join request shares this action. |
| A10 | [Current delegated-account request reference](https://docs.szamlazz.hu/third-party-invoicing/szamla-agent/request) | Corroborates A9's action/endpoint/root, documents onboarding fields and newer credential optionality. |
| N1 | [Agent-linked NAV 3.0 interface PDF](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf) | Printed pp. 63–69, §1.8.9 taxpayer semantics/fields; pp. 11–12 result semantics; pp. 162–166 generic errors; pp. 209–210 first-party repository links. Direct NAV contract, not a capture of Agent forwarding. |
| N2 | [NAV 3.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | Response inheritance 548–565; generic error 638–660; taxpayer response 1552–1581; software/business/address list 1756–1889. |
| N3 | [NAV 3.0 invoiceBase.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd) | Detailed address 185–264; simple address 265–302; tax number 303–328. |
| N4 | [NAV 2.0 invoiceApi.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd) | Header/result 596–705; generic error 778–795; taxpayer response 1668–1697; business/address list 1922–1993. |
| N5 | [NAV 2.0 invoiceData.xsd](https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceData.xsd) | Detailed address 965–1044; tax number 2225–2250. |
| N6 | [NAV Common 1.0 common.xsd](https://github.com/nav-gov-hu/Common/blob/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd) | Header/result 544–647; notifications 668–701; technical validation 702–727; generic roots 766–788. |
| X1 | [W3C XML Schema boolean](https://www.w3.org/TR/xmlschema-2/#boolean), [whitespace facet](https://www.w3.org/TR/xmlschema-2/#rf-whiteSpace) | Lexical comparison for T-3: `true/false/1/0`; whitespace collapse does not strip NBSP. |

## 2. Independent complete action inventory

Each operation link below is its freshly fetched official request page. All
mainstream requests use POST/multipart to `https://www.szamlazz.hu/szamla/`.

| Operation | Multipart action | Public implementation and current lines |
|---|---|---|
| [Generate invoice](https://docs.szamlazz.hu/agent/generating_invoice/request) | `action-xmlagentxmlfile` | `ops::invoice::CreateInvoice`, `src/ops/invoice.rs:678–680` |
| [Reverse invoice](https://docs.szamlazz.hu/agent/reversing_invoice/request) | `action-szamla_agent_st` | `ops::storno::StornoInvoice`, `src/ops/storno.rs:162–164` |
| [Register credit entry](https://docs.szamlazz.hu/agent/credit_entry/request) | `action-szamla_agent_kifiz` | `ops::credit_entry::RegisterCreditEntry`, `src/ops/credit_entry.rs:274–276`; explicit `ClearCreditEntries` shares it at 237–239 |
| [Query PDF](https://docs.szamlazz.hu/agent/querying_pdf/request) | `action-szamla_agent_pdf` | `ops::query_pdf::QueryInvoicePdf`, `src/ops/query_pdf.rs:58–60` |
| [Query XML](https://docs.szamlazz.hu/agent/querying_xml/request) | `action-szamla_agent_xml` | `ops::query_xml::QueryInvoiceXml`, `src/ops/query_xml.rs:540–542` |
| [Delete proforma](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request) | `action-szamla_agent_dijbekero_torlese` | `ops::proforma::DeleteProforma`, `src/ops/proforma.rs:60–62` |
| [Generate receipt](https://docs.szamlazz.hu/agent/generating_receipt/request) | `action-szamla_agent_nyugta_create` | `ops::receipt::CreateReceipt`, `src/ops/receipt.rs:193–195` |
| [Reverse receipt](https://docs.szamlazz.hu/agent/reversing_receipt/request) | `action-szamla_agent_nyugta_storno` | `ops::receipt::StornoReceipt`, `src/ops/receipt.rs:344–346` |
| [Query receipt](https://docs.szamlazz.hu/agent/querying_receipt/request) | `action-szamla_agent_nyugta_get` | `ops::receipt::QueryReceipt`, `src/ops/receipt.rs:424–426` |
| [Send receipt](https://docs.szamlazz.hu/agent/sending_receipt/request) | `action-szamla_agent_nyugta_send` | `ops::receipt::SendReceipt`, `src/ops/receipt.rs:506–508` |
| [Query taxpayer](https://docs.szamlazz.hu/agent/querying_taxpayer/request) | `action-szamla_agent_taxpayer` | `ops::taxpayer::QueryTaxpayer`, `src/ops/taxpayer.rs:264–266` |
| [Create delegated company account / join existing account](https://docs.szamlazz.hu/agent/self_billing#request) | `action-agent_ceg_mb` | **Absent** from built-in operations; current reference A10 confirms the action. O-1. |

Public reachability is established by `src/ops.rs:34–42`, `src/lib.rs:75–88`, the
public `AgentRequest` trait (`src/wire.rs:353–411`) and generic
`Client::send<R: AgentRequest>` (`src/client.rs:365–405`). There is no closed
client dispatch list that prevents any implemented action from being sent.

**O-1 disposition:** `git grep` found no `action-agent_ceg_mb` or `XmlCegMb` in the
crate. A9/A10 describe a whole operation, not a flag on invoice creation. It is
not in the eleven-action table, but it is within the requested documentation
tree and cannot be omitted from a complete inventory. `CONTEXT.md:146–147`
explicitly excludes third-party invoicing. Consequently this is a **high-confidence
missing capability relative to the wider vendor surface**, not an unsolicited
requirement to add it or a release blocker for the existing scope. A consumer
can implement `AgentRequest` itself; that is extensibility, not built-in support.
The current A10 accepts an Agent key for **onboarding the connection**; that must
not be confused with issuing as the dedicated delegate user afterward.

Other apparent operations are correctly accounted for:

- Ordinary, proforma, delivery-note, prepayment, final and corrective creation
  share the invoice action (`src/ops/invoice.rs:34–85`; A8 explicitly says no
  separate proforma/delivery-note endpoint). Preview is another request mode.
- Clearing credit entries reuses the credit-entry action; it is not action 13.
- `attachfile1`–`attachfile5` are invoice multipart attachments, not actions.
- A9 uses the same onboarding XML for creating an account and requesting a join;
  the UI approval, disconnect and login-triggered notification are not further
  documented Agent multipart action names.
- The crawl found no separate invoice-email-resend, incoming-invoice import,
  taxpayer batch, direct NAV token exchange or direct NAV invoice-reporting
  action. IPN and Adatkapcsolat remain separate integration surfaces. This is a
  bounded statement about the fetched documentation, not proof that no private
  or undocumented vendor endpoint exists.

## 3. Complete taxpayer request inventory

Sources: A1/A2/A5/A7. `R` = schema-required; `O` = schema-optional. Authentication
semantics additionally require one of the documented credential alternatives.

| Wire path / concern | Requirement | Current implementation / result |
|---|---|---|
| `xmltaxpayer` | Root, namespace `http://www.szamlazz.hu/xmltaxpayer` | Correct, `src/ops/taxpayer.rs:268–278` |
| `beallitasok` | R, first root child | Correct, 273–275 |
| `beallitasok/felhasznalo` | O, username alternative | Shared writer, `src/xml.rs:628–638` |
| `beallitasok/jelszo` | O, password alternative; after username | Same writer; XML-escaped |
| `beallitasok/szamlaagentkulcs` | O, preferred Agent key | Same writer; key alternative emits only this element |
| `torzsszam` | R, second root child; length 8 and `[0-9]{8}` | `TaxpayerPrefix` enforces eight ASCII digits, preserving zeroes, on construction and serde decode: `src/ops/taxpayer.rs:15–68,85–108,276` |
| Action/file/endpoint | A1 | Correct action at 265; multipart `to_wire` at `src/wire.rs:405–411`; client POST at `src/client.rs:374–383` |

No documented taxpayer request field is missing. Full tax numbers, whitespace,
alphabetic/non-ASCII digits and wrong lengths are refused locally; a valid prefix
does not validate its checksum or assert NAV existence. The Agent XSD does not
require a checksum check. Legacy key-in-both-fields is expressible through the
username/password credential alternative. XML escaping occurs at
`src/xml.rs:586–597`; checked `to_wire` additionally validates XML characters.

`valaszVerzio`, direct NAV `header/user/software`, password hashes,
`requestSignature`, `predecessorTaxNumber` and a NAV version selector are **not
fields of Agent's request**. Direct NAV's request contract cannot justify adding
them. An `xsi:schemaLocation` is a schema hint, not a missing business field; both
fresh request schemas validate the generated XML without it.

## 4. Response layout and complete field inventory

### Namespaces and parent paths

The reader selects the layout from the **expanded root name**, not prefixes or
`header/requestVersion` (`src/ops/taxpayer.rs:323–397,403–417`).

| Role | NAV 2.0 namespace | NAV 3.0 namespace |
|---|---|---|
| Root, `infoDate`, `taxpayerValidity`, `taxpayerData` and its API-declared children/containers | `http://schemas.nav.gov.hu/OSA/2.0/api` | `http://schemas.nav.gov.hu/OSA/3.0/api` |
| Root `header`, `result`, and their children | OSA 2.0 `api` | `http://schemas.nav.gov.hu/NTCA/1.0/common` |
| `software` and its children | OSA 2.0 `api` | OSA 3.0 `api` |
| Children **inside** `taxNumberDetail` and `taxpayerAddress` | `http://schemas.nav.gov.hu/OSA/2.0/data` | `http://schemas.nav.gov.hu/OSA/3.0/base` |

The `taxNumberDetail` and `taxpayerAddress` **containers** remain API elements;
their types supply data/base children. The current reader gets this distinction
right. Foreign or unknown subtrees cannot re-enter a recognized path. Duplicate
recognized singleton containers/leaves, including empty-then-populated copies,
and child elements in scalar fields are refused; repeated address items are
allowed and remain independent (`src/ops/taxpayer.rs:427–470`).

### Business fields

`R` below is NAV schema-required **if its enclosing optional container exists**;
`O` means `minOccurs=0`. These are schema facts, not assertions that Agent always
forwards them. Agent example presence is shown separately. The crate deliberately
accepts sparse business content (`README.md:288–316`). All line references in
this table are to `src/ops/taxpayer.rs`.

| Wire path under `QueryTaxpayerResponse` | NAV 2 / 3 | Public field; declaration / assignment | Agent example evidence |
|---|---|---|---|
| `infoDate` | O / O, `xs:dateTime` | `info_date: Option<String>`; 209–216 / 584 | Success example |
| `taxpayerValidity` | O / O, `xs:boolean` | `valid: bool`; 189–190 / 567–578,598–600 | Explicit true and false; **required by crate on OK** |
| `taxpayerData` | O / O | Flattened business fields, 188–225 | Present on success, absent on example false/error |
| `taxpayerData/taxpayerName` | R / R | `name`; 191–192 / 579 | Present |
| `taxpayerData/taxpayerShortName` | O / O | `short_name`; 193–196 / 580 | Not shown |
| `taxpayerData/taxNumberDetail` | R / R | Flattened tax-number components | Present |
| `…/taxNumberDetail/taxpayerId` | R / R, eight digits | `tax_number`; 217–219 / 585 | Present; **stem**, not assembled full number |
| `…/taxNumberDetail/vatCode` | O / O, one digit `[1-5]` | `vat_code`; 220–222 / 586 | Present |
| `…/taxNumberDetail/countyCode` | O / O, two digits | `county_code`; 197–200 / 581 | Not shown; leading zeroes retained |
| `taxpayerData/incorporation` | Not declared / R | `incorporation: Option<Incorporation>`; 111–182,206–208 / 583 | Not shown in 2.0 examples |
| `taxpayerData/vatGroupMembership` | O / O, eight-digit id | `vat_group_membership`; 201–205 / 582 | Not shown; identifier, not boolean |
| `taxpayerData/taxpayerAddressList` | O / O | `addresses: Vec<_>`; 223–224 | Present on success |
| `…/taxpayerAddressList/taxpayerAddressItem` | 1..unbounded / same | One address per item; 457–459 | One HQ row shown |
| `…/taxpayerAddressItem/taxpayerAddressType` | R / R; HQ, SITE, BRANCH | `kind: Option<String>`; 231–232 / 541 | HQ shown; other/new tokens retained |
| `…/taxpayerAddressItem/taxpayerAddress` | R / R, **DetailedAddressType** | Flattened address fields; 230–262 | Present |
| `…/taxpayerAddress/countryCode` | R / R | `country_code`; 233–234 / 542 | HU shown; 2.0 schema declares default HU, reader does not inject it |
| `…/taxpayerAddress/region` | O / O | `region`; 235–236 / 543 | Not shown |
| `…/taxpayerAddress/postalCode` | R / R | `postal_code`; 237–238 / 544 | Present |
| `…/taxpayerAddress/city` | R / R | `city`; 239–240 / 545 | Present |
| `…/taxpayerAddress/streetName` | R / R | `street_name`; 241–242 / 546 | Present |
| `…/taxpayerAddress/publicPlaceCategory` | R / R | `public_place_category`; 243–244 / 547–549 | Present |
| `…/taxpayerAddress/number` | O / O | `number`; 245–246 / 550 | Present |
| `…/taxpayerAddress/building` | O / O | `building`; 247–248 / 551 | Not shown |
| `…/taxpayerAddress/staircase` | O / O | `staircase`; 249–250 / 552 | Not shown |
| `…/taxpayerAddress/floor` | O / O | `floor`; 251–252 / 553 | Not shown |
| `…/taxpayerAddress/door` | O / O | `door`; 253–254 / 554 | Not shown |
| `…/taxpayerAddress/lotNumber` | O / O | `lot_number`; 255–256 / 555 | Not shown |
| `…/taxpayerAddress/additionalAddressDetail` | Not declared on detailed addresses in either version | Tolerated extension `additional_address_detail`; 257–261 / 556–558 | Not shown; not a promised taxpayer field |

Sources: N2:1552–1581,1812–1889; N3:185–328; N4:1668–1697,1922–1993;
N5:965–1044,2225–2250; N1 §1.8.9; A3/A4.

Important representation decisions:

- `incorporation` maps `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`, preserving
  unknown strings as `Other`. Acceptance in 2.0 is extension tolerance, not a 2.0
  declaration. No case-folding or trimming is applied to this business token.
- `infoDate` means **last data change**, not lookup time, TTL or expiry. The crate
  explicitly stores decoded advisory source text, including malformed/non-zoned
  text and arbitrary fractional precision. That is not a failed datetime parse.
- Optional strings preserve nonblank decoded characters, padding and NBSP;
  empty/XML-whitespace-only becomes `None` (`519–529`). Tax numbers, country codes
  and address facets are not revalidated as though this were a full XSD validator.
- List absence and an empty list both become `[]`; an empty item becomes one
  empty address. Required business fields can be absent without inventing values.
- Both taxpayer schemas use **DetailedAddressType directly**, not the general
  `AddressType` choice. There is no established missing `simpleAddress` alternative.
  `additionalAddressDetail` belongs to `SimpleAddressType` (N3:265–302); its current
  rustdoc correctly labels its taxpayer support as an extension.

### Result, envelope and potential error-only fields — T-1

This completes the response inventory beyond taxpayer business content.
`TaxpayerResponse` describes itself as a reduced projection
(`src/ops/taxpayer.rs:287–304`); the allowlist at 340–397 and conversion at 592–622
confirm these omissions.

| Field / structure | NAV declaration | Agent evidence and current behavior |
|---|---|---|
| `header` | R in both response bases | Shown in all Agent examples; ignored |
| `header/requestId` | R | Shown; ignored, so no typed request correlation |
| `header/timestamp` | R | Shown; ignored |
| `header/requestVersion` | R | Shown as 2.0; ignored (layout comes from root namespace) |
| `header/headerVersion` | O | Not shown; ignored |
| `result` | R | Recognized in version-appropriate namespace |
| `result/funcCode` | R, `OK` or `ERROR` | Required nonblank parser verdict; exact OK after trim succeeds; original token not separately exposed |
| `result/errorCode` | O in XSD; N1 says supplied for ERROR | Parsed; known Agent numeric code or open string; absent → `ErrorCode::Absent`; ignored on OK |
| `result/message` | O | Error message retained/fallback described below; success message discarded |
| `result/notifications` | O, NAV 3 Common 1.0; not declared in NAV 2 result | Not shown by Agent; ignored |
| `…/notifications/notification` | 1..unbounded when wrapper exists | Ignored |
| `…/notification/notificationCode` | R | Ignored |
| `…/notification/notificationText` | R | Ignored |
| `software` | R in both NAV response bases | Shown on success but omitted in Agent error/false examples; ignored |
| `software/softwareId` | R | Shown; ignored |
| `software/softwareName` | R | Shown; ignored |
| `software/softwareOperation` | R | Shown; ignored |
| `software/softwareMainVersion` | R | Shown; ignored |
| `software/softwareDevName` | R | Shown; ignored |
| `software/softwareDevContact` | R | Shown; ignored |
| `software/softwareDevCountryCode` | O | Not shown; ignored |
| `software/softwareDevTaxNumber` | O | Not shown; ignored |
| Generic `GeneralErrorResponse/technicalValidationMessages` | 0..unbounded error-only records; N2:652, N4:786 | **Not a QueryTaxpayerResponse child**; direct NAV possibility only; root unsupported |
| `…/technicalValidationMessages/validationResultCode` | R, ERROR/CRITICAL | No typed projection; N6:702–727 / N4 technical-validation type |
| `…/technicalValidationMessages/validationErrorCode` | O | No typed projection |
| `…/technicalValidationMessages/message` | O | No typed projection |
| Generic `GeneralExceptionResponse` | Extends result directly; no `result` wrapper | Direct NAV only; root unsupported; its funcCode/errorCode/message and 3.0 notifications follow the result types above |
| Common `GeneralErrorHeaderResponse` | Common 1.0 generic header+result root, N6:766–777 | Exists in imported common schema, but N1's Online Invoice error table does not establish its use as the taxpayer error root; no Agent evidence; unsupported |

Header/software facts: N4:596–705, N2:548–565,1756–1811, N6:544–647. Notifications:
N6:668–701. N1 §1.4.1 calls notifications future informational messages; the PDF
table abbreviates the path, whereas the XSD explicitly declares the plural wrapper.

**Impact and proof:** a scratch control parses identical successful bodies with
and without header/software, a success message and Common notifications and
asserts equal `TaxpayerInfo`. Code and the passing control establish real
projection loss. They do not establish present Agent notification forwarding.
Consumers using their own transport can retain `RawResponse::body()`;
`Client::send` constructs `RawResponse` internally and returns only the projection
(`src/client.rs:403–405`). Adding metadata would be an explicit public-result
capability decision, not a missing business-field fix.

## 5. Verdicts, failures and forwarding boundaries

### Established operation-wrapper behavior

`QueryTaxpayer::parse` first calls `RawResponse::check`
(`src/ops/taxpayer.rs:281–284`). The shared precedence is
`szlahu_down` → `szlahu_error_code` → known non-2xx status → body
(`src/wire.rs:251–310`; documented at `README.md:354–358`). Thus:

| Input | Current answer / assessment |
|---|---|
| Documented `OK` + true/false | `Ok(TaxpayerInfo)` with that validity; query success is separate from tax-number validity |
| `OK` + `1`/`0` | Same XML boolean meanings, correctly accepted |
| Missing/empty/blank `funcCode` | Parse error, not successful lookup |
| `OK` + absent/empty/invalid validity | Parse error; does not manufacture false |
| `ERROR/57` + message | `ApiError { code: MalformedXml, message }`; fresh Agent example passes |
| `ERROR/MAINTENANCE_MODE`, `INVALID_REQUEST` or another nonnumeric code inside the supported wrapper | Open `ErrorCode::Unknown(String)`, message retained; no numeric NAV reinterpretation |
| Non-OK future nonblank `funcCode` | API error, not success; the token is only retained in fallback text when no code/message exists |
| ERROR with missing code | `ErrorCode::Absent` |
| Missing message | Falls back to raw code, otherwise `NAV funcCode {token}` |
| Error/down header with apparently successful body | Header wins; taxpayer has no numbered-56 issuance exception |
| Body-only NAV error at known non-2xx status | `HttpStatus`, with bounded body excerpt; no typed body-code classification |

Body conversion is at `src/ops/taxpayer.rs:592–622`; boolean decoding is at
567–578. Shared `ErrorCode` trims and normalizes known numeric spellings (e.g.
`057`) but preserves unknown tokens (`src/error.rs:458–487`). The generic client
has no automatic retry loop (`src/client.rs:374–405`). Open NAV codes must not be
treated as established Agent credential/rejection codes just because they are
inside the same public error type.

NAV N1 §1.8.9 says invalid **or nonexistent** numbers return false. The returned
boolean cannot distinguish these conditions. The code retains accompanying
business content even on false, which avoids inventing a stricter meaning.

### Missing validity: schema possibility, documented local policy

Both NAV response schemas declare `taxpayerValidity` optional, not conditionally
required when `funcCode=OK`. Accordingly the XSD allows a successful wrapper
without it. That is a **known acceptance restriction** of the crate's `bool`
representation, expressly documented at `README.md:315–316` and reproduced
offline. However A3's success and invalid-number examples both include validity,
and N1's narrative specifies false for an invalid/nonexistent number. No source
here establishes the semantics of a legitimate Agent OK-without-validity reply.
Do not call this an observed rejected success or map absence to false. If the
vendor confirms such a case, represent unknown validity explicitly.

### T-2 — Generic NAV errors are not established Agent forwarding

N1 §§3.1–3.2 explicitly defines **direct NAV** generic exception/error replies,
including `INVALID_REQUEST`, `INVALID_SECURITY_USER`, and `MAINTENANCE_MODE`,
with associated direct NAV HTTP statuses. N4 defines both generic roots in OSA
2.0 API; N2 defines `GeneralErrorResponse` in OSA 3.0 API, while N6 defines
`GeneralExceptionResponse` in NTCA Common 1.0. The latter's fields are directly
under its root, not under `result`.

The current accepted roots are only `QueryTaxpayerResponse` in OSA 2.0/3.0
(`src/ops/taxpayer.rs:403–416`). A minimal offline reproduction is:

```xml
<GeneralExceptionResponse xmlns="http://schemas.nav.gov.hu/NTCA/1.0/common">
  <funcCode>ERROR</funcCode>
  <errorCode>INVALID_REQUEST</errorCode>
  <message>bad request</message>
</GeneralExceptionResponse>
```

With no error/down header: at HTTP 200 (or omitted status), parsing yields
`ResponseError::Parse`; at HTTP 400 it yields `ResponseError::HttpStatus`. A
separate synthetic `GeneralErrorResponse` control yields the same classifications.
This proves what the parser would do, **not that Agent emits either body**.

A3/A4 instead promise the taxpayer response type and even wrap Agent's own 57 in
it. Neither the linked NAV contract nor the generic phrase “data from NAV” proves
byte-for-byte root, namespace, header or HTTP-status forwarding by the intermediary.
Before treating this as a production defect, obtain a sanitized Agent capture or
vendor confirmation of those exact properties. N1's prose also uses the name
`GeneralTechnicalException`; its table/XSD spell the XML root
`GeneralExceptionResponse`, so the prose name must not become an invented root.

## 6. XML/content boundaries and T-3

Shared root parsing verifies UTF-8, expected expanded root, completion through
EOF and XML lexical validity, rejecting DTDs, extra roots, outside content,
undefined references and malformed ignored extensions (`src/xml.rs:201–300`).
Taxpayer scalar accumulation decodes text, CDATA, references and XML line endings
without dropping text around comments/processing instructions
(`src/ops/taxpayer.rs:472–505`). No wire date is indexed by byte offset here.

This is not complete response-XSD validation: business requiredness, sequence,
length/pattern facets and metadata shape are intentionally not enforced. A
malformed **recognized scalar** (including `message`) can fail parsing before
`into_info`; the parser's documented scalar rule applies even to optional fields.
There is no evidence in the fetched examples that Agent emits such a shape.

**T-3 proof:** `finish` applies Rust `str::trim()` to `funcCode`, `errorCode` and
`taxpayerValidity` (`src/ops/taxpayer.rs:519–529`). This body is accepted as true:

```xml
<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
  <result><funcCode>OK</funcCode></result>
  <taxpayerValidity>&#160;true&#160;</taxpayerValidity>
</QueryTaxpayerResponse>
```

The equivalent NAV 3.0 body was executed in the scratch test. NBSP is Unicode
whitespace but not XML Schema's whitespace-collapse alphabet (X1). The same trim
also accepts padded OK/code tokens. **Low-priority optional hardening**, not a
demonstrated vendor interoperability failure: the library explicitly assigns
verdicts their own normalization policy (`README.md:288–295`). If tightened,
restrict verdict whitespace deliberately without changing business-text fidelity.

## 7. T-4 — Source discrepancies and remaining uncertainty

1. **Version/forwarding:** A3's examples remain OSA 2.0, dated 2020-11-04, while
   the schema tab links NAV 3.0. Both layouts parse correctly offline. A linked
   3.0 schema is authority for its structure, not proof that every Agent account
   or every error currently uses it. The request carries no version selector.
2. **Strict NAV envelope conformance is not Agent evidence:** NAV's response
   bases require `software`, yet A3's error and false examples omit it. The
   reader appropriately does not demand it. Blindly applying every direct NAV
   requirement to Agent replies would reject the vendor's own examples.
3. **Optional data:** A3 demonstrates `infoDate`, name, taxpayerId/vatCode and a
   basic HQ address. It does not demonstrate short name, county code, VAT group,
   incorporation, every detailed address field or notifications. These are
   schema-supported potential fields, with code support verified independently;
   present Agent forwarding frequency is unknown.
4. **Request schema hint:** A2's example points to
   `http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd`; its HTTPS equivalent
   returned **404** in this review. The inline schema and A7 download work. The
   crate emits no broken schema hint, so this is a documentation defect, not a
   request-writer defect.
5. **Self-billing documents differ:** the older A9 inline login XSD requires
   `loginname/password`, whereas current A10 makes them optional when an Agent
   key identifies the caller. The action's existence is corroborated by both;
   an implementation should use the current dedicated reference rather than
   blindly copy the older schema. No onboarding execution was attempted.

## 8. Targeted verification and reproducibility

Executed without network calls to authenticated Agent/NAV endpoints:

```sh
cargo test -p szamlazz-agent --locked --offline --lib ops::taxpayer::tests
cargo test -p szamlazz-agent --locked --offline --test taxpayer_paths
python3 /tmp/opencode/taxpayer-28dcec1-acquire.py
python3 /tmp/opencode/taxpayer-28dcec1-inventory.py
python3 /tmp/opencode/taxpayer-28dcec1-run.py
```

- **15 taxpayer unit tests + 10 taxpayer-path tests passed.** They exercise
  business text, singleton/path guards, versioned fields, open incorporation,
  independent addresses, sparse records and failure verdicts.
- **7 scratch Rust tests passed**, compiled with `rustc --test` against the
  artifact selected from a fresh
  `cargo build -p szamlazz-agent --locked --offline --lib --message-format=json`.
  Scratch source: `/tmp/opencode/taxpayer-28dcec1-check.rs`.
- Fresh A3 HTML `<pre>` blocks were extracted without repairing the XML; **all
  three examples** passed their expected assertions (valid record, Agent 57,
  explicit false). This is stronger than merely reusing checked-in samples.
- Scratch coverage additionally asserts both namespace layouts, every declared
  business/address field, two independent rows, open incorporation, malformed
  advisory `infoDate`, four boolean spellings, invalid/missing validity, unknown
  verdict/code, wrong-parent verdict, duplicate validity, metadata omission,
  generic error rejection at 200/400, NBSP tolerance and header/status precedence.
  These are synthetic parser controls, not schema-valid/live response claims.
- Both credential forms with escaped dummy values and a leading-zero stem were
  generated through current writers and checked through `to_wire`. `xmllint
  --nonet --noout --schema` validated each against both A2's fresh inline XSD and
  A7's fresh download: **4/4 validations passed**. NAV response schemas were
  inspected, not claimed to have been compiled or used for response validation.
- The corrected sitemap text extraction covered **72 pages / 12 distinct
  actions**. An initial extractor concatenated adjacent HTML cells into false
  token suffixes; preserving text boundaries removed them. Initial scratch
  linking used the wrong dependency directory; using `target/debug/deps` fixed
  the harness. Neither was a product failure.
- The PDF was freshly downloaded and converted using `pdftotext -layout` through
  `nix shell nixpkgs#poppler-utils`; schema validation used `nixpkgs#libxml2`.
  These tools obtained public documentation only. No ignored live test was run.

### Fresh artifact fingerprints

Temporary files have prefix `/tmp/opencode/taxpayer-28dcec1-`. The primary URLs,
schema locations, code citations and reproductions above remain the durable
record; temporary files are supporting session artifacts.

| Artifact | SHA-256 |
|---|---|
| `nav3.pdf` | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| `nav3-api.xsd` | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| `nav3-base.xsd` | `49362a6ede64afcfeba1c5c3726f6216e3a8cd1dbad0c071b85811759ad4acc9` |
| `nav2-api.xsd` | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| `nav2-data.xsd` | `fb3dde53cb883ac89fdb43372961d3249885883ccb690895a15f1a4853705100` |
| `common1.xsd` | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| `request-download.xsd` | `51fe8565301b0f3a67199b3b3d666fd6abb5acebd5db4cd81c3a479cae816ed7` |
| `response-0.xml` (success) | `20ba9328e2047b0918e9a8e612bff84f5c3a2379c55bf23cddb18da54aaf1919` |
| `response-1.xml` (error) | `fe65bc9b4af7276aa5b2c753589bd6184225db4ba08619f01046add000fd3bc0` |
| `response-2.xml` (false) | `9fa150d457a5b53071befca3c618f3f631bdeaddf49ed95c3bf48a622f767400` |
