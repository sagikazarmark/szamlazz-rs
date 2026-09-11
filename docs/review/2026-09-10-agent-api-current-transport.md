# Current Számla Agent API compliance review — transport and recovery

**Reviewed commit:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`  
**Review and fresh source acquisition:** 2026-09-10  
**Scope:** `szamlazz-agent` shared transport, wire framing, authentication, session
cookies, all eleven operation routes, HTTP/header interpretation, error catalogue,
outcome classification and public recovery guidance.

## Conclusion

The current implementation matches the documented endpoint and eleven multipart
routes. All **37 numeric codes in the freshly fetched Agent documentation corpus**
have named mappings; the additional PHP-backed 56 and five explicitly observed
codes bring the implemented named catalogue to **43**. No definite vendor-protocol
violation or missing documented code was established in this scope.

**One P2 robustness finding remains:** a failed response-body transfer discards
already received status/header evidence. This is the prior report's R1, reproduced
again on the requested commit. The latest commit preserves an XML verdict across
an optional-payload decoding failure; it does **not** preserve headers across an
interrupted HTTP transfer. Those are distinct failure paths.

The other four prior findings are closed: URL-userinfo diagnostic redaction,
accurately scoped logging bounds, browser key-deployment guidance and qualified
identical-resend wording. The new unexpected-root bound and complete-body verdict
preservation have passing regression checks. No P0/P1 finding was established.

| Current ID | Category | Severity | Finding | Confidence |
|---|---|---|---|---|
| T1 | Robustness / recovery evidence | P2 | Interrupted response bodies still erase received header evidence | High behavior; remedy needs an explicit evidence policy |

P2 denotes a consequential recovery/diagnostic defect worth fixing, not a proven
duplicate invoice or live-account incident. Hardening opportunities and unresolved
source questions below are not counted as additional defects.

## Baseline and method

- HEAD resolved to the requested SHA at the beginning and at final verification.
  `git diff --exit-code fbda137 -- crates/szamlazz-agent fixtures/SOURCES.md
  docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock` passed. Citations describe
  that commit, not the old report's line numbers.
- Full reads of `src/client.rs`, `src/wire.rs`, `src/credentials.rs`, `src/error.rs`,
  `src/lib.rs`, `src/recovery.md`, crate `README.md` and manifest. Supporting reads
  covered the shared envelope and XML machinery, all eleven action/auth/version/
  parser entry points, focused tests, behavior notes and fixture provenance.
- Paths `src/…`, `tests/…` and `README.md` below are relative to
  `crates/szamlazz-agent/`. Workspace documentation/fixture paths are explicit.
- The vendor specification was read freshly, independently of the old report.
  `docs/review/2026-09-10-agent-api-transport.md` was used only as a closure
  checklist. The latest diff was also inspected to identify exactly what changed.
  This is a current-tree audit, not an empty `fbda137...HEAD` diff review.
- No delegation, source changes, repository-test changes or live-account calls.
  Network activity was public documentation/download GETs and local loopback
  verification. The sole repository write from this review is this report.
  Scratch source/binary and Cargo target artifacts are under `/tmp/opencode`.
- Existing untracked review/research documents were left intact. Concurrent work
  appeared under `crates/restate-szamlazz/` and `docs/design/`; the reviewed Agent,
  lockfile and evidence paths remained identical to the requested commit.
- This review covers operation routing and shared interpretation for every route,
  not every operation-specific XML field, layout, NAV schema or monetary rule.
  Existing library tests exercise some of those areas, but passing them is not
  represented as an exhaustive operation audit.

## Fresh official sources and exact claims

All URLs in this table were fetched during this review. Docs pages displayed
**`v202608271632`**: a site build label, not the date each statement changed.
The category page lists seven basics pages; all seven were fetched in full.
Short quoted phrases below distinguish actual published claims from inference.

| Ref | Fresh URL | Claim used in this review |
|---|---|---|
| B0 | <https://docs.szamlazz.hu/agent/category/basics> | Seven-page basics inventory. |
| B1 | <https://docs.szamlazz.hu/agent/basics/what-is> | Eleven capabilities; proforma/delivery note use invoice creation, with “no separate endpoint.” |
| B2 | <https://docs.szamlazz.hu/agent/basics/how-does> | XML in an HTTP POST to `https://www.szamlazz.hu/szamla/`; PDF and optional vendor email are separate delivery paths. |
| B3 | <https://docs.szamlazz.hu/agent/basics/authentication> | Key in `szamlaagentkulcs`, alternatively username/password; same key in both legacy fields supported. Key is case-sensitive, accepted “only in lowercase”; legacy user needs “exactly one billing account.” |
| B3 continued | same URL | Keys have identical permissions, no per-key scope, at most 17, no expiry until deletion; deletion “takes effect immediately.” “Do not include it in client-side code.” No cookie-versus-key precedence is stated. |
| B4 | <https://docs.szamlazz.hu/agent/basics/error-handling> | “At most five times” for the same request, then stop for human intervention; no until-success loop; maximum 500 test invoices in ten minutes. General code table and version-1 `[ERR]` format. |
| B4-HU | <https://docs.szamlazz.hu/hu/agent/basics/error-handling> | “Legfeljebb ötször” confirms five total sends; same numeric general catalogue. Code 262 describes product name where EN describes row number. |
| B5 | <https://docs.szamlazz.hu/agent/basics/session-cookie> | `JSESSIONID` reuse skips reauthentication; expiry after “90 minutes” inactive; disk storage advisable; new session advised after company/email edits. |
| B6 | <https://docs.szamlazz.hu/agent/basics/security> | Separate inbound CIDRs for HTTPS destinations and outbound IPs for vendor-initiated partner calls. |
| B7 | <https://docs.szamlazz.hu/agent/basics/sending-requests> | Same endpoint, routing by “name of the form field,” complete eleven-route table, `HTTPS POST`, XML file, one document per XML; recommends XSD validation and warns mistyped tags may fail or be ignored. |
| B8 | <https://tudastar.szamlazz.hu/gyik/technologiai-valtozasok-2025> | August 1, 2025 infrastructure/IP changes; certificate issuer changed from Let's Encrypt to Google Trust Services. |
| I0 | <https://docs.szamlazz.hu/agent/generating_invoice/request> | `multipart/form-data`, main file `action-xmlagentxmlfile`, optional `attachfile1`…`attachfile5`. |
| I1 | <https://docs.szamlazz.hu/agent/generating_invoice/response> | Version 2 is structured `xmlszamlavalasz` with optional base64 PDF. Invoice number/error text “URL encoded”; totals/error code “not URL encoded.” Other listed headers include payment method and customer URL without an explicit encoding designation. |
| I2 | <https://docs.szamlazz.hu/agent/reversing_invoice/response> | Same version distinction/header table; structured success and error examples currently present. |
| C1 | <https://docs.szamlazz.hu/agent/credit_entry/response> | Version 2 envelope and optional invoice/balance headers, success/false verdict; version 1 text. |
| Q1 | <https://docs.szamlazz.hu/agent/querying_pdf/response> | Version 2 base64 PDF; missing invoice/order/external selector gives code 7. |
| Q2 | <https://docs.szamlazz.hu/agent/querying_xml/response> | Success is full `szamla`; error is `xmlszamlavalasz`; unknown selector gives code 7. |
| Q3 | <https://docs.szamlazz.hu/agent/querying_pdf/xml> | Credentials directly below `xmlszamlapdf`; `valaszVerzio` at root; selector ordering. |
| Q4 | <https://docs.szamlazz.hu/agent/querying_xml/xml> | Credentials directly below `xmlszamlaxml`; order lookup returns the last invoice; external identifier can be queried. No external-id uniqueness promise. |
| D1 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response> | `xmlszamladbkdelvalasz`; success/false verdict; 335 example; critical errors may instead be plain text/HTML. |
| R1 | <https://docs.szamlazz.hu/agent/generating_receipt/response> | Receipt creation call identity must be unique; repeated XML “will not duplicate an existing receipt”; supplement 336–340; `xmlnyugtavalasz`, optional PDF. |
| R2 | <https://docs.szamlazz.hu/agent/reversing_receipt/response> | Returns storno receipt `SN`, not the original; documents refusals for absent, already reversed and storno-receipt targets, without assigning numeric codes to each case. |
| R3 | <https://docs.szamlazz.hu/agent/querying_receipt/response> | Query response matches receipt creation. |
| R4 | <https://docs.szamlazz.hu/agent/sending_receipt/response> | Success/false verdict in `xmlnyugtasendvalasz`; code 7 example means missing email subject. |
| R5 | <https://docs.szamlazz.hu/agent/sending_receipt/xml> | Absent email block means no email; present block without details requests previous email; children individually optional. Does not establish arbitrary partial-field merging. |
| R6 | <https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number> | Receipt repetition toggle is separate from invoice toggle; restriction bars a new receipt with a previously used order number. |
| N1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | `QueryTaxpayerResponse`; dated 2020-11-04 NAV 2.0 examples; numeric Agent 57 in NAV `result`; `OK` plus validity false is normal data. Current schema link points to NAV documentation. |
| O1 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number> | Duplicate checking is per document type with toggle on; storno/correctives exempt; reversal frees order. Successful replay additionally needs matching buyer/gross/three dates and an earlier invoice created “within the last 2 days.” |
| S1 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency> | Explicit refusal rules 551–556; OSS or non-Hungarian seller; item/VAT restrictions, corrective restrictions, final inheritance and VAT matching. |
| P0 | <https://docs.szamlazz.hu/php/> | Current offered first-party package is 2.12.4, dated 2026-08-12, with the ZIP link below. |
| P1 | <https://docs.szamlazz.hu/php/valasz-feldolgozas> | An invoice may be “successfully issued” although notification cannot be delivered; separate notification-error accessor. |
| P2 | <https://docs.szamlazz.hu/php/nyugta-lekerdezes> | Receipt number or order selector; order lookup returns “last matching document.” |
| P3 | <https://docs.szamlazz.hu/php/dijbekero-torles> | Order-based deletion can delete “multiple proforma invoices”; failure of a member triggers rollback. |
| P4 | <https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip> | Fresh binary acquisition, SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Inspected in memory, not extracted into the repository. |

