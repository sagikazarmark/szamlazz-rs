# Shared Számla Agent protocol — current primary-source review

**Reviewed:** 2026-09-10. **Code:** `f54dac78cd1f7f981cd70d2d29ee3376be5b9bd3` and the working tree; the scoped production files had no diff from that commit at completion. This is the revision actually inspected, rather than the earlier baseline in the coordinating README.

**Scope:** `crates/szamlazz-agent/src/{wire,client,credentials,error,lib}.rs`, crate README, and the included `src/recovery.md`. Operation modules were read only to trace dispatch, credential placement, response-version selection, shared-header/error entry points and the README's recovery predicate. Operation models, full XML parsers and domain-field completeness belong to the other reviewers.

## Summary

- All **eleven** built-in dispatch fields match the current central table and their current operation request pages. Endpoint, POST, file-part framing, normal UTF-8 emission, credential placement, session handling, four response-version pins, shared error precedence and the named error catalogue are substantially aligned.
- **Two current defects reproduced:** a collision-derived multipart boundary can exceed MIME's 70-character limit (**P05-01**, P3); the README's recovery example fails to adopt the documented/observed live-invoice shape with an absent reversal marker (**P05-02**, P2, example-only).
- **One documentation/transport contract mismatch reproduced:** “never retries automatically” does not cover a supplied reqwest retry policy (**P05-03**, P3). Default protocol-NACK retry support is additionally feature-dependent; this is **not** a finding that default requests repeat after an ambiguous application failure.
- No missing currently documented numeric code was found in the general catalogue plus examined operation supplements. The thirteen previous omissions are fixed.
- Source conflicts, unsupported specificity and deliberate capability boundaries are listed separately below. No live Számla Agent request was made. No production file or pre-existing report was edited. The temporary uniquely named probe test was removed.

Severity: **P2** = useful behavior broken in a realistic supported workflow; **P3** = bounded edge case or documentation correction. Confidence is in the stated code/evidence mismatch, not the frequency of vendor emission. No P0/P1 issue established.

## 1. Current primary sources

The following sources were fetched during this review, not inferred from the July fixtures or the historical review. Current Számlázz.hu documentation pages displayed build `v202608271632`; that is a site build identifier, not a date establishing every statement's freshness.

