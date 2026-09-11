# Számla Agent common-layer review

**Reviewed:** `382cf7615aca1d64a05c7c3f77110248dde51950` · **Sources fetched:** 2026-09-10

## Conclusion

The common layer matches the current documented endpoint, all eleven operation
routes, XML authentication, response-version selection and error catalogue.
**No definite protocol-conformance bug or missing documented error mapping was
established.** Unknown and absent codes remain conservative.

Five bounded findings remain: **two robustness issues and three documentation
issues**. Highest priority is **P2**, loss of received header evidence when the
body transfer fails. None establishes a new live-account incident. Unsupported
capabilities and unresolved vendor contradictions are listed separately below.

| ID | Category | Priority | Finding | Confidence |
|---|---|---|---|---|
| R1 | Robustness | P2 | Body-download failure discards received header evidence | High behavior / medium remedy |
| R2 | Robustness | P3 | Endpoint userinfo is accepted and exposed by debug formatting | High |
| D1 | Documentation | P3 | README overpromises bounded, log-ready error displays | High |
| D2 | Documentation | P3 | Browser feasibility discussion omits the vendor's prohibition on client-side keys | High |
| D3 | Documentation | P3 | Two retry descriptions overstate what identical invoice resends do | High |

P2 means a consequential recovery/diagnostic defect worth fixing; P3 means a
bounded hardening or documentation correction. There are no P0/P1 findings.

## Baseline, scope and method

- `git rev-parse HEAD` resolved to the requested SHA. The reviewed crate,
  `fixtures/SOURCES.md`, behavior notes and vendor-question draft had no diff
  against it. Existing unrelated workspace/lockfile/README/CONTEXT changes were
  present and left alone. During the review another session advanced workspace
  HEAD to `7da23b44cd006783eb47e60aa54ab8969bdd1c2c`; a final explicit comparison
  against the requested SHA confirmed these reviewed paths were still unchanged.
  Crate code locations below refer to the requested SHA.
- Full reads: `src/client.rs`, `src/wire.rs`, `src/credentials.rs`, `src/error.rs`,
  `src/recovery.md`, crate `README.md`, `src/lib.rs`, crate manifest. Supporting
  inspection: shared XML credential writer/verdict, shared envelope header
  consumers, all operation action/credential/version sites, relevant tests.
  Paths abbreviated as `src/…` are under `crates/szamlazz-agent/`.
- Read `docs/szamlazz-hu-behaviour.md`, relevant current `CONTEXT.md` vocabulary,
  `fixtures/SOURCES.md`, and
  `docs/research/2026-09-10-agent-vendor-questions.md`. Historical untracked
  `docs/review/2026-09-09-agent-api/FINAL.md` was used as a closure checklist,
  not evidence that its old findings still exist.
- This is a current-tree audit against current vendor documentation, not a diff
  review against the same HEAD. Standards/ownership and protocol evidence were
  checked directly. No production files or repository tests were edited by this
  review. This report is its only repository write.
- Public documentation GETs only; **no Számla Agent account calls**, including
  reads. All reproduction sources, dependency resolution and build artifacts
  are in `/tmp/opencode/agent-transport-audit-382cf761/`.
- Operation-specific XML parsing is owned by other reviews. Shared header
  behavior and recovery examples are included here; this is not an independent
  re-audit of every XML field, schema, arithmetic rule or NAV error taxonomy.

## Official source inventory

All links below were fetched in this review. Current docs pages reported site
build **`v202608271632`**; that is not the acquisition date or proof of when each
statement changed. Quotes are short excerpts, not guarantees inferred from tests.