P4 source locations, relative to
`PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:17`: `INVOICE_NOTIFICATION_SEND_FAILED = 56`.
- `Response/InvoiceResponse.php:319–322`: a number plus notification-send error
  makes invoice issuance successful.
- `Response/SzamlaAgentResponse.php:147–152`: checks nonblank `szlahu_down` first.
- `Response/InvoiceResponse.php:136–137,347–348`: customer URL passes through
  `rawurldecode` at ingestion and `urldecode` at its getter. That implementation
  is not evidence that every response URL must be decoded twice in Rust.

Operation error-list links lead to B4's inline table. There is no separate complete
Agent error download linked by these response pages. NAV's independent complete
error catalogue was not audited; unknown NAV strings remain open codes.

## T1 — Interrupted transfer still loses received evidence

**P2 robustness; high-confidence reproducible behavior. Prior R1 remains open.**

**Current code:** `src/client.rs:338–364`, specifically capture at `:349–359`
followed by `response.bytes().await?` at `:360`. `ClientError::Transport` carries
only the reqwest error (`:54–65`); its outcome remains `Unknown` (`:82–89`).
Compare the raw-response verdict policy in `src/wire.rs:291–310` and numbered-56
handling in `src/ops/envelope.rs:179–218`.

The client has already received the status and all headers when it starts reading
the body. If Content-Length framing fails or the download errors, `?` returns
before a `RawResponse` is constructed. Neither the status, vendor error/down
headers nor an invoice number is available to the caller through that error.