| Ref | Exact source | Relevant short quote / evidence |
|---|---|---|
| B1 | <https://docs.szamlazz.hu/agent/basics/how-does> | “sends an XML file … in an HTTP POST request to `https://www.szamlazz.hu/szamla/`” |
| B2 | <https://docs.szamlazz.hu/agent/basics/sending-requests> | “decides which function to perform using the name of the form field”; all eleven dispatch names; “Each XML file contains … a single invoice or receipt only.” |
| B3 | <https://docs.szamlazz.hu/agent/basics/authentication>, <https://docs.szamlazz.hu/hu/agent/basics/authentication> | “only in lowercase”; username/password user has “exactly one billing account”; keys “do not expire”, maximum 17, identical permissions; “do not include it in client-side code.” |
| B4 | <https://docs.szamlazz.hu/agent/basics/session-cookie> | “inactive for 90 minutes”; reuse `JSESSIONID`; after company/email edits “generating a new session cookie is advised.” |
| B5 | <https://docs.szamlazz.hu/agent/basics/error-handling>, <https://docs.szamlazz.hu/hu/agent/basics/error-handling> | “same request … at most five times”; 500 test invoices per 10 minutes; plain-text v1 errors; current general code catalogue. |
| B6 | <https://docs.szamlazz.hu/agent/basics/security> | Current inbound HTTPS IP ranges and three outbound partner-call IPs. It does not prescribe a request deadline, retry algorithm, TLS-version pin or CORS guarantee. |
| I1 | <https://docs.szamlazz.hu/agent/generating_invoice/request> | `multipart/form-data`, `action-xmlagentxmlfile`, optional `attachfile1` … `attachfile5`. |
| I2 | <https://docs.szamlazz.hu/agent/generating_invoice/response> | Version `2`: structured XML; invoice number/error text “URL encoded”; totals/code “not URL encoded”; “If error codes are present … invoice number and amounts are omitted.” |
| I3 | <https://docs.szamlazz.hu/agent/generating_invoice/xml> | UTF-8 example; credential/version fields in `beallitasok`; external id permits later query and must be supplied at creation. |
| I4 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number> | Duplicate check is account-configured, per type; storno/correctives exempt; repeat success requires matching fields and issuance “within the last 2 days.” |
| S1 | <https://docs.szamlazz.hu/agent/reversing_invoice/request>, <https://docs.szamlazz.hu/agent/reversing_invoice/xml>, <https://docs.szamlazz.hu/agent/reversing_invoice/response> | Storno dispatch, settings credentials/version, v1 text/PDF versus v2 XML; request page describes external id as an original selector (conflict below). |
| C1 | <https://docs.szamlazz.hu/agent/credit_entry/request>, <https://docs.szamlazz.hu/agent/credit_entry/xml>, <https://docs.szamlazz.hu/agent/credit_entry/response> | Credit dispatch; settings credentials/version; v1 `xmlagentresponse=DONE`, v2 XML; error/header table. |
| Q1 | <https://docs.szamlazz.hu/agent/querying_pdf/request>, <https://docs.szamlazz.hu/agent/querying_pdf/xml>, <https://docs.szamlazz.hu/agent/querying_pdf/response> | PDF dispatch; credentials at root; version `2`; missing number/order/external id is code 7. |
| Q2 | <https://docs.szamlazz.hu/agent/querying_xml/request>, <https://docs.szamlazz.hu/agent/querying_xml/xml>, <https://docs.szamlazz.hu/agent/querying_xml/response> | XML dispatch; root credentials, no response version; success `szamla`, failure `xmlszamlavalasz`, code 7. |
| D1 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request>, <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml>, <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response> | Deletion dispatch and settings credentials; `xmlszamladbkdelvalasz`; “On critical error, a plain text/html error message may be returned instead”; example 335. |
| R1 | <https://docs.szamlazz.hu/agent/generating_receipt/request>, <https://docs.szamlazz.hu/agent/generating_receipt/xml>, <https://docs.szamlazz.hu/agent/generating_receipt/response> | Receipt dispatch/settings credentials; XML plus optional PDF; unique `hivasAzonosito` prevents duplicates; numeric supplement 336–340. |
| R2 | <https://docs.szamlazz.hu/agent/reversing_receipt/request>, <https://docs.szamlazz.hu/agent/reversing_receipt/xml>, <https://docs.szamlazz.hu/agent/reversing_receipt/response> | Storno receipt dispatch; returns `SN`; already reversed receipt is an error. No numeric code assigned to the three listed storno-specific messages. |
| R3 | <https://docs.szamlazz.hu/agent/querying_receipt/request>, <https://docs.szamlazz.hu/agent/querying_receipt/xml>, <https://docs.szamlazz.hu/agent/querying_receipt/response>, <https://docs.szamlazz.hu/php/nyugta-lekerdezes> | Number/order selection; receipt response shape; PHP says “last matching document”. |
| R4 | <https://docs.szamlazz.hu/agent/sending_receipt/request>, <https://docs.szamlazz.hu/agent/sending_receipt/xml>, <https://docs.szamlazz.hu/agent/sending_receipt/response> | Receipt send dispatch/settings credentials; success/failure XML; code 7 example means missing `emailtargy`, not missing receipt. |
| T1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/request>, <https://docs.szamlazz.hu/agent/querying_taxpayer/xml>, <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | Taxpayer dispatch/settings credentials; NAV response, numeric Agent code 57 in NAV `errorCode`; `OK` plus false validity is data. |
| X1 | <https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd> | `<element name="sztornozott" type="boolean" maxOccurs="1" minOccurs="0">`. |
| P1 | <https://docs.szamlazz.hu/php/valasz-feldolgozas> | “an invoice is successfully issued, but … the invoice notification cannot be delivered”; distinct `hasInvoiceNotificationSendError()`. |
| P2 | <https://docs.szamlazz.hu/php/>, <https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip> | Current official package **2.12.4**, dated 2026-08-12; freshly fetched ZIP SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Read relevant response source directly from the ZIP in memory. |
| H1 | <https://www.rfc-editor.org/rfc/rfc7578#section-4.1>, <https://www.rfc-editor.org/rfc/rfc2046#section-5.1.1> | Multipart/form-data follows MIME multipart; boundary “must be no longer than 70 characters, not counting the two leading hyphens.” |
| H2 | <https://docs.rs/reqwest/0.13.4/reqwest/retry/index.html>, <https://docs.rs/reqwest/0.13.4/reqwest/struct.ClientBuilder.html#method.retry>, <https://docs.rs/reqwest/0.13.4/reqwest/struct.ClientBuilder.html#method.timeout> | Default retry is protocol NACKs; configured classifiers can retry; timeout runs from connecting until response body finishes. Locked 0.13.4 source also inspected locally. |

All eleven `/request` pages and all eleven `/response` pages were inspected. Remaining operation `/xml` pages were read for credential containers and version fields using HTML-decoded current inline schemas; full domain schema comparison is outside this slice. R4 `/xml` was inspected through its inline credential schema, not for renewed receipt-email behavioral conclusions. The NAV PDF linked by T1 was not re-fetched here; the taxpayer specialist owns it.

## 2. Coverage: outbound protocol

Paths in the following tables beginning `src/` or `README.md` are relative to `crates/szamlazz-agent/`.