| Ref | Official URL | Relevant statement/evidence |
|---|---|---|
| B0 | <https://docs.szamlazz.hu/agent/>; <https://docs.szamlazz.hu/agent/basics/what-is> | Current operations and links to the basics |
| B1 | <https://docs.szamlazz.hu/agent/basics/how-does> | “HTTP POST request to `https://www.szamlazz.hu/szamla/`” |
| B2 | <https://docs.szamlazz.hu/agent/basics/sending-requests> | Function selected “using the name of the form field”; eleven-route table; “HTTPS POST”; one document per XML |
| B3 | <https://docs.szamlazz.hu/agent/basics/authentication> | Key “only in lowercase”; user access to “exactly one billing account”; “do not include it in client-side code” |
| B4 | <https://docs.szamlazz.hu/agent/basics/session-cookie> | “inactive for 90 minutes”; new session advised after company/email edits; absent cookie causes authentication each call |
| B5 | <https://docs.szamlazz.hu/agent/basics/error-handling> | “at most five times”; stop for human intervention; 500 test invoices/10 minutes; complete general code table |
| B5-HU | <https://docs.szamlazz.hu/hu/agent/basics/error-handling> | “legfeljebb ötször”; same general numeric code set |
| B6 | <https://docs.szamlazz.hu/agent/basics/security> | Separate inbound CIDRs and outbound partner-call IPs |
| B7 | <https://tudastar.szamlazz.hu/gyik/technologiai-valtozasok-2025> | August 2025 IP changes; certificate issuer switches to Google Trust Services |
| I1 | <https://docs.szamlazz.hu/agent/generating_invoice/response> | Textual number/error “URL encoded”; totals/code “not URL encoded”; version 2 XML |
| I2 | <https://docs.szamlazz.hu/agent/reversing_invoice/response> | Same header table and version distinction, structured examples now present |
| I3 | <https://docs.szamlazz.hu/agent/credit_entry/response> | Optional response headers and XML verdict/balance |
| Q1 | <https://docs.szamlazz.hu/agent/querying_xml/response> | Unknown invoice/order/external id gives “error code 7” |
| Q2 | <https://docs.szamlazz.hu/agent/querying_pdf/response> | Version 2 XML/base64, same code 7 selector failure |
| P1 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response> | 335 example; “On critical error, a plain text/html error message may be returned” |
| P2 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request>; <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml> | Multipart upload and number/order selectors |
| RCP | <https://docs.szamlazz.hu/agent/generating_receipt/response> | Additional 336–340 table; repeated call id “will not duplicate an existing receipt” |
| EMAIL | <https://docs.szamlazz.hu/agent/sending_receipt/response>; <https://docs.szamlazz.hu/agent/sending_receipt/xml> | Code 7 missing email subject; empty-details resend versus omitted email block |
| NAV | <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | Numeric Agent 57 inside NAV result; false validity under OK is data; dated 2.0 examples, link to 3.0 schema documentation |
| ORDER | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number> | Toggle, per-type duplicate check, storno/corrective exemptions, “within the last 2 days” replay condition |
| SIMPLE | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency> | Explicit rejection rules 551–556, including inherited final setting |
| PHP1 | <https://docs.szamlazz.hu/php/valasz-feldolgozas> | “invoice is successfully issued” despite notification failure |
| PHP2 | <https://docs.szamlazz.hu/php/> and [official 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Numeric 56 and number-conditioned success; `szlahu_down` precedence |
| PHP3 | <https://docs.szamlazz.hu/php/nyugta-lekerdezes> | Order query returns “last matching document” |
| PHP4 | <https://docs.szamlazz.hu/php/dijbekero-torles> | “multiple proforma invoices can be deleted” by order; documented rollback on member failure |

The error-list links from the current operation pages point to B5's inline
`#error-codes-and-messages` table, not a distinct downloadable complete list.
Both language tables and the operation supplement/examples were checked.
The NAV documentation link was noted, not represented as a fetched/exhaustively
reviewed NAV catalogue in this common-layer review.

PHP ZIP freshly fetched and inspected in memory: SHA-256
`30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`.
Within `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`,
`Response/InvoiceResponse.php:17` defines 56,
`:319–322` makes number plus notification error nonfatal, and
`Response/SzamlaAgentResponse.php:147–152` checks nonblank `szlahu_down` first.
Its encoding behavior is not universally copied into Rust; see contradictions.

## Findings

### R1 — Preserve received evidence when response-body transfer fails

**Robustness · P2 · High confidence in behavior; medium in API remedy.**

**Code:** `src/client.rs:315–330`, especially `:326`; compare the advertised
precedence at `:299–303`, `src/wire.rs:291–310`, and the deliberate numbered-56
fallback at `src/ops/envelope.rs:179–211`.

The client captures status and headers, but `response.bytes().await?` returns
immediately on an interrupted body. The captured evidence is dropped, and the
caller receives only `ClientError::Transport` / `Unknown`. The operation parser
never sees a conclusive error header or a numbered-56 reply. A stalled body also
delays any header verdict until the transport deadline.

**Reproduction:** a loopback server fully receives the request, returns HTTP 200
with `Content-Length: 1000`, sends one body byte, then closes. With error header
3, and separately with header 56 plus invoice number `I-1`, both calls become
`Transport` / `Unknown`. Passing the latter headers with an empty body directly
to the current storno parser returns `I-1` with the notification flag.

**Official evidence:** I1/I2 define the number/error response headers; PHP1 says
an invoice can be “successfully issued” despite notification failure, and PHP2
implements the explicit number condition. These sources do not specify handling
of an interrupted HTTP body: the defect is evidence loss at the client's
transport boundary, not a demonstrated vendor protocol violation.

