# Számla Agent conformance audit: other operations

Date: **2026-09-09**. Audited revision: `a804c740eb8446211c1cdca3eea4fb93d298d25d`.

## Summary

The four request writers cover the current documented request elements, in their required order, with the correct operation selectors, namespaces, and response-version settings. No missing request field or wrong multipart action was found.

Response handling has two actionable correctness problems: incomplete/structurally misplaced XML can be accepted, most seriously in taxpayer lookup; and the shared monetary-header fallback does not read the comma decimal separator recorded in the live evidence. The taxpayer result is also a partial projection of NAV's documented business data. This is a coverage gap, not a failure to parse NAV 3.0 or multiple addresses. A smaller documentation defect describes proforma deletion as restricted to unpaid documents despite the recorded contrary observation.

| ID | Severity | Confidence | Finding |
|---|---|---|---|
| O4-01 | Medium | High (offline reproduced) | Taxpayer parsing can accept truncated XML, overwrite a failure with a second root, and read fields outside their proper path/namespace; shared envelope parsing also ignores a trailing second root |
| O4-02 | Medium | High (offline reproduced; production trigger conditional) | Monetary-header fallback rejects the observed comma decimal separator |
| O4-03 | Medium, coverage | High for omitted fields; medium for delivery through today's Számla Agent | Taxpayer projection drops county code, VAT-group membership, incorporation, short name, and last-change time |
| O4-04 | Low | High | Proforma module documentation implies an unpaid-only deletion guard that does not exist |

No High/Critical finding is established. Medium means a bounded correctness/data-loss problem or missing business capability; Low means misleading documentation. Malformed-response reproducers below establish parser behaviour, **not** that the official service normally emits those bodies. Accepted restrictions and unresolved vendor questions are listed separately.

## Scope and method

Production files, paths relative to `crates/szamlazz-agent/src/`:

- `ops/storno.rs:1–212`: request fields/defaults/writer and `CreatedInvoice` parsing.
- `ops/credit_entry.rs:1–287`: bounded credit entries, replace/additive semantics, request and balance parsing.
- `ops/proforma.rs:1–77`: number/order selector, delete writer and verdict.
- `ops/taxpayer.rs:1–368`: prefix validation, request, NAV pull parser and projection.
- Used shared code: `ops/envelope.rs:18–303`, `xml.rs:19–330`, `wire.rs:7–127,175–415`, `credentials.rs:5–97`, `types.rs:22–57,96–117,580–677,965–1013`, `ops.rs:27–31`, `error.rs:22–349`, and `client.rs:198–289` (transport entry point and defaults).
- Existing operation tests, envelope/wire/XML tests, `tests/upstream.rs`, `tests/golden/`, and the relevant upstream examples/XSDs were inspected. The cached corpus is supporting evidence, not the source of current protocol facts.
- Read `fixtures/SOURCES.md` and all of `docs/szamlazz-hu-behaviour.md`; used the supplied `CONTEXT.md` terminology and decisions, particularly storno/external-id/appearance, credit entries, response version, and taxpayer lookup.

Only public documentation/download GETs and offline Rust parsing/serialization checks were used. No account credentials, Számla Agent POSTs, live-account tests, production edits, or delegated agents were used. The report is the only workspace addition made for this audit. Temporary reproduction code and the downloaded NAV PDF were placed under `/tmp/opencode/`.

### Live source register

All successful URLs below were fetched during this audit. IDs are citation keys used in the tables and findings. The current operation pages show footer **`v202608271632`**; the still-served standalone `/xsd` pages show **`v202606031507`**. The combined `/xml` pages now contain the current inline XSD. Element declarations agree for these four requests; comments/presentation differ.

| ID | Official URL | Coverage |
|---|---|---|
| S-R | https://docs.szamlazz.hu/agent/reversing_invoice/request | POST, multipart action, original selector |
| S-X | https://docs.szamlazz.hu/agent/reversing_invoice/xml | Example and current inline request XSD |
| S-O | https://docs.szamlazz.hu/agent/reversing_invoice/response | Versions 1/2, headers, success/failure examples, response XSD |
| S-L | https://docs.szamlazz.hu/agent/reversing_invoice/xsd | Legacy standalone inline XSD, compared |
| C-R | https://docs.szamlazz.hu/agent/credit_entry/request | POST and multipart action |
| C-X | https://docs.szamlazz.hu/agent/credit_entry/xml | Entries, settings, cardinality, current inline XSD |
| C-O | https://docs.szamlazz.hu/agent/credit_entry/response | Versions, headers, structured examples and response XSD |
| C-L | https://docs.szamlazz.hu/agent/credit_entry/xsd | Legacy standalone inline XSD, compared |
| C-I | https://docs.szamlazz.hu/agent/credit_entry/other | IPN semantics adjacent to registration; receiver implementation outside scope |
| D-R | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request | POST and multipart action |
| D-X | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml | Both selectors, examples and current inline request XSD |
| D-O | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response | Verdict, 335, critical text/HTML, inline response XSD |
| D-L | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd | Legacy standalone request XSD, compared |
| T-R | https://docs.szamlazz.hu/agent/querying_taxpayer/request | Számla Agent wrapper and NAV source |
| T-X | https://docs.szamlazz.hu/agent/querying_taxpayer/xml | Request example and eight-digit restriction |
| T-O | https://docs.szamlazz.hu/agent/querying_taxpayer/response | Three OSA 2.0 examples, failure semantics, link to NAV 3.0 specification |
| T-L | https://docs.szamlazz.hu/agent/querying_taxpayer/xsd | Legacy standalone request XSD, compared |
| X-S | https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd | Downloaded storno request schema |
| X-C | https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd | Downloaded credit-entry request schema |
| X-T | https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd | Working taxpayer request schema URL from fixture provenance |
| X-O | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Shared invoice reply schema; storno includes PDF, credit inline reply omits PDF |
| B-S | https://docs.szamlazz.hu/agent/basics/sending-requests | Endpoint, file part, case/order and one document per XML |
| B-A | https://docs.szamlazz.hu/agent/basics/authentication | Agent key or username/password, lowercase key, legacy key-in-both-fields option |
| B-C | https://docs.szamlazz.hu/agent/basics/session-cookie | JSESSIONID, reuse, 90-minute inactivity, refresh after account edits |
| B-E | https://docs.szamlazz.hu/agent/basics/error-handling | Error codes, text-v1 format, retry ceiling |
| N-P | https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf | Linked NAV specification: §§1.4.1, 1.8.9, 3.1–3.3; printed pp.11–12, 63–69, 162 onward |
| N-A | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd | NAV 3.0 QueryTaxpayerResponseType, TaxpayerDataType, address types and general error root |
| N-B | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceBase.xsd | DetailedAddressType, SimpleAddressType, TaxNumberType |
| N-C | https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/catalog.xml | Common 1.0 import provenance |
| N-1 | https://raw.githubusercontent.com/nav-gov-hu/Common/Common-1.0.RC3/src/schemas/nav/gov/hu/NTCA/common.xsd | Catalog-linked historical Common 1.0: namespaces, BasicResultType, FunctionCodeType and scalar facets; not Common's current 2.0 branch |