### Fresh reproduction

Scratch test: `/tmp/opencode/current-transport-fbda137.rs`,
`interrupted_bodies_still_discard_all_header_evidence`.

1. Bind a TCP listener to an ephemeral loopback port.
2. Read the **complete multipart POST**, including its Content-Length bytes.
3. Return `HTTP/1.1 200 OK`, `Content-Length: 1000`, `Connection: close`, one of
   the header sets below, an empty line and one body byte `x`, then close.
4. Call `Client::send(&StornoInvoice::new("I-1"))` through the actual default
   native client. The test does not involve szamlazz.hu or a real credential.

| Fully received headers | Actual current result |
|---|---|
| `szlahu_error_code: 3`, `szlahu_error: login` | `Transport`, `Unknown`; credential refusal unavailable |
| `szlahu_down: maintenance` | `Transport`, `Unknown`; vendor unavailability reason unavailable |
| `szlahu_error_code: 56`, `szlahu_szamlaszam: I-2` | `Transport`, `Unknown`; number and notification evidence unavailable |
| `szlahu_szamlaszam: I-2` alone | `Transport`, `Unknown`; ordinary success remains unconfirmed, as it should |

All four exposed `reqwest::Error { kind: Decode, … IncompleteBody }`; no vendor
header or status survived in the public error. A direct parser control using the
numbered-56 headers, an empty body and status 200 returns `I-2` with
`notification_delivery_failed = true`.

**Impact:** received reconciliation identity and failure diagnostics disappear.
A caller may have to perform extra queries or remain unable to identify the issued
document despite having received its number. The conservative `Unknown` result
does not itself authorize a duplicate send; the public recovery guidance correctly
requires reconciliation. This is evidence loss, not proof of actual duplicate
issuance or a vendor guarantee about interrupted bodies.

**Source distinction:** I1/I2 establish the header channels; P1/P4 establish the
specific numbered-56 exception. None specifies HTTP interruption behavior. A
complete body could also contradict provisional header 56. Thus the most direct
fix is retaining status/header evidence alongside the body-transfer failure,
with an explicit operation-aware policy for any eventual promotion to a verdict.
Do not silently treat a truncated body as complete, infer success from every bare
number, or skip contradiction checks on a body that was fully received.

### Why the latest commit does not close this

`src/ops/envelope.rs:278–287` now separates a decoded verdict from a payload
decoding result. `:195–209` considers the refusal before tolerating malformed
numbered-56 metadata. The new
`tests/response_headers.rs:431–450` covers a **complete XML response** with header
56, body code 3 and malformed/duplicate optional gross elements. It passed.
This closes the separate payload-decoding verdict-loss case. It is unreachable
when `Client::send` exits on an interrupted HTTP body at `client.rs:360`.

## Full eleven-route inventory

Common endpoint: `https://www.szamlazz.hu/szamla/` (`src/wire.rs:7–14`).
Method and content type: POST of the generated multipart bytes
(`src/client.rs:341–345`, `src/wire.rs:66–100`). Request roots below have default
namespace `http://www.szamlazz.hu/{root}`. `B` means credentials under
`beallitasok`; `R` means directly under the root. `—` means the operation has its
own structured response format and no response-version selector is added.

| Operation / request type | Exact form field | Request root | Auth / version | Current source |
|---|---|---|---|---|
| Invoice family / `CreateInvoice` | `action-xmlagentxmlfile` | `xmlszamla` | B / 2 | `src/ops/invoice.rs:679,737–755,927–947` |
| Invoice storno / `StornoInvoice` | `action-szamla_agent_st` | `xmlszamlast` | B / 2 | `src/ops/storno.rs:163–183,211–213` |
| Credit entry / `RegisterCreditEntry` | `action-szamla_agent_kifiz` | `xmlszamlakifiz` | B / 2 | `src/ops/credit_entry.rs:214–251` |
| Invoice PDF / `QueryInvoicePdf` | `action-szamla_agent_pdf` | `xmlszamlapdf` | R / 2 | `src/ops/query_pdf.rs:59–93` |
| Invoice XML / `QueryInvoiceXml` | `action-szamla_agent_xml` | `xmlszamlaxml` | R / — | `src/ops/query_xml.rs:536–588` |
| Proforma deletion / `DeleteProforma` | `action-szamla_agent_dijbekero_torlese` | `xmlszamladbkdel` | B / — | `src/ops/proforma.rs:61–87` |
| Receipt creation / `CreateReceipt` | `action-szamla_agent_nyugta_create` | `xmlnyugtacreate` | B / — | `src/ops/receipt.rs:190,223–231,293–295` |
| Receipt storno / `StornoReceipt` | `action-szamla_agent_nyugta_storno` | `xmlnyugtast` | B / — | `src/ops/receipt.rs:341–365` |
| Receipt query / `QueryReceipt` | `action-szamla_agent_nyugta_get` | `xmlnyugtaget` | B / — | `src/ops/receipt.rs:421–450` |
| Receipt email / `SendReceipt` | `action-szamla_agent_nyugta_send` | `xmlnyugtasend` | B / — | `src/ops/receipt.rs:503–532` |
| Taxpayer / `QueryTaxpayer` | `action-szamla_agent_taxpayer` | `xmltaxpayer` | B / — | `src/ops/taxpayer.rs:266–285` |