**Impact:** recovery loses a received number or precise refusal and performs
unnecessary/incomplete reconciliation. A caller cannot recover the headers from
the returned error. This does not itself cause a duplicate: the current recovery
rules correctly forbid treating `Unknown` as permission to resend.

**Minimal remedy:** retain status/header evidence with a body-transfer failure,
or introduce a narrow operation-aware path for a header verdict independent of
the missing body. Do not turn arbitrary success-number headers into success,
parse a truncated body as complete, or discard a contradictory body when it was
successfully received. Test interrupted error/down/numbered-56 cases separately
from ordinary success and contradictory complete responses.

### R2 — Endpoint credentials bypass debug redaction

**Robustness · P3 · High confidence; conditional on caller-supplied userinfo.**

**Code:** `src/client.rs:104–110,180–198,207–213,272–276`.

The endpoint parser checks scheme and host but accepts
`https://alice:URL_PASSWORD@example.test/szamla/`. Derived `Debug` on the built
`Client` prints `URL_PASSWORD` through `reqwest::Url`; the builder also holds and
prints its original endpoint string. Invalid-endpoint errors echo their input.
The scratch check confirms the URL password is visible while the actual
`Credentials::AgentKey` is redacted.

**Official evidence:** B3 says “Treat the Agent key like a password” and requires
authentication inside XML, not URL userinfo. B2's endpoint contains no userinfo.
The worker's stricter endpoint rule in `CONTEXT.md` is useful ownership context,
not a requirement automatically imposed on this lower-level client.

**Impact:** an integration configuring an authenticated proxy/custom endpoint by
URL can leak that URL credential to diagnostic logs. Normal default-endpoint
users' XML keys are unaffected; this is not a remotely supplied endpoint exploit.

**Minimal remedy:** redact URL userinfo in `ClientBuilder`, `Client` and endpoint
error formatting, or reject userinfo at build time with a redacted error and
document the supported custom-authentication hook. Preserve deliberate HTTP
mock/proxy support. Rejecting alone does not fix a builder/error that still
prints the rejected secret.

### D1 — Limit the bounded-error-display promise to the paths that enforce it

**Documentation · P3 · High confidence.**

**Code:** crate `README.md:349`; actual implementations
`src/error.rs:595–604,614–618,628–645,656–692`, `src/client.rs:40–64`,
`src/wire.rs:155–179`.

The README says “Error displays quote at most a bounded excerpt of an upstream
body … a parse failure can be logged as is.” `body_excerpt` bounds unexpected
body/status excerpts, but `ApiError` formats the entire verbatim message and
`XmlError` forwards its source display. A synthetic 23,000-character
`hibauzenet` produces a **23,021-character** API display; the same text as an
invalid `sikeres` produces a **23,036-character** parse display. `RawResponse`
debug redacts `Set-Cookie` and the body, not every returned header.

**Official evidence:** B5 describes plain-text errors with stack traces and says
“Everything after that can be ignored”; its code 57 entry says the body contains
more information. P1 permits critical HTML/text errors. No source promises that
all diagnostic strings or returned business data are short or nonsensitive.

**Impact:** consumers may rely on an absent bound for logs/faults/journals, or
mistake cookie/body redaction for general data sanitization.

**Minimal remedy:** say precisely that `HttpStatus` and `UnexpectedBody` excerpts
are bounded, while API/parser messages and other headers may contain full
upstream text. Keep `ApiError.message` verbatim; changing stored vendor messages
would destroy a useful existing contract. A separately designed bounded display
is optional, not necessary to correct this documentation finding.

### D2 — Distinguish browser compilation from safe credential deployment

**Documentation · P3 · High confidence.**

**Code:** `src/lib.rs:51–66`, crate `README.md:300–302`, supporting
`src/client.rs:147–150` and `src/credentials.rs:5–9`.

The current browser section accurately explains CORS, hidden cookies and Fetch
credentials, but ends at whether direct access is technically feasible. It omits
the current authentication page's explicit instruction: **“do not include it in
client-side code.”** The same page says every key has identical permissions and
there is no per-key access restriction; this is not a safely scoped public key.

**Impact:** an integrator can read “XML authentication may work without cookies”
as deployment guidance for shipping an account key to browsers. This is a public
documentation gap, not an instruction to remove wasm support: server-side wasm
and a trusted intermediary remain valid consumers of the core.

**Minimal remedy:** add the vendor's server-side key ownership requirement next
to the browser feasibility paragraph and distinguish server-side wasm from
browser-delivered application code. No runtime gate or new authentication flow
is needed.

