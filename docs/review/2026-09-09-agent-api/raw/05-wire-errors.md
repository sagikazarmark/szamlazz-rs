# 05 — Számla Agent transport, XML envelopes and errors

**Audit date:** 2026-09-09. **Code baseline:** `a804c740eb8446211c1cdca3eea4fb93d298d25d` (initial worktree clean).

## Result

Two actionable findings: **one medium-severity catalogue coverage defect** and **one low-severity recovery-documentation defect**. Both are high-confidence discrepancies; downstream impact depends on the caller. No demonstrated critical/high-severity wire defect in this scope.

- **W05-01:** thirteen currently documented codes parse as `Unknown` and receive `OutcomeClass::Unknown`, despite documenting a refusal or missing receipt. Nine are in the general catalogue; four are in the receipt response supplement. This is missing known-code coverage, not a criticism of the defensive fallback for genuinely unknown codes.
- **W05-02:** shared recovery rustdoc tells receipt callers to query by an invoice-only external identifier.

The default endpoint, all eleven multipart action names, file-upload framing, supported invoice attachments, credential injection, native cookie storage, response-version selection and normal XML-envelope paths conform to the current sources examined. Code 56 handling agrees with the current first-party PHP implementation. Deliberate defensive parsing and live-observed differences are excluded from defect counts below.

This was a read-only protocol audit plus local synthetic checks: **no production edits, no account calls, no delegated agents**. The sole repository deliverable is this report. Public documentation, XSD and first-party package downloads were unauthenticated GETs.

## Sources and freshness

All sources in this table were fetched during this audit, rather than inferred from local fixtures. The rendered documentation pages report **`v202608271632`** in their footer. That is the site's reported build version, not a claim that every paragraph changed on that date. Quotes below are from the English page unless explicitly marked Hungarian. Section names identify passages even where the web renderer collapses example/XSD line breaks.

