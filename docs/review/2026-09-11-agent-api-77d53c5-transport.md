# Számla Agent conformance review — shared transport, protocol and errors

**Date:** 2026-09-11  
**Baseline:** `77d53c553c9ecdc86d5fa72ca932c636256ae807` (whole current implementation)  
**Assigned report:** transport/shared-protocol review, executed directly without subdelegation  
**Result:** **no confirmed actionable P0–P3 finding in the assigned scope**. **255 selected offline tests passed**, none failed or were ignored.

This conclusion means no documented valid exchange was demonstrated to fail incorrectly in this slice. It is not certification of every operation's business mapping, exhaustive XML compliance, vendor runtime behavior, or all deployment platforms. Malformed-response tests and unresolved vendor semantics are distinguished below from valid-exchange conformance.

## 1. Scope and provenance

Read production `crates/szamlazz-agent/src/client.rs`, `credentials.rs`, `wire.rs`, `xml.rs`, `error.rs`, `ops/envelope.rs` and `ops.rs` in full. Followed action constants, credential placement, version selection and shared-parser entry points in the operation modules; inspected `number.rs`, PDF conversion, `lib.rs`, `recovery.md`, the crate README and relevant regression tests. Read `docs/szamlazz-hu-behaviour.md` in full as bounded account evidence. Operation-specific field completeness and the Restate worker belong to other reviews.

HEAD was the requested commit at entry and the subsequent baseline check. Existing working-tree changes were in the Restate worker and its documentation. `git diff 77d53c553c9ecdc86d5fa72ca932c636256ae807 -- crates/szamlazz-agent Cargo.lock docs/szamlazz-hu-behaviour.md` was empty. Code references below are relative to `crates/szamlazz-agent/src/` unless qualified.

During final report verification another process committed the worker work, advancing HEAD to `370ff2e5398e9ff4a3aff3b6008ab82e38b9d058`. A repeated scoped diff remained empty; the commit-range stat contained only worker/worker-documentation changes. The reviewed Agent implementation therefore still exactly matches the requested baseline. No test rerun was necessary for this unrelated advancement.

Official pages and two downloadable XSDs were fetched afresh during this review. The documentation footer reported **v202608271632**; this is a site-build label, not evidence that every example is recent. The PHP archive linked from the current download page was retrieved anew and inspected in memory. PHP was not executed.

The previous `2026-09-11-agent-api-61c334f-transport.md` report was consulted for candidate leads after reading the current production implementation and basic sources. Its results and line numbers were not carried forward as evidence. In particular, its description of empty `sikeres` no longer applies: current `xml.rs:479–485,759–766` requires a nonempty valid boolean.

No live/probe test, authenticated vendor request, `.env` read, source edit, commit or subdelegation was performed. The only authored repository file is this report. A retrieval-only Python script was created with `apply_patch` at `/tmp/opencode/transport-77d53c5-php.py`.

## 2. Fresh first-party source register

Short quoted clauses below are exact excerpts; surrounding explanations are this review's interpretation.