| Operation | Current `ACTION` location and exact field | Credentials / version | Current-source result |
|---|---|---|---|
| Create invoice family | `src/ops/invoice.rs:679`: `action-xmlagentxmlfile` | `beallitasok`, `valaszVerzio=2` at 740–746 | B2, I1–I3 agree. |
| Storno invoice | `src/ops/storno.rs:163`: `action-szamla_agent_st` | `beallitasok`, version 2 at 172–182 | B2, S1 agree on transport. |
| Register credit entries | `src/ops/credit_entry.rs:214`: `action-szamla_agent_kifiz` | `beallitasok`, version 2 at 231–236 | B2, C1 agree. |
| Query PDF | `src/ops/query_pdf.rs:59`: `action-szamla_agent_pdf` | Root, version 2 at 67–75 | B2, Q1 agree. |
| Query XML | `src/ops/query_xml.rs:531`: `action-szamla_agent_xml` | Root at 539; no version | B2, Q2 agree. |
| Delete proforma | `src/ops/proforma.rs:57`: `action-szamla_agent_dijbekero_torlese` | `beallitasok` at 66; no version | B2, D1 agree. |
| Create receipt | `src/ops/receipt.rs:185`: `action-szamla_agent_nyugta_create` | `beallitasok` at 224; no version | B2, R1 agree. |
| Storno receipt | `src/ops/receipt.rs:333`: `action-szamla_agent_nyugta_storno` | `beallitasok` at 342; no version | B2, R2 agree. |
| Query receipt | `src/ops/receipt.rs:412`: `action-szamla_agent_nyugta_get` | `beallitasok` at 421; no version | B2, R3 agree. |
| Send receipt | `src/ops/receipt.rs:494`: `action-szamla_agent_nyugta_send` | `beallitasok` at 502; no version | B2, R4 agree. |
| Query taxpayer | `src/ops/taxpayer.rs:265`: `action-szamla_agent_taxpayer` | `beallitasok` at 274; no version | B2, T1 agree. |

| Shared concern | Current path | Assessment |
|---|---|---|
| Endpoint and POST | `src/wire.rs:7–14`; `src/client.rs:160–193,300–309` | Correct default HTTPS URL including `/szamla/`. Endpoint override validates HTTP(S) plus host at build time. Plain HTTP overrides are deliberately supported for mocks/proxies; B6 does not require hard-coded IPs. No operation-specific endpoint drift. |
| File upload, not plain XML POST | `src/wire.rs:66–100` | Main part has form-data name and filename, `text/xml`, CRLF framing and final delimiter. Body is the XML file's bytes. This addresses B5 code 53. File extension is not prescribed by I1. |
| Attachments | `src/ops/invoice.rs:409–516,938–948`; `src/wire.rs:78–96,102–107` | At most five, correctly numbered fields; binary bytes retained. Disposition escapes quote/backslash and strips CR/LF. Per-part type strips CR/LF. Boundary checks XML and binary content, but P05-01 identifies its missing length bound. |
| UTF-8/XML credentials | `src/wire.rs:398–425`; `src/xml.rs:21–40,350–358` | Serializes UTF-8 XML, escapes text, rejects invalid UTF-8 / XML 1.0 forbidden characters before transport. This is not full XSD validation and is not described as such. No base64/form-urlencoding of the XML part is needed. |
| Auth choice | `src/credentials.rs:5–24,45–79`; `src/xml.rs:350–358` | Agent-key or user/password, not both. No implicit lowercasing/trimming of secrets; B3's lowercase rule is documented. B3's old-webshop key-in-both-fields workaround can be expressed as `user_password(key, key)`; no extra auth mode needed. |
| Secret formatting | `src/credentials.rs:27–30,88–98`; `src/wire.rs:43–50,151–175` | Key/password and request body redacted in Debug; Set-Cookie values redacted after header-name normalization. Username and other response headers are not redacted. “Parse failure can be logged as is” is not a promise that arbitrary vendor messages or portal URLs contain no sensitive content. |
| Cookie lifecycle | `src/client.rs:131–146,236–264`; `src/wire.rs:309–335`; `README.md:284–294` | Native default jar reuses sessions; clones share jars; fresh jar after edits and per-account isolation are now explicit. Exact case-sensitive `JSESSIONID` extraction is fixed, while attributes belong to the transport. No mandated disk persistence or refresh timer. |
| TLS / redirect policy | `src/client.rs:229–247`; `Cargo.toml:24–25` | Default native TLS verification retained, no redirects. Avoiding redirects is appropriate; the comment that the endpoint “never redirects” is not a published guarantee established by B1/B6. Custom clients own these settings. |
| Deadline | `src/client.rs:216–247,322` | 60-second native total request deadline includes receiving the body (H2), not merely connection/header wait; cancellation does not establish cancellation of server work. No vendor-required timeout found. No separate body size cap or streaming interface. |
| Retry ownership | `src/client.rs:163–166,236–243,300–326`; `README.md:347`; `src/recovery.md:4–6` | No crate-level retry loop. P05-03 qualifies transport-level behavior and caller-supplied policies. The five-send limit is documented, not globally enforced by this low-level client. |
| Native/browser boundary | `src/lib.rs:45–66`; `src/client.rs:143–146,245–247`; `README.md:296–302` | Correctly distinguishes compilation from CORS/Fetch feasibility, browser-managed cookies and hidden Set-Cookie. No direct browser call or CORS acceptance was tested. B3's separate client-side-secret restriction deserves explicit mention alongside this technical discussion; see §6. |