Also fetched the four category introductions:

- https://docs.szamlazz.hu/agent/category/reversing-invoice
- https://docs.szamlazz.hu/agent/category/registering-credit-entry
- https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice
- https://docs.szamlazz.hu/agent/category/querying-taxpayer

NAV Online-Invoice `master` resolved to **`cc7a775d6dce361311e409abb9934eb755f2749c`**; use that commit instead of `master` in N-A/N-B/N-C for immutable citations. Relevant N-A line ranges at that revision: **1552–1581** (reply), **1812–1889** (address list/business fields).

N-P exceeded the webfetch tool's 5 MB limit; it was successfully downloaded with `curl` and converted with `pdftotext`, and the relevant sections were read. SHA-256: `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2`.

Broken links rechecked, all **404** after HTTPS upgrade:

- `http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` (D-X example's schemaLocation).
- `http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd` (D-O).
- `http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd` (T-X).

The inline deletion XSDs and working X-T were used instead. Initial guessed NAV paths containing `/OSA/3.0/` and Common's `master` branch also returned 404; the first-party repository trees/catalog supplied the working paths above.

### Drift from the cached source map

`fixtures/SOURCES.md:66–67` says storno and credit-entry response pages contain only text-error examples. **That is no longer current:** S-O and C-O now contain structured success/failure examples and inline reply schemas. `fixtures/SOURCES.md:86–88` says the taxpayer response schema heading is empty; T-O now links directly to N-P. These are provenance-age observations, not production-code defects, and the corpus was not edited.

## Request coverage, field by field

Notation: `!` is XSD `minOccurs="1"`; `?` is `minOccurs="0"`; all elements have maximum occurrence 1 unless stated otherwise. Listed order is the official sequence. A schema optionality is not a default: none of these request XSDs declares an XSD `default=` value.

### Shared request/transport behaviour

| Contract | Implementation | Assessment |
|---|---|---|
| B-S: “Számla Agent decides which function to perform using the name of the form field containing the XML file” at `https://www.szamlazz.hu/szamla/` | `wire.rs:14,66–99`; `client.rs:262–271`; four ACTION constants | Correct HTTPS endpoint by default, POST, multipart file part with filename and `text/xml` content; one XML per call |
| `felhasznalo? string`, `jelszo? string`, `szamlaagentkulcs? string`, in that order, in each `beallitasok` | `xml.rs:251–261`; `Credentials` | Key-only or username/password pair, valid schema subsequences and B-A authentication alternatives; no invented mixture |
| XML declaration UTF-8, exact operation root/default namespace | `xml.rs:19–41`; each writer | Correct. No schemaLocation emitted: it is a schema hint, not a request data requirement |
| Fixed field order, escaped text | `xml.rs:195–249`; explicit writers | Correct. `to_wire` checks forbidden XML 1.0 characters (`wire.rs:387–415`); direct `write_xml` is lower-level and does not validate |
| Dates `xs:date`; amounts `xs:double` | `Date` and `Decimal`, ordinary ISO civil dates and finite decimal strings | Appropriate business-domain representation; timezone-free dates are legal. Extreme dates/money and special floating-point values discussed below |
| Session reuse, 90 minutes inactivity (B-C) | Native default client's cookie store, `client.rs:209–224`; raw-client helper `wire.rs:308–326` | Supported. Caller-supplied HTTP clients own cookie configuration; new client is the available refresh route after account edits |
| B-E: “same request … at most five times” | `Client::send` issues one call, no retry loop | No automatic retry-ceiling violation in these operations; caller retry orchestration is outside scope |

`AgentKey` does not lowercase or prevalidate its text; its rustdoc states the rule, and credentials are transmitted as provided. This is not evidence of an authentication implementation defect: the operation schemas type credentials as unconstrained strings, B-A supplies the lowercase rule, and callers can supply the documented value. Legacy key-in-both-fields is expressible with `Credentials::user_password(key, key)`.

### Storno invoice

Sources: S-R/S-X/S-O/X-S. Code: `ops/storno.rs:53–156,160–211`.

| Block / official element | Type and optionality | Rust field/default and serialization | Result |
|---|---|---|---|
| root `xmlszamlast` | namespace `http://www.szamlazz.hu/xmlszamlast` | exact; ACTION `action-szamla_agent_st` | Match |
| `beallitasok!` / credentials | shared sequence | injected first | Match |
| `eszamla` | `boolean !` | `e_invoice=false`, always written | Match; explicit library default, not inferred from original |
| `szamlaLetoltes` | `boolean !` | `download_pdf=false`, always written | Match |
| `szamlaLetoltesPld` | `int ?` | `download_copies: Option<u8>=None` | Narrower integer domain, harmless deprecated field; see accepted deviations |
| `aggregator` | `string ?` | `Option<String>=None` | Match |
| `guardian` | `boolean ?` | `Option<bool>=None`, explicit false preserved | Match |
| `valaszVerzio` | `int ?` | always shared constant `2` | Supported XML format selected deliberately |
| `szamlaKulsoAzon` | `string ?` | `external_id=None`, transmitted verbatim | Match on wire; semantic conflict resolved by B6/XPRB evidence below |
| `fejlec!` / `szamlaszam` | `string !` | mandatory `InvoiceNumber`, no trimming or additional bounds | Match; no order-number or external-id-only selector is promised by current schema |
| `keltDatum` | `date ?` | `issue_date=None` | Correct omission/default documented with live 352 evidence |
| `teljesitesDatum` | `date ?` | `fulfillment_date=None` | Can send verified original date; omission intentionally allowed in low-level client |
| `megjegyzes` | `string ?` | `comment=None` | Match; reversal reason |
| `tipus` | `string ?` | hardcoded `SS` | Matches official example and operation; arbitrary type token is unnecessary |
| `szamlaSablon` | `string ?` | `template=None`; six documented tokens plus `Other` | Match; all six mappings in `types.rs:970–999` |
| `elado?` / `emailReplyto`, `emailTargy`, `emailSzoveg` | each `string ?` in that order | optional `SellerEmail`; emits empty container when absent | Match; empty optional complex type is valid |
| `vevo?` / `email`, `adoszam`, `adoszamEU` | each `string ?` in that order | three optional strings; empty container when absent | Match; tax fields can supplement a missing original tax number, per XSD comment |

All option fields default to absent in `new`; only the two required flags are false. The action is about one named original. The library intentionally does not pre-query the original, enforce its `tipus`, compare its tax numbers or appearance, or validate fulfillment-date equality. Those require document/account state; the Restate worker performs its own checks.

### Credit-entry registration

Sources: C-R/C-X/C-O/X-C. Code: `ops/credit_entry.rs:20–187,213–247`.

| Block / official element | Type and optionality | Rust field/default | Result |
|---|---|---|---|
| `xmlszamlakifiz` | namespace `http://www.szamlazz.hu/xmlszamlakifiz` | exact; ACTION `action-szamla_agent_kifiz` | Match |
| `beallitasok!` / credentials | shared sequence | first | Match |
| `szamlaszam` | `string !` | required invoice number | Match; no external/order selector in schema |
| `adoszam` | `string ?` | `issuer_tax_number=None` | Match; supports incoming-invoice matching, not the buyer's tax number |
| `additiv` | `boolean !` | `additive=false`, always emitted | Match; replacement is the explicit default |
| `aggregator` | `string ?` | `None` | Match |
| `valaszVerzio` | `int ?` | shared `2` | Match; XML reply |
| `kifizetes` | `0..5` blocks after settings | `CreditEntries`, constructor empty; conversion/push/Deserialize reject sixth | Exact upper limit; deliberate lower-limit restriction on replacement |
| `datum` | `date !` | required `Date` | Match |
| `jogcim` | `string !` | required open `PaymentMethod` | Match; arbitrary title token preserved via `Other`, not a closed method whitelist |
| `osszeg` | `double !` | required finite `Decimal` | Match for monetary values; no unsupported positivity restriction or forced rounding |
| `leiras` | `string ?` | `description=None` | Match; last element in entry |

C-X states: **“If true, former credit entries are retained; otherwise they are replaced.”** The code sends entries in caller order, but does not promise query order. D7 observed replacement, additive append, five accepted entries and unordered query results. The maximum is per request; it is not a five-entry lifetime limit on an invoice. There is no request currency, bank-account field or idempotency token to add here.

The sample comment about `adoszam` says “match the incoming invoice with the corresponding incoming receipt”; the current XSD comment says “match the incoming payment with the corresponding invoice.” The Rust doc repeats the former wording. This is ambiguous vendor prose, not evidence of a wrong field mapping.

C-I reports absolute invoice payment status through IPN and can coalesce pending notifications. It is not an acknowledgement of an individual `CreditEntry`, and the operation response correctly does not await IPN.

### Delete proforma

Sources: D-R/D-X/D-O/D-L. Code: `ops/proforma.rs:10–76`.

| Block / official element | Type and optionality | Rust behaviour | Result |
|---|---|---|---|
| `xmlszamladbkdel` | namespace `http://www.szamlazz.hu/xmlszamladbkdel` | exact; ACTION `action-szamla_agent_dijbekero_torlese` | Match |
| `beallitasok!` | credentials only | injected | Match; no response-version/aggregator setting in this schema |
| `fejlec!` / `szamlaszam?`, then `rendelesszam?` | strings | enum writes exactly one | Correct semantic restriction versus two independently optional XSD fields |

The **lowercase** `rendelesszam` is correct here (not the query operations' `rendelesSzam`). There is no external-id deletion selector in the current schema. The enum enforces one selector element, not a nonempty string: empty numbers are expressible and delegated to szamlazz.hu, as the unconstrained `xs:string` permits. No unsupported length or paid-state validation was introduced.

### NAV taxpayer lookup request

Sources: T-R/T-X/X-T and N-P §1.8.9.1. Code: `ops/taxpayer.rs:14–107,164–184`.

`xmltaxpayer` in `http://www.szamlazz.hu/xmltaxpayer`, ACTION `action-szamla_agent_taxpayer`; `beallitasok!` with the shared credential sequence, followed by `torzsszam!`. Schema type `torszszamTipus` is a string restricted by **`length value="8"` and `pattern value="[0-9]{8}"`**. All construction and serde paths enforce exactly eight ASCII digits, preserving leading zeroes. Full Hungarian tax number, EU number, whitespace and non-ASCII digits are deliberately not accepted by this request type. No check-digit arithmetic is required by this schema.

No `valaszVerzio`, NAV user/passwordHash/signature, taxpayer date, pagination or NAV-version request field is missing: these belong either to NAV's direct interface or do not exist on the Számla Agent wrapper. The caller cannot select the NAV version through this operation.

## Response coverage

### Storno and credit-entry envelopes

S-O: **“`2` — XML — Structured `xmlszamlavalasz` with optional base64 PDF in `<pdf>`.”** C-O likewise promises structured `xmlszamlavalasz`. Version 1 text/PDF handling is deliberately absent because both writers always request 2.

| Official response surface | Implementation | Assessment |
|---|---|---|
| root/namespace `xmlszamlavalasz` / `http://www.szamlazz.hu/xmlszamlavalasz` | `xml::response_text`, `envelope::{ROOT,NAMESPACE}` | Root URI checked, prefix-independent; whole-document/path limits in O4-01 |
| `sikeres! boolean`; optional `hibakod string`, `hibauzenet string` | `xml::Verdict`, required boolean; body-only errors supported | Correct normal success/failure and `true/false/1/0`; empty boolean is extra leniency noted below |
| `szamlaszam? string` | body first, decoded header second, trimmed/nonblank; mandatory to return success object | Deliberate semantic requirement: a storno must identify its document; credit balance identifies target |
| `szamlanetto?`, `szamlabrutto?`, `kintlevoseg?` as `double` | optional Decimal; body first then raw corresponding header | Ordinary finite/scientific values work; O4-02 affects comma header fallback |
| `vevoifiokurl? string` | body first, percent-decoded `szlahu_vevoifiokurl` second | Match |
| `pdf? base64Binary` (storno only) | `Pdf::from_base64`, strips whitespace and decodes standard base64 | Match; no raw-PDF-v1 path needed. Absent PDF stays `None` even if requested |
| `szlahu_szamlaszam`, `szlahu_error` URL encoding | `RawResponse::szlahu`, percent decoding and `+` → space | Match |
| `szlahu_fizetesmod` | credit balance's open `PaymentMethod` from decoded header | Covered for credit; omitted from `CreatedInvoice` (projection choice, raw header still accessible with own transport) |
| `szlahu_id` (recorded live, not in current operation header table) | storno `document_id: Option<i64>`, invalid/negative → None | Intentional auxiliary leniency; credit balance does not expose id |
| `szlahu_down`, header error, HTTP status | `wire.rs:256–306` | Nonempty down wins; error header precedes body; non-2xx without error header is HttpStatus before body. Header/body-only 200 failures work |

S-O/C-O say **“With an XML response, the same data is also in the XML body.”** Their actual XSDs have no payment-method body element; header-only payment-method extraction is consistent with the explicit schemas, not a missing `fizmod` parser.

Storno `parse_issued` handles the deliberate 56-with-number exception as successful issuance with `notification_delivery_failed=true`; absent number retains an API error. Malformed optional totals/PDF are dropped on that warning path only. No test-account probe produced 56, so this is a conservative compatibility decision, not a newly verified live shape. Ordinary success with malformed PDF/totals is a parse error, which the caller must not interpret as proof that nothing was issued.

### Delete response

D-O says: **“The response is XML (`xmlszamladbkdelvalasz`). On critical error, a plain text/html error message may be returned instead.”** The expected URI is `http://www.szamlazz.hu/xmlszamladbkdelvalasz`; sequence is `sikeres! boolean`, `hibakod? int`, `hibauzenet? string`. `xml::verdict` returns `()` on success and preserves API code/message on failure, including CDATA. The shared error code accepts unknown strings as well as the XSD's integer range; unknown codes are not discarded.

335 is correctly exposed as `ProformaNotFound`, not automatically turned into successful deletion by this low-level client. A header-free successful deletion is accepted (D1). Text/HTML produces an error with bounded body excerpt, rather than false success. Missing `sikeres` is rejected; false without a code yields `ErrorCode::Absent`. Trailing-document acceptance is included in O4-01.

### NAV versions, data and addresses

T-O explicitly says **“The response always matches the `QueryTaxPayerResponse` type of the NAV Online Invoice Platform. Last update for example responses: 2020-11-04.”** Its actual XML root spells **`QueryTaxpayerResponse`**, exactly what the code accepts. The capitalization in prose is not a second root to support.

| Surface | Official shape | Implementation / assessment |
|---|---|---|
| NAV 2.0 | API default namespace; `ns2` data namespace in T-O | Accepted; all three official examples pass |
| NAV 3.0 | API `http://schemas.nav.gov.hu/OSA/3.0/api`; common result namespace `http://schemas.nav.gov.hu/NTCA/1.0/common`; base tax/address namespace `http://schemas.nav.gov.hu/OSA/3.0/base` | Accepted via local names; reproduced with actual 3.0 namespace mix. Existing 3.0 unit test merely replaces `/2.0/` with `/3.0/`, which does not model the real common/base split |
| Header and software | NAV transaction id/timestamp/requestVersion/headerVersion and software metadata | Not exposed; transport/diagnostic projection, not taxpayer business data |
| `result/funcCode` | required `OK` or `ERROR` (N-P §1.4.1; N-1) | `OK` requires explicit validity; every other nonempty token returns API error |
| `result/errorCode`, `message` | optional open string and human message | Numeric szamlazz.hu code 57 typed; NAV `INVALID_REQUEST` retained as Unknown; missing code remains Absent; CDATA/entities supported except O4-01 |
| `taxpayerValidity` | `xs:boolean`, optional in N-A; N-P says invalid/nonexistent numbers return false | Required on `OK`, accepts all four boolean lexical forms; error does not require validity. No absent→false fabrication |
| `infoDate` | optional dateTime, last change of taxpayer data | Omitted, O4-03 |
| `taxpayerData/taxpayerName` | required when taxpayerData exists | optional `name`, trimmed; lenient missing data accepted |
| `taxpayerShortName` | optional string | Omitted, O4-03 |
| `taxNumberDetail/taxpayerId`, `vatCode`, `countyCode` | first required, last two optional strings | first two preserved; county omitted, O4-03 |
| `incorporation`, `vatGroupMembership` | required economic type; optional eight-digit VAT group | Omitted, O4-03 |
| `taxpayerAddressList` | optional; when present 1..unbounded address items | `Vec<TaxpayerAddress>`; separate item pushed per start; two sequential addresses tested successfully, no five-entry limit copied from credits |
| `taxpayerAddressType` | `HQ`, `SITE`, `BRANCH` | all retained as open string, not forced to HQ |
| `taxpayerAddress` | `base:DetailedAddressType`, not `AddressType` choice (N-A:1824) | every detailed-address element represented, same list item |

Detailed-address sequence audited against N-B: required `countryCode`, optional `region`, required `postalCode`, `city`, `streetName`, `publicPlaceCategory`, then optional `number`, `building`, `staircase`, `floor`, `door`, `lotNumber`. Every one has an `Option<String>` field and a setter arm in `taxpayer.rs:295–309`. String values remain strings, preserving leading zeroes/alphanumeric house numbers. Optional omissions do not contaminate the next item.

NAV facets are not imposed on returned strings: country `[A-Z]{2}`, taxpayer/group id eight digits, vat code `[1-5]`, county code two digits; name max 512, short name max 200, city/street max 255, region/category/house/building/staircase/floor/door/lot max 50, and nonblank restrictions. N-P's postal-code table is older/narrower than catalog-linked N-1's 3..10-character postal type with spaces/hyphens; the unvalidated Rust string accepts both. This is response leniency, not a request-validation omission.

`additionalAddressDetail` and nested simple/detailed wrappers are tolerated by the pull parser, but **simple addresses are not a documented QueryTaxpayerResponse alternative**: N-A directly uses DetailedAddressType. Existing simple-address tests establish extra compatibility only.

N-P §1.8.9.2 point 1 says: **“Nem érvényes vagy nem létező adószámra false érték kerül visszaadásra.”** (Invalid or nonexistent tax numbers return false.) Thus `valid=false` is an ordinary successful query, not an API failure. A schema-optional `taxpayerValidity` on an `OK` answer remains an ambiguity between schema and operational prose; no recommendation to silently default it to false.

## Findings

### O4-01 — The parser can accept something other than one complete response document

**Severity:** Medium. **Confidence:** High for implementation/reproduction; no evidence the service normally emits these malformed shapes.

**Exact code ranges:** `ops/taxpayer.rs:206–285` (root check, event loop, no completion/depth check), `ops/taxpayer.rs:287–338` (global local-name assignments), `ops/taxpayer.rs:245–258` (unknown entity silently dropped). Shared part: `xml.rs:63–108` checks only the first root start, and `xml.rs:152–176` / `ops/envelope.rs:259–264` deserialize without ensuring no trailing document remains.

**Official basis:** T-O says the response “always matches” NAV's QueryTaxpayerResponse. N-P §1.8.9.2: **“A /queryTaxpayer operáció válaszának struktúráját a QueryTaxpayerResponse element tartalmazza.”** (The QueryTaxpayerResponse element contains the operation's response structure.) N-A scopes `taxpayerData`, `taxNumberDetail`, and `taxpayerAddressList` under their declared parents; N-1 places `funcCode` inside `result`. S-O/C-O/D-O likewise specify one named XML envelope. Reading only matching local names anywhere does not implement these boundaries.

**Triggers / offline results:** Pass these bodies to `QueryTaxpayer::parse` with no error headers. Namespace `A` below means `http://schemas.nav.gov.hu/OSA/3.0/api`.

1. `<QueryTaxpayerResponse xmlns="A"><result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerName>ACME</taxpayerName>` — missing both final closing tags, yet returns `Ok`, `valid=true`, `name=ACME`.
2. A closed QueryTaxpayerResponse carrying `ERROR/57`, followed by `<other><funcCode>OK</funcCode><taxpayerValidity>true</taxpayerValidity></other>` — returns **success**, overwriting the earlier failure.
3. A normal `ACME` taxpayer, then `<extension><taxpayerName>WRONG</taxpayerName></extension>` inside the root — returns `name=WRONG`. Unknown containers are traversed, not skipped as the parser comment claims.
4. Correct root URI but `result/funcCode` and `taxpayerValidity` in `urn:wrong` — still returns `valid=true`.
5. `<taxpayerName>A&bogus;B</taxpayerName>` with no entity declaration — returns `AB`, silently deleting the undefined entity rather than refusing non-well-formed XML.
6. A valid deletion success envelope followed by `<other/>` — `DeleteProforma::parse` returns `Ok(())`. In contrast, a truncated deletion envelope correctly fails in serde. The missing-close bug is specifically the taxpayer pull loop; ignored trailing content also affects the shared parsing boundary.

**Impact:** A truncated answer can be reported as a complete taxpayer result. Structural collisions can change validity/error classification or overwrite taxpayer identity/address values. Undefined entities silently alter names. The impact is bounded to malformed or unexpected replies, but correctness of a read that feeds invoicing should not depend on field-name uniqueness over the entire body.

**Live-evidence reconciliation:** No recorded NAV account probe establishes this as intentional leniency. Supporting NAV 2.0 and 3.0 prefixes is intentional and works, but it does not require accepting foreign namespaces, trailing roots, or incomplete XML. The Adatkapcsolat shape/content policy is a separate surface; it is not a justification for these outbound response behaviours.

**Suggested fix:** Validate a single complete XML document, reject unknown entities (or resolve explicitly supported declarations), and maintain an expanded-name/path stack for the NAV parser. Read only the declared result/taxpayer/address locations, with version-specific API/data/common/base namespace bindings. Skip unknown subtrees atomically. At the shared response boundary, reject non-whitespace content/another root after the document ends while allowing legal comments/processing instructions. No need to impose every NAV content facet or make optional taxpayer fields mandatory.

**Regression:** Table tests for all six triggers; truncated after each major node; duplicate scalar fields; actual NAV 3.0 common/base namespaces with arbitrary prefixes; two differently populated addresses; entity and CDATA concatenation; legal comments before/after the root. Assert errors remain errors when unrelated later elements contain `funcCode=OK`.

### O4-02 — Header totals reject szamlazz.hu's observed comma decimal separator

**Severity:** Medium. **Confidence:** High for bug and recorded formatting, conditional for a current storno/credit response lacking the corresponding body amount.

**Exact code ranges:** `ops/envelope.rs:146–155,285–302`; storno consumption `ops/envelope.rs:205–225`; credit consumption `ops/credit_entry.rs:253–274`.

**Official basis:** S-O/C-O list `szlahu_nettovegosszeg` as **“Net total (not URL encoded)”**, `szlahu_bruttovegosszeg` as **“Gross total (not URL encoded)”**, and allow optional body amounts (`type="double" … minOccurs="0"`). The docs do not define a comma format themselves; the repository's live evidence supplies it: `docs/szamlazz-hu-behaviour.md:160`, P60-E1/E3, explicitly records **`szlahu_nettovegosszeg 100,01`**. This is a live-format compatibility defect rather than a claim that commas are legal XML `xs:double` text.

**Trigger:** A valid version-2 success envelope with a number, absent body gross, and header `szlahu_bruttovegosszeg: -127,50`. Both storno and credit parsing return `ParseError::Invalid` naming the header. Positive comma totals behave identically. Dot decimals and `1.27E3` succeed; scientific notation is **not** a defect in the current Decimal dependency.

**Impact:** The fallback expressly offered by the shared helper is unusable for fractional totals in the observed header format. A successful storno or credit registration is surfaced as a parse failure even though the write may have landed. The issue is normally masked by body-first parsing, and on the 56 warning path the malformed optional amount is dropped rather than failing the whole issuance.

**Live-evidence reconciliation:** P60 established comma formatting on create headers, not a missing-body storno or credit balance. D7 established body/header agreement, not every fractional header representation. Therefore do not claim all current credit/storno calls fail: it is the combination of the established header format and an exercised fallback path that fails. Body amounts remain standard XML doubles and should not be globally rewritten as Hungarian numbers.

**Suggested fix:** Give monetary **headers** an explicit dot/comma decimal parser, preserving signs and precision, refusing ambiguous thousands grouping/mixed separators. Keep XML amount parsing separate. Decide whether blank headers mean absent consistently with blank body values.

**Regression:** Each of net/gross/outstanding as header-only success using `100,01` and `-127,50`, in both operations; body values overriding headers; dot and exponent forms; blank and malformed/grouped values; 56 warning with fractional header values.

### O4-03 — Taxpayer business-data coverage stops short of the linked NAV model

**Severity:** Medium **coverage gap**, not rejection/corruption of the fields currently exposed. **Confidence:** High for model omissions; medium for whether all fields are presently forwarded by Számla Agent (no account lookup made).

**Exact code ranges:** `ops/taxpayer.rs:110–127` (public result), `187–199` (internal fields), `317–337` (recognized non-address fields), `345–354` (projection).

**Official basis:** T-O links directly to N-P §1.8.9. N-A:1852–1887 declares `taxpayerShortName`, `incorporation`, `vatGroupMembership` alongside the existing name/tax number/address fields. N-B's TaxNumberType includes `countyCode`. N-P pp.68–69 states:

> “A vatGroupMembership tag tartalmazza a lekérdezett adózó áfacsoport tagságát, ha az adózó tagja áfacsoportnak.”

The VAT-group-membership tag contains the queried taxpayer's VAT group when it is a member.

> “A taxNumberDetail csomópont tartalmazza a lekérdezett adózó teljes adószámát …”

The taxNumberDetail node contains the taxpayer's full tax number.

> “Az infoDate az adózó adatainak utolsó változását mutatja.”

infoDate gives the last change of the taxpayer's data. N-P §1.8.9.2 also enumerates incorporation values `ORGANIZATION`, `SELF_EMPLOYED`, `TAXABLE_PERSON`.

**Trigger:** A normal NAV 3.0 answer includes any of those fields. It parses successfully but `TaxpayerInfo` has no place for `countyCode`, `vatGroupMembership`, `incorporation`, `taxpayerShortName`, or `infoDate`. A namespaced local probe with county `42`, incorporation `ORGANIZATION`, and group `87654321` returned only the existing name/id/vat fields.

**Impact:** A typed-client consumer cannot reconstruct the full `NNNNNNNN-N-NN` number from this result, discover the returned VAT-group id or distinguish the economic type. Short-name and freshness data are also unavailable. A custom transport retaining `RawResponse` can recover them itself; the bundled `Client::send` exposes only the projection. This does not make `TaxpayerInfo::tax_number` incorrect: its rustdoc explicitly calls it the eight-digit taxpayerId.

**Live-evidence reconciliation / intentionality:** The private parser doc calls its result “reduced to the fields this crate surfaces,” so the reduction is explicit in code. Neither the supplied context nor behaviour notes records a domain decision to exclude these specific NAV business fields from the general Számla Agent client. NAV itself says clients decide how much returned information to use. Accordingly this is a **full-conformance coverage limitation**, not an assertion that NAV demands every SDK expose every element, and it is distinct from the Restate worker's deliberately narrow journal projection. The stale 2020 Számla Agent example lacks most of the fields and cannot prove their current absence.

**Suggested fix:** Expose optional county code, VAT-group membership, incorporation (open token type), short name and last-change time on the agent result, keeping compatibility with NAV 2.0/sparse Számla Agent examples. Alternatively explicitly document the result as a selected subset and record the field exclusions as an accepted API-scope decision. Do not infer the VAT group from the taxpayerId or invent a county code.

**Regression:** A realistic 3.0 response using common/base namespaces with all five fields, two addresses and optional omissions; preserve leading zeroes and unknown incorporation tokens. Keep the old 2.0 success/failure/nonexistent examples passing. Worker projection tests should continue to decide separately what it exposes/journals.

### O4-04 — “Unpaid proforma” documentation suggests a nonexistent guard

**Severity:** Low. **Confidence:** High.

**Exact code range:** `ops/proforma.rs:1–2` (module rustdoc). Actual unconditional writer: `ops/proforma.rs:48–67`.

**Official basis:** The category introduction says **“you can delete an existing pro forma invoice.”** D-R/D-X impose no paid-state condition and define only credentials plus a selector. Recorded D3 (`docs/szamlazz-hu-behaviour.md:110`) says **“a fully paid D deletes without any guard, taking its payment history with it.”**

**Trigger / impact:** A caller reads “removes an unpaid proforma” as meaning the operation is limited to unpaid documents and deletes a paid proforma expecting rejection. The request performs no balance query/guard and the test account accepted that deletion. This is documentation risk, not a missing protocol validation rule.

**Live-evidence reconciliation:** D3 directly disproves the implied restriction. The worker's paid guard/force choice is a separate layer; it should not be implied by this low-level client.

**Suggested fix:** Describe deletion of an existing proforma, explicitly note that a paid one can also be deleted and that any paid-state guard is the caller's responsibility. No production behaviour change needed.

**Regression:** Documentation review is sufficient; an existing synthetic delete success does not prove paid-state behaviour, and no live account request is warranted for this wording fix.

## Accepted deviations and intentional restrictions

1. **Empty replacing credit entries are refused.** C-X/X-C allow `kifizetes minOccurs="0"`; `credit_entry.rs:217–220` rejects only empty replacement. Empty additive remains expressible. D7 confirmed replacement semantics; the zero-entry destructive case was never probed (`behaviour.md:211–215`). This is the deliberate #70 restriction, **not a defect or unsupported schema interpretation**. Do not silently remove it as part of conformance work.
2. **Response version 2 is pinned.** Storno/credit are not required to parse v1 text/PDF because they never request it. Error text arriving unexpectedly is still retained as a bounded diagnostic rather than assumed success.
3. **Storno external id attaches to the new storno.** S-R still says the original may be referenced by external id, but requires its number, while S-X describes later lookup by that key. B6 and XPRB-P4 establish the id goes on the new SS when the original number is supplied; repeat storno does not attach a new id (B4x/P48-P6). `storno.rs:84–96` documents this contradiction correctly. Do not replace invoice_number with an external-id-only selector or put the original's id on the storno.
4. **Date and appearance are not checked by the low-level operation.** P48 showed omitted fulfillment date inherits the original, explicit matching or mismatching dates are accepted, and non-today issue date is rejected with 352. P73 showed storno appearance takes the request flag even when it mismatches the original. The optional fields and explicit false default accurately expose the wire. The worker derives the original's fulfillment date/appearance; conformance is not a reason to strip those options from the agent client.
5. **Storno repeat/no-op distinctions need semantic checking.** B4 repeat returns existing SS; B5 D/SL returns unchanged original. `CreatedInvoice::reverses` is a deliberate observed-behaviour helper, not a protocol field. `StornoInvoice::parse` returning a numbered response is not by itself proof a new reversal was issued.
6. **Deprecated download-copy count is narrowed.** `Option<u8>` covers only 0..255 of XSD int, but S-X says “this field can be omitted, as our system no longer processes it.” No functional conformance defect established. Omission is the default and correct.
7. **Deletion selector is exclusive.** The XSD allows both/neither structurally, but the prose/examples say alternatives. The enum removes ambiguous shapes without removing a documented useful operation.
8. **Errors remain an open set.** Live-only 14/221/352/463 and delete 335 are typed; unseen string/numeric codes are retained. N-P §3 explicitly avoids enumerating error codes in the XSD to prevent client coupling. No obligation to exhaustively name every NAV error token was inferred.
9. **Sparse response content and auxiliary values are tolerated.** Optional amount/name/address/tax-number fields, invalid auxiliary document-id header → None, and unknown response elements are consistent with a projection-oriented client. That does not justify O4-01's structural/name collisions.
10. **Finite money/civil dates are a bounded domain.** `Decimal` cannot represent every IEEE double (NaN/INF or all extreme exponents); the ordinary finite exponent `1.27E3` is supported. Special floats are not sensible credit amounts. Civil dates omit optional XSD timezone suffixes. These are type-domain limits, not observed interoperability failures. XSD 1.0's lack of year zero versus `jiff::Date`'s wider civil domain remains a theoretical caller-input edge; no contemporary invoicing trigger was established.
11. **Projection-only response metadata.** `CreatedInvoice` omits storno payment method; credit balance omits live `szlahu_id`; NAV header/software and success diagnostic messages/notifications are not exposed. These are explicit response-surface limits, not parser rejection defects. O4-03 separates the more useful missing taxpayer business data from this envelope metadata.

## Unknowns and evidence limits

- **Which NAV version and business fields szamlazz.hu forwards today:** T-O's current page still uses 2020 OSA 2.0 examples and links NAV 3.0. The parser handles both versions' namespace layouts, but no public example or existing behaviour probe establishes today's live forwarding of all 3.0 fields. This limits the real-world impact confidence of O4-03.
- **NAV failures outside QueryTaxpayerResponse:** N-P §3.1 documents `GeneralErrorResponse` and generic exception roots for NAV's direct interface. T-O says the Számla Agent response “always” uses QueryTaxpayerResponse and demonstrates its own failure in that wrapper. The agent parser rejects other roots; there is no evidence szamlazz.hu passes the direct NAV roots through. Treat this as a vendor-clarification question, not a proven missing failure parser. Nonempty `ERROR`/numeric or symbolic code in the documented wrapper works.
- **Non-2xx body-only szamlazz.hu answers:** `RawResponse::header_verdict` checks status before body when no error header exists, despite broader `RawResponse` prose about body-first interpretation. A hypothetical HTTP 500 QueryTaxpayerResponse with an error code becomes HttpStatus rather than Api. The recorded service errors are HTTP 200; direct NAV's HTTP status table is not evidence of Számla Agent forwarding. No operational defect claimed without that trigger.
- **Blank `sikeres`:** `xml.rs:292–295` maps empty to false, yielding `Api(Absent)` rather than a parse error. This exceeds the XSD boolean lexical space and is not backed by a live sample; it still never produces success. Resolve if tightening the shared verdict, separately from missing fields (already rejected).
- **Storno of a negative-gross original/corrective:** `reverses` requires nonpositive returned gross (`envelope.rs:72–75`), which would reject a genuine positive-gross reversal of a negative original. B1/B5 only establish the helper for positive originals/no-op documents. `behaviour.md:224–225` leaves HS/ES/VS storno unverified, and code 14/221 may forbid some candidates. Do not claim all positive-gross storno replies are impossible; a future probe should separate number-change/type/reference verification from this sign heuristic. No new probe was made.
- **Zero-total storno, SS credit entries, sixth-entry server error, empty replacing credits, incoming-invoice credit matching, e-mail behaviour and 56** retain the limitations in the behaviour notes. Constructor/collection tests cannot establish server behaviour for requests the crate refuses or the account never produced.
- **Header-format fallback:** body/header amounts are documented together and normally both available; the actual fractional storno/credit header-only case is unobserved, even though the comma format and local failure are established.
- **Account/settings scope of live facts:** B/D/P48/P60/P73/XPRB evidence is one TEST account on the recorded dates. Its date/appearance/external-id/deletion behaviour is intentionally preferred over conflicting examples, not promoted into a universal vendor guarantee.

## Verification performed and regression coverage gaps

Executed:

```text
cargo test -p szamlazz-agent --lib --test upstream
178 library tests passed; 9 upstream tests passed; 0 failed.
```

These are offline tests; no `--ignored` live suite or `tests/live.rs` was run. The existing upstream test compares cached request outlines and parses cached response examples; it is not a current XSD validator. It cannot detect the newly added official storno/credit response examples or missing NAV fields by itself.

Temporary external consumer:

```text
cargo run --manifest-path /tmp/opencode/04-agent-audit/Cargo.toml --offline
```

It calls only `AgentRequest::parse` on synthetic `RawResponse` values. Observed results are recorded under O4-01/O4-02 and the NAV coverage table. It compiled the workspace crate by path with quick-xml 0.42.0 and rust_decimal 1.43.0; its independent temporary lock resolved some unrelated transitive patch versions differently from the workspace. Both the targeted workspace suite and the external consumer passed compilation; reproductions are not server observations.

Priority regression additions for a follow-up implementation:

1. Single-complete-document/path-aware parsing tests from O4-01, shared boundary and NAV pull parser.
2. Header decimal-separator tests from O4-02, covering both storno and credit balance.
3. A genuine NAV 3.0-shaped synthetic sample with common/base namespaces, all business fields, and multiple addresses; existing “3.0” test is a namespace string replacement and existing address tests each cover one address.
4. Full optional-field request cases validated against the fetched current schemas, including both credential modes, all storno settings/header/email/tax fields, five-entry credits, both deletion selectors and leading-zero taxpayer prefix. Existing golden/order tests cover representative subsets; the field-by-field comparison here is static review, not a claim that exhaustive schema validation was executed.
5. Refresh workspace-only fixture provenance/coverage for current S-O/C-O/T-O if maintaining the official corpus. Do not copy the upstream corpus into published packages, and do not treat the docs' abbreviated base64 (`....`) as a valid complete PDF test fixture.

The accepted restrictions above should be retained when adding these tests. No production changes were made by this audit.