All fields match B7 exactly. The four writers share the single
`ops::RESPONSE_VERSION` constant (`src/ops.rs:27–31`). Adding that element to
the other seven writers would not improve conformance.

Independent scratch verification covered **each route**, not just a sample:

- Exact action, multipart file disposition/name/filename, XML declaration,
  request root/namespace, credential location, ending boundary and version field.
- Both key and username/password authentication, with `&`/`<` escaping.
- Header codes 3, 56 **without a number**, 557, `NAV_FUTURE` and `%33` at HTTP
  500; message decoding `a%2Bb+c` → `a+b c`.
- Down-before-code precedence and HTTP 502 with only a success-number header.
- Body-only code 57 at HTTP 200, using the appropriate response envelope; NAV
  uses its `result/funcCode/errorCode/message` shape.

These establish client-side route/auth/error plumbing. Synthetic 57 on every
route does not prove that the vendor emits it there in every circumstance.

## Shared-layer assessment

| Area | Current evidence and assessment |
|---|---|
| Multipart file, not a text field | `wire.rs:66–100` supplies filename and `Content-Type: text/xml`, CRLF separators and a terminating boundary. B7/I0 and error 53 agree. Filename need not end in `.xml`; the routing key is the form name. |
| Attachments | `invoice.rs:938–947` emits numbered attachment fields; bounded collection and size checks at `:407–488,508–511`. Existing exact multipart and count/size tests passed. No attachment content is interpreted as XML. |
| Boundary collision/injection | `wire.rs:102–125` selects a boundary absent from XML/file contents, keeping it below MIME's 70-character limit. Attachment disposition values strip CR/LF and escape quotes/backslashes; content type strips CR/LF (`:80–92`). Built-in actions are trusted constants. An arbitrary custom `AgentRequest::ACTION` is implementer code, not untrusted invoice input. |
| XML/authentication | `xml.rs:19–40,414–424,456–466` emits UTF-8/XML 1.0 and escapes values; `wire.rs:402–429` refuses non-UTF-8/forbidden XML characters before HTTP. Key and legacy fields preserve caller bytes; the lowercase rule is documented at `credentials.rs:5–9`, not silently normalized. Invalid/empty credentials remain vendor-owned authentication failures. |
| Validation boundary | `to_wire` calls operation validation then scans XML encoding/characters. It is not a general XSD validator, nor a promise to validate arbitrary third-party `AgentRequest` structure. B7 recommends XSD validation; exact full validation remains an integration concern. |
| Credential privacy | `credentials.rs:27–30,88–98` hides key/password in Debug, while intentionally showing legacy username. `WireRequest` hides body (`wire.rs:43–50`). RawResponse hides cookie values/body, not all business headers (`:155–179`). Current README scopes this accurately. |
| Endpoint parsing/redaction | `client.rs:128–136,207–220` uses reqwest's URL parser, checks HTTP(S)/host and sanitizes userinfo in endpoint diagnostics; malformed/unredactable URLs show `[invalid URL]`. Builder and Client custom Debug hide underlying transport details (`:113–123,303–310`). Fresh malformed/supported/unsupported-scheme cases and a connection-refused transport error did not expose username/password/key. |
| HTTP override | Default is HTTPS; deliberate custom HTTP endpoints remain supported for local mocks/proxies. URL-userinfo support is retained. No default downgrade, credential-byte normalization or production-host allowlist is imposed. Query/path secrets in custom URLs are not covered by userinfo redaction. |
| TLS/network | Crate manifest `:24–25` enables reqwest rustls/platform verification. No certificate pin, TLS-verification bypass or obsolete IP pin exists in the reviewed default path. B6/B8 network allowlisting belongs to deployments; vendor outbound receiver IPs are not Agent response authentication. No live TLS probe was run. |
| Redirects | Default native `Policy::none()` at `client.rs:264–271`. Fresh actual-default loopback 302, 307 and 308 checks returned HttpStatus with zero connections to the redirect target. This covers both POST-to-GET conversion and body-preserving forwarding. An injected client/browser owns its own redirect behavior. |
| Timeouts | Native deadline is 60 seconds (`client.rs:244–255,269`); injected clients own their deadline. A fresh 30-ms custom timeout against a 200-ms delayed loopback response became Transport/Unknown. No vendor-mandated duration was found. The deadline does not cancel server work or bound synchronous serialization/parsing CPU. |
| Retry ownership | `client.rs:157–176`, `recovery.md:4–9` and README `:351` correctly disclaim an application recovery loop and note injected retries. reqwest 0.13.4's `src/retry.rs:9–17,195–203,273–317` has protocol-NACK retry defaults, not arbitrary application-status/time-out resends. Default HTTP/2 NACK handling and feature-dependent HTTP/3 logic preclude a universal one-send/one-transmission promise. No unsafe default resend was established here. |
| Buffering | `client.rs:360–362` buffers and copies the whole body. No response-size cap or streaming API. A deadline is not a memory limit. This is a hardening/capability limitation, not a violated vendor requirement or measured memory incident. |
| Session ownership | `client.rs:257–292`: native default jar, clone shares it, new default client gets a fresh jar. Supplied provider ownership is caller-controlled. Tests `client.rs:143–219` confirm injected clone/reuse/fresh-jar isolation on the same origin. Company/email refresh guidance matches B5. |
| Cookie extraction | `wire.rs:313–340`, tests `response_headers.rs:73–112`: exact case-sensitive `JSESSIONID`, repeated Set-Cookie scanning, malformed/nonmatching pairs skipped, empty values and later `=` preserved. Attributes/expiry/path/domain are transport-owned; native reqwest uses its own jar, not this helper. |
| Header capture | Header names are case-insensitive and repeated pairs survive capture (`client.rs:349–359`, `wire.rs:186–199`). Lookup explicitly selects first occurrence (`wire.rs:227–237`). Conflicting duplicate verdict headers have no published resolution rule; first-wins is policy, not a proven vendor guarantee. |
| Text versus numeric decoding | `wire.rs:239–249,343–351`: one form-style decoding pass for textual headers; raw totals/id/code. `+` → space, `%2B` → plus; invalid percent-UTF-8 falls back to the plus-adjusted input. Raw non-UTF-8 HTTP bytes are lossy at capture, not a promised alternative charset conversion. |
| Monetary/URL fallback | `envelope.rs:120–168,290–342`: body first, headers second; decimal-comma header support retained (`100,01` was observed). Invalid nonblank body money is not hidden behind a good header. XML URL gets XML entity decoding only. Numeric precision policy is checked by existing tests, not independently re-audited here. |
| HTTP/error precedence | `wire.rs:291–310`: nonblank down, nonblank code interpreted by operation, known non-2xx status, then body. Ordinary success-number headers do not defeat HTTP failure. Body-only code at 200 is parsed; at 500 it remains HttpStatus/Unknown. T1 is the failure before this policy runs. |
| Numbered 56 | `envelope.rs:179–249`: requires a number; malformed optional metadata is tolerated. A complete decoded body refusal beats provisional header 56 even if payload decoding fails. Unnumbered 56 is Unknown. Credit registration does not treat 56 as success; PDF query still requires a PDF (`query_pdf.rs:83–93`). |
| Structured response completion | `xml.rs:63–166` checks root/namespace and completion through EOF. Foreign namespaces cannot supply protocol fields (`:169–228`). New wrong-root excerpt is bounded (`:127–135`). Tested malformed tails, extra roots, truncation and huge unexpected names do not become ordinary success. |