### D3 — Qualify the identical-resend wording with the vendor's conditions

**Documentation · P3 · High confidence.**

**Code:** `src/client.rs:54–60` says a post-issuance timeout “means a retry issues
a duplicate”; `src/error.rs:77–92`, especially `:85–87`, says a byte-identical
resend while the first document is live returns it instead of an error.

Neither is unconditional. ORDER documents replay only with the duplicate toggle
on, matching buyer/gross/three dates, and an earlier document created **“within
the last 2 days.”** A still-live document outside that window can give 71/152.
With protection disabled a repeat can issue another document. The recorded
short-interval replay and reuse after reversal do not prove indefinite replay.

**Impact:** these API docs respectively exaggerate certain duplication and
understate possible duplicate refusals. The main README/recovery table is more
careful and does not authorize unsafe retries, which limits the severity.

**Minimal remedy:** “can issue a duplicate” in `ClientError::Transport`; describe
the bounded documented replay conditions beside `DuplicateOrderNumber`, keeping
the live observations separately identified. Preserve conservative classification
and the instruction to reconcile before repeating; no retry engine is needed.

## Systematic error-catalogue comparison

**Counts:** 30 general-table codes + five receipt-supplement codes + two distinct
operation-example codes (7, 335) = **37 currently documented numeric codes** in
this Agent corpus. All 37 map to named variants. Add PHP-backed 56 and the five
live-only codes 14/73/221/352/463 = **43 named Rust codes**. No missing mapping,
duplicate numeric mapping or incorrect reverse token was found.

Legend: U = `Unknown`, R = `Rejected`, D = `DuplicateOrderNumber`, N = `NotFound`.
“Retry” is a potentially transient **read hint**, not permission to repeat a
write. Only 1/55 return true. Only 3/135/136/164 are credential errors.
The forward and reverse tables are `src/error.rs:210–313`; classification is
`:349–418`, retry `:315–328`, credential detection `:330–347`.

| Code | Current meaning / Rust variant | Class | Source / assessment |
|---|---|---|---|
| 1 | Maintenance / `Maintenance` | U, retry | B5: internal failure/maintenance does not settle issuance |
| 3 | Login failure / `InvalidCredentials` | R, credential | B3/B5 |
| 53 | Missing XML file / `XmlNotAFile` | R | B5; multipart file, not a text field |
| 54 | E-invoice not permitted / `EInvoiceNotEnabled` | R | B5; entitlement/certificate permission |
| 55 | Signing failed / `EInvoiceSigningFailed` | U, retry | B5; timestamp may recover, expired certificate needs remediation; issuance unproven |
| 57 | XML read/XSD failure / `MalformedXml` | R | B5/NAV; short variant description also encompasses schema-invalid XML |
| 71 | Existing order / `DuplicateOrderNumber` | D | B5/ORDER; exact replay caveat D3 |
| 135 | Browser session / `BrowserSessionActive` | R, credential | B5; documented login condition |
| 136 | Login blocked / `LoginBlocked` | R, credential | B5; account/subscription intervention |
| 152 | Existing named order / `DuplicateOrderNumberNamed` | D | B5/ORDER; message identifies order, not invoice |
| 164 | Multiple accounts / `MultipleAccounts` | R, credential | B3/B5; use an account key |
| 202 | Empty/unregistered prefix / `UnregisteredPrefix` | R | B5; no prefix synthesis |
| 259 | Net = unit price × quantity / `NetValueMismatch` | R | B5 |
| 260 | VAT arithmetic / `VatValueMismatch` | R | B5 |
| 261 | Gross = net + VAT / `GrossValueMismatch` | R | B5; exact receipt rule is operation-owned |
| 262 | Net row error / `NetValueInvalid` | R | B5; EN says row number, HU says name (see contradictions) |
| 263 | VAT row error / `VatValueInvalid` | R | B5 |
| 264 | Gross row error / `GrossValueInvalid` | R | B5 |
| 363 | HUF receipt whole gross / `ReceiptGrossNotWhole` | R | B5 |
| 364 | HUF receipt net precision / `ReceiptNetPrecision` | R | B5 |
| 365 | HUF receipt VAT precision / `ReceiptVatPrecision` | R | B5 |
| 537 | 400 erasure-code maximum / `ErasureCodeLimit` | R | B5 |
| 538 | Demo/test erasure restriction / `ErasureCodesUnavailable` | R | B5 |
| 539 | Erasure setting off / `ErasureCodesDisabled` | R | B5 |
| 551 | OSS/non-Hungarian seller / `SimplifiedImageAccountIncompatible` | R | B5/SIMPLE; OR condition, inherited final included |
| 552 | Simplified item count / `SimplifiedImageItemLimit` | R | B5/SIMPLE; 2, final 4 |
| 553 | Simplified VAT invalid / `SimplifiedImageVatInvalid` | R | B5/SIMPLE; entire document refused |
| 554 | Simplified original cannot be corrected / `SimplifiedImageCannotCorrect` | R | B5/SIMPLE |
| 555 | Final/prepayment VAT mismatch / `SimplifiedImagePrepaymentVatMismatch` | R | B5/SIMPLE |
| 556 | Simplified corrective/delivery note forbidden / `SimplifiedImageDocumentForbidden` | R | B5/SIMPLE |
| 336 | Invoice-used receipt prefix / `ReceiptPrefixUsedForInvoices` | R | RCP |
| 337 | Receipt prefix alphabet / `InvalidReceiptPrefix` | R | RCP |
| 338 | Existing receipt call id / `DuplicateReceiptCallId` | R | RCP; refuses this send, does not recover earlier success |
| 339 | Receipt absent / `ReceiptNotFound` | N | RCP; query/send/storno consumers retain code |
| 340 | Tender sum differs / `ReceiptPaymentMismatch` | R | RCP |
| 7 | Missing data / `MissingData` | N | Q1/Q2/EMAIL; receipt send can mean missing subject, not missing receipt |
| 335 | Proforma absent/deleted / `ProformaNotFound` | R | P1; settled delete refusal, not replayed success |
| 56 | Notification failure / `InvoiceNotificationDeliveryFailed` | U as error | PHP1/PHP2; numbered issuing reply is success with warning; unnumbered stays U |
| 14 | Storno of reversal / `StornoOfReversalInvoice` | R | Behavior notes B5, line 88; live-only |
| 73 | Prepayment not identifiable / `PrepaymentInvoiceNotIdentifiable` | R | Behavior notes C6, line 119; live-only |
| 221 | Original has corrective / `HasCorrectiveInvoice` | R | Behavior notes B7, line 89; live-only |
| 352 | Issue date must be today / `IssueDateMustBeToday` | R | Behavior notes B3, line 90; observed storno, not universal create rule |
| 463 | Credit on reversed invoice / `PaymentOnReversedInvoice` | R | Behavior notes D8, line 135; body-only; credit on SS itself unprobed |

