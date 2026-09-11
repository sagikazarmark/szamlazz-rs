# Számla Agent current-crate review — shared transport and protocol

**Reviewed:** 2026-09-11. **Start/checked HEAD:** `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

**Conclusion:** no confirmed P0–P2 shared-protocol implementation defect found. One **P3, high-confidence public evidence/provenance correction** remains: the error catalogue still says none of the thirteen receipt/simplified-image additions were observed, although the September 11 execution record documents 337. All eleven multipart action names and all four response-version writers match the fresh sources checked. HTTP conflicts, exact customer-URL header encoding, successful-number guarantees and some session semantics remain explicitly unresolved rather than proven compliant.

This is a current-implementation review, not an empty-diff review: HEAD equalled the requested start, so `git diff <start>...HEAD` and the intervening commit list were empty. The user explicitly requested the whole shared surface. No source edits, authenticated vendor requests or subagents were used. Broad Cargo testing belongs to the parent review; **no Cargo tests were run here**, and the older report's 255 passes are not this review's result.

## 1. Scope, method and evidence boundaries

Read production sections of `crates/szamlazz-agent/src/{client.rs,credentials.rs,wire.rs,xml.rs,error.rs,ops.rs,ops/envelope.rs}`, public `recovery.md`, crate README and `lib.rs`; followed operation routing, credential/version writers and parser entry points. Also inspected exact-number/PDF helpers, selected shared regression tests, and locked reqwest source for transport/platform claims. Line references below are current at the start commit; unqualified source paths are relative to `crates/szamlazz-agent/src/`.

Compared fresh official basic, authentication, security, cookie, request, settings and **all eleven operation response pages**, plus the downloaded invoice request/response XSDs and current official PHP package. The operation specialists own full request-field matrices, legal/business rules and individual document projections; this report checks their shared protocol boundary and recovery implications.

Evidence is ranked and kept distinct:

1. Fresh public official documents/XSDs specify the contract, but conflicting editions and illustrative placeholders need adjudication.
2. Fresh first-party PHP source corroborates implementation choices, especially 56; it is not a capture of the vendor server executing them.
3. `docs/research/*-live.md` records executed account observations, bounded by its stated account/date/capture method.
4. `docs/szamlazz-hu-behaviour.md` records historical observations plus design consequences. Its historical raw logs are explicitly outside this repository (`:11–18`); a design consequence is not additional evidence.
5. Code and offline tests establish local behavior only. Existing untracked `77d53c5` reports were preserved; the transport report was consulted for leads, not adopted as a current verdict or test result.

The English site footer consistently reported **v202608271632**. This is a documentation build label, not a server version or proof every example is recent. All citations in §2 were fetched successfully in this review. No secrets/configuration files were inspected.

## 2. Fresh primary-source register

| Ref | Source | Clauses used |
|---|---|---|
| B1 | [How it works](https://docs.szamlazz.hu/agent/basics/how-does) | XML in HTTP POST to `https://www.szamlazz.hu/szamla/`; creation and email delivery are distinct steps. |
| B2 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | Same URL, HTTPS POST, file-field action table, one document per XML, case-sensitive elements, XSD-validation advice; misspelled tags can produce 57 or be ignored. |
| B3 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Preferred XML key; lowercase/case-sensitive; legacy username/password and key/key supported; legacy user must access exactly one account. Keys do not expire, have identical permissions, maximum 17; deletion immediate. No keys in client-side code. |
| B4 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Reuse `JSESSIONID`; deletion after 90 minutes inactive; without reuse each call authenticates; fresh session advised after company/email edits. File persistence is advice. |
| B5 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | No retry-until-success loop; same request at most five attempts then human intervention; 500 test invoices/10 minutes; v1 plain-text `[ERR]` format; 30 general codes. |
| B6 | [Network/security](https://docs.szamlazz.hu/agent/basics/security) | HTTPS destination CIDRs effective August 1, 2025, separately from vendor outbound sender IPs. No exact HTTP-status matrix or mandatory client timeout given. |
| I1 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request) | `multipart/form-data`, main file action, `attachfile1`…`attachfile5`. |
| I2 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | v1 text/PDF versus v2 `xmlszamlavalasz`; number/error text URL-encoded, totals/error code not encoded; payment method/customer URL; false/code/message failure. |
| S1 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Same v2 envelope and header distinction; number is that of storno. |
| S2 | [Storno XML](https://docs.szamlazz.hu/agent/reversing_invoice/xml) | Ordered settings: credentials, appearance/download/copies, aggregator/guardian, version, external id. |
| C1 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response) | v2 balance envelope and headers; `szamlaszam` optional in schema covering both outcomes. |
| C2 | [Credit-entry XML](https://docs.szamlazz.hu/agent/credit_entry/xml) | Credentials/number/tax number/additive/aggregator/version order; zero through five entries; replace versus additive. |
| Q1 | [PDF response](https://docs.szamlazz.hu/agent/querying_pdf/response) | v2 base64 PDF in XML; unknown selector code 7; optional number in envelope XSD. |
| Q2 | [PDF request XML](https://docs.szamlazz.hu/agent/querying_pdf/xml) | Root-level credentials, selector, version, external id; version 2. |
| Q3 | [XML response](https://docs.szamlazz.hu/agent/querying_xml/response) | `szamla` on success, `xmlszamlavalasz` on error, unknown selector code 7. |
| D1 | [Delete response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | `xmlszamladbkdelvalasz`; critical errors may be text/HTML; false/335 for absent/deleted proforma. |
| D2 | [Delete request XML](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml) | Credentials under settings; number or lowercase `rendelesszam`. |
| D3 | [First-party PHP deletion](https://docs.szamlazz.hu/php/dijbekero-torles) | Order-based deletion can delete multiple proformas; rollback on a failed member. |
| R1 | [Receipt creation response](https://docs.szamlazz.hu/agent/generating_receipt/response) | `xmlnyugtavalasz`, receipt data plus optional PDF; reused call identifier refuses rather than duplicates; supplement 336–340. |
| R2 | [Receipt storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) | Same envelope, `SN` data; already reversed or storno target is a refusal, not documented invoice-style replay. |
| R3 | [Receipt query response](https://docs.szamlazz.hu/agent/querying_receipt/response) | Same response as receipt creation. |
| R4 | [Receipt send response](https://docs.szamlazz.hu/agent/sending_receipt/response) | `xmlnyugtasendvalasz`, boolean verdict; code 7 example means missing email subject. |
| R5 | [Receipt send XML](https://docs.szamlazz.hu/agent/sending_receipt/xml) | Present empty email details request prior details; omitted block sends no email. |
| R6 | [Receipt order setting](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) | Separate receipt repetition toggle; restriction bars another receipt for previously used order. |
| R7 | [First-party PHP receipt lookup](https://docs.szamlazz.hu/php/nyugta-lekerdezes) | Number or order; “last matching document.” |
| T1 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | NAV `QueryTaxpayerResponse`; 2020-11-04 OSA 2.0 examples; `funcCode`, numeric/text error code; `OK` plus false validity is successful lookup. Links NAV 3.0 specification. |
| G1 | [Settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) | Actual per-topic settings pages; no single shared settings contract. |
| G2 | [Invoice order/replay](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Account toggle; per-type check; storno/corrective exemption; reuse after reversal; replay requires buyer/gross/three dates and last-two-days condition. |
| G3 | [Email/attachments](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Omitted/true `sendEmail` with address sends; false suppresses; five attachments, 2 MB each, bad attachments individually omitted; test mail goes to account-configured address. |
| G4 | [Simplified image](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Meanings of 551–556, including inherited final account restriction. |
| G5 | [Receipt amount rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) | HUF/Ft whole gross, two-place net/VAT, exact sum; 261, 363–365. |
| X1 | [Downloaded response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Qualified namespace, required singleton boolean verdict; optional singleton strings, double totals, base64 PDF. |
| X2 | [Downloaded invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Credential/settings order and version; preview comment says no document is made. |
| P1 | [PHP download](https://docs.szamlazz.hu/php/) | Currently offers 2.12.4 (2026.08.12.). |
| P2 | [PHP response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Successful invoice issuance can coexist with failed notification; separate success/warning methods. |
| P3 | [Official PHP ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Freshly retrieved and inspected in memory; SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. |

P3 paths relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:17,319–323`: code 56; comment and predicate say notification failure **with invoice number** is successful issuance.
- `Response/InvoiceResponse.php:128–158`: number/id/outstanding/net/gross/error channels; `urldecode` for error, **`rawurldecode` for customer URL**.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank down header checked before XML/text interpretation. Its exception code 500 is not an assertion about the HTTP status sent by szamlazz.hu.
- `SzamlaAgentRequest.php:480–505,507–555`: POST, `CURLFile` with `text/xml`, cookie handling, numbered attachments, configurable total/connect timeouts. Default 30 seconds at `:30` is PHP-client policy, not a protocol-mandated deadline. PHP version/charset headers are not requirements documented for all clients.

## 3. Prioritized findings

### F1 — P3 / high confidence: public error-catalogue provenance excludes an executed 337 observation

**Current locations:** `error.rs:359–374`, especially `:374`; `recovery.md:57–62`. Related accurate text: `error.rs:157–159` already notes the September 11 five-character prefix reply.

`ErrorCode::outcome_class` says “Neither 55 nor the thirteen receipt/simplified-image additions were observed on the account.” Code 337 is one of those thirteen (see `tests/error_classification.rs:49–75`). The executed record `docs/research/2026-09-11-receipts-live.md:20–42` records a seven-character prefix refused with **337**, quoting the returned Hungarian text and identifying the run and call id. The historical-account continuity caveat (`:5–10`) means this is not proof about the September 3 account, but the public catalogue does not qualify “the account” as that historical account. `recovery.md:57–62` likewise presents the whole addition set as not live-account observations without this later exception.

**Impact:** readers get contradictory evidence status from the same crate and can mistakenly treat the now-observed receipt prefix restriction as only a hypothetical/documentation-derived case. This is provenance drift, not incorrect numeric classification, a demonstrated acceptance bug, or justification to claim all receipt codes were observed.

**Recommendation:** retain the source-derived origin of the thirteen classifications, add a dated bounded exception for 337, and state which remaining codes lack execution evidence. Keep 55/56 explicitly unobserved. Do not upgrade 336/339/340/363–365 or 551–556 from the successful lifecycle run.

**Verification:** direct current text versus dated execution record; no runtime reproduction needed. The record is a transcribed execution, not an archived raw HTTP response. This limits claims about the channel carrying 337, but does not erase the recorded code.

### No confirmed shared implementation defect

No fetched documented valid common exchange was demonstrated to be corrupted, routed incorrectly or wrongly retried. This is a scoped negative finding, not proof of complete XML conformance or every server/platform behavior. Questions below are not promoted to P1/P2 without the missing contract/runtime evidence.

## 4. Request, authentication and transport coverage

| Surface | Current lines | Assessment |
|---|---|---|
| Endpoint/method | `wire.rs:7–14`; `client.rs:374–383` | Exact B1/B2 HTTPS endpoint and POST. URL belongs to transport, not operation. |
| Multipart main file | `wire.rs:66–100` | File disposition includes name and filename; `text/xml`; CRLF and closing boundary. Matches I1/P3. No source requires an `.xml` filename extension or HTML submit-button field. |
| Attachments and boundary | `wire.rs:78–131` | Raw bytes retained; collision candidate checked against XML and all file contents; decimal suffix below MIME boundary limit. Disposition CR/LF removed, quotes/backslash escaped; MIME CR/LF removed. Built-in action constants are trusted static strings. |
| Checked outbound XML | `wire.rs:365–381,398–442`; `xml.rs:157–179,567–639` | Operation validation then UTF-8/XML 1.0 character scan, escaped text, ordered writer/default namespace. `write_xml` is explicitly unchecked. This is not runtime XSD validation of arbitrary custom `AgentRequest` implementations. |
| Date domain | `xml.rs:19–35`; README `:242–249` | Checked outbound positive years, documented 1–9999. No silent date conversion. Full per-operation date rules delegated. |
| Credentials | `credentials.rs:5–24,45–81`; `xml.rs:628–638` | XML key or legacy user/password, key/key supported. No Basic/Bearer header required by B3. Lowercase documented, supplied spelling not silently changed. Permitting invalid keys to reach authentication is not an interoperability failure. |
| Redaction | `credentials.rs:27–30,90–99`; `client.rs:149–172,339–346`; `wire.rs:43–50,155–180` | Key and legacy key-as-username hidden in Debug; request body omitted; URL userinfo removed in builder/client/build errors; response cookies hidden. No blanket redaction of arbitrary API messages/URL query secrets claimed. |
| Endpoint overrides | `client.rs:182–191,242–257` | Requires http/https plus host at build. Explicit HTTP and URL userinfo remain allowed; default is HTTPS. This is caller-selected transport flexibility, not enforced production HTTPS-only mode. |
| TLS/network | crate `Cargo.toml:24–25`; `client.rs:300–312` | reqwest rustls/platform trust, no verification bypass in default client, no pinned outdated vendor IPs. B6's outbound sender IPs are for receivers, not destinations for these POSTs. Actual vendor TLS handshake not tested here. |
| Deadline/redirects | `client.rs:280–312` | Native 60-second whole request timeout and no redirects; avoids credential-bearing redirected POSTs and POST→GET conversion. Timeout does not stop vendor work or bound synchronous serialization/parsing. |
| Cookie ownership | `client.rs:193–207,315–331`; `wire.rs:313–340` | Default fresh in-memory jar, clones share it. Injected client/provider can share it; caller isolates accounts and refreshes after changes. B4 supports reuse/90-minute inactivity/refresh advice, not credential-cookie precedence. |
| Standalone cookie helper | `wire.rs:313–340` | First exact case-sensitive `JSESSIONID` pair across repeated headers, skips nonmatches/malformed pairs, trims SP/HTAB, preserves later `=`, accepts empty value. Attributes are deliberately discarded; caller jar owns domain/path/expiry. Native client uses reqwest jar directly. |
| Retry ownership | `client.rs:193–207,374–405`; `recovery.md:4–9,36–55` | No application retry/recovery loop; injected policy retained. Five-total-send limit documented, not globally enforced by the crate. No universal one-POST guarantee. |
| Browser/native distinction | `lib.rs:45–70`; README `:366–374` | Compilation differs from direct access: Fetch credentials/CORS/header exposure and browser cookie restrictions disclosed; keys stay server-side per B3. No browser execution/CORS/Cloudflare deployment proven. |

Locked reqwest is **0.13.4** (`Cargo.lock:2078–2079`). Inspected local dependency source `src/retry.rs:9–17,195–202`: default protocol-NACK classifier allows up to two additional tries; this is not application reconciliation. No unsafe duplicate issuance was demonstrated, and no downstream-feature-unification matrix was run. Browser `src/wasm/request.rs:46–48` initializes timeout/credentials to None; `src/wasm/client.rs:222–238` applies them only when set per request. This supports the current qualified Fetch claims, not a live browser capability claim.

### All eleven actions and response families

Every action exactly matches B2. Clear credit entries adds a request type sharing registration's route/writer, not a twelfth vendor operation.

| Operation | `ACTION` | Current action line | Response family/source |
|---|---|---|---|
| Create invoice | `action-xmlagentxmlfile` | `ops/invoice.rs:679` | `xmlszamlavalasz`, I2 |
| Storno invoice | `action-szamla_agent_st` | `ops/storno.rs:163` | Same, S1 |
| Register/clear credit entries | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:275,238` | Same, C1 |
| Query PDF | `action-szamla_agent_pdf` | `ops/query_pdf.rs:59` | Same plus PDF, Q1 |
| Query XML | `action-szamla_agent_xml` | `ops/query_xml.rs:541` | `szamla` or error envelope, Q3 |
| Delete proforma | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:61` | `xmlszamladbkdelvalasz`, D1 |
| Create receipt | `action-szamla_agent_nyugta_create` | `ops/receipt.rs:194` | `xmlnyugtavalasz`, R1 |
| Storno receipt | `action-szamla_agent_nyugta_storno` | `ops/receipt.rs:345` | `xmlnyugtavalasz`, R2 |
| Query receipt | `action-szamla_agent_nyugta_get` | `ops/receipt.rs:425` | `xmlnyugtavalasz`, R3 |
| Send receipt | `action-szamla_agent_nyugta_send` | `ops/receipt.rs:507` | `xmlnyugtasendvalasz`, R4 |
| Query taxpayer | `action-szamla_agent_taxpayer` | `ops/taxpayer.rs:265` | NAV `QueryTaxpayerResponse`, T1 |

**Response version:** `ops.rs:28–32` defines 2 once; invoice `:767`, storno `:189`, credit `:302` and PDF `:75` emit it. Their settings order agrees with X2/S2/C2/Q2. V1 text/raw PDF intentionally unsupported; no version field invented on fixed-format routes. PDF/XML queries put credentials under the root (`query_pdf.rs:67`, `query_xml.rs:549`); remaining operations use `beallitasok` (`invoice.rs:761`, `storno.rs` settings writer, `credit_entry.rs:297`, `proforma.rs:69–71`, `receipt.rs:232–235,353–356,433–436,515`, `taxpayer.rs:273–275`). `ops.rs:10–26` distinguishes required, true-only and tri-state booleans; its `fizetve` omission equivalence is appropriately qualified as unverified across every account/method.

## 5. Completed/incomplete response decisions

| Evidence | Current outcome | Current lines |
|---|---|---|
| Body transfer fails after status/headers | `IncompleteResponse`, retains status/raw repeated HeaderMap/source; Unknown even with a number/error header | `client.rs:66–100,117–126,385–393` |
| Complete, nonblank down header | `ServiceUnavailable` before code/status/body | `wire.rs:291–297` |
| Complete, ordinary nonblank error-code header | `Api` before status/body | `wire.rs:262–270,298–300`; `ops/envelope.rs:180–186` |
| Header 56 | Operation judges it; invoice/storno envelope can retain numbered issuance, ordinary verdict parsers return Api | `ops/envelope.rs:179–248`; `xml.rs:528–539` |
| Non-2xx without prior down/code verdict | `HttpStatus` with bounded excerpt; success-number/other headers cannot bypass | `wire.rs:301–308` |
| HTTP 200 XML refusal | Unique scalar verdict/code then `Api`; absent code is `Absent` | `xml.rs:478–523` |
| Normal numbered success | Created document with body-first optional metadata, header fallback | `ops/envelope.rs:209–248` |
| Success without number | Internal unnumbered reply; invoice preview branch decides, storno/PDF reject missing number | `ops/envelope.rs:211–216,268–279`; `ops/invoice.rs:948–960` |
| Numbered 56 with bad optional metadata | Issued plus warning, bad total/PDF becomes None | `ops/envelope.rs:205–248,285–315` |
| Header 56 but readable non-56 body refusal | Body refusal wins, even if optional payload malformed | `ops/envelope.rs:197–209` |
| Header 56 with empty/plain notification body | Narrow fallback still requires number | `ops/envelope.rs:188–196,251–254` |
| Header 56 with malformed XML or duplicate/nested body number | Parse/Unknown; header cannot overwrite invalid identity | `ops/envelope.rs:188–209,295–313` |
| 56 without usable number | Api 56, Unknown | `ops/envelope.rs:211–216`; `error.rs:376–386` |

The native client buffers the whole body before invoking the parser and supplies status (`client.rs:385–405`). RawResponse users may omit status; that explicitly skips the status decision. Complete HTTP 500 with a body-only refusal becomes `HttpStatus`, not an API refusal. This is conservative policy, not a claim that only intermediaries return non-2xx. Content-Type is not a verdict: the expected complete XML and operation payload decide after header/status checks. A 200 HTML/plain critical-error body is rejected, not interpreted as successful deletion (D1).

P2/P3 support down-first and the **numbered invoice notification exception**. They do not define all contradictory header/body/status combinations, repeated-code semantics or body/header identity disagreement. The generic I2/S1 “error codes … invoice number and amounts are omitted” wording has a more specific first-party 56 exception. No historical D6 probe triggered 56 (`docs/szamlazz-hu-behaviour.md:173,200–201`). Synthetic fallback tests are robustness policy, not captured vendor response shapes.

**Public wording boundary:** `error.rs:369–372` and README `:415` summarize “56 with a number” too broadly if read across every operation. Actual tolerance is the invoice/storno issuing parser, also reused by PDF query; credit/receipt generic verdict paths still return Api 56. `tests/response_headers.rs:203–223` explicitly encodes that distinction. Treat this as a scope clarification to fold into documentation maintenance, not evidence that receipt/credit parsers must promote an undocumented 56 response. A PDF query can still fail for its required missing PDF after shared-envelope processing (`ops/query_pdf.rs:83–93`).

### Header fidelity and XML/content handling

| Header/data | Representation and assessment |
|---|---|
| `szlahu_szamlaszam` | Case-insensitive lookup; form-style URL decode once, then nonblank trimmed number. I2/S1/C1 explicitly encode it. Body number wins (`wire.rs:227–249,343–351`; `envelope.rs:120–133,326–329`). |
| `szlahu_error` | Same decode; diagnostic independent from raw error-code token. Error text without code alone is not an API verdict. |
| `szlahu_error_code` | Raw trimmed open token, not percent-decoded; `%33` remains unknown. |
| Net/gross/outstanding headers | Raw exact finite numbers, dot/comma, sign/exponent and SP/HTAB padding; no grouping. `1,234` means 1.234. Comma acceptance has P60-E1 account evidence (`behaviour.md:180`). `envelope.rs:151–168,344–371`. |
| `szlahu_id` | Nonnegative i64 or None, auxiliary; bad id does not erase issuance (`envelope.rs:331–342`). P3 and historical `behaviour.md:164–165` support document-id meaning. |
| `szlahu_fizetesmod` | Decoded open `PaymentMethod` header, never invented from an absent envelope element (`envelope.rs:318–324`). Listed I2/S1/C1; exact encoding less explicit than number/error. |
| `szlahu_vevoifiokurl` | Body-first optional URL; header decoded once using form semantics. XML URL gets entity decoding only, preserving `+`/percent escapes. P3 uses raw URL decode: literal-plus ambiguity remains Q2 below. |
| `szlahu_down` | Nonblank decoded text, unavailable verdict. P3 supports control-header meaning; exact encoding unspecified. |
| Repeated/unknown headers | Stored, but generic lookup selects first; complete client uses lossy UTF-8 strings (`client.rs:394–403`, `wire.rs:182–237`). Documented ASCII/URL-encoded headers survive; arbitrary invalid bytes are not lossless. Incomplete response retains original bytes. |
| Body excerpts/logging | 256-byte bounded excerpt with total length when truncated (`error.rs:668–704`). API diagnostics/other parser messages are not universally bounded or redacted; README `:422` says so. |

Invalid escaped UTF-8 in a textual header falls back to the plus-adjusted encoded string (`wire.rs:343–351`); no vendor malformed-encoding rule is established. Present malformed nonblank body totals fail rather than using a valid header; missing/blank body metadata can fall back, while present blank numeric headers fail. Ordinary success can therefore become Parse/Unknown for optional malformed metadata; numbered 56 deliberately salvages identity. Complete parse failures do not return the full RawResponse through `ClientError`; callers needing all completed raw evidence can own the transport. This is a diagnostic/recovery capability boundary, not a proven protocol bug.

## 6. XML, namespace and scalar coverage

- **Structure through EOF:** `xml.rs:201–275` requires UTF-8 and one completed expected expanded root; rejects truncation, additional roots, outside text/CDATA/references, DTDs and invalid prolog/epilog. Schema locations are not fetched. `:280–300` checks lexical tokens/entities even in ignored extensions. No external-entity expansion is offered.
- **Namespaces:** `xml.rs:37–155` normalizes declarations before reserved-binding/duplicate-expanded-attribute checks, rejects undeclared element/attribute prefixes and reserved `xmlns` element prefix. `:310–378` canonicalizes protocol element names for serde and removes entire foreign subtrees, including descendants reentering protocol namespace. A placeholder prevents `tr<foreign/>ue` becoming true. Proper aliases can vary between repeated rows.
- **Declarations/PI:** `xml.rs:396–459` checks XML 1.0 version, ordered declaration attributes/separators, encoding-name grammar and legal nonreserved NCName PI target. The parser is UTF-8-only, not a general transcoder; no exhaustive XML standard certification is claimed.
- **Verdict versus diagnostic:** `xml.rs:478–523,759–787` requires one readable boolean `true/false/1/0` with XML whitespace. Missing/empty/NBSP-only verdict is Parse/Unknown, even beside a body code. False plus blank/missing code becomes `Absent`; true plus body code follows the true verdict. Duplicate/nested optional diagnostic is unavailable rather than erasing known facts. Malformed facts/identity remain errors.
- **Sparse content, not XSD validation:** optional absent/empty data is supported and unknown extensions ignored; sequence/business constraints are not fully enforced on input. Numeric `hibakod` schemas on receipt/deletion are intentionally read as open strings. These choices do not make every XSD-valid value representable.
- **Numbers:** `number.rs:61–146`, `xml.rs:647–665` parse exact finite Decimal, including exponent notation; excess precision/underflow/overflow fail instead of silent float rounding. NaN/infinity and finite values outside Decimal are known domain restrictions versus `xs:double`. Exponents do not trigger unbounded expansion.
- **Dates:** `xml.rs:667–725` preserves a civil date, strips only complete supported timezone suffixes up to ±14:00, checks UTF-8 boundaries, refuses malformed nonempty dates. This is deliberately different from Adatkapcsolat's lenient invalid-date content policy.
- **Business text/PDF:** `xml.rs:745–757` retains nonblank decoded business text including padding/NBSP, with XML line-ending/entity normalization rather than raw bytes. `types.rs:104–117` strips whitespace and decodes standard base64; no PDF syntax/signature/rendering validation follows from decoding.
- **Cross-operation roots:** XML query selects `szamla` versus error envelope, refusing a successful error-envelope substitute (`ops/query_xml.rs:568–613`). Receipt data operations use shared `xml::valasz` (`ops/receipt.rs:695`); send/deletion use shared `xml::verdict` (`:532–536`, `ops/proforma.rs:83–87`). Taxpayer uses shared header/full-body/namespace checks and its own NAV path extraction (`ops/taxpayer.rs:281–283,400–516`), not a fabricated `sikeres` envelope.

Fresh examples are illustrative: I2/S1/C1 contain unescaped ampersands in URL text, I2/S1/Q1/R1 contain truncated PDF placeholders, and Q3 contains descriptive PDF text. Refusing those literal malformed/placeholder bytes is not proof of rejecting actual vendor replies. R4's empty integer error-code example is tolerated as absent. Existing normalized fixture tests cannot convert these examples into execution evidence.

## 7. Error-code coverage and recovery claims

All **43 named numeric mappings** were compared with the current source/evidence catalogue (`error.rs:215–318,376–424`). `U` = Unknown, `R` = Rejected, `D` = DuplicateOrderNumber, `N` = NotFound.

| Codes | Current meaning/class | Primary source or bounded evidence |
|---|---|---|
| 1 / 3 | Maintenance U / invalid credentials R | B5/B3 |
| 7 | Missing data N, interpreted per operation | Q1/Q3; R4 missing subject, not missing receipt |
| 14 | Storno/credit original cannot itself be reversed/credited R | Historical `behaviour.md:107` |
| 53 / 54 / 55 / 56 / 57 | Missing XML file R / e-invoice permission R / signing failed U / notification U as error / malformed XML R | B5; P2/P3 for 56 |
| 71 / 152 | Duplicate order D | B5/G2 |
| 73 | Prepayment not identifiable R | Historical `behaviour.md:138` |
| 135 / 136 / 164 | Browser-session/access/subscription/multi-account R | B5/B3 |
| 202 / 221 | Unregistered prefix R / has corrective R | B5; historical `behaviour.md:108` for 221 |
| 259–264 | Net/VAT/gross mismatch or row-invalid R | B5 |
| 335 | Proforma absent/deleted R | D1 |
| 336 / 337 / 338 / 339 / 340 | Prefix collision R / invalid prefix R / reused call id R / missing receipt N / tender sum R | R1; September 11 execution corroborates 337 and completed-create 338 |
| 352 | Storno issue date must be today R | Historical `behaviour.md:109`; not fulfillment date or e-invoice-only rule |
| 363–365 | HUF/Ft receipt gross/net/VAT precision R | B5/G5 |
| 463 | Credit entry on reversed/reversing invoice R | Historical `behaviour.md:155,161` |
| 537–539 | Erasure-code count/demo-test/account restriction R | B5 |
| 551–556 | Simplified-image account/item/VAT/corrective/final/type restrictions R | B5/G4 |

Coverage arithmetic: 30 basic-table codes + five receipt supplement codes + operation-example 7/335 + PHP 56 + historical 14/73/221/352/463 = 43. No omitted code from those fetched numeric catalogues or swapped mapping found. Code **153**, observed for receipt notification spacing, intentionally remains `Unknown("153")`: the execution record `:86–91` explicitly avoids establishing its meaning on all operations. Naming it globally as always-rejected would require more evidence.

Known numeric tokens normalize (`007` → 7); unknown numeric/NAV textual tokens retain trimmed text, absent becomes `Absent` (`error.rs:458–487`). Unknown/Absent remain U and noncredential. Only 1/55 are potentially transient (`:320–333`), only 3/135/136/164 are credential/access codes (`:335–353`). Exact server processing order is **not** claimed in the current credential method; access refusal describes this exchange, not an earlier lost write. HTTP/down/parse/transport/incomplete are U; local checked-request refusal is R.

Public recovery (`recovery.md:4–55`, README `:91–167`) correctly separates:

- invoice issuance: persist identity, query external id, validate holder/order/type/reversal; nonunique newest-holder semantics; empty immediate query and elapsed time do not settle a send;
- storno: known original plus matching reversal type/reference, historical repeat success and proforma/delivery-note no-op treated as bounded observations;
- receipt create: keep the call id, 338 prevents duplication but does not return original success; no fresh id while unresolved;
- receipt storno: original reversal state alone does not recover SN identity/PDF; invoice-style replay and storno-specific 338 are not invented;
- reads: can be repeated for new observations, but cannot settle a different mutation by document existence alone;
- credit registration/clear: reconcile entries/balance, additive duplication and replacing-over-newer-state risks; explicit empty replacement supported;
- deletion: order selector can affect multiple proformas (D3), newly appearing matches make repeat dangerous; absence may be consumption;
- email: receipt existence is not delivery evidence; deliberate resend may duplicate mail; prior email block needed (R5).

The five-total-send limit is correctly initial-inclusive (B5), with combined write/query accounting unspecified. No generic retry loop or transport timeout is presented as cancellation/negative settlement. The client does not centrally enforce the 500-test-invoices/10-minute limit; this is account/caller load policy, not a guarantee the crate offers.

## 8. Executed evidence reconciliation and open questions

### Current records, not historical-report assumptions

| Record | What it establishes | What it does not establish |
|---|---|---|
| `docs/szamlazz-hu-behaviour.md:3–47,56–90,157–173,180,184–211` | Bounded historical account observations: HTTP 200 in-band refusals, per-operation header presence, newest external-id holder, storno echo/no-op, comma money; explicit failed effort to trigger 56. | Universal behavior, fresh captures, every operation/header/status combination. Raw logs are unavailable here. |
| `docs/research/2026-09-11-credit-clearing-live.md:14–32,64–74` | Both populated and empty clear probes passed; parsed number/outstanding plus post-query empty entries and verified storno identity. | Raw response channel, universal number echo, concurrency behavior or permission to retry uncertain clear. |
| `docs/research/2026-09-11-receipts-live.md:20–59,61–73,75–118` | 337 prefix refusal, successful lifecycle, completed-create duplicate 338, EUR MNB lookup, immediate email 153 refusal, deliberate delayed resend and operator-confirmed inbox delivery, three originals reversed. | Concurrent call-id deduplication/retention, full PDFs or raw headers, general code-153 meaning, NAV reporting or exact email/PDF content. |
| `docs/research/2026-09-11-agent-vendor-clarification.md:3–7,18–85,118–134` | **Unsent draft** about universal credit/PDF success identity and authoritative schemas; documents what remains unanswered. | A vendor answer or effective server schema. |

Historical design prose is not silently promoted: `behaviour.md:276–282` attributes before-write sequencing to vendor docs, but fresh B3/B5 describe authentication/access failures rather than proving exact execution order. Current `error.rs:335–343` is more careful and should remain authoritative. Likewise `behaviour.md:191` speculates test accounts may not send mail; fresh G3 describes routing to account email and the separate receipt record confirms delivered test receipt emails. Neither establishes invoice 56 generation. The detailed stalled-create row (`:172`) found no issuance by its query; current timeout/recovery text correctly says a broader delayed-issuance assertion lacks linked evidence, without treating absence as proof an in-flight request could never execute later.

### Questions / known deviations — not confirmed runtime defects

| ID / priority to clarify | Current boundary and next evidence |
|---|---|
| Q1 / medium | **Success-specific invoice-number guarantee:** C1/Q1/X1 make number optional across success/failure. Credit and PDF parsers require a nonblank reported number (`credit_entry.rs:319–322`, `envelope.rs:275–279`). A schema-valid numberless success is locally refused; no executed numberless success or universal success guarantee exists. Two clear successes do not settle it. Vendor clarification is drafted, not answered. Operation specialists own disposition. |
| Q2 / medium | **Customer URL literal plus:** P3 `rawurldecode` preserves `+`; Rust's shared form decoder maps it to space (`wire.rs:343–351`, `envelope.rs:137–142`). I2 lists the URL but does not specify its exact encoding alphabet. A raw header `...?q=a+b` differs between implementations; no archived vendor header with meaningful literal plus was found. Confirm raw wire grammar before calling this observed URL corruption or changing all textual-header decoding. |
| Q3 / medium | **HTTP/header/body conflicts and numbered 56 forms:** down→code→status→body and body-first metadata are local policy. No complete vendor HTTP matrix, repeated-code resolution, disagreement rule or capture for malformed/empty-body 56. Tests demonstrate decisions, not vendor emission. Preserve uncertainty for incomplete/malformed responses. |
| Q4 / medium | **Session after deletion/rotation:** B3 says immediate key deletion, B4 describes retained sessions. Neither establishes whether an existing cookie is revalidated or wins over a changed key. Fresh per-account jars/refresh are supported ownership policy. Need explicit vendor answer or authorized controlled account experiment. |
| Q5 / medium | **Effective request XSDs:** current research draft records inline/downloaded schema conflicts (including preview/simpleItems order and missing declarations). X2 was freshly retrieved for settings, but this transport review did not repeat the specialist's complete dual-schema comparison. `to_wire` is not full XSD validation despite B2 advice; schema-test passes cannot identify the effective server schema. No combined-preview run claimed. |
| Q6 / low | **Resource limits and evidence retention:** whole response buffered and copied, namespace projection allocates, no explicit body/depth cap (`client.rs:387–403`, `xml.rs:310–378`). No load/OOM reproduction or vendor maximum obtained. Completed parse failure lacks whole raw response, unlike incomplete-transfer evidence. Potential hardening/design work, not a demonstrated conformance failure. |
| Q7 / low | **Broader scalar/transport domains:** UTF-8-only, finite exact Decimal, finite civil dates, first repeated header, lossy complete-header UTF-8, body-first fields and permissive unknown extensions are local policies, not full XSD/HTTP conformance. Actual ordinary valid values were not shown corrupted. |
| Q8 / low | **Attachment limits:** local limits can reject before sending where G3 says vendor drops individual bad attachments. Exact meaning of 2 MB and filename rendering not established. Shared multipart preserves bytes; operation-specific validation belongs to invoice review. |
| Q9 / low | **Platform and retry accounting:** browser feasibility, fresh wasm build, effective downstream features, certificate chain, 90-minute expiry and exact post count under every transport policy remain untested. Do not rewrite the present “no application-level loop” claim as “one physical POST.” |

## 9. Tests inspected, commands run and limitations

**No Cargo tests or live tests executed in this review.** Parent runs the broad suite. The following current tests were inspected as local controls, not imported as passing results:

| Test source | Behavior under test |
|---|---|
| `tests/client.rs:187–261,275–337` | Injected native cookie clone/isolation; incomplete Content-Length body with error/down/numbered-56/number headers retains evidence and Unknown. Not vendor cookie precedence or TLS-root validation. |
| `tests/response_headers.rs:150–223,227–338,407–602` | Header/status ordering; operation-specific 56; one-pass URL decoding/raw codes/numbers; comma totals; optional-payload salvage without ambiguous identity. |
| `tests/response_booleans.rs:79–169` | Missing/empty/invalid verdict remains Unknown even beside known codes; valid tokens and authoritative headers. |
| `tests/response_completion.rs:12–193` | Complete roots, prolog/epilog, declarations, forbidden lexical content even in extensions; bounded wrong-root diagnostic. |
| `tests/response_namespaces.rs:155–357` | Prefix aliases, ignored foreign subtrees, undeclared/reserved bindings, duplicate expanded attributes, no manufactured scalar, singleton/quoting controls. |
| `tests/error_classification.rs:1–5,49–257`; `error.rs:774+` | Explicitly synthetic source-derived catalogue/classification/open-token controls, not execution proof. |
| `wire.rs:444–663`; `envelope.rs:373–725`; `xml.rs:896–1100` | Multipart/cookie/header, common envelope/body fallback, verdict and writer controls read with implementation. |

No additional offline executable reproduction was necessary for the text-proven F1 or the explicitly unknown vendor conditions. The only scratch script, `/tmp/opencode/transport-28dcec1-php.py`, retrieved P3 without credentials, hashed/opened the ZIP in memory and printed selected numbered source ranges. It executed no PHP and extracted no package files.

Commands actually run:

```sh
git status --short && git rev-parse HEAD && git rev-parse 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2 && git diff --stat 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2...HEAD && git log 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2..HEAD --oneline
ls /tmp/opencode
python3 /tmp/opencode/transport-28dcec1-php.py
git diff 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2 -- crates/szamlazz-agent Cargo.lock docs/szamlazz-hu-behaviour.md docs/research && git status --short && git rev-parse HEAD
```

Both pre-report state checks showed the requested HEAD, empty scoped source/evidence diff and the eight pre-existing untracked `77d53c5` reports. Public documents used read-only web fetching; local source/evidence used dedicated reads/searches. This report is the sole repository file authored by this reviewer. No authentication, issuance, email, account editing, test-account load or source modification occurred.

Final verification used `git diff --no-index --check /dev/null docs/review/2026-09-11-agent-api-28dcec1-transport.md` (no whitespace diagnostics; the new-file diff's nonzero status stopped the initially chained commands), then separately `git diff --check`, `git status --short`, `git rev-parse HEAD` and the scoped baseline diff above. HEAD and scoped source/evidence remained unchanged. Another reviewer had added the untracked `28dcec1-queries.md`; it and all eight old reports were left intact.

**Disposition:** preserve the current conservative shared protocol/recovery behavior; correct F1's evidence claim. Obtain primary clarification for Q1–Q5 before treating preferred alternative behavior as a vendor-required implementation fix. This report supplies neither the parent suite's test verdict nor a certification of the eleven specialists' operation-specific scope.