## 3. Coverage: responses, precedence and recovery

### Response versions and formats

`src/ops.rs:27–31` supplies the single `RESPONSE_VERSION="2"` used by exactly four writers. Current I2/S1/C1/Q1 define those formats. Query XML, deletion, receipt operations and taxpayer lookup have their own XML responses without that selector. The absence of version fields on them is correct.

V1 text (`xmlagentresponse=DONE…`), raw PDF and `[ERR]…` are deliberately not selectable as built-in response modes. Errors with code headers can still be interpreted without XML. Unheaded text/HTML critical deletion failures become parse failures/Unknown, rather than invented settled refusals. This is conservative and compatible with D1's lack of structured meaning for that critical-error format.

### Shared interpretation order

`src/wire.rs:247–306`, `src/ops/envelope.rs:179–242`, `src/xml.rs:254–257`, `src/ops/query_xml.rs:559–569` and `src/ops/taxpayer.rs:282` establish:

1. Nonblank decoded `szlahu_down` → `ServiceUnavailable`.
2. Nonblank trimmed raw `szlahu_error_code` → API error for the operation to judge. Issuance/storno specially tolerate 56 with a number; credit-entry parsing does not.
3. If no preceding error header decided, supplied non-2xx status → `HttpStatus` **before the body**. Success-number and unrelated headers do not bypass it.
4. Otherwise the expected body decides. Body-only code 7 at 200 is API error, while the same body at 500 is `HttpStatus`. No claim that status identifies a proxy.
5. A non-56 body error wins over provisional header 56. Numbered 56 preserves known issuance and drops malformed optional metadata; 56 without a number stays Unknown.

This now matches `README.md:286`, `src/client.rs:293–299`, and `ResponseError::HttpStatus` docs. The status order is the crate's defensive policy, not an exact algorithm dictated by the official pages. A sans-I/O caller must supply status to get that protection; `Client::send` always supplies it. The general `src/error.rs:3–5` “never via HTTP status codes” is broader than the sources establish: in-band business errors are supported, but the current pages do not guarantee every infrastructure answer is HTTP 200.

**Transport completeness boundary:** `src/client.rs:311–326` buffers the entire body before invoking the parser. Even already-received code/number headers are lost to the public result if body download fails: result is Transport/Unknown. This is conservative but prevents early/header-only settlement of truncated downloads. The documented precedence applies to a completed `RawResponse`; no new vendor conformance defect is inferred from it.

### Header encoding and body precedence

- `src/wire.rs:182–244,339–347` lowercases names, keeps raw values, and applies percent/plus decoding only when explicitly requested. Number and error headers have direct I2/S1/C1 support for URL encoding. Numeric totals/code/id stay raw. Known codes trim and parse numerically (`007` → 7); unknown and absent codes stay open.
- Shared envelopes prefer XML number, totals and URL to missing-body header fallback (`src/ops/envelope.rs:113–142,158–167`). XML URLs are entity-decoded only. Invalid nonblank body money is not rescued by a header. HTTP comma decimals are supported after the earlier fix; observed `100,01` from P60 is now read correctly.
- Payment method, portal URL and down-message decoding have less precise published encoding guarantees than number/error. PHP P2 uses `rawurldecode` for the URL on input and `urldecode` again in its getter, and `urldecode` for error text. This is not a sound mandate to duplicate that double decode. Existing Rust once-only policy and malformed-percent fallback are explicit and covered by synthetic tests; no captured counterexample establishes a new defect.

### Code 55 versus 56

B5 says 55 means signing failed: “Either your certificate expired or the timestamp server could not be reached.” `src/error.rs:66–74,315–327,370–376` correctly keeps it Unknown and marks only potential transience; it does not claim the invoice was definitely issued. Expired certificates need intervention.

P2's `Response/InvoiceResponse.php:17` defines `INVOICE_NOTIFICATION_SEND_FAILED=56`; lines 319–322 expressly override an error only when `hasInvoiceNumber()` and `hasInvoiceNotificationSendError()` both hold. `Response/SzamlaAgentResponse.php:147–152` checks nonblank `szlahu_down`. These were re-read from today's downloaded ZIP. Rust's numbered-56 behavior is corroborated by current first-party implementation, **not live observation**. `docs/szamlazz-hu-behaviour.md:153,180–181` says probes could not trigger it. Reusing the issuance envelope for PDF query does not prove PDF queries emit 56.

### Current numeric catalogue coverage

Classes: **U** Unknown, **R** Rejected, **N** NotFound/missing data, **D** DuplicateOrderNumber. Current mappings are `src/error.rs:210–313`, classification at `370–417`, credential set at `339–346`, open parsing at `455–465`.