## Error catalogue and classifications

The forward/reverse tables are `src/error.rs:213–316`; retry hints `:318–331`;
credential detection `:333–350`; outcome classes `:352–452`; parsing `:455–493`.
The following is an independent source-based enumeration, not a count inherited
from an old report or only a round-trip over the enum itself.

Legend: **U** Unknown, **R** Rejected, **D** DuplicateOrderNumber, **N** NotFound.
Only **1/55** have a true potentially-transient read hint. Only **3/135/136/164**
are credential codes. Classification describes this exchange; it does not settle
an earlier lost send.

| Code | Rust variant | Class | Fresh source or bounded observation |
|---|---|---|---|
| 1 | `Maintenance` | U | B4/B4-HU; maintenance/internal failure does not settle issuance |
| 3 | `InvalidCredentials` | R | B3/B4; credential |
| 53 | `XmlNotAFile` | R | B4; missing multipart XML file |
| 54 | `EInvoiceNotEnabled` | R | B4; e-invoice entitlement/certificate permission |
| 55 | `EInvoiceSigningFailed` | U | B4; failed signing, transient timestamp access or expired certificate; issuance unproven |
| 57 | `MalformedXml` | R | B4/N1; XML read or XSD failure |
| 71 | `DuplicateOrderNumber` | D | B4/O1; existing order, bounded replay conditions |
| 135 | `BrowserSessionActive` | R | B4; credential/session intervention |
| 136 | `LoginBlocked` | R | B4; credential/subscription intervention |
| 152 | `DuplicateOrderNumberNamed` | D | B4/O1; message names order, not invoice |
| 164 | `MultipleAccounts` | R | B3/B4; credential, legacy user has multiple accounts |
| 202 | `UnregisteredPrefix` | R | B4; empty or unregistered prefix |
| 259 | `NetValueMismatch` | R | B4; net versus unit price × quantity |
| 260 | `VatValueMismatch` | R | B4; VAT arithmetic |
| 261 | `GrossValueMismatch` | R | B4; gross versus net + VAT |
| 262 | `NetValueInvalid` | R | B4/B4-HU; net row error, locale wording differs |
| 263 | `VatValueInvalid` | R | B4; VAT row error |
| 264 | `GrossValueInvalid` | R | B4; gross row error |
| 363 | `ReceiptGrossNotWhole` | R | B4; HUF receipt gross must be whole |
| 364 | `ReceiptNetPrecision` | R | B4; HUF receipt net at most two decimals |
| 365 | `ReceiptVatPrecision` | R | B4; HUF receipt VAT at most two decimals |
| 537 | `ErasureCodeLimit` | R | B4; 400 maximum per item |
| 538 | `ErasureCodesUnavailable` | R | B4; demo/test restriction |
| 539 | `ErasureCodesDisabled` | R | B4; account setting off |
| 551 | `SimplifiedImageAccountIncompatible` | R | B4/S1; OSS **or** non-Hungarian seller, inherited final included |
| 552 | `SimplifiedImageItemLimit` | R | B4/S1; two rows, final four |
| 553 | `SimplifiedImageVatInvalid` | R | B4/S1; one invalid VAT code refuses entire document |
| 554 | `SimplifiedImageCannotCorrect` | R | B4/S1; simplified original cannot be corrected |
| 555 | `SimplifiedImagePrepaymentVatMismatch` | R | B4/S1; final/prepayment VAT mismatch |
| 556 | `SimplifiedImageDocumentForbidden` | R | B4/S1; corrective/delivery note restriction |
| 336 | `ReceiptPrefixUsedForInvoices` | R | R1 supplement |
| 337 | `InvalidReceiptPrefix` | R | R1 supplement; uppercase letters/numbers |
| 338 | `DuplicateReceiptCallId` | R | R1 supplement; refuses another issue without replaying prior success |
| 339 | `ReceiptNotFound` | N | R1 supplement; absent receipt |
| 340 | `ReceiptPaymentMismatch` | R | R1 supplement; tender sum differs from gross |
| 7 | `MissingData` | N | Q1/Q2 and R4; unknown selector **or operation-specific missing data**, including subject |
| 335 | `ProformaNotFound` | R | D1 example; absent/deleted proforma, no replayed delete success |
| 56 | `InvoiceNotificationDeliveryFailed` | U as error | P1/P4; number-conditioned success exception, not observed live |
| 14 | `StornoOfReversalInvoice` | R | Behavior notes `:88`, B5-storno-SS; observed |
| 73 | `PrepaymentInvoiceNotIdentifiable` | R | Behavior notes `:119`, C6-4/C6-5; observed |
| 221 | `HasCorrectiveInvoice` | R | Behavior notes `:89`, B7-storno-corrected-orig; observed |
| 352 | `IssueDateMustBeToday` | R | Behavior notes `:90–91`; observed storno, not a universal create rule |
| 463 | `PaymentOnReversedInvoice` | R | Behavior notes `:135`; observed on reversed original, body-only; credit on SS itself untested |