| ID | Current official source | Coverage |
|---|---|---|
| G0 | [Számla Agent introduction](https://docs.szamlazz.hu/agent/) and [How it works](https://docs.szamlazz.hu/agent/basics/how-does) | General flow; old ZIP explicitly dated 2019, not used as current authority |
| G1 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | Endpoint, all action names, POST, one XML/document, XSD/case sensitivity |
| G2 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Agent key, lowercase, legacy credentials, account selection |
| G3 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | JSESSIONID, 90-minute inactivity expiry, refresh after account changes |
| G4 | [Network and security](https://docs.szamlazz.hu/agent/basics/security) | Current network ranges; no transport timeout mandate on this page |
| E1 | [Error handling and catalogue](https://docs.szamlazz.hu/agent/basics/error-handling) | Entire general catalogue, retry ceiling, v1 errors |
| E2 | [Hungarian catalogue](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Independent locale cross-check of catalogue and five-call ceiling |
| I1 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request) | Multipart requirements and attachments |
| I2 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | v1/v2, headers, XML verdict and inline reply schema |
| I3 | [Invoice XML + XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml) | Current inline request schema, credentials/settings order, preview |
| I4 | [Invoice notification e-mail](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Five files, 2 MB each, no-mail behavior, partial attachment delivery |
| I5 | [Order number and duplicate checking](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | 71/152, existing document, conditional replay, storno/corrective exemptions |
| R1 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Current v1/v2 and inline reply schema |
| R2 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response) | Current v1/v2, headers and inline reply schema |
| R3 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | Current v1/v2, code 7 and inline reply schema |
| R4 | [XML-query response](https://docs.szamlazz.hu/agent/querying_xml/response) | `szamla` success versus `xmlszamlavalasz` failure |
| R5 | [Proforma-delete response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | XML verdict, 335, critical text/HTML and inline schema |
| R6 | [Receipt-create response](https://docs.szamlazz.hu/agent/generating_receipt/response) | Receipt envelope/schema, duplicate call id, 336–340 |
| R7 | [Receipt-query response](https://docs.szamlazz.hu/agent/querying_receipt/response) and [XML + XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml) | Shared receipt reply; supported query identifiers; current inline request schema |
| R8 | [Receipt-send response](https://docs.szamlazz.hu/agent/sending_receipt/response) | Verdict/schema; code 7 can mean missing e-mail subject |
| R9 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | NAV envelope, numeric Agent error example, `OK` plus false validity |
| X1 | [Invoice request XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Cross-cutting settings and difference from current inline schema |
| X2 | [Invoice reply XSD download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Required boolean verdict; optional strings, doubles, base64 |
| X3 | [Storno request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | Credentials/settings sequence; response-version position |
| X4 | [Credit-entry request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd) | Credentials/settings sequence; response-version position |
| X5 | [PDF-query request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd) | Root-level credentials; required response-version element |
| X6 | [XML-query request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd) | Root-level credentials; no response-version element |
| X7 | [Receipt-create XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd), [storno XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd), [query download](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd), [send XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd) | Shared auth/roots; query download is older than R7 inline schema |
| X8 | [Receipt reply XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd), [send-reply XSD](https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd) | Shared verdict/root contracts |
| X9 | [Taxpayer request XSD](https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd), [proforma-delete XML + inline XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml) | Shared auth/roots |
| P1 | [First-party PHP response guidance](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Issuance success despite notification failure |
| P2 | [Current PHP package page](https://docs.szamlazz.hu/php/) and [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Numeric code 56 and `szlahu_down` confirmed in first-party source, fetched and read in memory |

**Local evidence:** `fixtures/SOURCES.md:34–135` supplied the official URL inventory and documented inline/download disagreements; `docs/szamlazz-hu-behaviour.md:1–308` was read in full, particularly `137–160`, `174–181`, `255–261`. These are provenance/live evidence, not substitutes for current official docs. The old `/agent/basics` guess returned 403; the actual individual pages were discovered through the current [sitemap](https://docs.szamlazz.hu/sitemap.xml) and fetched successfully. Current XSD content is on `/xml` and `/response` tabs, rather than relying on the historical `/xsd` paths in the inventory.

**Schema boundaries:** all eleven operations' auth/root shapes were checked from the sources above; the four version-selecting writers were compared to the corresponding settings sequences. Invoice/receipt document internals, arithmetic and NAV's full taxpayer schema are not re-audited here. No claim of full automated validation of every generated request against every current XSD is made. `lxml` was unavailable; schema review was by reading the official XML and targeted ElementTree inspection of downloaded schemas.

## Code coverage and protocol comparison

Paths in this report are repository-relative; abbreviated source paths in tables are relative to **`crates/szamlazz-agent/src/`**. Line numbers refer to the baseline above.

### HTTP, multipart, authentication and sessions

| Concern | Exact code references | Assessment and official evidence |
|---|---|---|
| Endpoint/method | `wire.rs:7–14`; `client.rs:262–288` | G1: “same URL every time: `https://www.szamlazz.hu/szamla/`”; `HTTPS POST`. Exact default match. XML is posted as bytes, not query parameters or URL-encoded form text. |
| Multipart file upload | `wire.rs:66–100`, `387–393` | I1: `multipart/form-data`, main file `action-xmlagentxmlfile`; E1 code 53: “XML was probably not sent as a file”. Main part has both `name` and `filename`, `text/xml`, CRLF delimiters and a closing boundary. Filename need not end in `.xml`: no such requirement found. No extra HTML submit field is needed. |
| Multipart binary integrity | `wire.rs:78–96`, `102–127` | Attachment bytes appended unchanged; framing collision checked against XML and attachment content, extending the boundary. Filename CR/LF stripped and quote/backslash escaped. Deliberate framing defenses, not vendor deviations. |
| Attachment capability | `ops/invoice.rs:328–465`, `517–519`, `880–890` | I4: “up to 5 files”, `attachfile1` … `attachfile5`, “Size limit per file: **2 MB**”. Supported, including filenames and MIME types. Private bounded collection enforces count/size through `push`, vector conversion and serde. Decimal 2,000,000-byte interpretation is explicitly documented and conservative. |
| Attachment/email coupling | `ops/invoice.rs:266–274`, `880–890` | I4: with `sendEmail=false`, attachments “are not processed”; bad files notified separately, valid attachments still delivered. The crate may upload attachments when mail is disabled; the server's documented no-op applies. A 2 MB local rejection is intentionally stricter than sending an oversized file and receiving only the valid attachments. No attachment-delivery result exists in the examined XML schema. |
| Agent key / user-password | `credentials.rs:5–25`, `45–77`; `xml.rs:251–261` | G2: use `<szamlaagentkulcs>` or username/password; exact element spellings/order. Exactly one credential mode is serialized. Key bytes are preserved, not lowercased automatically. G2's “only in lowercase” remains the caller's credential-validity responsibility; uppercase credentials are allowed to fail normally. |
| Credential placement | `ops/invoice.rs:684–702`; `ops/storno.rs:164–182`; `ops/credit_entry.rs:225–237`; `ops/query_pdf.rs:54–70`; `ops/query_xml.rs:523–540`; `ops/receipt.rs:204–212,317–325,390–398,469–475`; `ops/proforma.rs:52–65`; `ops/taxpayer.rs:168–177` | X1/X3–X9 and I3/R7 agree: PDF/XML query credentials directly under the root, others in `beallitasok`. `Credentials`' general “every request” settings-block rustdoc is imprecise, but implementation is correct. |
| Native cookie reuse | `client.rs:209–237`, `262–288`; `wire.rs:308–326` | G3: save/reuse `JSESSIONID`; deleted after “inactive for 90 minutes”. Default native reqwest cookie jar handles response cookies and later requests; fresh `Client::new` creates a fresh jar. Cloning a client shares its session with the same credentials. Custom transports can use `session_cookie()`. |
| Client injection / session lifecycle | `client.rs:124–155`, `232–237` | A supplied reqwest client carries its own cookie/timeout/redirect policy. Sharing its jar across differently authenticated clients is possible; there is no library isolation guarantee for this hook. Default clients are separate. G3 advises a new cookie after company/e-mail changes: construct a fresh default client; no explicit reset API. Persistence across process restarts is optional, not provided by default. |
| Cookie helper detail | `wire.rs:316–325` | Scans repeated `Set-Cookie` entries and returns the first prefix starting `JSESSIONID`, without attributes. Exact-name check would be more precise (`JSESSIONIDOTHER` also matches), but no evidence the vendor emits such a competing name: robustness edge, not an official-protocol finding. Bundled client uses reqwest's jar, not this helper. |
| Timeout/retries/redirects | `client.rs:198–228`, `262–288`; `error.rs:256–274` | Native default timeout 60 s; no application retry loop; redirects disabled. G1 requires a direct endpoint; G4 specifies no timeout. E1 says at most **five** sends of the same request, never retry until success. The client does not spend an automatic application retry budget; caller responsible for repeat invocations. No mandate to increase/decrease 60 s found. Custom HTTP clients and browser WASM own these policies. |
| TLS / overrides | `client.rs:113–121,158–175,219–223`; `Cargo.toml:24–25` | Default uses HTTPS/rustls and hostname URL, no pinned vendor IPs. HTTP endpoint override deliberately permits local mocks/proxies. It is not the production default and is not counted as violating G1. Browser CORS/session feasibility was not established by current docs or executed in this audit. |
| Diagnostic secrecy | `credentials.rs:27–30,86–96`; `wire.rs:43–50,148–173`; `error.rs:583–619` | G2: treat key as password. Debug omits XML body/key/password and redacts cookies. Explicit raw XML/body access remains possible by design. Error body excerpts are bounded and character-safe; they are not a general secret scrubber. |

### Complete action-name check

All entries match G1 verbatim. The action is the **form field**, not a URL suffix and not the XML root.

| Operation | `AgentRequest::ACTION` | Definition |
|---|---|---|
| Invoice create | `action-xmlagentxmlfile` | `ops/invoice.rs:625–627` |
| Invoice storno | `action-szamla_agent_st` | `ops/storno.rs:160–162` |
| Credit entry | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:213–215` |
| PDF query | `action-szamla_agent_pdf` | `ops/query_pdf.rs:50–52` |
| XML query | `action-szamla_agent_xml` | `ops/query_xml.rs:519–521` |
| Proforma delete | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:48–50` |
| Receipt create | `action-szamla_agent_nyugta_create` | `ops/receipt.rs:170–172` |
| Receipt storno | `action-szamla_agent_nyugta_storno` | `ops/receipt.rs:313–315` |
| Receipt query | `action-szamla_agent_nyugta_get` | `ops/receipt.rs:386–388` |
| Receipt send | `action-szamla_agent_nyugta_send` | `ops/receipt.rs:465–467` |
| Taxpayer query | `action-szamla_agent_taxpayer` | `ops/taxpayer.rs:164–166` |

### XML serialization and response formats

| Concern | Code references | Assessment |
|---|---|---|
| Request encoding | `xml.rs:19–40,209–241`; `wire.rs:387–414` | XML 1.0 UTF-8 declaration, default namespace, escaped leaf text; true/false, plain decimal, ISO civil dates. Matches examples and schema lexical types. Complete generated UTF-8 text scanned for XML 1.0 prohibited characters. `write_xml()` alone bypasses `to_wire()` validation; the bundled client calls `to_wire()`. |
| Ordering | `xml.rs:195–219`; `ops/invoice.rs:684–702`; `ops/storno.rs:169–182`; `ops/credit_entry.rs:230–237`; `ops/query_pdf.rs:59–70` | I3: “order of the fields is fixed, they cannot be interchanged.” Writer preserves sequence. In particular version follows `szamlaLetoltesPld` on create, follows `guardian` on storno, follows `aggregator` on credit entry, precedes external id on PDF query. Correct against X1/X3–X5. No need for emitted `xsi:schemaLocation`: it is a schema hint, not a required business element. |
| Version pin | `ops.rs:27–31`; version-writing lines `ops/invoice.rs:693`, `ops/storno.rs:180`, `ops/credit_entry.rs:236`, `ops/query_pdf.rs:67` | I2/R1–R3: `2` gives structured XML. All four writers pin `RESPONSE_VERSION="2"` in their correct position. Other operations have their own XML response and no such selector. |
| v1 coverage | `ops.rs:27–31`; `xml.rs:47–108`; `ops/envelope.rs:167–182` | v1 is deliberately not selectable or normally parsed: invoice/storno text `xmlagentresponse=DONE;{number}` or raw PDF, credit text `xmlagentresponse=DONE`, PDF query raw PDF, failures `[ERR]…`. Correct not to interpret arbitrary text as success after requesting v2. Header errors still work with a text body; special 56 path discussed below. |
| Envelope identification | `xml.rs:63–108,115–188`; `ops/envelope.rs:19–21,258–264` | Full-body UTF-8 check; root local name **and resolved namespace** verified before serde. Prefix spelling is immaterial. Invoice envelope matches X2; generic verdict also used for proforma/receipt/send roots. This is envelope checking, not full namespace-aware XSD validation of every descendant. |
| Success/failure fields | `xml.rs:115–146,152–176,269–312` | `sikeres` required; accepts XML boolean `true/false/1/0`. Failure reads `hibakod` and `hibauzenet`, preserving open code tokens; missing/blank code is `Absent`, never a fabricated numeric code. Optional payloads can be absent. Empty booleans/trimmed optionals and ignored extra elements are deliberate leniency. |
| Shared creation payload | `ops/envelope.rs:88–156,193–229`; `ops/invoice.rs:865–877` | XML body wins over header fallback for number, totals and buyer URL. Number required for issued outcome; unnumbered success accepted as preview only when requested and carrying a PDF. I3 defines preview as no actual document. Storno requires number (`ops/envelope.rs:251–255`). |
| Numeric types | `ops/envelope.rs:146–155,278–303`; `xml.rs:315–329` | XML schema totals are `double`; finite values representable as Decimal are accepted, including exponent notation (locally checked). NaN/infinities/out-of-range money rejected deliberately; not a new defect for refusing nonfinancial values. Raw numeric headers are **not** URL-decoded, matching I2. |
| PDF | `ops/envelope.rs:133–135,227`; `types.rs:104–116`; `ops/query_pdf.rs:75–83` | Base64 decoded with whitespace removal. PDF optional on issue; required on PDF query and preview. Official abbreviated `....`/`...` samples are not valid real PDFs and correctly do not decode. |
| Other reply envelopes | `ops/query_xml.rs:544–584`; `ops/proforma.rs:70–75`; `ops/receipt.rs:490–495,653–657`; `ops/taxpayer.rs:181–184,201–219` | R4 document/error alternatives, R5 delete verdict, R6/R8 receipt verdicts, R9 NAV envelope all respected. NAV `funcCode`, not `sikeres`, decides taxpayer results; `OK` plus `taxpayerValidity=false` is data. Full document projections are outside this report. |

### Headers, decoding and actual precedence

I2 explicitly distinguishes `szlahu_szamlaszam` and `szlahu_error` **“URL encoded”** from totals and error code **“not URL encoded”**. `wire.rs:179–189,220–243,329–337` performs case-insensitive lookup and decodes percent escapes plus form-style `+` only when `szlahu()` is requested. Error codes are read raw/trimmed (`256–264`), as are totals (`ops/envelope.rs:295–303`) and document id (`278–283`). Body text is XML-decoded, not URL-decoded. This correctly preserves literal percent escapes and plus signs in XML URLs/messages.

`szlahu_fizetesmod` is exposed for credit entries via `ops/credit_entry.rs:281–286`; number/totals/URL are exposed on created invoices; `szlahu_id` and `szlahu_kintlevoseg` have first-party PHP and local live support even though the current I2 header table omits them. `szlahu_vevoifiokurl` uses the URL-decoding helper; first-party PHP also decodes this header (P2 `InvoiceResponse.php:136–137`). Its exact escaping rules beyond documented examples remain unspecified; do not double-decode a URL already read from XML.

Actual precedence, read directly from `wire.rs:286–305`, `xml.rs:152–160` and `ops/envelope.rs:167–199`:

1. Nonblank `szlahu_down` → `ServiceUnavailable` (`Unknown` outcome).
2. Nonblank header error code → `ApiError`, regardless of HTTP status. Creation/storno accept code 56 provisionally.
3. With neither of those headers, known non-2xx status → `HttpStatus` **before reading the body**.
4. Otherwise identify/read XML verdict. Failed verdict → body `ApiError`; successful verdict → payload.
5. For creation/storno, any non-56 body error overrides provisional header 56. A number plus only 56 → successful numbered document with notification warning. No number plus 56 → that API error, class `Unknown`.

The ordinary error path is correct for documented responses. **There is no official tie-breaker for contradictory status/header/body combinations.** Header-first refusal, empty-header tolerance and non-2xx defenses are accepted policy, not findings. `wire.rs:129–136` overstates “headers and body first”: only error/down headers are consulted before HTTP status. A body-only code 7 with status 500 becomes `HttpStatus`, and a success-number header alone does not bypass it. No examined official source requires this inconsistent combination; flag the documentation precision, not a conformance defect. Similarly `sikeres=true` plus a body code is ignored by `Verdict::api_error`; contradictory XML is not a documented error form.

#### Code 56: side effects and source strength

The general E1/E2 tables currently **omit 56**. That does not justify deleting the variant. P1 says: “an invoice is successfully issued, but … the invoice notification cannot be delivered” and exposes `hasInvoiceNotificationSendError()` separately from success. In the currently downloadable official P2 package, relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:14–17`: `INVOICE_NOTIFICATION_SEND_FAILED = 56`.
- `Response/InvoiceResponse.php:314–323`: after an error is detected, `hasInvoiceNumber() && hasInvoiceNotificationSendError()` makes `isError()` false. The Hungarian comment says issuance succeeded when notification failed **and the response contains an invoice number**.
- `Response/SzamlaAgentResponse.php:147–152`: nonblank `szlahu_down` checked before body processing.

Rust's `ops/envelope.rs:167–229` implements the same number-dependent success rule, for either error channel. Malformed optional totals/PDF are dropped on this warning so they cannot hide known issuance. Header 56 plus non-XML body is tolerated if the header names the document. Without a number the result stays `Api(56)` / `Unknown`; it never becomes a settled refusal. `error.rs:303–320` and `client.rs:62–82` preserve this uncertainty.

I2/R1 say error headers omit invoice number/amounts; the P2 56 special case is more specific first-party evidence and is kept. Local probes could not trigger 56 (`docs/szamlazz-hu-behaviour.md:153,180–181`), so this audit claims **current first-party implementation corroboration, not live verification**. No new 56 defect found. `QueryInvoicePdf` reuses `parse_issued`, then requires its PDF; no current docs say a PDF query emits notification failure, so that reuse does not establish an additional supported warning channel.

## Explicit catalogue-to-code/class comparison

**Legend:** R = `Rejected`; U = `Unknown`; D = `DuplicateOrderNumber`; N = `NotFound`. “Retry” means `ErrorCode::is_retryable()`, not permission to resend a write. Mapping and reverse token table are at `error.rs:177–254`; classification at `295–349`; retry and credentials at `256–293`; open token parsing at `382–420`.

### Entire current general catalogue (E1, cross-checked E2)

| Official code | Official meaning (abridged) | Rust variant | Class / retry | Assessment |
|---|---|---|---|---|
| 1 | Maintenance/internal error; try in a few minutes | `Maintenance` | U / yes | Conservative uncertainty appropriate; temporary retry guidance documented |
| 3 | Login failed | `InvalidCredentials` | R / no | Correct credential refusal |
| 53 | Missing XML file | `XmlNotAFile` | R / no | Correct upload refusal |
| 54 | E-invoice not permitted | `EInvoiceNotEnabled` | R / no | Correct account prerequisite refusal |
| 55 | E-invoice signature unsuccessful | `EInvoiceSigningFailed` | U / yes | Uncertainty accepted; see evidence caveat below |
| 57 | XML reading/XSD error | `MalformedXml` | R / no | Correct |
| 71 | Duplicate order number | `DuplicateOrderNumber` | D / no | Correct; I5 describes refusal versus successful replay |
| 135 | Browser session active | `BrowserSessionActive` | R / no | Correct credential/session refusal |
| 136 | Login blocked; resolve account issue | `LoginBlocked` | R / no | Correct credential/account refusal |
| 152 | Duplicate order number, names order | `DuplicateOrderNumberNamed` | D / no | Correct |
| 164 | User accesses multiple accounts | `MultipleAccounts` | R / no | Correct credential/account-selection refusal |
| 202 | Empty/unregistered invoice prefix | `UnregisteredPrefix` | R / no | Correct classification; variant docs emphasize unregistered case |
| 259 | Item net mismatch, product name | `NetValueMismatch` | R / no | Correct |
| 260 | Item VAT mismatch, product name | `VatValueMismatch` | R / no | Correct |
| 261 | Item gross mismatch, product name | `GrossValueMismatch` | R / no | Correct |
| 262 | Item net mismatch, row number (English) | `NetValueInvalid` | R / no | Correct; Hungarian text says name, locale discrepancy is not a code defect |
| 263 | Item VAT mismatch, row number | `VatValueInvalid` | R / no | Correct |
| 264 | Item gross mismatch, row number | `GrossValueInvalid` | R / no | Correct |
| 363 | HUF receipt item gross must be integer | `Unknown("363")` | U / no | **W05-01: documented refusal should be R** |
| 364 | HUF receipt item net ≤2 decimals | `Unknown("364")` | U / no | **W05-01: R** |
| 365 | HUF receipt item VAT ≤2 decimals | `Unknown("365")` | U / no | **W05-01: R** |
| 537 | Maximum 400 data-erasure codes/item | `ErasureCodeLimit` | R / no | Correct |
| 538 | Data-erasure codes unavailable in demo/test | `ErasureCodesUnavailable` | R / no | Correct |
| 539 | Data-erasure setting disabled | `ErasureCodesDisabled` | R / no | Correct |
| 551 | Simplified invoice incompatible with OSS/non-Hungarian seller tax number | `Unknown("551")` | U / no | **W05-01: R** |
| 552 | Too many simplified invoice items | `Unknown("552")` | U / no | **W05-01: R** |
| 553 | Invalid simplified invoice VAT rate | `Unknown("553")` | U / no | **W05-01: R** |
| 554 | Simplified invoice cannot be corrected | `Unknown("554")` | U / no | **W05-01: R; reachable without sending `simpleItems`** |
| 555 | Simplified final/prepayment VAT rates differ | `Unknown("555")` | U / no | **W05-01: R** |
| 556 | Simplified image forbidden on delivery note/corrective | `Unknown("556")` | U / no | **W05-01: R** |

**Count:** 30 general catalogue entries; 21 named correctly; 9 currently unmatched. Named variants in Rust also total 30, but these are not the same set.

### Operation supplements, first-party special code, and live-only additions

| Code | Source / meaning | Rust variant | Class / retry | Assessment |
|---|---|---|---|---|
| 7 | R3/R4 missing invoice selector; R8 missing subject | `MissingData` | N / no | Correct documented class contract includes missing write fields; callers must not infer “document never existed” from every 7 |
| 335 | R5 proforma missing/deleted | `ProformaNotFound` | R / no | Correct settled delete refusal; existence is not issuance |
| 336 | R6 prefix already used for invoices | `Unknown("336")` | U / no | **W05-01: R** |
| 337 | R6 receipt prefix invalid | `Unknown("337")` | U / no | **W05-01: R** |
| 338 | R6 call id already exists | `DuplicateReceiptCallId` | R / no | Correct: this call issued no duplicate; does not mean the earlier call issued nothing |
| 339 | R6 receipt number absent | `Unknown("339")` | U / no | **W05-01: N, or explicit operation-specific missing-receipt handling** |
| 340 | R6 paid amount differs from gross | `Unknown("340")` | U / no | **W05-01: R** |
| 56 | P1/P2 notification failed after issue | `InvoiceNotificationDeliveryFailed` | U / no, unless parsed as numbered success | Correct special case, examined above |
| 14 | Live B5 storno of reversal | `StornoOfReversalInvoice` | R / no | Accepted live addition (`behaviour.md:88`) |
| 73 | Live C6 referenced prepayment not identifiable/settled | `PrepaymentInvoiceNotIdentifiable` | R / no | Accepted live addition (`behaviour.md:119`) |
| 221 | Live B7 invoice has corrective | `HasCorrectiveInvoice` | R / no | Accepted live addition (`behaviour.md:89`) |
| 352 | Live B3/P48 storno issue date not today | `IssueDateMustBeToday` | R / no | Accepted live addition (`behaviour.md:90–95`) |
| 463 | Live D8 credit entry on reversed invoice | `PaymentOnReversedInvoice` | R / no | Accepted live addition, body-only (`behaviour.md:135`) |

Thus the general catalogue plus explicitly examined operation supplements has **37 distinct codes**, **24 named** and **13 missing**. Adding corroborated 56 and the five live-only variants accounts for all 30 existing named Rust variants. No existing named numeric token maps to the wrong variant. Truly unknown tokens, NAV textual errors, out-of-u16 codes and absent codes remain conservatively U; preserve that behavior.

**Classification evidence caveats:** E1 describes 55 signing failure, not “issued, signing failed” as an established fact. `error.rs:261–264` states the latter more strongly than the fetched source supports; U is still the correct conservative choice. The current official pages do not explicitly prescribe retrying 55 or its delay; certificate expiry may require operator action. The `is_retryable` documentation's “at most ~5” (`256–258`) should say the exact ceiling **5**, and distinguish a potentially transient timestamp outage from an expired certificate. Likewise `error.rs:299`/`854–857` say the table was verified/observed, but local evidence explicitly says credential codes and 56 were not observed. These are provenance/documentation precision issues, not grounds to weaken uncertainty or change the four credential classifications.

## Findings

### W05-01 — Current documented refusals remain unknown outcomes

**Severity:** Medium. **Confidence:** High in catalogue/mapping discrepancy and reproduced behavior; medium in downstream operational impact, which depends on how callers use `OutcomeClass`.

**Official evidence / quotes:**

- E1 code 363: “the gross value (`<brutto>`) of the item must be a whole number on HUF receipts”; 364/365 require at most two decimal places.
- E1 code 554: “An invoice issued with the simplified invoice image cannot be corrected, **even if the corrective request does not contain the `simpleItems` field**.”
- E1 code 553: “A single item with an invalid VAT code **rejects the whole document**.”
- R6 codes 336/337/339/340: “Prefix already used for invoices”; “Prefix format invalid”; “Receipt number does not exist”; “Paid amount differs from gross amount.”

**Code:** `crates/szamlazz-agent/src/error.rs:220–253` omits `336,337,339,340,363,364,365,551,552,553,554,555,556`. `382–395` maps them to `Unknown(String)`; `314–320` gives U; `256–274` gives retryable=false. `ops/invoice.rs:869–877` and the shared `ops/envelope.rs:168–189` propagate such API errors, as do receipt verdicts through `xml.rs:130–145`. Existing completeness tests only enumerate the crate's own named set (`error.rs:690–734,859–913`), so they do not detect a missing official code.

**Concrete triggers:**

1. A HUF receipt with gross `100.5` and otherwise suitable data receives documented 363. A failed `xmlnyugtavalasz` carrying that code becomes `Api(Unknown("363"))`, U.
2. Correct an existing invoice created elsewhere with the simplified image. The crate need not support `simpleItems` on requests for documented 554 to occur. A failed `xmlszamlavalasz` carrying 554 becomes U instead of a settled refusal.
3. Query a nonexistent receipt number and receive 339: the caller cannot identify it through the typed missing-document class.

**Impact:** error text/code are retained, so this is not swallowed failure or false success. But callers using the documented outcome API cannot distinguish these settled refusals from “may already have issued”; they may enter unnecessary reconciliation, delay reporting an input problem, or page an operator for an uncertain write. `is_retryable=false` does not correct the separate outcome API. The crate itself performs no automatic resend. These effects are inferred from its public contract, not asserted as observed downstream incidents.

**Fix:** add named variants and token round-trips for all thirteen codes; classify twelve refusal codes R and the missing-receipt code 339 N (updating N's documentation beyond invoice code 7), or explicitly model its missing-receipt outcome. Preserve `Unknown` for codes without known meaning. Document each source and distinguish general-catalogue from operation-supplement entries.

**Regression checks:** table-test the complete externally sourced catalogue independently of the `NAMED` list; numeric/string/reverse-token checks; body-only 363/339 receipt errors and 554 invoice error; header form 554; class R/N and retry=false as appropriate. Keep future code `999` and nonnumeric NAV code U. Tests must assert the source-derived expectation, not only that current enum variants round-trip.

### W05-02 — Receipt recovery guidance names an unsupported external-id query

**Severity:** Low (public guidance, not an automatic send path). **Confidence:** High.

**Official quote:** R7: “identify the receipt to query by **receipt number** (`nyugtaszam`) or by **order number** (`rendelesSzam`)”; `hivasAzonosito` is “an optional unique call identifier”. R6: reusing the call id makes the call unsuccessful and “will not duplicate an existing receipt”. Invoice-only I3 places `szamlaKulsoAzon` in the invoice settings schema.

**Code:** `crates/szamlazz-agent/src/error.rs:265–268`: “Before re-sending a create, storno or **receipt**, query by the external id (`szamlaKulsoAzon`) the request carried”. The shared U guidance repeats the invoice external-id recipe at `368–370`; `client.rs:69–72` gives it for all calls. But `ops/receipt.rs:204–230` has a receipt call id/order number, not that external id; `342–370,390–409` offers only receipt/order selectors plus optional call id.

**Concrete trigger:** a receipt-create response is lost before the caller learns its receipt number. Following `ClientError::outcome_class()` recovery docs leads the caller to an invoice external-id query that cannot locate this receipt; that query's not-found answer cannot establish that no receipt was issued.

**Impact:** recovery instructions cannot be carried out as written; adapting the invoice recipe blindly could cause duplicate issuance if no stable receipt call id was supplied. This is conditional caller behavior, not a claim that the current crate retries incorrectly.

**Fix:** qualify external-id reconciliation as invoice/storno guidance. For receipts, describe keeping a stable `hivasAzonosito` on the original request, reuse returning 338 rather than replaying success, and the supported receipt/order lookup channels. Do not promise that the query's optional call-id field is an independent selector without vendor confirmation. Keep the five-send ceiling and operator resolution where no conclusive lookup exists.

**Regression checks:** a compiling documentation example using actual `ReceiptSelector` variants; a synthetic timeout/reuse scenario demonstrating retained call id and 338 as refusal of this duplicate send, not proof that the first send failed. No live call needed to check the guidance matches the exposed API.

## Accepted deviations, missing optional capabilities and uncertainties

### Accepted / excluded from findings

- **Live differences:** body-only query/credit errors and missing delete-success headers (`docs/szamlazz-hu-behaviour.md:109,135,141–145`) are handled. Do not impose universal header presence from old descriptions. Repeat storno/no-op reversal semantics (`86–98`) justify `CreatedInvoice::reverses` (`ops/envelope.rs:56–75`); this audit does not replace them with schema-only assumptions.
- **Defensive parsing:** U for unknown/absent codes, unparseable replies, HTTP failures, transport errors and `szlahu_down` (`error.rs:666–681`; `client.rs:74–82`); 56 optional-value leniency; unnumbered non-preview refusal; optional id leniency; extra/empty element tolerance; strict root identity. These are intentional safeguards, not conformance failures.
- **Finite Decimal representation:** normal/exponent finite values accepted; unsupported nonfinancial IEEE values not demanded simply because XSD uses `double`. Outbound decimals and booleans are valid lexical forms. No byte-boundary wire-text panic was found in the shared code.
- **Stored schemas:** today's X1 download still lacks `csoportazonosito`/`torloKod` present in I3 inline schema; X7 query download lacks `rendelesSzam` present in R7. This corroborates `fixtures/SOURCES.md:124–135`; do not “fix” current writers against stale downloads. Receipt-create download also omits the erasure element; receipt item capability is outside this cross-cutting audit.
- **Comma header totals:** local live evidence reports `szlahu_nettovegosszeg=100,01` (`behaviour.md:160`). Scratch parsing confirms comma fallback headers fail `parse_decimal_header`, whereas body totals take precedence. Current official pages only say “not URL encoded”, without specifying decimal separator. This is a separately evidenced live-format limitation, **not counted as a current-doc conformance finding** under the requested exclusion. A future live-format follow-up should cover missing-body-total fallback without changing XML decimal grammar.

### Missing optional capabilities / documented limits

- **v1 transport mode:** deliberately unavailable through built-in operation types; raw-text/raw-PDF parsers are not needed for the v2-only contract (`ops.rs:27–31`). A custom `AgentRequest` can supply another wire operation, but built-in response parsers remain v2.
- **Session persistence/reset:** no exported bundled-client cookie jar, persistence or explicit refresh method. Rebuild a default client after account-data edits; a supplied client can manage its own jar. Also document the per-credential jar boundary on the injection hook. Current G3 treats absent cookies as slower reauthentication, not rejected requests.
- **Legacy key-in-both-fields mode:** G2 supports sending the same agent key as `felhasznalo` and `jelszo`. There is no special helper, but `Credentials::user_password(key, key)` can express it; no missing wire capability.
- **Successful payment-method header:** I2/R1 list `szlahu_fizetesmod`; `CreatedInvoice` (`ops/envelope.rs:27–54`) does not expose it, while `InvoiceBalance` does. Additional optional metadata, not failure to issue or parse the documented success body.
- **`simpleItems`:** I3 now includes optional `fejlec/simpleItems`; `InvoiceHeader` (`ops/invoice.rs:138–187`) has no field and the writer proceeds from template to preview without it (`763–768`). Missing optional invoice feature, recorded for coverage only; detailed invoice-request audit owns its design. It does **not** excuse missing 554 because a corrective of an externally created simplified invoice can receive it.
- **Attachment delivery accounting:** no per-file delivery status in the supported reply. I4 documents separate notifications for invalid files and no processing when mail disabled. Presence of bytes in an accepted request is not delivery confirmation.

### Remaining uncertainty / evidence limits

- Current docs describe no universal HTTP-status policy or contradictory-channel precedence, no mandated native timeout, no session-cookie invalidation semantics when a key is deleted, and no guarantee that changing XML credentials overrides a preexisting session. No live experiments were attempted. Custom-client sharing and credential rotation require careful session ownership by the embedder.
- Official auth docs say key deletion is immediate and keys are lowercase/case sensitive. The type deliberately wraps opaque text; it neither validates key shape nor rotates an existing client's credentials. No valid-call conformance defect demonstrated.
- `wire.rs:231–242` calls all `szlahu_*` headers URL encoded in general prose, but the real parsers correctly distinguish encoded textual and raw numeric headers. Unknown malformed percent sequences/UTF-8 are preserved defensively; no official malformed-header requirement exists.
- U is an uncertainty classification for document issuance, not a full classification of **every side effect**: credit-entry mutation, deletion and e-mail sending require operation-specific recovery. `338`/`335`/`7` do not erase the possibility of earlier successful operations.
- The PHP package is corroborating first-party implementation evidence, not an independently versioned wire specification. Its URL handling itself differs between `rawurldecode` on assignment and `urldecode` on access; this is not evidence to copy double-decoding into Rust.
- Browser WASM cookies, cross-origin permission and redirects were not exercised. Native mock tests prove the HTTP shell under the test client's settings; they do not prove browser support or production TLS/network behavior.

## Verification performed and recommended checks

**Executed successfully:**

```text
cargo test -p szamlazz-agent --features client-reqwest --lib --test client
179 unit tests passed; 6 client integration tests passed; 0 failed.
```

These are local/synthetic tests, not `tests/live.rs`. Existing checks cover multipart construction, binary attachments and limits, XML escaping, credentials redaction, header error URL decoding, down/status precedence, 56 with/without number and malformed optional fields, body-only error handling, preview versus issued responses and code-class tables. Test references: `wire.rs:434–618`; `xml.rs:424–620`; `ops/envelope.rs:343–619`; `ops/invoice.rs:1191–1270`; `tests/client.rs:65–246`.

**Additional external scratch executable:** `/tmp/opencode/wire-conformance-20260909`, using a path dependency on this crate, with no repository source/test changes and no HTTP client feature. It confirmed all thirteen W05-01 codes map to U and retry=false; totals `1.27E3`, `1.27e+3`, `-1.27E3` parse; comma decimal headers fail as noted above. An initial scratch-only import mistake was corrected to `szamlazz_agent::wire::{AgentRequest, RawResponse}` before the successful run. Command:

```text
cargo run --quiet --manifest-path /tmp/opencode/wire-conformance-20260909/Cargo.toml
```

**Useful follow-up regression coverage (not executed as new repository tests):** source-derived catalogue table from W05-01; receipt recovery example from W05-02; actual two-request cookie reuse/rotation and separate default-client jars; non-2xx body-only code behavior matching its documented policy; non-56 body error versus header 56; boundary collision in XML plus attachment; UTF-8/percent decoding with `%2B` versus `+` and body URLs retaining their escapes. Tests should distinguish supported documented replies from intentionally inconsistent defensive cases. None requires a live account.