| Codes | Current class | Source and assessment |
|---|---|---|
| 1 | U; retry hint true | B5 maintenance/internal error; conservative unknown outcome. |
| 3, 135, 136, 164 | R; credential=true | B3/B5 invalid login, browser session, blocked login, multiple accounts. No code omitted; precise execution-order attribution is qualified in §6. |
| 53, 54, 57, 202 | R | B5 missing XML file, e-invoice permission, malformed/XSD-failing XML, prefix. |
| 55 | U; retry hint true | B5 signing failure, distinction above. |
| 71, 152 | D | B5/I4 duplicate order under the configured rule. Refusal of this send does not settle an earlier send. |
| 259–264 | R | B5 line arithmetic. HU prose for 262 says item name where EN says row number; the numeric token remains unambiguous. |
| 363–365 | R | B5 HUF receipt precision; now named. |
| 537–539 | R | B5 erasure count, demo/test restriction, disabled setting. |
| 551–556 | R | B5 simplified-image restrictions, including 554 on an existing original even if corrective omits the flag; now named. |
| 7 | N | Q1/Q2 missing selector match; R4 missing email subject. Current enum documents operation dependence rather than universally claiming absent invoice. |
| 335 | R | D1 deletion refusal; existing success is not replayed. |
| 336, 337, 338, 339, 340 | R, R, R, N, R | R1 receipt supplement; 338 is duplicate prevention, not original-result recovery. |
| 56 | U as error; numbered issuance warning | P1/P2, not in B5's general table. |
| 14, 73, 221, 352, 463 | R | Named as observed, backed by `docs/szamlazz-hu-behaviour.md:88–90,119,135,141–143`; not promoted to current general-catalogue claims. |
| Future numeric / NAV textual / absent | U; no retry or credential hint | Deliberately conservative, including an unknown number outside u16. T1 demonstrates a numeric Agent error inside NAV XML; the enum preserves unfamiliar text. |

There are **37 distinct numeric codes** in the fetched general catalogue plus examined operation supplements (30 general, seven additional 7/335/336–340), all named. Adding PHP's 56 and five live-only names accounts for **43 current named variants**. Receipt-storno error messages without published numbers do not justify inventing mappings. No repeat of historical F2 is warranted.

### Recovery documentation

`src/recovery.md:8–17` now distinguishes invoice creation/storno, receipt creation/storno, reads, credit entries, deletion and receipt email. Stable receipt call IDs, 338's non-replay semantics, mutation-specific reconciliation and the insufficiency of an immediate empty query are all present. `README.md:151–153` correctly states five total sends and qualifies A4d evidence. The prose does not infer successful email delivery from receipt existence. P05-02 is a new concrete predicate defect in the revised invoice example, not a reopening of the former generic recovery advice.

## 4. Confirmed current findings

### P05-01 — Collision handling can generate an invalidly long multipart boundary

**Severity P3. Confidence high in generated nonconformance; vendor rejection unobserved.**

**Current path:** `src/wire.rs:109–120` repeatedly appends `x` to `BASE_BOUNDARY` while it occurs anywhere in XML/file content, without a length ceiling. `AgentRequest::to_wire` at `398–404` passes the result through as a successful `WireRequest`; `src/client.rs:303–309` sends it unchanged.