| Ref | URL | Contract/evidence used |
|---|---|---|
| B1 | [How does Számla Agent work?](https://docs.szamlazz.hu/agent/basics/how-does) | “HTTP POST request to `https://www.szamlazz.hu/szamla/`”; generation and email delivery are separate steps. |
| B2 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | “same URL every time”; selection by “name of the form field containing the XML file”; “HTTPS POST”; all eleven action names; one document per creation XML; names case-sensitive. |
| B3 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | “Use the `<szamlaagentkulcs>` tag”; legacy key/key: “use the same key in both fields”; key accepted “only in lowercase”; legacy user accesses “exactly one billing account”; “do not include it in client-side code.” Keys have identical permissions, do not expire until deleted, deletion takes effect immediately. |
| B4 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Save/reuse `JSESSIONID`; “If the session is inactive for 90 minutes, Számlázz.hu will delete it”; no persistence means authentication each request; new session after company/email edits “is advised”; saving to a file is “advisable.” |
| B5 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | “Do not retry in a loop”; same request “at most five times”, then stop for operator action; maximum 500 test invoices/10 minutes; v1 errors are plain text; 30 general numeric codes. |
| B6 | [Network and security](https://docs.szamlazz.hu/agent/basics/security) | Destination HTTPS CIDRs, effective August 1, 2025, are distinct from outbound sender IPs. No fixed client deadline, mandatory extra request header, or complete HTTP-status matrix is specified. |
| I1 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request) | “Content type: `multipart/form-data`”; main file `action-xmlagentxmlfile`; optional `attachfile1` … `attachfile5`. |
| I2 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | v2 “Structured `xmlszamlavalasz` with optional base64 PDF”; number and error text URL encoded, net/gross and error code not URL encoded; payment-method/customer-URL headers. Generic prose: “If error codes are present in the header, invoice number and amounts are omitted.” PHP's specific 56 exception is separately corroborated below. |
| I3 | [Settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) | Links to the actual per-topic settings pages, rather than a shared `/basics/settings` contract. |
| I4 | [Invoice email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | “up to 5 files”; “Size limit per file: 2 MB”; bad files individually omitted/notified while valid attachments are sent; ignored if email disabled. Test mail goes to the account-configured address. |
| I5 | [Order-number rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Per-type duplicate restriction; storno/corrective exemption; reversed order reusable; successful replay requires matching buyer/gross/three dates and creation “within the last 2 days.” |
| S1 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Same v2 envelope and encoded/raw header distinction; `szlahu_szamlaszam` names the storno invoice. |
| S2 | [Storno XML/XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml) | Fields “cannot be interchanged”; settings order includes credentials, appearance/download/copies, aggregator, guardian, version, external id. |
| C1 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response) | v2 `xmlszamlavalasz`, balance headers, false/code/message failure. |
| C2 | [Credit-entry XML/XSD](https://docs.szamlazz.hu/agent/credit_entry/xml) | Settings order: credentials, number, tax number, additive flag, aggregator, version; zero through five entries. |
| Q1 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | v2 “base64-encoded PDF inside `<pdf>`”; missing number/order/external selector returns 7. |
| Q2 | [PDF-query XML/XSD](https://docs.szamlazz.hu/agent/querying_pdf/xml) | Root-level credentials; selector then `valaszVerzio`, then external id; example selects 2. |
| Q3 | [XML-query response](https://docs.szamlazz.hu/agent/querying_xml/response) | Success `szamla`, failure `xmlszamlavalasz`; missing selector returns 7. |
| D1 | [Proforma deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | `xmlszamladbkdelvalasz`; “On critical error, a plain text/html error message may be returned instead”; false/335 example for nonexistent/already-deleted proforma. |
| R1 | [Receipt creation response](https://docs.szamlazz.hu/agent/generating_receipt/response) | False/code/message envelope; numeric supplement 336–340; reused call identifier makes the call unsuccessful and “will not duplicate an existing receipt.” |
| R2 | [Receipt-send response](https://docs.szamlazz.hu/agent/sending_receipt/response) | `xmlnyugtasendvalasz`; false/7 example: `Hiányzó adat: emailtargy elem.` Missing subject, not necessarily missing receipt. |
| R3 | [Receipt amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) | 261 exact sum, 363 whole gross, 364 net precision, 365 VAT precision; HUF/Ft-specific. |
| E1 | [Simplified image rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Source meanings of 551–556; request rejected for account/item/VAT/type restrictions. Inherited final account restriction included. |
| E2 | [Data erasure](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | “At most 400” per item, excess 537; disabled setting 539. B5 supplies demo/test 538. |
| X1 | [Response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Qualified `http://www.szamlazz.hu/xmlszamlavalasz`; required singleton `sikeres` boolean; optional singleton code/message/number/URL strings, totals `double`, PDF `base64Binary`. |
| X2 | [Invoice request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Ordered settings, XML credentials and optional integer version. Preview comment: `bizonylat előnézeti pdf (bizonylat nem készül)` — document preview PDF, no document made. |
| P1 | [PHP download](https://docs.szamlazz.hu/php/) | Current offered package “2.12.4 (2026.08.12.)”. |
| P2 | [PHP response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Invoice may be “successfully issued” despite notification delivery failure; exposes `hasInvoiceNotificationSendError()` separately. |
| P3 | [Official PHP 2.12.4 archive](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Fresh SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`; inspected locations below. |

The guessed URLs `/agent/basics`, `/agent/basics/settings`, and `/agent/generating_invoice/settings` returned HTTP 403. Actual linked basic pages and I3 succeeded. No unavailable page was treated as verified.

### PHP corroboration

Paths relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/` in P3:

- `Response/InvoiceResponse.php:17`: `const INVOICE_NOTIFICATION_SEND_FAILED = 56;`.
- `Response/InvoiceResponse.php:319–323`: `Ha a számlaértesítő kézbesítése sikertelen volt, de a válasz tartalmaz számlaszámot, akkor a számla kiállítása sikeres.` Translation: if notification delivery failed but the response contains an invoice number, issuance succeeded. The code tests number plus notification error. This does not guarantee behavior for contradictory or malformed responses.
- `Response/InvoiceResponse.php:128–158`: number/id/outstanding/net/gross/error headers read; error uses `urldecode`, customer URL uses `rawurldecode`. The latter is a reason to retain uncertainty about literal-plus encoding, not proof of a vendor emission.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank `szlahu_down` checked before body processing. Exception code 500 at line 151 is not a claim about wire HTTP status.
- `SzamlaAgentRequest.php:480–499,507–555`: POST, `CURLFile`/`text/xml`, action field, cookie handling, numbered attachments, configurable total/connect timeout. Its 30-second default (`:30`) is a client choice, not a required Számla Agent deadline. PHP's `charset`, `PHP` and `API` headers (`:501–505`) are not documented requirements on Rust clients.

## 3. Checked capability inventory

| Capability | Current code | Assessment |
|---|---|---|
| Endpoint and method | `wire.rs:7–14`; `client.rs:374–383` | Exact HTTPS endpoint from B1/B2; POST for all built-ins. |
| Multipart file, not raw XML or text field | `wire.rs:66–100` | Main part has `name`, `filename`, `text/xml`, CRLF separators and final boundary. Matches I1 and PHP. No requirement for `.xml` filename extension or the sample form's submit button was found. |
| Boundary and attachment bytes | `wire.rs:78–131` | Boundary candidate checked against XML and all attachment content; deterministic suffix remains below MIME's 70-character limit. Attachment bytes preserved. Disposition CR/LF stripped, quote/backslash escaped; MIME CR/LF stripped. Built-in action constants are static trusted strings. |
| Ordered XML output | `xml.rs:157–179,567–639`; `wire.rs:405–442` | UTF-8 XML 1.0 declaration/default namespace, ordered text writer, escaped markup. `to_wire` validates the operation then UTF-8/XML-character domain before HTTP. It does not claim to XSD-validate arbitrary custom `AgentRequest` implementations. |
| Authentication | `credentials.rs:5–24,45–81`; `xml.rs:628–638` | Key or username/password, including legacy key/key. No silent key normalization. Letting invalid uppercase credentials reach the vendor is not an interoperability failure. |
| Credential diagnostics | `credentials.rs:27–30,90–99`; `client.rs:149–172,339–346`; `wire.rs:43–50` | Keys/passwords and legacy key-as-username redacted; request body omitted; URL userinfo removed from builder/client/build-error diagnostics. This is not a blanket guarantee for every transport/parser error. |
| Native TLS/network | `Cargo.toml:24–25`; `client.rs:242–257,300–312` | rustls/platform trust, DNS host; no hardcoded old vendor IPs or certificate-verification bypass. B6 outbound IPs concern receivers. Explicit HTTP override is for caller-selected mocks/proxies; default remains HTTPS. |
| Native deadline/redirect | `client.rs:280–312` | 60-second request deadline; native redirects disabled. Avoids POST-to-GET redirects and credential-bearing body forwarding. Deadline covers transfer, not synchronous request serialization/response parsing. No vendor requirement for a different deadline established. |
| Native session lifecycle | `client.rs:193–212,315–331` | Default in-memory jar; clones share jar; a fresh default client gets a fresh jar. Injected clients/providers can share cookies and must be isolated/refreshed by the caller. Matches B4's performance and refresh guidance without claiming vendor key/cookie precedence. |
| Standalone cookie helper | `wire.rs:313–340` | First exact case-sensitive `JSESSIONID` pair in repeated Set-Cookie; skips malformed/nonmatching entries, trims SP/HTAB, preserves later `=`, permits empty value. Discards attributes explicitly; transport jar owns domain/path/lifetime. |
| Retries | `client.rs:193–207,374–405`; `recovery.md:4–9,36–55` | No application retry loop. Supplied client's policy retained; one `send` need not mean one POST. Caller owns five-total-send budget and reconciliation. No until-success loop or timeout-as-nonexecution claim. |
| HTTP evidence | `client.rs:385–405`; `wire.rs:182–311` | Bundled client always supplies status, headers and completed body. Incomplete body has its own error retaining original HeaderMap/status/source; never invents empty completed body. |
| Response decoding | `xml.rs:181–565`; `ops/envelope.rs:100–371` | Expected complete namespace-aware XML; shared verdict; optional payload/header fallback; conservative errors; specific numbered-56 support. Detailed matrix below. |
| Request/response version | `ops.rs:28–32` | One constant `RESPONSE_VERSION = "2"`, used by the four writers whose operations expose that choice. V1 support deliberately absent. |

### All eleven routing actions

All match B2 exactly. Clear credit entries shares the registration action and writer, so there are twelve request types but eleven distinct routes.

| Operation | Multipart action | Current reference |
|---|---|---|
| Invoice | `action-xmlagentxmlfile` | `ops/invoice.rs:679` |
| Storno invoice | `action-szamla_agent_st` | `ops/storno.rs:163` |
| Register/clear credit entries | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:238,275` |
| Invoice PDF | `action-szamla_agent_pdf` | `ops/query_pdf.rs:59` |
| Invoice XML | `action-szamla_agent_xml` | `ops/query_xml.rs:541` |
| Delete proforma | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:61` |
| Create receipt | `action-szamla_agent_nyugta_create` | `ops/receipt.rs:194` |
| Storno receipt | `action-szamla_agent_nyugta_storno` | `ops/receipt.rs:345` |
| Query receipt | `action-szamla_agent_nyugta_get` | `ops/receipt.rs:425` |
| Send receipt | `action-szamla_agent_nyugta_send` | `ops/receipt.rs:507` |
| Taxpayer | `action-szamla_agent_taxpayer` | `ops/taxpayer.rs:265` |

Version 2 is emitted at `ops/invoice.rs:767`, `ops/storno.rs:189`, `ops/credit_entry.rs:302`, `ops/query_pdf.rs:75`. Their settings ordering agrees with X2/S2/C2/Q2. PDF/XML queries inject credentials directly under their roots (`query_pdf.rs:67`, `query_xml.rs:549`); remaining built-ins inject under `beallitasok`. No response-version field is invented for the fixed-format receipt/deletion/XML-query/taxpayer routes.

### Native versus browser claims

`lib.rs:45–70` and README `:366–374` correctly distinguish core portability and browser transport compilation from usable direct access. The browser controls cookies/redirects, hides Set-Cookie, and requires CORS permission/header exposure. `Client::send` has no per-request Fetch credential override; an injected client cannot add one.

Inspected locked **reqwest 0.13.4**: `src/wasm/request.rs:46–48` starts timeout/credentials as None; `src/wasm/client.rs:196–240` sets credentials and timeout only if the request supplies them. This substantiates the same-origin Fetch-default claim and lack of the native 60-second default on wasm. Vendor B3 explicitly says no keys in client-side code; current README/lib guidance keeps keys server-side. No browser CORS, browser execution, Cloudflare deployment or fresh wasm compilation was established here (`rustup` unavailable).

Native reqwest source `src/retry.rs:9–17,195–202,273–317` distinguishes protocol-NACK retries from application recovery, with two additional retries configured in its default policy. The isolated `cargo tree` showed cookies/rustls, **no HTTP/2 or HTTP/3 feature**; relevant retry branches are feature-gated. Downstream feature unification or a supplied client can change that. No unsafe default duplicate issuance was demonstrated. “No application-level loop” is the supportable statement, not “exactly one physical POST under every build.”

## 4. Completed-response decisions, headers and numbered 56

| Received evidence | Actual decision | Reference |
|---|---|---|
| Nonblank `szlahu_down` | `ServiceUnavailable`, before code/status/body; blank treated as absent | `wire.rs:291–297` |
| Nonblank ordinary error-code header | `Api`, before status/body | `wire.rs:262–270,298–300`; `ops/envelope.rs:180–186` |
| Header 56 | Handed to operation: issuing envelope evaluates number/body; ordinary verdict parsers return API 56 | `ops/envelope.rs:179–248`; `xml.rs:528–539` |
| Non-2xx with no prior verdict header | `HttpStatus` with bounded body excerpt; success-number/id headers cannot bypass it | `wire.rs:301–308` |
| HTTP 200 body refusal | Required scalar verdict/code read, then API error; optional diagnostic may be unavailable | `xml.rs:478–523` |
| Normal numbered success | `CreatedInvoice`; optional body metadata before header fallback | `ops/envelope.rs:209–248` |
| Success without number | Internal unnumbered reply; invoice operation permits preview only when requested and PDF exists; storno requires number | `ops/envelope.rs:211–216,268–279`; `ops/invoice.rs:948–960` |
| Numbered 56 | Issued plus `notification_delivery_failed=true`; malformed optional amounts/PDF dropped | `ops/envelope.rs:205–248,285–315` |
| Header 56, non-56 body refusal | Readable body refusal wins, even when optional payload is malformed | `ops/envelope.rs:197–203` |
| Header 56, empty/plain text body | Narrow fallback, still requires number | `ops/envelope.rs:188–196,251–254` |
| Header 56, malformed XML or duplicate/nested body identity | Parse error/Unknown; header cannot launder unusable identity into success | `ops/envelope.rs:188–209,295–313` |
| 56 without usable number | API 56/Unknown | `ops/envelope.rs:211–216`; `error.rs:376–386` |
| Body transfer incomplete | `IncompleteResponse`/Unknown, retaining status, repeated raw headers and source; no partial body retained | `client.rs:66–100,117–126,385–393` |

These are deliberately more specific than the vendor's happy-path/error examples. PHP corroborates down-first and the numbered-56 exception. It does **not** establish arbitrary HTTP conflict precedence, repeated-header semantics, or every fallback shape. A body-only error at HTTP 500 yields HttpStatus rather than Api by explicit policy; both leave potentially completed writes unresolved where appropriate. No fresh source established that a valid vendor exchange requires a different decision.

The PDF-query parser also uses `parse_issued` (`ops/query_pdf.rs:83–93`), then requires a PDF. It does not expose the creation notification flag. No documentation establishes that a PDF query emits 56; the shared path's tolerance is not evidence of such behavior. Similarly, the blanket rustdoc shorthand that “56 surfaces as an error only without a number” (`error.rs:369–372`) must be read as the **issuing-parser rule**, not as a rule of credit-entry/receipt parsers. Current operation tests explicitly retain API 56 for registration (`tests/response_headers.rs:203–223`). No required operation-specific 56 behavior is missing on the evidence fetched.

### Header coverage and fidelity

| Header | Representation/decoding | Evidence and boundary |
|---|---|---|
| `szlahu_szamlaszam` | One form-style URL decode; trim/nonblank; body number preferred | Explicit encoded header in I2/S1/C1; `wire.rs:239–249,343–351`, `envelope.rs:120–133,326–329`. |
| `szlahu_error` | One form-style decode into API diagnostic | Explicit encoded header; no error-code header means this text alone is not a verdict. |
| `szlahu_error_code` | Raw trimmed token, open numeric/text mapping | Explicit not encoded; `%33` stays unknown rather than becoming code 3. |
| `szlahu_nettovegosszeg`, `szlahu_bruttovegosszeg` | Raw ungrouped numeric text, dot/comma, sign/exponent, SP/HTAB padding | Explicit not encoded; observed comma header in behavior note P60-E1. `envelope.rs:151–168,344–371`. |
| `szlahu_kintlevoseg` | Same numeric reader, optional body-first outstanding | PHP P3 and behavior note `:152,165` corroborate header; X1 carries body element. |
| `szlahu_id` | Raw nonnegative i64 or None | PHP P3; observed document id, not account identity (`behavior note :164`). Auxiliary malformed value cannot erase issuance (`envelope.rs:331–342`). |
| `szlahu_vevoifiokurl` | Optional body-first URL; encoded header fallback | Listed I2/S1/C1, decoded by PHP. XML URLs receive entity decoding only, not URL decoding. Exact literal-plus header grammar remains unestablished. |
| `szlahu_fizetesmod` | Decoded open `PaymentMethod` token | Listed I2/S1/C1; `envelope.rs:318–324`. Exact wire encoding is less explicit than number/error; unknown token preserved. No nonexistent XML payment-method field substituted. |
| `szlahu_down` | Decoded nonblank text; explicit unavailable error | PHP P3 establishes control header; exact encoding is not a complete documented grammar. |
| `Set-Cookie` | Native jar; standalone exact session helper; incomplete raw evidence | B4; cookie values redacted from RawResponse Debug and omitted from IncompleteResponse Debug. |

The complete-response client converts header values with lossy UTF-8 (`client.rs:394–403`); the documented ASCII/URL-encoded values are representable. RawResponse exposes the first matching repeated header (`wire.rs:227–237`), without a vendor-defined conflict-resolution guarantee. Unknown response headers remain accessible.

Numeric body text wins when nonblank, even if malformed; a valid header does not overwrite bad reported body money. Empty body metadata may fall back; a present blank numeric header is malformed. `1,234` is 1.234, not grouping. Mixed separators/underscores/embedded spaces fail. Text headers decode once: `+` → space, `%2B` → plus, `%252B` → `%2B`; invalid escaped UTF-8 preserves the plus-adjusted encoded input. Those fallback choices are local policy.

## 5. XML and scalar conformance

- **Completed document:** `xml.rs:201–275` checks UTF-8, expected expanded root, matching completion, one root, legal prolog/epilog through EOF; rejects DTDs, truncation, extra roots, outside text/CDATA/references and malformed tails. `:280–300` adds lexical token/reference checks even in ignored fields. No external-entity resolution or schema retrieval occurs.
- **Namespaces:** `xml.rs:37–155` normalizes namespace declaration values before reserved-binding and expanded-attribute uniqueness checks; undeclared element/attribute prefixes and reserved `xmlns` element prefix fail. `:310–378` strips foreign subtrees, including descendants reentering the protocol namespace, and canonicalizes aliases. A placeholder prevents ignoring a foreign child from turning `tr<foreign/>ue` into a true verdict.
- **Verdict:** `xml.rs:478–499,759–787` requires exactly one readable boolean `sikeres`; accepts true/false/1/0 with XML whitespace. Missing/empty/NBSP-only/invalid verdict remains Parse/Unknown, even next to a readable body refusal. False with absent/blank code becomes honest `ErrorCode::Absent`. A true verdict overrides a body code; the docs use false for errors and establish no contradictory true/code response rule.
- **Diagnostics versus facts:** unusable nested/duplicate `hibauzenet` becomes absent diagnostic, independently of the unique verdict/code. This cannot erase readable refusal or numbered-56 evidence. Duplicate/nested facts and ambiguous identity still fail (`xml.rs:478–517`, `envelope.rs:285–315`).
- **Text fidelity:** regular business strings are XML-decoded and retain nonblank character content, including padding/NBSP (`xml.rs:745–757`). Envelope number/URL trimming uses separate explicit policy. Entity/CDATA support does not mean byte preservation; XML line-ending normalization applies.
- **Money:** `number.rs:61–140` parses finite exact decimal/exponent representations, without f64 rounding or unbounded exponent expansion. XSD double's entire value space (NaN/infinities/out-of-range finite numbers) is intentionally unsupported. Normal malformed optional issuance metadata can cause Parse/Unknown; numbered 56 tolerates it to retain known issuance.
- **Dates:** outbound years 1–9999 checked via `xml.rs:19–35`; response civil reader `:667–725` accepts supported timezone suffixes up to ±14:00 without UTC conversion, uses checked character boundaries, and refuses nonempty malformed dates. This is not Adatkapcsolat's date-content policy or a full XSD date validator.
- **PDF:** `types.rs:104–117` strips whitespace and decodes standard base64. This verifies encoding, not PDF syntax, signatures, pages or invoice correctness. No unsupported PDF guarantee is inferred from synthetic `%PDF-` controls.

Fresh I2/S1/C1 examples still contain unescaped URL ampersands; PDF samples contain ellipses, and Q3 contains descriptive PDF placeholder text. These literal bytes are not complete valid response artifacts. Refusing malformed illustrative samples is not evidence of a vendor interoperability regression. The upstream corpus tests label their transformations; they do not turn placeholders into live evidence.

## 6. Complete named numeric-error coverage

All **43 named numeric codes** were checked against the current table and sources. `code()`/numeric lookup: `error.rs:215–318`; class dispatch: `:376–424`; string/numeric parsing: `:458–487`. `U` = Unknown; `R` = Rejected; `D` = DuplicateOrderNumber; `N` = NotFound. Only **1/55** are potentially retryable; only **3/135/136/164** are credential/access errors (`:320–353`).

| Code | Rust variant | Class | Meaning/source |
|---|---|---|---|
| 1 | Maintenance | U | Maintenance/internal error, wait minutes (B5). |
| 3 | InvalidCredentials | R | Login/key/password failure (B3/B5). |
| 7 | MissingData | N | Missing invoice selector (Q1/Q3) or missing request data, e.g. receipt email subject (R2). |
| 14 | StornoOfReversalInvoice | R | Observed storno/credit original cannot itself be reversed/credited (behavior note `:107`). |
| 53 | XmlNotAFile | R | Missing properly uploaded XML file (B5). |
| 54 | EInvoiceNotEnabled | R | E-invoice permission/certificate setup absent (B5). |
| 55 | EInvoiceSigningFailed | U | Signing failed; certificate expired or timestamp server unavailable (B5). No issuance proof. |
| 56 | InvoiceNotificationDeliveryFailed | U as error | Specific numbered issuance-warning exception (P2/P3), not observed in historical account attempts. |
| 57 | MalformedXml | R | Request XML read/XSD failure (B2/B5). |
| 71 | DuplicateOrderNumber | D | Account duplicate-order restriction (B5/I5). |
| 73 | PrepaymentInvoiceNotIdentifiable | R | Observed missing/unusable/already-settled prepayment (behavior note `:138`). |
| 135 | BrowserSessionActive | R | Browser-session access refusal (B5). |
| 136 | LoginBlocked | R | Subscription/payment/account access blocked (B5). |
| 152 | DuplicateOrderNumberNamed | D | 71 with offending order number in message (B5/I5). |
| 164 | MultipleAccounts | R | Legacy user accesses multiple accounts (B3/B5). |
| 202 | UnregisteredPrefix | R | Empty/unregistered invoice prefix (B5). |
| 221 | HasCorrectiveInvoice | R | Observed original has corrective and cannot be stornoed (behavior note `:108`). |
| 259 | NetValueMismatch | R | Net versus unit price × quantity (B5). |
| 260 | VatValueMismatch | R | VAT versus net × rate / 100 (B5). |
| 261 | GrossValueMismatch | R | Gross versus net + VAT (B5/R3). |
| 262 | NetValueInvalid | R | Net check, row diagnostic (B5). |
| 263 | VatValueInvalid | R | VAT check, row diagnostic (B5). |
| 264 | GrossValueInvalid | R | Gross check, row diagnostic (B5). |
| 335 | ProformaNotFound | R | Absent/deleted proforma; refusal, not replayed deletion success (D1). |
| 336 | ReceiptPrefixUsedForInvoices | R | Receipt prefix used for invoices (R1). |
| 337 | InvalidReceiptPrefix | R | Capital letters/digits required (R1); later account note also reports five-character cap. |
| 338 | DuplicateReceiptCallId | R | Reused creation call identifier refused; no original success replay (R1). |
| 339 | ReceiptNotFound | N | Receipt number absent (R1). |
| 340 | ReceiptPaymentMismatch | R | Tender sum differs from gross (R1). |
| 352 | IssueDateMustBeToday | R | Observed storno issue-date refusal, not fulfillment-date rule (behavior note `:109`). |
| 363 | ReceiptGrossNotWhole | R | HUF/Ft receipt whole gross (B5/R3). |
| 364 | ReceiptNetPrecision | R | HUF/Ft receipt net precision (B5/R3). |
| 365 | ReceiptVatPrecision | R | HUF/Ft receipt VAT precision (B5/R3). |
| 463 | PaymentOnReversedInvoice | R | Observed credit entry on reversed/reversing invoice, body only (behavior note `:155`). |
| 537 | ErasureCodeLimit | R | Maximum 400 erasure codes per item (B5/E2). |
| 538 | ErasureCodesUnavailable | R | Demo/test restriction (B5). |
| 539 | ErasureCodesDisabled | R | Account setting disabled (B5/E2). |
| 551 | SimplifiedImageAccountIncompatible | R | OSS/non-Hungarian seller tax number, inherited final included (B5/E1). |
| 552 | SimplifiedImageItemLimit | R | At most two items, four on final (B5/E1). |
| 553 | SimplifiedImageVatInvalid | R | Prohibited VAT token (B5/E1). |
| 554 | SimplifiedImageCannotCorrect | R | Cannot correct simplified original (B5/E1). |
| 555 | SimplifiedImagePrepaymentVatMismatch | R | Final/prepayment VAT mismatch (B5/E1). |
| 556 | SimplifiedImageDocumentForbidden | R | Simplified delivery-note/corrective prohibition (B5/E1). |

Coverage arithmetic: **30 general + 5 receipt supplement + 2 operation-example codes (7/335) + 1 PHP warning + 5 historically observed codes = 43**. No numeric mapping gap, swapped meaning or unjustified future-code refusal was found. The newer observed receipt-send **153** in behavior note `:35–43` is **not a named variant**: it remains `Unknown("153")`/Unknown. The conservative open mapping preserves the message and does not automatically authorize resend. Typing that operation-specific refusal would be a separate evidence-backed catalogue extension, not a missing code from the fetched basic contract.

Known numeric spellings normalize (`007` → 7); unknown tokens preserve trimmed text, including values above u16 and NAV strings. Empty token → Absent. Unknown/Absent are U, noncredential, and not affirmatively retryable. The complete NAV catalogue is outside this shared Agent-code review.

Local `ClientError::Request` is R (nothing sent). HTTP/down/parse/transport/incomplete errors are U (`error.rs:753–771`, `client.rs:103–138`). A refusal class describes **this exchange**, never settlement of an earlier lost send. Code 7's N class must be interpreted for the operation. Code 338 refuses a duplicate without proving the original's outcome or returning its identity. Current `error.rs:335–342` explicitly treats credential codes as access-refusal interpretation and says exact server processing order is unestablished.

## 7. Findings, reproducers and separate uncertainty register

### Confirmed actionable findings

**None.** There is no current finding with a verified implementation failure, authoritative requirement, affected exchange and impact sufficient to assign P0–P3 severity. No fix is recommended on this evidence.

The following concrete reproducers were executed by the existing offline tests to independently check high-impact historical candidates. These are controls that passed, not new defect reports or vendor captures:

| Candidate/reproducer | Current observed result and impact avoided |
|---|---|
| Legacy `Credentials::user_password("legacy-key", "legacy-key")`, format credentials/client/builder | Secret absent from Debug. Supported authentication form no longer leaks its key through these formatters. |
| HTTP 200, Content-Length 1000, only `x` transferred before close, with repeated cookies and code 3/down/numbered 56/number headers | `IncompleteResponse`, original status/headers/source retained, U; no invented success/refusal from incomplete evidence (`tests/client.rs:275–337`). |
| Header 56 + number I-2; body false/code 3 plus duplicate or nested optional total | API 3, never issuance (`tests/response_headers.rs:465–484`). |
| Body false/56 plus unique I-2 and nested total or duplicate PDF | Issued I-2 with notification warning; optional bad payload cannot hide identity (`tests/response_headers.rs:507–536`). |
| Body false/56 plus duplicate/same-value duplicate/nested number, with valid header number and optional bad total | Parse/U; header cannot replace malformed body identity (`tests/response_headers.rs:539–582`). |
| Empty/missing/NBSP-only `sikeres`, beside 3/53/57/463/56/FUTURE | Parse/U across shared envelope users, no false settled refusal (`tests/response_booleans.rs:79–130`). |
| Header total `100,01`; code `%33`; textual `a%2Bb+c`; XML URL `...?q=a%2Bb+c&amp;x=%252B` | Exact 100.01; unknown `%33`; text `a+b c`; XML URL keeps literal `+`/percent spelling, entity decoded only (`tests/response_headers.rs:226–338`). |
| Valid completed root plus extra root, truncated close, malformed prolog/epilog, bad namespaces or forbidden characters in ignored extension | Refused; valid aliases/CDATA/foreign extensions pass. Full-body checks do not stop after obtaining an early success field (`tests/response_completion.rs`, `response_namespaces.rs`). |

### Ambiguity, intentional policy and hardening — not confirmed defects

| Topic | Current evidence and boundary |
|---|---|
| Header/status contradictions | No source defines every conflict, repeated code header or disagreement between body/header number. Down → code → non-2xx → body and body-first metadata are explicit library policy. A synthetic counterexample to a different preferred policy is not a vendor-contract violation. |
| Exact encoded-header grammar | Number/error encoding explicit; literal-plus meaning for customer URL/payment method/down, invalid percent escapes and conflicting duplicates not fully specified. PHP uses raw URL decode for the customer URL. Retain uncertainty pending raw evidence; no invented guarantee either way. |
| Numbered 56 wire variants | PHP provides strong specific support; historical D6 did not produce 56. Empty/plain-text and malformed-metadata test combinations are local robustness policy. No account capture establishes all those shapes. |
| Cookies after key deletion/account changes | B3 immediate key deletion and B4 sessions do not specify whether an existing session is revalidated or whether cookie/key wins. Fresh jar ownership/refresh is supported policy; native mock isolation does not establish vendor account selection. |
| Endpoints and diagnostics | Explicit caller-supplied HTTP/userinfo URLs remain accepted; builder diagnostics redact userinfo. No guarantee is made that arbitrary query-string secrets or reqwest error URLs are redacted. This is an override/logging boundary, not default credential leakage demonstrated here. |
| Response resource limits | Entire body buffered/copied (`client.rs:387–403`), protocol projection allocates, no explicit response-size/depth cap. No load/OOM proof or mandated vendor limit was established; potential resource hardening stays separate. |
| Complete parse-error evidence | Incomplete transfer preserves headers/status; ordinary completed parse failure does not return the full RawResponse through ClientError. BYO transport can retain it. Useful recovery/diagnostic enhancement, not evidence that a supported reply is misclassified. |
| Logging | `HttpStatus`/`UnexpectedBody` excerpts bounded; API messages and other parser errors may contain full upstream text. RawResponse Debug only redacts cookies, not all headers. README `:419–422` accurately states this; no general log-safety promise. |
| XSD versus application domain | Exact finite Decimal, finite civil dates, sparse optional data, unknown extensions and nonnegative auxiliary id are deliberate policies; parser is not full XSD validation. No ordinary valid amount/date was shown corrupted. |
| Attachments | Local five-file/decimal 2,000,000-byte cap is conservative versus I4's unspecified “2 MB”; vendor individually drops bad files while library can refuse before send. Filename rendering and exact byte threshold not established. |
| Retry budget/deadline | B5 says five total sends of the same request, not initial plus five; no exact combined budget for write/reconciliation queries. Sixty seconds is local. Timeout or immediate empty query does not establish nonexecution. |
| Existing account observations | Behavior note `:3–47,157–173,184–299` bounds evidence. Comma totals, body-only query 7/credit 463, storno replay/no-op and external-id reuse justify intentional behavior. Historical worker “design consequence” claims, including pre-auth sequencing at `:276–282`, are not fresh vendor proof. |
| Reversal heuristic | `CreatedInvoice::reverses` (`envelope.rs:61–87`) explicitly says changed number/nonpositive known gross is heuristic. False is inconclusive; query type/reference. No zero-original or negative-original acceptance guarantee is invented. |
| Browser capability | Compilation/platform claims are appropriately qualified; B3 prohibits shipping the key to browsers. This review did not establish runtime support, CORS/header exposure or wasm compilation. |

## 8. Commands actually run and verification limits

Repository commands:

```sh
git status --short && git rev-parse HEAD

cargo test -p szamlazz-agent --locked --offline --features client-reqwest --lib --test client --test custom_http_client --test response_headers --test response_completion --test response_namespaces --test response_booleans --test numeric_fidelity --test error_classification --test upstream --test business_text --test request_dates

cargo tree -p szamlazz-agent --locked --offline --features client-reqwest -e features -i reqwest

rustup target list --installed

git diff 77d53c553c9ecdc86d5fa72ca932c636256ae807 -- crates/szamlazz-agent Cargo.lock docs/szamlazz-hu-behaviour.md && git rev-parse HEAD
```

`rustup` failed with `command not found`; no target availability/wasm build is claimed. Cargo tests/tree succeeded. Baseline diff was empty and HEAD unchanged.

Final verification commands (after report creation):

```sh
git diff --check && git status --short && git rev-parse HEAD
git diff 77d53c553c9ecdc86d5fa72ca932c636256ae807 -- crates/szamlazz-agent Cargo.lock docs/szamlazz-hu-behaviour.md && git diff --stat 77d53c553c9ecdc86d5fa72ca932c636256ae807..HEAD
```

The first reported the new worker-only HEAD described in §1 and only this report as untracked; the second reconfirmed an empty audited-scope diff. `git diff --check` passed for tracked changes (the newly authored report is untracked).

Scratch/first-party archive retrieval commands:

```sh
ls /tmp/opencode
python /tmp/opencode/transport-77d53c5-php.py
python3 /tmp/opencode/transport-77d53c5-php.py
```

`python` failed with `command not found`; `python3` succeeded. The script uses `urllib.request.urlopen(..., timeout=60)`, hashes the fresh ZIP, opens it in memory with `zipfile.ZipFile`, and prints numbered selected source ranges. No extraction or execution of vendor code. All other official retrievals used `webfetch` at the URLs in §2; local reads/searches used dedicated tools.

| Test target | Passed |
|---|---:|
| Library unit tests | 188 |
| business_text | 2 |
| client | 9 |
| custom_http_client | 1 |
| error_classification | 3 |
| numeric_fidelity | 6 |
| request_dates | 3 |
| response_booleans | 3 |
| response_completion | 4 |
| response_headers | 14 |
| response_namespaces | 11 |
| upstream | 11 |
| **Total** | **255** |

Offline Cargo prevents dependency fetching; local HTTP tests intentionally use loopback. They do not contact Számla Agent. Native client tests inject equivalent cookie/timeout/no-redirect settings without roots (`tests/client.rs:18–38`); they do not verify the current vendor certificate chain, actual 90-minute expiry or a timed 60-second stall. The upstream suite exercises the existing fixture corpus; request outline equivalence is not byte equality or proof of every omitted/empty semantic.

**Disposition:** scoped implementation conforms on the established evidence. Keep the current uncertainty-preserving error/56 behavior and evidence-qualified platform/session/recovery claims. Resolve the separate vendor ambiguities with first-party clarification or authorized captures before treating a preferred alternative policy as a required fix.