Counts: **30 general + 5 receipt supplement + 2 distinct example codes = 37**;
plus **56 + 5 observed = 43**. All matched typed variants, canonical reverse tokens,
numeric padded input and expected flags/classes in the scratch catalogue check.
Existing tests add public-parser/message coverage (`tests/error_classification.rs`)
and original-catalogue/unit controls (`error.rs:769–1072`).

Unknown numeric/text tokens, including values outside `u16`, remain trimmed
`Unknown(String)` and U; empty code is `Absent` and U. `007` normalizes to 7;
`%33` remains unknown rather than decoding to credential code 3. Manually
constructing `Unknown("3")` does not grant it known-code semantics. No default
arm silently classifies future codes as refusal. Request validation is Rejected
because no send happened; transport/parse/HTTP/down failures are Unknown.

## Recovery documentation assessment

`src/recovery.md:4–62` and README `:77–217,288–306,345–353` distinguish the
operations and preserve uncertainty correctly. The classifier is about possible
document creation, not a universal delivery/mutation retry permission.

| Operation | Current caller guidance / assessment |
|---|---|
| Invoice create | Persist external id/order before sending, query newest holder, check identity/type/reversal, serialize logical issuance. Empty immediate lookup does not authorize resend. Nonunique external ids and collisions are explicit (`recovery.md:13`; README `:79–81,124–153`). |
| Invoice storno | Verify original and matching SS/reference, including order and reversal. Repeated storno echo and proforma/delivery-note no-op are observations, not all-operation guarantees (`recovery.md:14`; `envelope.rs:61–86`). |
| Receipt create | Persist/retain call id; 338 prevents another issue but returns no original success. Use known number or managed order with identity/type checks; no fresh call id while unresolved (`recovery.md:15,22–34`). |
| Receipt storno | Keep logical call identity; query original reversal, then known SN if available. Current docs now cite refusal semantics. Neither invoice-style successful repeats nor storno-specific 338 is invented (`recovery.md:16`, README `:196`). |
| Queries | Repeat is a new observation, not a write. Code 7 is contextual; an empty observation does not resolve earlier issuance (`recovery.md:17`). |
| Credit entries | Query entries/balance; additive repeat may double amounts, replacing repeat may overwrite intervening state. Invoice existence is insufficient (`recovery.md:18`). |
| Proforma delete | Number targets one; order can affect multiple matches, including new matches on repeat. Reconcile intended target set. Absence may mean consumption; 335 is refusal, not replay (`recovery.md:19`, source P3). |
| Receipt email | Receipt existence does not prove delivery; another send may duplicate email. Present-empty resend needs previous details (`recovery.md:20`, README `:198–217`, source R5). |
| Send limits | Five total sends of the same request, including first; human intervention after exhaustion, no tight loop. Vendor does not specify a combined write/query budget (`recovery.md:38–42`; B4/B4-HU). |
| Timeout evidence | A4d found no issuance after a stalled create. Broader delayed-issuance assertion lacks a linked probe. Both are qualified; timeout/elapsed time does not prove cancellation or negative settlement (`recovery.md:50–55`). |