Unknown numeric/textual tokens and values outside `u16` retain their trimmed
string; empty codes become `Absent`, also U (`error.rs:452–490`). Known numeric
padding/leading zeros normalize (`007` → 7); this is documented, not byte-exact
token preservation. `Unknown("3")` manually constructed by a caller also remains
U; only parsed known codes gain settled meaning. Raw percent-encoded error codes
are not decoded (`%33` is unknown). HTTP/parse/transport/down failures are U;
local `RequestError` is R because nothing was sent. None classifies an earlier
unresolved exchange as refused.

## Coverage inventory and positive checks

### Transport and request construction

| Area | Current implementation and verdict |
|---|---|
| Endpoint/method | `wire.rs:7–14`, `client.rs:307–313`: exact HTTPS endpoint and POST; host/scheme checked at build. HTTP override is deliberate caller configuration, not a default downgrade. R2 covers userinfo. |
| Eleven form routes | Independently asserted against B2: invoice `invoice.rs:679`; storno `storno.rs:163`; credit `credit_entry.rs:214`; PDF `query_pdf.rs:59`; XML `query_xml.rs:531`; delete `proforma.rs:61`; receipt create/storno/get/send `receipt.rs:185,333,412,494`; taxpayer `taxpayer.rs:264`. All match. |
| Multipart file | `wire.rs:66–100`: content-disposition name and filename, `text/xml` file part, CRLF delimiters and closing boundary. One generated XML per operation, attachments distinct. No requirement that filename end in `.xml`; routing uses field name. |
| Boundary/injection | `wire.rs:102–125`: CR/LF removed and quote/backslash escaped in attachment disposition; MIME CR/LF removed; deterministic boundary changed if present in XML/binary contents. Built-in action constants are trusted. A custom `AgentRequest::ACTION` is implementer code, not untrusted invoice data. |
| XML/auth encoding | `xml.rs:19–40,414–424,456–466`: XML 1.0 UTF-8 declaration, escaped text, key or username/password in wire order. `wire.rs:402–429` rejects non-UTF-8/forbidden XML characters before HTTP. This is not full XSD validation. |
| Credential location | Root on XML/PDF queries (`query_xml.rs:539`, `query_pdf.rs:67`), settings block on the other nine writers. `credentials.rs:45–47` and `wire.rs:367–369` now describe the exception correctly. |
| Credential choices | Key preferred; alternative username/password supported; same-key-in-both legacy fields is representable through `user_password`. Exact key bytes preserved, never silently lowercased/trimmed. Invalid/empty key rejection remains vendor-owned; lowercase requirement is documented. |
| Redaction | `credentials.rs:27–30,88–98`, `wire.rs:43–50,155–179`: key/password, request body and response cookie/body redacted. Username and ordinary response headers are not secret-free by contract. R2/D1 bound the remaining promises. |
| TLS/DNS | `Cargo.toml:24–25` enables reqwest rustls verification; no certificate/IP pin, verification bypass or hardcoded obsolete address. B6/B7 allowlist management is deployment-owned; outbound IPs are for inbound receivers, not outbound Agent response authentication. |
| Redirects | `client.rs:240–248`: default native redirects disabled; fresh loopback test sees 302 `HttpStatus` and zero follow-up requests. This avoids both method conversion and credential-body forwarding. Injected clients own redirects; browser behavior is not the native guarantee. |
| Timeout | `client.rs:220–253`: 60-second native request deadline; no vendor-mandated timeout found. It covers HTTP transfer, not synchronous serialization/parsing CPU or cancellation of vendor work. Custom 30-ms deadline tested against a 200-ms loopback response: Transport/U. No real 60-second stall run needed to verify the constant. |
| Retries | No application recovery loop. Supplied transport policies can resend and remain caller-owned (`client.rs:131–150`, `recovery.md:4–9`). Reqwest 0.13.4 also has default low-level protocol-NACK retry behavior, not arbitrary status/timeout retry; `retry.rs:9–17,195–203,273–317` inspected. Never interpret a `send()` count as an absolute network-transmission counter under every feature composition. No unsafe default resend was established. |
| Buffering | `client.rs:326–328` buffers entire body and copies it; no maximum response size or streaming API. A 60-second deadline is not a memory bound. Recorded as capability/robustness limitation, not a vendor violation or a proven incident; no invented fixed cap recommended. |