**Primary source:** I1 requires “Content type: `multipart/form-data`” (<https://docs.szamlazz.hu/agent/generating_invoice/request>). RFC 7578 §4/4.1 (<https://www.rfc-editor.org/rfc/rfc7578#section-4.1>) adopts MIME multipart; RFC 2046 §5.1.1 (<https://www.rfc-editor.org/rfc/rfc2046#section-5.1.1>) says boundaries “must be no longer than 70 characters, not counting the two leading hyphens.”

**Reproduction:** no custom `AgentRequest` or malformed XML needed:

```rust
let value = format!("----szamlazz-agent-4f7d1a2b9c3e{}", "x".repeat(70));
let request = QueryInvoiceXml::new(InvoiceSelector::ExternalId(value));
let wire = request.to_wire(&Credentials::agent_key("test-key")).unwrap();
let boundary = wire.content_type
    .strip_prefix("multipart/form-data; boundary=").unwrap();
assert_eq!(boundary.len(), 102); // reproduced
```

Every shorter appended candidate is a substring of the supplied text; the algorithm only stops at 102 bytes. The same trigger can reside in an invoice comment or an allowed binary attachment. A conforming strict multipart receiver can reject a request whose document data was otherwise valid; the “ready-to-send” result is not guaranteed to be valid multipart. Large deliberately repetitive input also makes repeated full scans expensive, but no performance severity is claimed without a benchmark.

**Bounded remedy:** choose a collision-free candidate within 70 characters, e.g. a bounded-width changing suffix, checking all payloads; preserve deterministic serialization if desired. Do not merely truncate a colliding candidate. Regression should assert both collision avoidance and the length bound. Ordinary boundaries and binary bytes should remain unchanged where practical.

### P05-02 — README reconciliation refuses the normal live-invoice reversal shape

**Severity P2 for the executable recovery example; production parser is correct. Confidence high.**

**Current path:** `README.md:136–142`, especially line 140, requires `document.info.reversed == Some(false)` to return `Outcome::Issued`. The parser preserves omitted `sztornozott` as `None` (`src/ops/query_xml.rs:750–751,784`); its public field docs explicitly say “Treat `reversed != Some(true)` as ‘live’” (`300–316`).

**Primary source:** current invoice XML XSD (<https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd>) declares `sztornozott` with **`minOccurs="0"`**. Query response documentation (<https://docs.szamlazz.hu/agent/querying_xml/response>) supplies a successful document without that marker. The semantic connection is backed by the recorded account evidence, `docs/szamlazz-hu-behaviour.md:77`: “absent (not false) before a storno”, then true after reversal, probes B1/A5/B6. This is existing live evidence, not a fresh call.

**Reproduction/impact:** parse a current-style `szamla` carrying `id=1`, `szamlaszam=I-1`, `tipus=SZ`, `eszamla=1`, `rendelesszam=ORD-1`, ordinary supplier/buyer/totals, with no `sztornozott`. The public parser returns a document with matching order/type and `reversed=None`; the exact README predicate rejects it. Adding explicit false makes the predicate succeed; adding true correctly fails. All three controls passed in the temporary offline probe.

After an invoice landed but its create reply was lost, a correct external-id query therefore still becomes `Outcome::Unknown` when using this example. Repeated fresh queries cannot resolve it while the server keeps its normal absent marker. This creates unnecessary operator reconciliation/stalled completion. The example does **not** automatically resend, so duplicate issuance is not a demonstrated consequence.

**Bounded remedy:** use `document.info.reversed != Some(true)` alongside the existing order/type checks. Keep absence of a query result and identity collisions unresolved; do not change the parser to invent false. Verify the example's decision with absent/false/true controls, rather than relying only on compiling the doctest.

### P05-03 — Unqualified “never retries automatically” omits transport retry behavior

**Severity P3, documentation/transport-policy contract. Confidence high for injected-policy reproduction; high for dependency source, no default duplicate incident established.**

**Current path:** `README.md:347` says “The client never retries automatically”; `src/recovery.md:4–6` similarly says no automatic retry. `src/client.rs:131–150,163–166` accepts an already configured reqwest client and `300–309` submits a cloneable in-memory body. Default construction at `236–243` does not set `.retry(reqwest::retry::never())`.

**Primary sources:** vendor B5 (<https://docs.szamlazz.hu/agent/basics/error-handling#retry-limit>) permits the same request “at most five times”. Dependency H2 (<https://docs.rs/reqwest/0.13.4/reqwest/retry/index.html>) says a client retries “by sending additional copies” and defaults to safe protocol-NACK retry. <https://docs.rs/reqwest/0.13.4/reqwest/struct.ClientBuilder.html#method.retry> says “Default behavior is to retry protocol NACKs.”

**Reproduction:** build a loopback reqwest client with `retry::for_host("127.0.0.1").no_budget().max_retries_per_request(2).classify_fn(...)`, classifying 503 as retryable. Supply it via `http_client`. One `Client::send(QueryTaxpayer)` to a mock returning 503 produced **three byte-identical POST bodies**, then one final `HttpStatus(503)`. The same transport path is used for writes. This test demonstrates the configuration boundary, not a recommendation to retry writes this way.

**Default qualification:** the locked version is 0.13.4 (`Cargo.lock:2078–2079`). Its `src/retry.rs:195–202,253–267,301–314` retains protocol-NACK retry for replayable bodies under HTTP/2 (`REFUSED_STREAM`/remote graceful GOAWAY). `cargo tree --offline --locked -p szamlazz-agent --features client-reqwest -e features -i reqwest` showed the isolated crate activates cookies/rustls, **not HTTP/2/3**; do not claim this isolated default retries application 503s. A consumer can enable HTTP/2 through Cargo feature unification, and injected policies can retry arbitrary selected statuses. Safe unprocessed-stream retries are not evidence of duplicate legal documents.

**Impact/remedy:** callers must not count one `send` invocation as one external submission under every supported configuration or assume injected retries run the crate's reconciliation logic. Say “no application-level retry/recovery loop” and document the supplied transport's retry responsibility, alongside its existing cookie/deadline/redirect responsibility. If strict suppression is an intended default guarantee, explicitly disable reqwest's retry policy and test that contract; document that an injected client remains caller-controlled. No automatic recovery engine is proposed.

## 5. Intentional live-backed exceptions and source conflicts

These are **not additional defects** and must not be removed to make examples/XSDs appear uniform.

| Topic | Current code / source tension | Disposition |
|---|---|---|
| Body-only errors / headerless deletion success | `src/wire.rs:247–256`; C1 header table says headers “may” arrive; behavior notes `109,135,141–145` record query 7/credit 463 body-only and successful deletion without `szlahu_*`. | Both channels remain supported. Header absence is not success; no universal header requirement. |
| Numbered 56 versus generic header rule | I2/S1 say error headers omit number/amounts; P2 explicitly allows number+56. `src/ops/envelope.rs:179–242` implements the specific exception. | First-party implementation exception, not live-backed 56. Retain condition and evidence qualification. |
| External-id uniqueness | I3 says “later the invoice can be queried with this key”; behavior notes `63–70` establish duplicate holders, newest result and no external-id echo. `README.md:79–81,149` is appropriately cautious. | Keep collision checks and caller persistence. No vendor uniqueness/idempotency guarantee inferred. |
| Storno external id | S1 request page says it can reference the original if set at creation. Behavior notes `70,95` show the sent id attaches to the new SS and a repeat does not store a new id. `src/recovery.md:11` queries the storno id. | Preserve observed semantics. Operations reviewer owns the model detail; shared recovery must not silently substitute the original's id. |
| Storno repeat/no-op | S1 describes creating a storno; behavior notes `86–87` record existing-SS echo and proforma/delivery-note no-op. `src/recovery.md:11` qualifies them. | Keep distinct from receipt storno. R2 currently documents an already-reversed receipt as error. |
| Duplicate replay fingerprint | I4 explicitly limits repeat-success to matching dates and two days; account notes `51–56,185–191` record partial fingerprint differences and untested window. `src/error.rs:79–87` lacks the published window in its abbreviated repeat sentence. | Preserve test-account facts, but do not read “byte-identical resend” as indefinite replay. A useful documentation qualification, not a new idempotency implementation request. |
| Sparse reversal field | X1 makes marker optional; observed live shape omits it. | Parser behavior is intentional; the README mismatch is P05-02. |
| Header comma decimal | I2 says totals are not URL encoded, without a precise number grammar; notes `160` record `100,01`. | Current header-specific grammar supports it; historical F3 is fixed. Missing-body/comma compound server emission remains unobserved. |
| Response-version inline comment | I3 inline schema has “DEPRECATED: this field can be omitted, as our system no longer processes it” between `szamlaLetoltesPld` and `valaszVerzio`; response page explicitly defines version 1/2. | Comment attachment is ambiguous. Do not remove the version-2 pin from the four writers on that basis. No live response-version switch was tested. |
| Malformed published examples | I2/S1/C1 success examples contain unescaped `&` in portal URLs; I2/S1/Q1 PDFs contain `....`. `fixtures/SOURCES.md:149–170` records provenance and separate test transformations. | These are defective/abbreviated published examples, not grounds to weaken XML/PDF parsing or pretend fixture repair is a captured response. |
| Schema authority | `fixtures/SOURCES.md:193–228` records inline/download/PHP ordering conflicts and retained project transformations. | Do not call cached modified schemas universal upstream truth. Detailed simpleItems ordering belongs to invoice review. |

## 6. Unsupported specificity and capability boundaries

### Unsupported or only partially supported claims

These are evidence notes, **not confirmed runtime defects**:

1. **Credential execution-order attribution:** `src/error.rs:330–335` says all four credential codes are answered “before it looks at the request (its documentation)”. Current B3/B5 describe authentication failures, but no explicit global processing-order guarantee was found there or in the operation responses. `docs/szamlazz-hu-behaviour.md:255–261` also says these were not observed. “Before reading the XML” cannot literally describe XML-based authentication. Keep the sensible refusal classification, but distinguish an inference of no authenticated business write from an exact vendor execution-order guarantee. Source quote: B3 “requests must be authenticated”; B5 code 164 “the operation will fail”. Confidence high that the searched sources do not contain the stronger claim; no assertion that some unexamined first-party page cannot.
2. **Browser feasibility versus secret placement:** `src/lib.rs:59–66` and `README.md:302` now correctly qualify CORS/cookies, but only discuss technical feasibility. B3 separately says “do not include [the Agent key] in client-side code.” Adding that source-linked deployment boundary would prevent interpreting compilation support as endorsement of shipping an account-wide merchant key to browsers. No actual exposed key exists in the inspected examples. This is a documentation gap, not a reason to remove wasm support or treat server-side Workers as browsers.
3. **Status/redirect absolutes:** `src/error.rs:3–5` “never via HTTP status codes” and `src/client.rs:232–234` “the endpoint never redirects” go beyond the current source guarantees. In-band business errors and direct endpoint use are supported; no 24/7 infrastructure/status invariant was established. Current runtime handling already errs conservatively.
4. **Empty credit-entry replacement:** `src/error.rs:533–540` / `README.md:348` say it “would clear” payments. C1 says `additiv=false` replaces existing entries, and the XSD permits zero; behavior notes `211–215` explicitly say the empty call was never sent. This is a reasonable implied effect and intentional refusal, not a newly observed server result or established missing capability. Keep its evidence qualification.
5. **55/56 and delayed issuance:** current README/recovery docs already distinguish documentation, PHP source and A4d observations. There is no newly verified 55/56 shape, credential-selection behavior or delayed-issuance incident. Do not revive the historical unsupported certainty as a new finding.

### Deliberate capability gaps, not conformance defects

- **V1 mode/raw streaming:** built-ins always request v2 where supported and buffer requests/responses. No v1-selectable client, direct PDF stream or body-size configuration is offered. The requested v2 protocol is supported; no vendor requirement makes v1 mandatory.
- **Key management:** B3 documents issuance/deletion/account limits, not an Agent key-management operation. Manual rotation via a fresh `Client`/jar works; no key-expiry timer, lowercase normalization, per-key scopes or refresh endpoint should be invented.
- **Full XSD validation:** `to_wire` scans XML legality and invokes selected operation checks, not general schema validation. B2 recommends XSD checking, but the repository records conflicting first-party schemas. No blanket validator is proposed by this review.
- **Recovery engine / call budgets:** serializing logical issuance, storing identities, reconciling uncertain mutations and coordinating the five-send vendor ceiling belong to the caller. Stable receipt IDs are available; no documented call-ID-only receipt selector was found. Automatic reissue and key renewal are not justified by a timeout or empty immediate query.
- **Raw exchange retention:** typed `Client::send` does not expose received headers after a body read failure or return the complete `RawResponse` beside a parsed error. Sans-I/O callers can retain their own exchange. This is an observability/recovery capability decision, not evidence that Unknown is the wrong classification.
- **Legacy key-in-username/password redaction:** callers can express B3's fallback using `Credentials::user_password(key, key)`, but `Credentials` Debug prints the username (`src/credentials.rs:92–95`). Preferred `agent_key` redacts the key. Treat the fallback username as sensitive if choosing that legacy shape; no need for the built-in modern key path to use it.

## 7. Historical checklist disposition

`docs/review/2026-09-09-agent-api/FINAL.md` was used as a historical checklist only. Reinspection shows the following shared-protocol repairs present:

- E.E1 exact cookie-name/pair parsing: `src/wire.rs:326–335`, independent tests at `tests/response_headers.rs:74–111`.
- E.E2 header/status/encoding prose: README `284–288`, matching source-derived synthetic precedence cases.
- E.E3 fresh cookie jars, clone sharing and browser boundaries: `src/client.rs:131–146,251–267`, README `290–302`.
- F2 thirteen named code additions: mapping/classification and `tests/error_classification.rs`; no missing mappings re-reported.
- F3 monetary-header grammar and metadata exposure: `src/ops/envelope.rs:328–375`, `tests/response_headers.rs`.
- B.E01/B.E02/D3 recovery corrections: exact five-send wording, uncertain 55, numbered/PHP-sourced 56, operation-specific recovery and qualified stalled-send history.
- Current `fixtures/SOURCES.md` records September structured examples and source conflicts; its July acquisition descriptions are explicitly historical, not current claims.

P05-01 and P05-03 are independent edge/boundary findings. P05-02 tests a current predicate in the revised example; it is not a claim that the earlier broad recovery-doc repair is absent.

## 8. Verification and evidence limits

**Executed offline/loopback only:**

```text
cargo tree --offline --locked -p szamlazz-agent --features client-reqwest -e features -i reqwest
cargo test --offline --locked -p szamlazz-agent --features client-reqwest --test review_protocol_20260910_05 --test response_headers --test error_classification
cargo test --offline --locked -p szamlazz-agent --features client-reqwest --test review_protocol_20260910_05 -- --nocapture
```

- Existing focused suites: **9 response-header tests + 3 error-classification tests passed**.
- Initial temporary README probe failed in test setup because its minimal synthetic query document omitted required `alap/id`; this was not a production defect. After supplying the ordinary document blocks, the three temporary probes passed: README absent/false/true controls, 102-byte multipart boundary, and one logical send producing three identical POSTs under an explicitly injected retry policy.
- Temporary file `crates/szamlazz-agent/tests/review_protocol_20260910_05.rs` was created with `apply_patch` and deleted with `apply_patch` after execution. No other reviewer's temporary file was touched.
- No full suite was run here; the coordinator owns that check. No claim that merely passing these tests proves complete XML fidelity, all vendor statuses, every credential case or server-side idempotency.
- Bounded live evidence comes from `docs/szamlazz-hu-behaviour.md`, one test account over the recorded dates. Its referenced raw exchange logs are explicitly outside this repository (`11–18`), so this review could not independently inspect those bytes.
- Documentation GETs, current PHP ZIP inspection and RFC/dependency docs are primary-source evidence. They are not authenticated Számla Agent executions. No production or test-account mutation/read request was made.
- Not established: default HTTP/2 NACK reproduction, strict vendor rejection of overlong multipart boundaries, current CORS, conflicting XML-key/session precedence, revocation of existing sessions, 55/56 live emission, receipt call-ID retention/scope, or indefinite external-id/order replay guarantees.

**Output:** this report is the only retained change made by this reviewer.