The README's reconciliation example sends once and examines one lookup, leaving
wrong identity, reversed, absent or unanswered results unresolved. It does not
promise that a single lookup establishes full recovery. A refused exchange also
does not settle earlier sends. Its examples compiled in this review without
executing their network functions.

The client deliberately owns no durable identity store, cross-request lock,
unresolved-write marker, global send counter, rate limiter or account mapping.
Those are caller responsibilities, not absent features claimed by this crate.

## Prior-finding closure on the current commit

| Prior finding | Current disposition | Fresh evidence |
|---|---|---|
| R1: interrupted download loses headers | **Open as T1** | `client.rs:360` still returns early; four fresh broken-framing loopback cases and direct numbered-56 parser control. |
| R2: endpoint userinfo visible in debug/error text | **Closed for the reported userinfo surfaces** | Custom Debug and parser-based redaction at `client.rs:113–136,207–238,303–310`; unit test `:375–399` and independent scratch URL/transport-error checks passed. |
| D1: universal bounded/log-ready display promise | **Closed by corrected promise** | README `:353` names only HttpStatus/UnexpectedBody bounds and explicitly warns that API/parser text and other headers remain visible. Scratch API display remains 23,021 bytes by contract, while unexpected-root display is 459 bytes. No claim of universal truncation. |
| D2: browser discussion omits client-side-key prohibition | **Closed** | `lib.rs:67–70`, README `:306` quote/link vendor prohibition and direct browser apps through trusted server ownership. |
| D3: unconditional duplicate/replay descriptions | **Closed** | `client.rs:56–61` says “can issue”; `error.rs:85–90` includes matching conditions/two-day window and bounds live observations. Fresh O1 supports this wording. |

Additional latest-commit checks: malformed optional payload cannot erase a
decoded body refusal beneath header 56 (`tests/response_headers.rs:431–450`);
huge unexpected root names are bounded (`tests/response_completion.rs:86–100`).
Both passed. They do not establish header retention on HTTP transfer failure.

Earlier cookie-prefix, comma-header, generic-recovery and HTTP-precedence defects
remain fixed. No historical finding count is carried forward as a current count.

## Evidence provenance and unresolved questions

### Behavior notes and fixtures

- `docs/szamlazz-hu-behaviour.md:3–28,168–172` bounds observations to one test
  account and the recorded September dates; raw logs are explicitly **outside
  this repository** (`:11–12`). This review did not recreate or authenticate them.
- External-id nonuniqueness/newest-holder behavior (`:63–71`), per-operation
  header presence (`:141–145`), decimal-comma header (`:160`) and repeat-storno
  echo (`:86`) are retained as account observations, not promoted into universal
  vendor guarantees.
- `:54–55,185–191` already records the tension between observed issue-date
  replacement/replay and O1's three-date rule. Current `error.rs` quotes the
  published rule while distinguishing short-interval observations. No indefinite
  replay or safe negative-settlement interval follows.
- `:152–153` records the stalled create and failure to trigger 56. Neither 55 nor
  56 received new live evidence in this review.
- Some behavior-note **design-consequence** cells retain old worker assertions:
  `:63` mentions checking `teszt`; `:133` says a delay prevents in-flight resend;
  `:152` describes historical worker delay policy. These are not fresh protocol
  facts. Current domain amendments remove the account pin and state that elapsed
  time does not settle an unresolved write. Agent recovery prose is more careful.
- `fixtures/SOURCES.md:34–39` is a historical July acquisition record;
  `:142–180` separately records September response examples. Structured storno/
  credit examples are now present, independently confirmed by fresh I2/C1.
- `fixtures/SOURCES.md:126–140` identifies the invoice XSD as **project-modified**;
  `:182–191` records unresolved receipt-create acquisition provenance;
  `:193–228` records inline/download schema-order disagreements. None is silently
  treated here as a fresh authoritative schema or an exact live exchange.
- Current I1/I2/C1 still contain unescaped ampersands in example customer URLs;
  I1/I2 include abbreviated PDF base64. These are published-example defects,
  consistent with `SOURCES.md:163–170`, not grounds for loosening normal XML
  success parsing. No fixture was refreshed or rewritten.
- Synthetic fixtures and golden expectations are project-authored
  (`SOURCES.md:23–32`); source-derived parser tests prove local behavior, not
  vendor execution. Crate manifest `:12` excludes upstream corpus from packages.

### Ambiguities / hardening, not additional proven compliance bugs

