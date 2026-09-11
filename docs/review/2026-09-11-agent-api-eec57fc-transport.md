# Számla Agent conformance review — transport and shared protocol

**Reviewed HEAD:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd`

**Review date / fresh documentation retrieval:** 2026-09-11

**Result:** **no verified actionable finding: 0 P0 / 0 P1 / 0 P2 / 0 P3.**

All eleven multipart operation names, all thirty entries in the current general error table, the receipt supplement, and the shared authentication/response paths were independently compared with current source. The remaining questions are identified below rather than promoted into defects without evidence. This is a full-source review of the assigned cross-cutting slice, not a diff review or certification of every business field in the crate.

**Verification boundary:** the parent owns the cargo test suite. This review did not run that suite and does not inherit earlier reviews' pass counts. An isolated, feature-free scratch executable compiled the current source and passed **15 controls**. Public documentation GETs and an in-memory inspection of the official PHP archive supplied additional evidence. No authenticated vendor request or live/probe test was executed.

## 1. Scope and method

Read in full `crates/szamlazz-agent/src/{wire.rs,client.rs,credentials.rs,error.rs,xml.rs,ops/envelope.rs,lib.rs}`, the crate README and `src/recovery.md`. Followed every operation's action constant, credential injection, response-version emission and response entry point, plus shared number/PDF readers, receipt and queried-invoice date readers, and NAV envelope extraction. Inspected transport/header/completion/namespace/error/numeric/business-text tests, public-client example, upstream corpus runner and its transformations, request-date tests, schema-matrix machinery and relevant operation unit tests. Read recorded account evidence and the current vendor-question drafts; inspected live/probe scenario code as executable coverage, not observations.

Unless otherwise stated, source paths in tables are relative to **`crates/szamlazz-agent/src/`**; `tests/` and `README.md` are relative to that crate. Line numbers identify the reviewed source at the pinned HEAD. The initial tree was clean. Later unrelated working-tree edits appeared under `crates/restate-szamlazz`; HEAD and the reviewed Agent files remained unchanged. This report is the only repository file authored by this review. Scratch files are under `/tmp/opencode/transport-eec57fc/`.

The older `2026-09-11-agent-api-61c334f-transport.md` was consulted **after** the initial independent source and documentation reading, to check historical candidates and avoid resurrecting fixed findings. Its conclusions, line numbers and test results are not the evidence for this report.

Evidence levels used here:

- **D — current official documentation/schema:** a published contract or example, fetched during this review. Examples can themselves be malformed or incomplete.
- **P — first-party implementation:** freshly retrieved PHP 2.12.4 source. Corroboration, not a recording of server execution.
- **O — recorded observation:** dated account results in the repository. Bounded to those accounts/settings; historical raw A–D logs are not in the repository.
- **S — source/local execution:** proves the Rust implementation or a synthetic input's behavior, not vendor acceptance or delivery.

## 2. Fresh source register

The documentation site reported build **`v202608271632`**. This is not the acquisition date or the date of every example.

### Basics: complete category and all seven pages

| Ref | URL | Quoted requirement / relevant content |
|---|---|---|
| B0 | [Basics category](https://docs.szamlazz.hu/agent/category/basics) | Lists the seven pages below; all were fetched. |
| B1 | [What is Számla Agent?](https://docs.szamlazz.hu/agent/basics/what-is) | Invoice/proforma/delivery-note generation share an operation; receipt create/reverse/query/send and taxpayer lookup are separate operations. |
| B2 | [How does it work?](https://docs.szamlazz.hu/agent/basics/how-does) | “XML file … in an HTTP POST request to `https://www.szamlazz.hu/szamla/`”; generation and email delivery are distinct steps. |
| B3 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Prefer `<szamlaagentkulcs>`; legacy key placement may “use the same key in both fields”; keys accepted “only in lowercase”; legacy user must access “exactly one billing account”; “do not include it in client-side code.” Keys do not expire, have identical permissions, and deletion takes effect immediately. |
| B4 | [Error handling EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Same request “at most **five times**” / “legfeljebb ötször”, then human intervention; no until-success loop; 500 test invoices/10 minutes; complete general code table and v1 `[ERR]` format. |
| B5 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Store/reuse `JSESSIONID`; “inactive for 90 minutes” deletes the session; without persistence every request authenticates again. New session after company/email edits is “advised”; file storage is “advisable.” |
| B6 | [Network/security](https://docs.szamlazz.hu/agent/basics/security) | Destination CIDRs differ from the three outbound receiver IPs. No fixed client timeout or complete HTTP-status matrix is specified. |
| B7 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | “same URL every time”; “name of the form field containing the XML file”; “HTTPS POST”; all eleven action names; one invoice/receipt per creation XML; case-sensitive tags and XSD guidance. |
| B8 | [Linked 2025 migration notice](https://tudastar.szamlazz.hu/gyik/technologiai-valtozasok-2025) | Previous fixed destination IPs “ceased to exist”; certificate issuer changed from Let's Encrypt to Google Trust Services. |

### Operation response pages: all eleven

| Ref | URL | Contract used |
|---|---|---|
| R1 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | v2 “Structured `xmlszamlavalasz` with optional base64 PDF”; invoice number/error text URL encoded, totals/error code not encoded; payment-method/customer-URL headers. |
| R2 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Same v2 envelope; reported number is the storno invoice's. |
| R3 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response) | v2 `xmlszamlavalasz`; “Additional data may also arrive” in headers; schema only requires `sikeres`, while the success example includes number/totals. |
| R4 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | v2 envelope plus base64 PDF; unknown number/order/external selector gives code 7. |
| R5 | [XML-query response](https://docs.szamlazz.hu/agent/querying_xml/response) | Success is `szamla`; error is `xmlszamlavalasz`; unknown selector gives 7. |
| R6 | [Proforma-deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | `xmlszamladbkdelvalasz`, success boolean; failure example 335; critical error may be text/HTML. |
| R7 | [Receipt-create response](https://docs.szamlazz.hu/agent/generating_receipt/response) | Successful `xmlnyugtavalasz` contains `nyugta`; optional `nyugtaPdf`; reused `hivasAzonosito` is unsuccessful and prevents duplicate issuance; codes 336–340. |
| R8 | [Receipt-storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) | Same envelope, data of the **storno receipt**, type `SN`; missing/already-reversed/storno-target refusals described without numeric codes. |
| R9 | [Receipt-query response](https://docs.szamlazz.hu/agent/querying_receipt/response) | “matches the response when generating new receipts.” |
| R10 | [Receipt-send response](https://docs.szamlazz.hu/agent/sending_receipt/response) | `xmlnyugtasendvalasz`, success boolean; code-7 example says `Hiányzó adat: emailtargy elem.` |
| R11 | [Taxpayer response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) | NAV `QueryTaxpayerResponse`; dated 2020-11-04 NAV 2.0 examples; `OK`/false validity is data; schema link points to NAV 3.0. |

Additional fresh retrievals:

- [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request): explicit `multipart/form-data`, main file and `attachfile1` … `attachfile5`.
- [Storno request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [PDF request](https://docs.szamlazz.hu/agent/querying_pdf/request), [XML request](https://docs.szamlazz.hu/agent/querying_xml/request): routing and selector context.
- [Invoice email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification): five files, “2 MB” each; invalid attachments individually omitted/notified; no processing when email disabled; test email goes to account-configured address.
- [Order-number rule](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number): per-type duplicate toggle, storno/corrective exemptions, reversed order reusable; successful replay requires matching buyer, gross, three dates and issuance “within the last 2 days.”
- [Simplified-image rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency): code meanings 551–556, including inherited final restriction.
- [Downloaded response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd): matches R1–R4's shared namespace, required singleton `sikeres`, optional code/message/number/URL, `double` money and `base64Binary` PDF.
- The **`/xml` page for each operation in R1–R11** was freshly retrieved by the scratch `docs_matrix.py`. Parsed published request examples confirmed root namespaces and credential locations for all eleven operations. This was an auth/root extraction, not a claim of fresh whole-XSD validation. The deletion examples include both credential forms; B3 permits alternatives, so the crate's one-form choice is valid.
- [PHP download page](https://docs.szamlazz.hu/php/) and [response processing](https://docs.szamlazz.hu/php/valasz-feldolgozas): PHP 2.12.4, dated 2026-08-12; “an invoice is successfully issued” although its notification fails.

**Fresh PHP archive:** [official ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), SHA-256 **`30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`**. Read in memory, never executed. Paths below are relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:17,314–323,427–431`: code **56**, with explicit comment that a notification failure **with a returned invoice number** is successful issuance. This is the specific exception to the generic “error headers omit number” prose in R1/R2.
- `Response/InvoiceResponse.php:128–158`: metadata ingestion; error text uses `urldecode`, customer URL uses `rawurldecode`. This does not establish every header's literal-plus behavior.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank `szlahu_down` checked before body interpretation. Its exception code 500 is not a captured HTTP status.
- `SzamlaAgentRequest.php:30,480–499,507–515,527–555`: default **30-second** PHP timeout; POST with `CURLFile` / `text/xml`, cookie handling and numbered attachments. PHP's timeout is an implementation default, not a mandatory service limit.

## 3. Findings and reproducible unresolved question

### Verified actionable findings

**None.** No current implementation behavior, authoritative requirement and affected exchange together established an actionable P0–P3 defect in this scope. Fixed historical findings are explicitly closed in §8. This conclusion does not equate the implementation with the entire XML Schema value space or resolve undocumented server behavior.

### Q1 — successful credit-entry acknowledgement without a reported number

**Priority:** clarification; potentially P2 interoperability impact **if** the vendor confirms this is a legitimate success. **Not counted as a verified finding.**

- **Current source:** `ops/credit_entry.rs:316–322` requires `body.invoice_number(response)`; `ops/envelope.rs:123–133` permits body/header fallback, but neither substitutes the request's number. `ClearCreditEntries` shares this parser (`ops/credit_entry.rs:238–249`).
- **Current official evidence:** [R3](https://docs.szamlazz.hu/agent/credit_entry/response) says headers “may also arrive” and “Elements marked `minOccurs="0"` may not always be included”; `szamlaszam` is optional in the schema. The schema covers errors as well as successes; the successful example reports a number. Thus schema validity alone does not establish actual success-specific emission.
- **Exact local repro:** `RegisterCreditEntry::new("I-1").parse(&RawResponse::new::<&str,&str>([], br#"<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>"#.to_vec()).with_status(200))` returns **`Err(Parse(Missing("szamlaszam")))`**, whose outcome class is Unknown. Confirmed by the scratch executable against current source.
- **Potential impact:** a completed mutation could be surfaced as uncertain. Repeating additive/replace/clear operations to recover could duplicate entries or overwrite intervening changes; current recovery guidance correctly forbids blind repetition.
- **Recorded evidence:** the September 11 clearing runs returned the expected number for both populated and already-empty invoices, but archived **parsed results, not raw channels** (`docs/research/2026-09-11-credit-clearing-live.md:14–32,64–74`). They establish those successes, not a universal number guarantee.
- **Disposition:** retain as a contract question pending the existing **unsent** clarification (`docs/research/2026-09-11-agent-vendor-clarification.md:3,18–50,89–94`). Do not manufacture a vendor echo from the requested number. No new vendor answer was obtained in this review.

## 4. Transport, authentication and multipart coverage

| Area | Source | Assessment |
|---|---|---|
| Target and verb | `wire.rs:7–14`; `client.rs:374–383` | Exact HTTPS endpoint and POST match B2/B7. One complete XML file is submitted per request. |
| File framing | `wire.rs:66–100` | Main part has `name` **and** `filename`, `Content-Type: text/xml`, CRLF framing, blank line before data and a closing delimiter. No documented requirement for a filename extension, HTML submit-button field, or PHP-specific diagnostic headers. |
| Boundary safety | `wire.rs:109–131` | Deterministic boundary is checked against XML and all attachment content; finite suffix selection stays within MIME's 70-character boundary limit. A caller's literal base boundary does not split the file. |
| Attachment metadata | `wire.rs:78–107`; `ops/invoice.rs:959–969` | Numbered raw binary file parts; disposition CR/LF removed, quote/backslash percent-escaped; MIME CR/LF removed. Unusual filename rendering/percent-decoding is not specified by the vendor. No valid documented filename case was demonstrated to be corrupted. |
| Attachment constraints | `ops/invoice.rs:405–407,430–437,480–500` | Five files and conservative decimal 2,000,000-byte cap; validated collection cannot grow beyond the bound. Server-side omission of invalid files differs intentionally from local refusal. “2 MB” has no exact byte definition in the source. |
| XML writing and validation | `xml.rs:159–178,572–638`; `wire.rs:405–440` | UTF-8 XML 1.0 declaration, ordered escaped elements, forbidden characters refused before transport. `write_xml` is documented unchecked; `to_wire` is the checked boundary. It does not claim full XSD validation for arbitrary custom trait implementations. |
| Auth forms | `credentials.rs:5–24,45–81`; `xml.rs:630–637` | Key or username/password in XML, correct element order; lowercase requirement documented without silently changing credentials. No mandatory HTTP Basic auth inferred. |
| Sensitive diagnostics | `credentials.rs:90–99`; `client.rs:149–172,339–346`; `wire.rs:155–180` | Key/key legacy mode redacts **both** fields; client URL diagnostics remove userinfo; raw response Debug hides cookies/body. Other headers and API/parser messages are not blanket-redacted, accurately disclosed at README:407. |
| TLS/network | `client.rs:242–257,300–312`; `Cargo.toml:24–25` | DNS hostname, rustls/platform trust, no old destination-IP/issuer pin, no default certificate bypass. Explicit HTTP endpoint override is for configurable transport/mock use, not a default downgrade. Receiver source-IP lists do not authenticate outgoing client responses. |
| Native session jar | `client.rs:193–207,300–308,315–328`; `wire.rs:313–340` | Default jar reuse and clone sharing; independent accounts need distinct jars; fresh default client resets session, injected provider must also be fresh. Matches B5's performance/refresh guidance. Disk persistence and periodic refresh are not required by the docs. |
| Cookie extraction | `wire.rs:330–339` | Repeated `Set-Cookie` supported; exact case-sensitive `JSESSIONID` pair, skips malformed/nonmatching entries, preserves later `=`, accepts empty value; path/domain/expiry belong to jar. |
| Deadline/redirect | `client.rs:280–312` | Native total HTTP deadline 60 s includes body acquisition; redirects disabled. XML serialization and synchronous parsing are outside this deadline. No source mandates another timeout or a separate connect timeout. |
| Custom clients/browser | `client.rs:193–212`; `lib.rs:45–70`; README:309–359 | Supplied transport settings remain active. Fetch uses same-origin credentials; browser cannot read Set-Cookie; CORS/response-header exposure unestablished. The docs explicitly prohibit putting account keys in client-side code. |

### All action names, credential locations and response versions

All actions match the **complete B7 table**, independently of the existing golden files. `beallitasok` denotes the settings child of the root. For every row, the fresh `/xml` request example had the root/namespace and credential location used by the writer.

| Operation | Exact multipart action | Writer source | Credentials | Explicit response version |
|---|---|---|---|---|
| Invoice and its kinds | `action-xmlagentxmlfile` | `ops/invoice.rs:679,759–767` | `beallitasok` | 2 |
| Invoice storno | `action-szamla_agent_st` | `ops/storno.rs:163,173–190` | `beallitasok` | 2 |
| Register / clear credit entries | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:238–249,275,291–302` | `beallitasok` | 2 |
| Invoice PDF | `action-szamla_agent_pdf` | `ops/query_pdf.rs:59–78` | root | 2 |
| Invoice XML | `action-szamla_agent_xml` | `ops/query_xml.rs:536–554` | root | none; structured by operation |
| Delete proforma | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:61–78` | `beallitasok` | none |
| Create receipt | `action-szamla_agent_nyugta_create` | `ops/receipt.rs:190,223–231` | `beallitasok` | none |
| Storno receipt | `action-szamla_agent_nyugta_storno` | `ops/receipt.rs:341,344–352` | `beallitasok` | none |
| Query receipt | `action-szamla_agent_nyugta_get` | `ops/receipt.rs:421,424–432` | `beallitasok` | none |
| Send receipt | `action-szamla_agent_nyugta_send` | `ops/receipt.rs:503–511` | `beallitasok` | none |
| Query taxpayer | `action-szamla_agent_taxpayer` | `ops/taxpayer.rs:265–276` | `beallitasok` | none |

`ops::RESPONSE_VERSION` (`ops.rs:28–32`) is the single constant used by all four version-selecting writers. The crate deliberately asks for v2 even where an official request example uses v1 or omits the field. Missing DONE/raw-PDF/v1-stack-trace decoders are not incompatibilities with the response format it requests.

### Retry policy versus physical submissions

The client has no application retry/reconciliation loop (`client.rs:374–405`). README:161 and `recovery.md:38–42` correctly translate B4 into **five total sends including the first**, not five retries after it; they do not invent combined accounting for a write and its separate reconciliation queries. The caller owns cross-call limits.

Fresh `cargo tree -p szamlazz-agent --locked --offline --features client-reqwest -e features -i reqwest` showed **reqwest 0.13.4**, cookies/rustls and their internal TLS features; no HTTP/2 or HTTP/3 in this isolated feature graph. Read its installed `src/retry.rs:9–17,195–202,273–317`: default retries are protocol-NACK handling (two extra retries), with HTTP/2/3 branches feature-gated. This is not a generic 5xx/vendor-code loop. Feature unification or an injected client can change this boundary. No unsafe default resend was demonstrated; no “one send always means one physical POST” guarantee is inferred. Injected retries are expressly disclosed in `client.rs:199–202` and README:405.

## 5. Response preservation, status and metadata

### Transport acquisition

`client.rs:385–405` saves status/headers **before** consuming the body. A failed body transfer returns `IncompleteResponse { status, headers, source }`, retaining original header bytes and repetitions (`:66–100`). It does not turn header evidence into a completed response with an invented empty body. This is always Unknown (`:117–126`). The source chain survives; Debug prints header names only. `tests/client.rs:275–337` contains actual truncated loopback HTTP cases with ordinary errors, down, numbered 56 and plain number headers.

For a **completed** body, every received header is passed to `RawResponse`, names case-folded, values converted with lossy UTF-8 (`client.rs:394–403`; `wire.rs:186–198`). Documented numeric/URL-encoded headers are ASCII and survive. RawResponse retains body bytes unchanged and optional status. Completed parse errors expose typed errors/bounded excerpts rather than the entire raw response; arbitrary non-UTF-8 header bytes are not preserved exactly on that path. Those are API capability boundaries, not demonstrated losses of documented valid responses.

### Shared decision order

| Input | Current decision | Source |
|---|---|---|
| Nonblank down header | ServiceUnavailable before code/status/body | `wire.rs:291–297` |
| Nonblank error-code header | Api before HTTP/body; issuing parser handles 56 separately | `wire.rs:262–270,298–300`; `ops/envelope.rs:179–186` |
| Known non-2xx, no preceding verdict header | HttpStatus/Unknown; number/id/other headers do not bypass it | `wire.rs:301–308` |
| Complete expected XML at 2xx/unknown status | Read operation verdict/data | `xml.rs:201–275,478–554` |
| Body-only false/code | Api, even with no error headers | `xml.rs:506–522`; `ops/envelope.rs:197–203` |
| Normal numbered issuing success | CreatedInvoice with optional metadata | `ops/envelope.rs:209–248` |
| Unnumbered create success | Preview only if requested and PDF present; otherwise Missing number | `ops/invoice.rs:948–956` |
| Numbered 56 | Issued with notification flag; optional malformed metadata may be absent | `ops/envelope.rs:205–248,285–315` |
| Header 56 + non-56 body refusal | Body refusal wins, including with malformed optional metadata | `ops/envelope.rs:197–203` |
| Header 56 + empty/plain notification body | Narrow fallback; still needs number; malformed XML excluded | `ops/envelope.rs:188–196,251–254` |
| 56 without number | Api(56), Unknown | `ops/envelope.rs:211–216`; `error.rs:375–381` |
| XML query | `szamla` data or false `xmlszamlavalasz`; successful envelope alone refused | `ops/query_xml.rs:563–608` |
| Receipt create/storno/query | Shared `xmlnyugtavalasz` verdict + receipt data | `ops/receipt.rs:293–295,364–366,449–451,688–697` |
| Delete / receipt email | Root-specific structured acknowledgement | `ops/proforma.rs:82–88`; `ops/receipt.rs:527–532` |
| Taxpayer | Expected NAV 2.0/3.0 envelope/path; non-OK code retained; OK requires validity | `ops/taxpayer.rs:281–284,403–416,592–622` |

R1–R11 do not specify the entire conflict/status matrix. Body-only code at HTTP 500 becoming HttpStatus rather than Api is documented **library policy** (README:341), not an observed vendor emission error. Repeated contradictory headers use the first value (`wire.rs:230–237`); ordinary header error beats success body; body values beat headers. Synthetic tests establish these choices, not vendor guarantees. P corroborates down-first and the numbered-56 exception, not every contradictory combination.

### Metadata encoding/fidelity

- `wire.rs:239–249,343–351`: textual headers decode once, form-style (`+` → space, `%2B` → plus, `%252B` → `%2B`). Malformed escaped UTF-8 falls back to plus-adjusted original text. Numeric totals/id/codes use raw headers. R1/R2 explicitly distinguish encoded number/error from raw totals/code.
- `ops/envelope.rs:123–167,344–370`: nonblank XML amount wins; malformed XML amount does not fall back to a valid header. Absent/empty XML can fall back. Headers accept ungrouped comma/dot, signs, exponent and HTTP SP/HTAB padding. `100,01` has recorded P60 evidence; no grouping heuristic is invented.
- `ops/envelope.rs:137–143`: XML URL undergoes entity decoding only, never header decoding; `%2B` and literal `+` survive. Header URL uses the shared decoder. Exact literal-plus requirements for URL/payment/down headers remain unspecified (Q2 below).
- `ops/envelope.rs:318–324`: decoded payment-method header retained as open PaymentMethod. R1/R2/R3 list that header; no XML payment-method field exists in the shared reply XSD. Create/storno and credit projections expose it.
- `ops/envelope.rs:337–342`: auxiliary document id is nonnegative i64 or None, never an account identifier. Its exact presence is O rather than required by R1's table.
- `types.rs:104–117`: standard base64 after whitespace removal; raw bytes exposed, no PDF signature/page validation claimed. Normal malformed PDF fails parsing; numbered-56 malformed optional PDF becomes None. PDF query requires the artifact (`ops/query_pdf.rs:83–93`); create/storno retain issuance when no optional PDF was returned.

## 6. Complete error catalogue and outcome audit

**No missing general-list mapping.** The full current EN/HU general table has these **30** codes:

`1, 3, 53, 54, 55, 57, 71, 135, 136, 152, 164, 202, 259, 260, 261, 262, 263, 264, 363, 364, 365, 537, 538, 539, 551, 552, 553, 554, 555, 556`.

The crate's **43 named numeric codes** reconcile exactly as **30 general + 5 receipt supplement (336–340) + 2 operation-example codes (7,335) + 1 first-party warning (56) + 5 explicitly observed codes (14,73,221,352,463)**. Table below covers every named code. Mapping/round trip: `error.rs:218–317,460–484`; classification: `error.rs:375–423`.

`U` = Unknown, `R` = Rejected, `D` = DuplicateOrderNumber, `N` = NotFound. These describe this exchange, not all prior sends. Only **1/55** are potentially retryable; only **3/135/136/164** are credential errors (`error.rs:319–352`).

| Code | Variant | Class | Evidence / meaning |
|---|---|---|---|
| 1 | Maintenance | U | B4: maintenance/internal error; retry after minutes. |
| 3 | InvalidCredentials | R | B3/B4: authentication failure. |
| 7 | MissingData | N | R4/R5: unknown selector; R10: missing email subject. Not universally a missing document. |
| 14 | StornoOfReversalInvoice | R | O, B5-storno-SS: reversal/credit invoice cannot be reversed/credited. |
| 53 | XmlNotAFile | R | B4: XML file missing/not uploaded as file. |
| 54 | EInvoiceNotEnabled | R | B4: e-invoice permission/certificate setup. |
| 55 | EInvoiceSigningFailed | U | B4: expired certificate or unreachable timestamp server. Not proof of issuance or nonissuance. |
| 56 | InvoiceNotificationDeliveryFailed | U when error | P: with a number, issuance warning; without number remains uncertain. |
| 57 | MalformedXml | R | B4: XML reading/XSD failure. |
| 71 | DuplicateOrderNumber | D | B4/order rules: duplicate setting refusal. |
| 73 | PrepaymentInvoiceNotIdentifiable | R | O, C6-4/5: prepayment not identifiable/already settled. |
| 135 | BrowserSessionActive | R | B4: log out of browser. |
| 136 | LoginBlocked | R | B4: subscription/pending invoice/payment-related access problem. |
| 152 | DuplicateOrderNumberNamed | D | B4: same as 71, names order. |
| 164 | MultipleAccounts | R | B3/B4: legacy user has multiple account access. |
| 202 | UnregisteredPrefix | R | B4: empty/unregistered invoice prefix. |
| 221 | HasCorrectiveInvoice | R | O, B7: original with corrective cannot be reversed. |
| 259 | NetValueMismatch | R | B4: net = price × quantity; product name. |
| 260 | VatValueMismatch | R | B4: VAT = net × rate / 100; product name. |
| 261 | GrossValueMismatch | R | B4: gross = net + VAT. |
| 262 | NetValueInvalid | R | B4: net check; EN says row number, HU says product name. Diagnostic retained, no parsing by message. |
| 263 | VatValueInvalid | R | B4: VAT check identifying row. |
| 264 | GrossValueInvalid | R | B4: gross check identifying row. |
| 335 | ProformaNotFound | R | R6: absent/already-deleted proforma; not successful replay. |
| 336 | ReceiptPrefixUsedForInvoices | R | R7: receipt prefix already used for invoices. |
| 337 | InvalidReceiptPrefix | R | R7: uppercase letters/numbers only. |
| 338 | DuplicateReceiptCallId | R | R7: call id already exists; refuses duplicate, does not recover prior result. |
| 339 | ReceiptNotFound | N | R7: receipt number does not exist. |
| 340 | ReceiptPaymentMismatch | R | R7: paid amount differs from gross. |
| 352 | IssueDateMustBeToday | R | O, B3: storno issue date, not fulfillment date; observed on paper too. |
| 363 | ReceiptGrossNotWhole | R | B4: HUF receipt gross must be whole. |
| 364 | ReceiptNetPrecision | R | B4: HUF receipt net ≤2 decimals. |
| 365 | ReceiptVatPrecision | R | B4: HUF receipt VAT ≤2 decimals. |
| 463 | PaymentOnReversedInvoice | R | O, D8: credit on reversed invoice; body-only. |
| 537 | ErasureCodeLimit | R | B4: maximum 400 data erasure codes per item. |
| 538 | ErasureCodesUnavailable | R | B4: not usable in demo/test account. |
| 539 | ErasureCodesDisabled | R | B4: disabled in account settings. |
| 551 | SimplifiedImageAccountIncompatible | R | B4/simplified-image page: OSS/non-Hungarian seller tax number, including inherited final. |
| 552 | SimplifiedImageItemLimit | R | B4: max two items; four for final. |
| 553 | SimplifiedImageVatInvalid | R | B4: prohibited simplified-image VAT token. |
| 554 | SimplifiedImageCannotCorrect | R | B4: cannot correct simplified original. |
| 555 | SimplifiedImagePrepaymentVatMismatch | R | B4: final/prepayment VAT mismatch. |
| 556 | SimplifiedImageDocumentForbidden | R | B4: simplified corrective/delivery note prohibited. |

Unknown numeric or textual/NAV codes preserve trimmed text and stay U. Known numeric spelling normalizes (`007` → 7); above-u16 values remain Unknown strings. Failure without code is `Absent`, empty wire code, U — not invented code 0. `is_retryable=false` on unknown is not a proof of permanent failure or permission to send a write again. Tests distinguish these questions (`tests/error_classification.rs:185–256`; `error.rs:812–1079`).

Credential refusals reasonably map to R from their documented access meanings, but exact server processing order is **not** established; current public rustdoc correctly says so (`error.rs:334–342`). Historical comments in `error.rs:1006–1009` and the behavior note's design-consequence prose are stronger than that evidence. They are not observations or a reason to strengthen the public guarantee. Correcting credentials permits reevaluation, not guaranteed document success.

`ResponseError::{Parse,ServiceUnavailable,HttpStatus}` and client transport/incomplete counterparts stay U (`error.rs:752–770`; `client.rs:103–137`). Local Request errors are R. Recovery documentation distinguishes invoice issuance, receipt creation/storno, reads, credit mutation, proforma deletion and email (`recovery.md:11–20`); it does not infer mutation/delivery success merely from document existence. Receipt 338 does not replay success; receipt storno is not assigned invoice-style replay semantics. Immediate empty query or elapsed time is not negative settlement.

## 7. XML lexical handling and tests

The current shared reader validates UTF-8, expected expanded root, one completed document, matching closes, legal prolog/epilog and EOF (`xml.rs:201–275`). Namespace declarations are normalized before reserved-binding and expanded-attribute checks (`:37–155`). A tokenizer complements structural checks (`:280–301`). DTDs are deliberately refused; this is not a general-purpose validating XML processor.

Protocol projection (`xml.rs:310–378`) canonicalizes element names for serde while excluding foreign subtrees. The placeholder prevents `tr<foreign/>ue` becoming `true`. Namespace aliases and intervening ignored extensions do not split repeated lists; recognized singleton duplicates/scalar children remain failures. Optional diagnostic parsing is independent (`:478–499`), so a duplicate/nested `hibauzenet` does not erase a readable refusal or numbered-56 identity. Whole-document lexical failure still refuses the response.

Valid forms affecting documented fields were checked: boolean `true/false/1/0`, XML padding, entity/CDATA text, comments/PIs splitting scalar text, namespaced aliases, legal attribute quoting, base64 line wrapping, and finite signed/exponent numeric forms. Dates preserve a civil date and accept complete `Z`/`±hh:mm` suffixes without byte-boundary panics (`xml.rs:667–726`). Current invoice/receipt unit tests cover invalid calendar/suffix/offset cases and optional emptiness. This Agent policy differs from Adatkapcsolat's lenient invalid-date content handling.

**Explicit domain deviations:** finite exact Decimal is narrower than XSD `double` (nonfinite and unrepresentable finite values rejected); Jiff's finite date range and legacy date grammar are not all XSD dates. Optional empty elements are often absent; empty `sikeres` uses the legacy false reading (`xml.rs:768–779`), yielding Absent/Unknown when no code exists, rather than an invented success. `sikeres=true` wins over a contradictory body code (`:506–509`); documented errors use false. No current documented valid monetary/date observation was shown to fail due to these policies. This report does not call them complete XSD value-space conformance.

### Independent scratch execution

```sh
cargo run --offline --manifest-path /tmp/opencode/transport-eec57fc/Cargo.toml
```

The scratch manifest uses the current Agent path dependency with no transport feature. It compiled source afresh in its own target directory. Resolved parsing dependencies match the workspace's relevant versions: quick-xml 0.42.0, xmlparser 0.13.6, Jiff 0.2.35, rust_decimal 1.43.0, base64 0.23.1. It is not a workspace `--locked` suite execution.

**15 controls passed:** eight valid envelope cases (entity scalar, CDATA scalar, comment/PI-split scalar, padded signed exponent, wrapped base64, namespace alias, supplementary-plane extension name, prefixed attribute); four BOM/declaration cases using SP/HTAB/CR/LF after `xml`; Q1 numberless-credit policy; two adverse numbered-56 identities (duplicate/nested body number with a valid number header), both refused. These are synthetic parser controls, not vendor observations. No production/test files were modified.

### Existing coverage read, not rerun here

| Test source | What it establishes / important limit |
|---|---|
| `tests/client.rs:108–435` | Multipart POST, errors/status, cookie clone/fresh isolation, interrupted body evidence, validation before HTTP. Injects cookie/timeout/no-redirect settings without root CAs; does not establish production TLS or actual 60-second timing. |
| `tests/custom_http_client.rs:13–74` | Non-reqwest wire round trip and cookie extraction. One successful response, not non-2xx behavior. README's newer ureq example explicitly disables status-as-error. |
| `tests/response_headers.rs:9–602` | Diagnostics, payment headers, cookie exactness, status precedence, one-pass decoding, comma/exponent money, body precedence, optional metadata, numbered-56 identity controls. |
| `tests/response_completion.rs:12–193` | Complete document and lexical checks including ignored extensions; legal declaration controls. |
| `tests/response_namespaces.rs:16–357` | Expanded-name identity, list grouping, foreign isolation, reserved bindings, attribute normalization and legal quoting. |
| `tests/error_classification.rs:49–256`; `error.rs` unit tests | Source-derived added-code table, all original named mappings/classes, absent/unknown and credentials. The supplement's synthetic Hungarian messages are labeled synthetic. |
| `tests/numeric_fidelity.rs:15–186`; `tests/business_text.rs:9–86` | Exact monetary conversion and business-character preservation. Boundary refusals are local policy, not vendor-execution claims. |
| `tests/upstream.rs:403–1019,1022–1234` | Dated published examples, explicit malformed sample/PDF transformations; request outline is deliberately lossy. Corpus absence in a package can skip; passing is not automatically fresh vendor coverage. |
| `tests/request_dates.rs:42–171`; `tests/schema_requests.rs:31–90` | Checked outbound date positions; schema export uses actual XML multipart bytes and both auth forms. Export itself is not XSD validation; the parent owns the broader checks. |
| `wire.rs:449–663`; invoice attachment unit tests | Debug redaction, boundaries, framing, action field and bounded file parts; no vendor filename-rendering observation. |

No new browser, TLS handshake, HTTP/2/3 failure, resource-load, fuzzing or complete XML test-suite run was performed. Entire response buffering and copies have no explicit body cap (`client.rs:387–403`); this is a hardening/capability boundary without a documented size cap to compare against.

## 8. Historical candidates suppressed at this HEAD

| Candidate | Independently checked current state | Disposition |
|---|---|---|
| Legacy key/key leaks through Debug | Both username/password redacted at `credentials.rs:90–99`, inherited by client formatters. B3 confirms key/key is a valid auth form. | Fixed; not reported again. |
| Body transfer failure drops received headers/status | `IncompleteResponse` at `client.rs:66–100,385–393`; actual loopback regression present. | Fixed; retained evidence remains Unknown. |
| Header number hides invalid body identity under 56 | `ops/envelope.rs:205–209,285–315`; current test controls plus two independent scratch controls refuse duplicate/nested number. | Fixed. |
| Cookie-name prefix match | Exact pair comparison at `wire.rs:337`; tests reject `JSESSIONIDOTHER`. | Fixed. |
| Decimal-comma header loss | Shared parser at `ops/envelope.rs:363–370`, used by issuing/PDF/balance paths. | Fixed, consistent with P60 observation. |
| URL-userinfo diagnostic leakage | Parsed URL sanitization at `client.rs:164–172`, used at build/debug. | Fixed in reviewed diagnostics. |
| Blanket HTTP/credential sequence claims | Public `error.rs:3–7,334–342` and README:341 explicitly bound them. | Closed as public-contract finding; historical comments are not stronger evidence. |
| Browser-key advice / indefinite retry/replay | `lib.rs:59–70`, README:89–163, `recovery.md`; B3/B4 and order rule freshly checked. | Current text distinguishes compilation, deployment and bounded recovery. |
| Missing documented receipt/simplified-image codes | All thirteen additions in the current map, §6. | Fixed; complete current list rechecked. |

## 9. Recorded live evidence, deviations and remaining questions

`docs/szamlazz-hu-behaviour.md:3–37` bounds the historical evidence to one TEST account on September 3/6/7 and says raw A–D logs are outside this repository. The later clearing evidence explicitly does **not** establish continuity with that account. Historical “Design consequence” cells can contain superseded worker policy; those cells are not additional server observations.

| Topic | Actual record / evidence strength | Disposition |
|---|---|---|
| Body-only errors | Behavior note:151–155 records create/storno/delete headers, query 7 and credit 463 body-only. O, specific operations/codes. | Current parsers read body as well as headers; generic optional-header docs are compatible. |
| Header comma | Behavior note:170 records P60-E1 `szlahu_nettovegosszeg 100,01`. O. | Preserve comma support; no inference of grouping or other locale spellings. |
| Storno replay/no-op | Behavior note:95–104 records existing-SS echo and unchanged proforma/delivery-note echo. O. | Preserve; `CreatedInvoice::reverses` at `ops/envelope.rs:61–87` is properly a heuristic. False is inconclusive, not proof of no reversal. Zero/negative-original cases remain unobserved. |
| Storno external id | Fresh storno request prose describes referencing the original by external id; behavior note:79 records B6/XPRB assigning it to the new SS. D/O conflict. | Retain the recorded behavior; do not change identity/recovery on generic wording alone. |
| Appearance/fulfillment dates | Behavior note:99–107 records P48/P73 behavior, including silently accepted mismatches. O. | No transport inference from a prefix or illustrative positive storno total. |
| External id/replay | Behavior note:60–80 records short replay, nonunique id, newest holder and id attached only on creating call. O; bounded D replay rule freshly checked. | Identity checks and retained uncertainty in recovery are appropriate; no indefinite dedupe promise. |
| 55/56 | Behavior note:163,181,190–191 says 56 could not be triggered; 55 not observed. Current email docs explain test-account redirection. P establishes numbered 56; no O for its wire shape. | Keep warning exception and Unknown without number. Executable probes do not upgrade this evidence. |
| Stalled send | Behavior note:162 records ≥57-second stall and no issuance found by order query. Broader delayed-issuance assertion has no linked probe. O with provenance limit. | `client.rs:283–290` / `recovery.md:50–55` correctly qualify this. 60 seconds is policy; elapsed time alone cannot settle uncertainty. |
| Explicit clearing | Dated clearing research:14–32,64–75 records two passes, expected numbers/balances, empty post-clear entries and verified storno cleanup. O, parsed output only. | Real new clearing evidence; not universal success echo or raw-header evidence; Q1 remains. |
| Sample defects | R1/R2/R3 have raw `&` in example URLs; PDFs use dots/prose placeholders. Freshly visible, also documented at `fixtures/SOURCES.md:142–170`. D example defects. | Refusing literal malformed samples is correct. Escaping/substituting PDFs in tests does not reconstruct actual vendor responses. |

Additional unresolved questions, with no confirmed code correction from current evidence:

1. **Q2 — exact header encoding and conflicts.** R1/R2 explicitly specify number/error encoding and raw totals/code, but not literal-plus semantics for customer URL, payment method or down, nor repeated/conflicting headers. PHP URL handling differs (`rawurldecode`) from error text (`urldecode`). Obtain raw valid captures or a vendor statement before changing the Rust policy. No double decoding is justified.
2. **Q3 — HTTP status/envelope matrix.** Does a legitimate body-only error arrive at non-2xx? Can numbered 56 arrive with HTML or truncated XML? What wins when channels disagree? Docs and O do not establish these combinations. Current tests are policy tests; incomplete responses remain uncertain.
3. **Q4 — live-session credential revocation/precedence.** B3 says deleting a key takes effect immediately; B5 describes authenticated sessions. Neither tells whether an existing cookie can outlive key deletion or which credential wins if a cookie and XML key refer to different accounts. Distinct/fresh jars are a caller ownership policy, not proof of vendor invalidation.
4. **Q5 — response domain limits.** Exact finite money and finite civil dates are deliberately narrower than the published generic XSD types. No recorded operational value exceeds them. DTD/alternate-encoding/very-large response behavior is not established by the UTF-8 examples; avoid claiming universal XML/XSD conformance.
5. **Q6 — effective schemas.** Current vendor clarification records combined preview/simpleItems order and missing downloaded declarations (`docs/research/2026-09-11-agent-vendor-clarification.md:52–87`). This review freshly extracted auth/root examples, not a full revalidation of those business-field conflicts. Existing draft is unsent; schema exports/probes are not vendor answers.
6. **Q7 — current NAV forwarding and receipt recovery.** R11 still uses old examples; generic NAV failure roots and newer optional-field forwarding require direct Agent evidence. R8's refusal descriptions do not establish a storno-specific call-id/338 guarantee. The current recovery text correctly leaves these open.

**Conclusion:** retain the scoped implementation and its fixed regressions. The independent full-source comparison found no new verified actionable transport/shared-protocol defect. Pursue the success-specific credit acknowledgement and raw status/header/session questions as evidence work; do not replace bounded observations with stronger guarantees, or treat a fresh probe's existence as execution evidence.