### Cookies and response interpretation

- Default native client owns an in-memory jar; clone shares it and a new default
  client starts fresh (`client.rs:233–268`). An injected shared provider stays
  shared even when wrapped in another client. The documented fresh-jar advice
  after account/company/email changes matches B4. Disk persistence and a timer
  are recommendations/options, not missing mandatory mechanisms.
- `session_cookie` (`wire.rs:313–340`) now matches exact case-sensitive
  `JSESSIONID`, skips malformed/nonmatching pairs across repeated Set-Cookie
  headers, permits empty values and preserves later `=`. Attribute handling is
  explicitly the custom transport jar's responsibility; native reqwest never
  uses this stripped helper. The prior prefix-match defect is fixed.
- Header names are case-insensitive and repeated values retained in the raw
  vector (`wire.rs:186–199`, `client.rs:315–325`). Singleton lookup deliberately
  uses the first (`wire.rs:227–237`); conflicting duplicate vendor verdict
  headers have no documented meaning. No arbitrary last-wins rewrite justified.
- Textual headers are decoded once, with `+` as space and `%2B` as plus
  (`wire.rs:239–249,343–351`). Numeric totals/id/error tokens use raw lookup.
  Malformed percent-UTF-8 returns the plus-adjusted original rather than
  panicking; non-UTF-8 raw HTTP bytes are lossy at the reqwest boundary. This is
  tolerant handling, not a promise to recover a non-UTF-8 vendor encoding.
- Monetary-header comma format is retained and tested (`envelope.rs:314–329`,
  `tests/response_headers.rs:265–303,372–448`), including the recorded `100,01`.
  Body money wins; malformed nonblank body values are not hidden by headers.
  XML URLs get entity decoding only, not a second URL decode. The prior comma
  fallback bug is fixed.
- Complete-response policy is accurately documented: nonblank down → header
  code (operation-aware 56) → known non-2xx → body. A bare success number does
  not defeat HTTP failure. Body-only 7/463 at 200 still work; body-only errors at
  500 are U/HttpStatus by deliberate policy. Omitted status remains possible
  for custom transports; bundled client supplies it. R1 concerns transport
  failure before this policy can run, not changing this policy.
- Numbered 56 retains success and tolerates malformed optional metadata/non-XML
  text; other complete body errors override provisional 56. Unnumbered 56 is U.
  This matches PHP-backed recovery policy and is not relabeled a live finding.