| Topic | What is established and what is not |
|---|---|
| Number plus error | I1/I2 generically say number/totals are omitted with error headers; the more specific first-party PHP number-plus-56 exception exists. Preserve the exception. No raw account 56 capture was obtained. |
| Text-header encoding | Number/error URL encoding is explicit. Payment-method/customer-URL encoding, literal-plus behavior and malformed encoding are less precise in prose; PHP's mixed decoding does not justify blindly changing Rust. Need raw captures or clarification for a stronger rule. |
| Credential-code sequencing | `error.rs:333–340` and behavior notes `:255–261` attribute “before it looks at the request” / “before any write” to docs. Fresh B3/B4 establish authentication failures, not that exact internal sequence. Treat the pre-action reading as authentication-semantics inference, with no live proof. The per-exchange Rejected classification is reasonable; no evidence supports inventing success or settling earlier uncertain sends. This provenance wording remains worth qualifying. |
| HTTP status absolutism | `error.rs:3–5` says errors are never via HTTP status; understood as application codes, while `client.rs`/README correctly handle HTTP failures. “Application errors are in-band” would be more precise. No runtime defect follows. |
| Sessions/revocation | B3 says key deletion is immediate; B5 says sessions reuse authentication. Existing-session revocation and conflicting key/cookie precedence are not specified. Fresh-jar ownership is appropriate, not a tested vendor precedence rule. |
| Logging | Userinfo redaction is verified; arbitrary query/path credentials, API messages, XML parser details and non-cookie headers are not generally sanitized. Current README admits this. More universal logging controls are optional hardening, not a reopened D1/R2. |
| Large bodies | No response cap/streaming. Optional cap/streaming design could bound resource use but needs operation/PDF expectations; no arbitrary fixed cap is inferred from docs. |
| Custom transports | Optional status omission, first duplicate-header selection, caller-managed cookies and supplied retry/redirect policies are documented boundaries. The default native guarantees do not apply to every injected client. |
| Browser | Compilation support is not a tested CORS/header-exposure/cookie guarantee. No browser or wasm runtime test was run. The independent client-side-key prohibition is now explicit. |
| Receipt identity | Call-id scope/retention, optional query-call-id meaning, exact last-match ordering and storno-specific call-id behavior remain unestablished. Recovery correctly avoids stronger promises. |
| Numeric semantics | Decimal comma is observed; grouping cannot be inferred from `1,234`. Exact finite Decimal representation and auxiliary-id leniency are implementation policies; no new live currency/precision evidence was gathered here. |

## Verification commands, results and limits

The locked workspace was used directly. No scratch dependency resolution or copied
crate was needed. Compiled versions included **reqwest 0.13.4**, **quick-xml 0.42.0**,
**rust_decimal 1.43.0**, **tokio 1.53.1**, **jiff 0.2.35**, from the current lockfile.

```sh
git status --short
git rev-parse HEAD
git log --oneline -5
git show --stat fbda137
git diff fbda137^ fbda137 -- crates/szamlazz-agent/src/client.rs crates/szamlazz-agent/src/error.rs crates/szamlazz-agent/src/recovery.md crates/szamlazz-agent/README.md crates/szamlazz-agent/tests/client.rs

cargo test -p szamlazz-agent --features client-reqwest --locked --offline --target-dir /tmp/opencode/current-transport-target --lib --test client --test response_headers --test error_classification --test custom_http_client --test response_completion

cargo test -p szamlazz-agent --features client-reqwest --locked --offline --target-dir /tmp/opencode/current-transport-target --doc

rustc --edition=2024 --test /tmp/opencode/current-transport-fbda137.rs -L dependency=/tmp/opencode/current-transport-target/debug/deps --extern szamlazz_agent=/tmp/opencode/current-transport-target/debug/deps/libszamlazz_agent-b67ddb7a713ee73b.rlib --extern tokio=/tmp/opencode/current-transport-target/debug/deps/libtokio-92c4c7c67c80bee5.rlib -o /tmp/opencode/current-transport-fbda137
/tmp/opencode/current-transport-fbda137 --nocapture

git diff --exit-code fbda137 -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock
git rev-parse HEAD
git status --short
```

The first Cargo invocation hit the tool's **120-second build timeout**, while
compiling dependencies/the crate; no test failure was reported. Rerunning the
same command with a 240-second tool allowance completed successfully. Counts:

| Suite | Result |
|---|---|
| Library unit tests | 187 passed |
| `tests/client.rs` | 7 passed |
| `tests/response_headers.rs` | 10 passed |
| `tests/error_classification.rs` | 3 passed |
| `tests/custom_http_client.rs` | 1 passed |
| `tests/response_completion.rs` | 2 passed |
| Doctests, including README | 8 passed |
| Independent scratch checks | 6 passed |

**Total:** 210 repository unit/integration tests, 8 doctests, 6 scratch checks;
no test failures or skips in these selected suites. The T1 reproducer asserts
the observed defect, so its pass confirms that T1 remains present.

Scratch checks enumerate all routes/auth/header/status/body paths, all 43 named
codes, four interrupted-body cases, URL diagnostics through build and transport
failure, actual native-default 302/307/308 refusal, an injected short deadline,
and the deliberately different API versus unexpected-root display bounds.

Public pages were retrieved with `webfetch`. For P4, Python `urllib.request`
downloaded the exact ZIP URL above, `hashlib.sha256` hashed its bytes, and
`zipfile.ZipFile(io.BytesIO(...))` printed the cited first-party source ranges.
No archive member was executed or extracted to the repository.

Not run: live tests or account queries, a real 60-second stall, browser/wasm tests,
live TLS/auth/session-expiry/revocation probes, full workspace tests, a fresh
complete XSD comparison, or exhaustive NAV catalogue/operation-field validation.
The fresh corpus proves what is published today; local tests prove what this
commit does with those inputs. Neither replaces live vendor evidence.