- Version 2 is selected via `ops::RESPONSE_VERSION` on invoice, storno, credit,
  PDF operations. XML query, receipts, deletion and taxpayer use their own
  structured formats; adding `valaszVerzio` to them would not improve conformance.
  Generic version-1 `[ERR]` text is not promoted to a structured refusal.
- Sparse data and absent auxiliary metadata remain accepted. No universal
  required-header rule or full response-XSD gate is proposed. Fresh/current
  vendor examples still contain placeholders and unescaped ampersands; they
  are not evidence for accepting broken XML as normal success.

## Recovery ownership and public consistency

`src/recovery.md:4–58` and README failure/receipt/session sections are substantially
aligned. The important earlier corrections are present:

| Operation/concern | Current guidance and assessment |
|---|---|
| Invoice create | Persist identity before send; external id is not unique; validate order/type/reversal and resolve collisions; immediate absence does not settle in-flight issuance. Caller serialization and deliberate renewal are required, not implemented by `Client`. |
| Invoice storno | Inspect original and matching SS/reference; repeat echo and success-shaped proforma/delivery-note no-op are bounded observations. Optional gross is not turned into a common-layer certainty of no reversal. |
| Receipt create | Persist and retain call id; 338 prevents a duplicate but supplies no prior success; number/order lookup with identity/type checks. Never generate a fresh call id merely because recovery is unresolved. |
| Receipt storno | Query original reversal; known SN can be verified; do not promise invoice-style repeats or storno-specific 338 protection. |
| Reads | A new read is a new observation. Code 7 is operation-specific. The classifier is about document uncertainty, not a universal retry authorization. |
| Credit entries | Reconcile credit entries/balance; additive repeats may double amounts, replacement can overwrite intervening state. Invoice existence is insufficient evidence. |
| Proforma delete | Order selector can affect multiple proformas, including new matches on a later send (PHP4); number set and intent belong to caller. Absence may mean consumption. 335 is not replayed success. |
| Receipt email | Receipt existence does not establish delivery; resend can duplicate email. Empty-present block retains documented resend behavior, with partial override/recipient behavior left unspecified. |
| Retry ceiling | Five **total** sends of the same request, not five retries plus initial; no tight loop; operator intervention afterward. Cross-call accounting and test rate limiting are caller responsibilities. Vendor does not define a combined write/query budget or a negative-settlement delay. |

The crate makes no cross-request lock, durable identity store, unresolved-write
marker, automatic retry budget or account-mapping guarantee. Their absence is
not a defect in this client. Restate's policies/state are outside this review.
The supplied current CONTEXT amendments explicitly supersede older renewal
advice: elapsed time/empty observations do not settle earlier sends. The current
Agent recovery text follows that conservative rule.

## Unsupported capabilities and accepted deviations

These are **not additional findings**:

1. No built-in response-version-1 selection, streaming download, persistent
   cookie file, automatic session refresh, shared rate limiter or general XSD
   validator. The available transport hook/sans-I/O boundary gives the caller
   ownership; current docs do not promise these capabilities.
2. Full XSD validation of arbitrary external `AgentRequest` implementations is
   not what `to_wire` does: implementers own XML structure/action names; the
   shared layer validates encoding/characters and frames bytes. Built-in actions
   are fixed and escaped user data cannot choose one.
3. Native default supports plain HTTP overrides for mocks/proxies; TLS settings,
   proxies and redirects on an injected client are that caller's configuration.
   No production-host HTTPS allowlist or unsolicited endpoint policy is imposed.
4. Non-2xx with body-only vendor error stays unknown rather than settled. The
   stronger evidence of explicit error headers is checked first. Keep the
   observed HTTP-200 body-only failures and absent delete-success headers.
5. Sparse responses, open tokens, optional id/metadata, numeric comma headers,
   storno echoes/no-ops and newest-holder external ids remain supported even
   where an example/schema would suggest a stricter shape.
6. Empty replacing credit entries are deliberately unavailable until behavior
   is verified (`error.rs:536–545`); this is an explicit operation restriction,
   not a missing common transport capability. Foreign-currency/receipt policy
   and invoice layout fields remain with their operation reviewers.
7. Agent-key lifecycle/permissions, dedicated legacy user's billing role,
   subscription state, test/live account setting, key secrecy and correct
   account selection are caller/vendor responsibilities. Lowercase validation
   could improve diagnostics but silent credential normalization is unwarranted.

## Unresolved contradictions and evidence limits

| Question | Evidence and disposition |
|---|---|
| Number plus error header | I1/I2 generally say error headers omit number/totals, but PHP2 explicitly allows numbered 56. Preserve the more specific warning exception; do not delete it to satisfy the generic table. |
| Header encoding beyond number/error | I1/I2/I3 do not explicitly label encoding for payment method/customer URL. Rust preserves its existing form-style one-pass convention. PHP2 uses `rawurldecode` on URL ingestion (`InvoiceResponse.php:136–137`) and `urldecode` in its getter (`:348`); that is not proof that Rust should decode twice or change literal-plus handling. Obtain raw header captures/vendor clarification before changing conventions. |
| Credential failure order | `error.rs:330–335` attributes “before it looks at the request” to documentation. B3/B5 describe authentication failures but do not specify that exact internal ordering. The per-exchange R classification is reasonable; precise provenance should be qualified as inferred authentication semantics, with no live observation. No evidence warrants classifying these as successful writes or weakening earlier-send uncertainty. |
| Error 262 locale | EN B5 says offending row number; HU B5 says item name. Current code preserves the message and its neutral row-error name/class works with either. No behavioral change justified. |
| Timeout history | Behavior record A4d (`docs/szamlazz-hu-behaviour.md:152`) found no issuance after a ≥57-second stall; broader delayed-issuance assertion lacks its probe. `recovery.md:46–51` already qualifies this. Neither proves timeout cancellation or a safe resend delay. |
| Cookie/key precedence and revocation | B3 says deletion takes effect immediately; B4 says sessions reuse authentication and edits may need a fresh cookie. Neither specifies mismatched XML key/cookie precedence or existing-session revocation details. Fresh jar ownership advice is appropriate, not proof of precedence. |
| Browser access | No direct CORS/header exposure or cross-origin cookie guarantee established. Native loopback tests prove neither. D2 adds the separate key-deployment restriction. |
| Receipt call identity | Creation deduplication documented, but call-id scope/retention, query `hivasAzonosito` meaning, exact “last” ordering and storno reuse remain unestablished. Current recovery docs correctly avoid those promises. |
| Schema/layout disputes | The required vendor-question draft records inline-vs-download/PHP preview/simpleItems order and reversed template labels. It is a draft, not a vendor answer. Preserve the chosen writer and supported fields; these operation-level questions are not newly adjudicated here. |
| Corpus freshness | SOURCES distinguishes July acquisitions, patched invoice schema, uncertain receipt-schema provenance and September examples. Do not treat a cached/project-edited XSD or synthetic golden as fresh upstream proof. This review did not refresh fixtures. |

Also, `error.rs:3–5`'s “never via HTTP status codes” describes in-band application
errors, not an absence of HTTP failures. The detailed client/README precedence
correctly handles those. “Application error codes are in-band” would be more
precise; no runtime change follows.

## Verification record

Scratch manifest references the unchanged crate and four existing integration
test files directly. It pins **reqwest 0.13.4**, the version in the reviewed
HEAD's lockfile. Other scratch dependencies resolved offline to locally cached
versions; this is not claimed as a full locked-workspace build. No workspace
lockfile mutation or repository target directory was needed.

Commands:

```text
cargo test --manifest-path /tmp/opencode/agent-transport-audit-382cf761/Cargo.toml --offline -- --nocapture
cargo test --manifest-path /tmp/opencode/agent-transport-audit-382cf761/Cargo.toml --offline --test audit -- --nocapture
```

The first run passed all four initial scratch cases and **20 existing checks**:
`tests/client.rs` 7, `tests/response_headers.rs` 9,
`tests/error_classification.rs` 3, `tests/custom_http_client.rs` 1.
The second run followed addition of routing and redirect/deadline controls and
passed **all six scratch cases**:

- independently enumerated 37-code current catalogue, flags, reverse mappings,
  padded numeric forms and unknown/absent controls;
- all eleven documented form routes and endpoint;
- error/numbered-56 headers discarded after body-transfer failure (R1);
- endpoint userinfo visible in debug output, XML key still redacted (R2);
- API and parser error displays exceed the excerpt bound (D1);
- default native redirect refusal and injected timeout propagation.

The scratch tests assert observed current behavior; a passing reproducer does
not mean its demonstrated problem is fixed. Existing wire/credential unit tests
were inspected, not separately rerun; README examples were read, not freshly
doctested. No full suite, wasm/browser run, live TLS handshake or account probe
is represented by these results.

**Prior-fix disposition:** the historical missing-code, cookie prefix-match,
comma-header, generic recovery, session ownership, HTTP-precedence prose and
receipt-retry claims are fixed in the reviewed code/docs. Current shared XML
completion and text-fidelity policies are visible, but their full parsing audit
belongs to the parallel operation-specific reviews. None of the old report's
counts is carried forward as a current defect count.
