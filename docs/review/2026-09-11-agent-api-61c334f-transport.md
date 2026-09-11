# Számla Agent review — transport, authentication and shared replies

**Date:** 2026-09-11

**Requested baseline:** `61c334f9508e8b63df2f3db182d6ca83f4feb8f0`, including the working tree

**Result:** no verified actionable defect in this assigned scope; **0 P0 / 0 P1 / 0 P2 / 0 P3**.

**Verification:** **249 offline unit/integration tests passed**, none failed or ignored in the selected targets.

This is a full inspection of the assigned transport/shared-parser slice, not a diff-only review or a claim that the entire crate's business models are correct. No currently documented valid exchange was shown to fail incorrectly. Malformed-response robustness was evaluated separately; the earlier numbered-56 identity defect is fixed and its adverse cases pass. No source change is recommended from this evidence.

## Scope, provenance and method

Reviewed production code in `crates/szamlazz-agent/src/{client.rs,credentials.rs,wire.rs,error.rs,xml.rs,ops/envelope.rs}` in full. Followed invoice/storno response entry points, PDF/balance projections, action constants and credential/version emission in other operations, shared PDF and numeric readers, recovery/platform documentation, and relevant tests. Code line ranges below are relative to `crates/szamlazz-agent/src/` unless qualified.

At entry, HEAD was the requested `61c334f`; `wire.rs` already had a user change extracting `validate_xml_text`. That exact working-tree code was read and tested. During the review another process committed the existing work, advancing HEAD to **`5c6d5ead33a3587c4ea29cc973bedaefc3ddcb1f`**. A subsequent `git diff 61c334f -- crates/szamlazz-agent Cargo.lock docs/szamlazz-hu-behaviour.md fixtures/SOURCES.md` showed **only the original `wire.rs` extraction**. Thus the audited source did not change under the review despite the moving HEAD. The report keeps the requested filename and baseline, and cites working-tree line numbers, including the nine-line shift after `wire.rs:411`.

Current official pages and downloadable schemas were fetched afresh; the site reported build **`v202608271632`**. That label is not the date of every example. The official PHP archive was fetched and inspected in memory as corroborating first-party source, without executing PHP. Historical reports were consulted as candidate lists after initial production/source inspection, not accepted as proof. `docs/szamlazz-hu-behaviour.md` was read as bounded account evidence, not a universal specification.

The assigned review was executed directly: this session had no subagent tool. Independent retrievals were parallelized. No authenticated Számla Agent request, credential lookup, live test, source/test/fixture edit, or commit was performed. The only authored repository file is this report.

## Fresh primary-source register

| Ref | Authoritative URL | Exact claim or schema fact used |
|---|---|---|
| B1 | [How does it work?](https://docs.szamlazz.hu/agent/basics/how-does) | XML file in an HTTP POST to `https://www.szamlazz.hu/szamla/`; generating the document and emailing it are distinct steps. |
| B2 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | “same URL every time”; function selected by the “name of the form field containing the XML file”; “HTTPS POST”; eleven action names; one invoice/receipt per creation XML; tag names are case-sensitive. |
| B3 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Prefer `<szamlaagentkulcs>`; legacy integrations may “use the same key in both fields” (`felhasznalo`, `jelszo`); keys accepted “only in lowercase”; legacy user must access “exactly one billing account”; keys must not be included in client-side code. Keys have identical permissions and deletion takes effect immediately. |
| B4 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Save/reuse `JSESSIONID`; “inactive for 90 minutes” causes session deletion; without storage each request authenticates again; new session after company/email edits is “advised”; file persistence is “advisable.” |
| B5 | [Error handling EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Same request “at most five times” / “legfeljebb ötször”, then stop for human intervention; no until-success loop; maximum 500 test invoices/10 minutes; v1 plain-text error format; 30 general numeric error codes. |
| B6 | [Network/security](https://docs.szamlazz.hu/agent/basics/security), [linked 2025 migration notice](https://tudastar.szamlazz.hu/gyik/technologiai-valtozasok-2025) | Destination CIDRs differ from outbound receiver IPs; old fixed destination addresses ceased; certificate issuer changed from Let's Encrypt to Google Trust Services. No exact client timeout or complete HTTP-status matrix is stated. |
| I1 | [Invoice request](https://docs.szamlazz.hu/agent/generating_invoice/request) | POST, `multipart/form-data`, file `action-xmlagentxmlfile`, optional `attachfile1` … `attachfile5`. |
| I2 | [Invoice response](https://docs.szamlazz.hu/agent/generating_invoice/response) | v2: “Structured `xmlszamlavalasz` with optional base64 PDF”; number/error text URL encoded, totals/error code not encoded; `szlahu_fizetesmod` and customer URL headers; error XML has `sikeres=false`, code, message. |
| S1 | [Storno request](https://docs.szamlazz.hu/agent/reversing_invoice/request) | Same endpoint/verb/content type, `action-szamla_agent_st`, original invoice number in `fejlec`. Its external-id description conflicts with recorded B6/XPRB account evidence; see exclusions below. |
| S2 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Same v2 envelope/header contract; `szlahu_szamlaszam` is the storno invoice number. |
| X1 | [Reply XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Qualified namespace `http://www.szamlazz.hu/xmlszamlavalasz`; required singleton boolean `sikeres`; optional singleton string code/message/number/URL, `double` totals/outstanding, `base64Binary` PDF. Structurally agrees with I2/S2 inline schemas. |
| X2 | [Invoice request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Ordered settings put credentials before flags and `valaszVerzio`; `elonezetpdf` comment says preview PDF, no document created. Used for transport/settings and preview semantics, not to re-audit all business fields. |
| X3 | [Storno XML/inline XSD](https://docs.szamlazz.hu/agent/reversing_invoice/xml), [linked download](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd) | Ordered settings: legacy credentials/key, appearance/download/copies, aggregator, guardian, response version, external id. |
| E1 | [Invoice email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Up to five files, “2 MB” each; bad attachments individually omitted/notified while valid ones are sent; attachments ignored when email disabled; test email goes to the account-configured address. |
| E2 | [Order-number rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Per-type duplicate toggle; storno/corrective exemption; reversed order reusable; successful replay requires matching buyer, gross, three dates and creation “within the last 2 days.” |
| C1 | [Credit-entry response](https://docs.szamlazz.hu/agent/credit_entry/response) | v2 `xmlszamlavalasz`, balance/header data, structured refusal. |
| C2 | [PDF-query response](https://docs.szamlazz.hu/agent/querying_pdf/response) | v2 `xmlszamlavalasz` with base64 PDF; unknown number/order/external selector gives 7. |
| C3 | [XML-query response](https://docs.szamlazz.hu/agent/querying_xml/response) | Success `szamla`, error `xmlszamlavalasz`; unknown selector gives 7. |
| C4 | [Proforma-deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | `xmlszamladbkdelvalasz`; 335 example for absent/deleted proforma; critical errors may be text/HTML. |
| C5 | [Receipt-create response](https://docs.szamlazz.hu/agent/generating_receipt/response) | 336–340 supplement; reused `hivasAzonosito` makes the call unsuccessful and prevents duplicate receipt creation. |
| C6 | [Receipt-send response](https://docs.szamlazz.hu/agent/sending_receipt/response), [receipt-storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) | Send error 7 example means missing `emailtargy`; storno returns the `SN` record and describes missing/already-reversed/storno-target refusals without assigning their numeric codes. |
| C7 | [Receipt amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) | 261 exact HUF sum; 363 whole gross; 364/365 net/VAT precision. |
| C8 | [Simplified image](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Rejection codes 551–556, including inherited final-invoice account restrictions, item count, VAT matching and prohibited document types. |
| C9 | [Data erasure](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Maximum 400 codes per item, errors 537/539; B5 also supplies test/demo refusal 538. |
| P1 | [PHP download page](https://docs.szamlazz.hu/php/), [response processing](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Offered PHP version 2.12.4 (2026-08-12); invoice can be “successfully issued” while notification delivery fails, exposed separately. |
| P2 | [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Freshly retrieved SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`; source locations below. |

P2 source paths, relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:17,314–323,427–431`: notification failure is code **56**; comment explicitly says that when notification delivery failed **but the reply contains an invoice number**, invoice issuance succeeded. This establishes the specific warning exception, not arbitrary malformed XML tolerance.
- `Response/InvoiceResponse.php:128–158`: number/id, raw totals/outstanding, decoded error text, customer URL ingestion. Its URL ingestion uses `rawurldecode`; that alone is not a normative plus-handling specification.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank `szlahu_down` checked before body processing. The thrown exception's 500 is not proof of wire HTTP status 500.
- `SzamlaAgentRequest.php:480–499,507–515,527–555`: POST, `CURLFile`/`text/xml`, action field, cookie handling, numbered attachments, total/connect timeout configuration. Line 30 gives **30 seconds** as PHP's default, not a mandatory vendor limit.

An initially guessed `.../agent/xmlszamlast.xsd` URL returned 404; following the actual X3 link retrieved `.../agentst/xmlszamlast.xsd` successfully. No unavailable schema was silently treated as verified.

## Ranked verified issues

**None established.** There is no finding whose current code behavior, authoritative requirement, affected valid exchange and impact together justify a P0–P3 correction in this scope. In particular, a previously reported defect is not kept open solely because its historical reproducer was serious.

### Independently rechecked historical candidates

| Candidate | Current evidence | Disposition |
|---|---|---|
| Legacy key/key authentication leaks through `Debug` | B3 explicitly allows key in both fields. `credentials.rs:90–99` now redacts both; `client.rs:149–158,339–346` uses that redacted representation. Tests `credentials::tests::debug_is_redacted` and `client::tests::legacy_agent_key_is_redacted_in_client_diagnostics` passed. | Fixed. Valid documented auth form; no remaining leak through these formatters. |
| Header number makes malformed body identity a successful 56 | X1 says singleton scalar number. `ops/envelope.rs:205–209` propagates payload identity failure; `:285–315` salvages only independently readable identity after optional metadata failure. `tests/response_headers.rs:539–581` checks duplicate/same-value duplicate/nested identity with number headers and optional bad metadata. | Fixed. All controls passed; this was malformed-response robustness, not a captured vendor interoperability defect. |
| Body download failure discards received headers/status | `client.rs:66–100,385–393` retains original HeaderMap/status/source as `IncompleteResponse`, always Unknown (`:117–126`). `tests/client.rs:275–337` passes real truncated loopback HTTP cases for code 3, down, numbered 56 and plain number, retaining repeated cookies without logging values. | Fixed. Evidence retained without claiming success. |
| Overstated “never HTTP status” or exact pre-auth server sequence | `error.rs:3–7` now allows HTTP failures; `:334–342` explicitly calls access-refusal classification an interpretation and says exact server processing order is unestablished. B3/B5 support access refusal, not stronger sequencing. | Fixed. No classifier change warranted. |
| Cookie prefix match / decimal comma loss / URL-userinfo diagnostics | Exact `JSESSIONID` pair comparison at `wire.rs:330–339`; comma normalization at `ops/envelope.rs:363–370`; URL parser-based diagnostic redaction at `client.rs:162–172`. Current unit/header/client regressions passed. | Fixed. |
| Browser deployment, blanket log safety, indefinite identical replay promises | `lib.rs:59–70`, README `:348–350,395–398`, `error.rs:78–96`, and `recovery.md:4–9,36–55` now bound these claims. B3 prohibits client-side keys; E2 bounds replay. | Closed in current text. |

Historical leads consulted: `2026-09-11-agent-api-transport.md`, `2026-09-11-agent-api-837dad0-transport.md`, and the September 10 transport report candidate headings. Their pass counts, acquisition dates and conclusions were not substituted for this review's own checks.

## Valid documented interoperability: transport and authentication

| Area | Current implementation | Assessment |
|---|---|---|
| Endpoint/verb/file framing | `wire.rs:7–14,66–100`; `client.rs:374–383` | Exact B1/B2/I1/S1 endpoint and POST. Main part has both `name` and `filename`, MIME `text/xml`, CRLF delimiters, blank line before XML and final boundary. Filename extension and sample submit-button fields are not documented routing requirements. |
| Multipart collision/attachments | `wire.rs:78–131`; `ops/invoice.rs:938–948` | Boundary checked against XML and every attachment byte sequence; bounded suffix until absent. Files remain raw bytes in numbered parts. Disposition metadata removes CR/LF and escapes quotes/backslashes. Determinism is not a collision bug. |
| XML generation | `xml.rs:139–161,554–620`; `wire.rs:402–439` | UTF-8 XML 1.0, ordered elements, escaped text, forbidden characters rejected before HTTP. The user's extracted helper preserves this behavior. Custom trait implementations still own XML structure/action constants; `to_wire` does not claim general XSD validation. |
| Auth forms | `credentials.rs:5–24,45–81`; `xml.rs:610–619` | Key or username then password, injected as XML; no invented HTTP Basic requirement. Credentials are not silently trimmed/lowercased. Lowercase is documented; letting the vendor reject a caller's uppercase key is not a protocol failure. |
| TLS/network | `client.rs:242–257,300–312`; crate manifest `:24–25` | HTTPS default, rustls/platform trust verification, DNS endpoint, no obsolete hardcoded IP/issuer pin or certificate bypass. Explicit HTTP endpoint override supports mocks/proxies; not a default downgrade. B6's outbound IPs concern receivers, not these replies. |
| Native cookies | `client.rs:193–212,293–331`; `wire.rs:313–340` | Default jar persists sessions; clones share it; fresh default clients get fresh jars. Caller must refresh with a fresh provider too. Exact helper handles repeated Set-Cookie, skips malformed/nonmatching pairs, preserves later `=`, accepts empty value; path/domain/expiry stay the jar's responsibility. Matches B4. |
| Redirect/timeout | `client.rs:280–312` | Native redirects disabled; correct rationale distinguishes POST-to-GET conversion from forwarding credential-bearing body. Total HTTP deadline is 60 s, including body download; synchronous XML generation/parsing is outside it. No separately required connect timeout was found. |
| Retries | `client.rs:193–207,374–405`; `recovery.md:4–9,38–42` | No application recovery loop; injected transport retains its own retries. Guidance correctly says five total sends, not initial plus five. Library cannot enforce a cross-invocation send budget on callers. |

All eleven distinct built-in action constants match B2:

| Operation | Action | Code |
|---|---|---|
| Invoice | `action-xmlagentxmlfile` | `ops/invoice.rs:679` |
| Storno invoice | `action-szamla_agent_st` | `ops/storno.rs:163` |
| Register/clear credit entries | `action-szamla_agent_kifiz` | `ops/credit_entry.rs:238,275` |
| Invoice PDF | `action-szamla_agent_pdf` | `ops/query_pdf.rs:59` |
| Invoice XML | `action-szamla_agent_xml` | `ops/query_xml.rs:536` |
| Delete proforma | `action-szamla_agent_dijbekero_torlese` | `ops/proforma.rs:61` |
| Create/storno/query/send receipt | `action-szamla_agent_nyugta_create`, `action-szamla_agent_nyugta_storno`, `action-szamla_agent_nyugta_get`, `action-szamla_agent_nyugta_send` | `ops/receipt.rs:190,341,421,503` |
| Taxpayer | `action-szamla_agent_taxpayer` | `ops/taxpayer.rs:265` |

Invoice/storno/credit/PDF emit the shared `RESPONSE_VERSION` constant at `ops/invoice.rs:746`, `storno.rs:182`, `credit_entry.rs:297`, `query_pdf.rs:75`. `ClearCreditEntries` delegates to the same writer/parser (`credit_entry.rs:237–249`), adding no transport route. The XML/PDF queries put credentials at root; the other built-in writers put them in `beallitasok`. Requests that explicitly select v2 do not require a v1 DONE/raw-PDF/plain-error decoder to interoperate.

Locked reqwest is **0.13.4**. Its source `src/retry.rs:9–17,195–202,273–317` documents/defaults protocol-level retries, with two additional retries configured; it is not a generic 5xx/API-error loop. Relevant retry branches are feature-gated. The actual isolated feature tree shows cookies/rustls, no HTTP/2 or HTTP/3. Downstream feature unification or an injected client changes this boundary. No unsafe default resend was demonstrated; “no application loop” should not become an unconditional one-physical-POST promise.

## Shared completed-response decision and fidelity

| Evidence | Actual decision | Code |
|---|---|---|
| Nonblank `szlahu_down` | ServiceUnavailable, ahead of code/status/body; blank treated as absent | `wire.rs:291–297` |
| Nonblank error header | Ordinary code is API error before HTTP/body; issuing parser separately evaluates 56 | `wire.rs:262–270,298–300`; `ops/envelope.rs:179–186` |
| Non-2xx without preceding verdict header | HttpStatus with bounded body excerpt; number/id/other headers do not bypass it | `wire.rs:301–308` |
| HTTP 200 body-only refusal | Shared `sikeres`/code/diagnostic read produces API error | `xml.rs:455–504`; `ops/envelope.rs:188–203` |
| Numbered normal success | CreatedInvoice; optional metadata from body before headers, id from header | `ops/envelope.rs:209–248,337–342` |
| Unnumbered create success | Preview only if requested and PDF present; otherwise missing-number error | `ops/invoice.rs:923–935` |
| Unnumbered storno success | Missing-number error | `ops/envelope.rs:268–279`; `ops/storno.rs:211–213` |
| Numbered 56 | Issued with notification flag; malformed optional metadata may be None, identity must remain usable | `ops/envelope.rs:205–248,285–315` |
| Header 56 plus readable non-56 body refusal | Body refusal wins even with malformed optional payload | `ops/envelope.rs:197–203` |
| Header 56 plus plain/empty body | Narrow fallback, still requires number; malformed XML excluded | `ops/envelope.rs:188–196,251–254` |
| 56 without number | API error 56, Unknown | `ops/envelope.rs:211–216`; `error.rs:375–381` |
| PDF query / credit balance | PDF query requires number and PDF; balance requires number and uses same amount/URL readers, ordinary verdict | `ops/query_pdf.rs:83–93`; `ops/credit_entry.rs:311–338` |

P2 corroborates down-first and numbered-56 behavior. I2/S2/C1 describe matching body/header data and ordinarily omit numbers/totals when error headers exist. They do **not** define the library's entire conflict-resolution matrix. A body-only error at 500 being HttpStatus rather than Api is explicit conservative policy, not a demonstrated valid-vendor incompatibility. First matching repeated header (`wire.rs:227–237`) and body-first identity are likewise defined policies without evidence of conflicting vendor emissions.

Header fidelity:

- `wire.rs:239–249,343–351`: one form-style decode for textual headers; `+` becomes space, `%2B` becomes plus, `%252B` remains `%2B`. Invalid escaped UTF-8 falls back to the plus-adjusted input. Codes/id/totals use raw access, preserving numeric `+` and leaving `%33` unknown.
- `ops/envelope.rs:120–167,344–370`: amounts use body first; nonblank malformed body values do not silently use good headers. Empty body values allow fallback. Headers accept ungrouped dot/comma, signs/exponents and outer SP/HTAB. P60-E1 specifically observed `100,01`; no grouping interpretation is invented.
- XML URLs receive XML entity decoding only; `%2B` and `+` survive. Envelope number/URL trimming is explicit local policy, separate from queried business text. Payment method is decoded from `szlahu_fizetesmod`, retaining unknown tokens (`ops/envelope.rs:318–324`); X1 defines no XML payment-method field.
- Optional document id uses nonnegative i64 or None (`ops/envelope.rs:331–342`); it is not mistaken for an account id. Complete headers become lossy-UTF-8 strings in `client.rs:394–403`; documented ASCII/URL-encoded headers fit this interface. Incomplete responses retain original bytes instead.
- `types.rs:104–117` decodes standard base64 after whitespace removal. Normal invalid base64 produces an uncertain parse error; numbered-56 optional PDF failure becomes None. No PDF format/page/signature validation is promised. Create/storno do not discard successful issuance solely because a requested PDF is absent; PDF query requires the artifact.

## Malformed-response robustness and XML limits

These checks concern adversarial/broken replies, not claims that szamlazz.hu emits them:

- `xml.rs:183–283`: complete UTF-8 document, expected expanded root, legal prolog/epilog, matching completion through EOF, no extra root/truncated tail/DTD. Tokenizer checks lexical syntax and references, including ignored extensions.
- `xml.rs:19–137`: normalized namespace declarations, reserved binding rules, undeclared element/attribute prefixes and duplicate expanded attributes checked.
- `xml.rs:285–359`: protocol namespace projected for serde; foreign subtrees cannot inject identity or verdict, even when descendants re-enter the protocol namespace. A placeholder prevents `tr<foreign/>ue` becoming `true`. Prefix aliases preserve repeated row membership/order.
- Recognized singleton duplicates and scalar child elements fail; optional malformed `hibauzenet` is independently unavailable instead of erasing a readable refusal (`xml.rs:455–481`). Passing numbered-56 regression controls explicitly distinguish bad metadata from bad identity.
- `Verdict` requires present `sikeres`. Its legacy flexible boolean reader treats empty as false (`xml.rs:750–782`), not an XSD-valid spelling; without a code that remains Absent/Unknown. No concrete false-issued result was established from this tolerance. `sikeres=true` with a body code is governed by success (`xml.rs:488–491`); documented errors use false, so a contradictory true/code input is not proof of vendor interoperability failure.
- `number.rs:61–140`, `xml.rs:629–647`, envelope readers accept finite exponent forms without f64 conversion and refuse values that cannot fit Decimal exactly. XSD `double` has a wider/nonfinite domain; the crate's finite exact-money policy is narrower by design, not full XSD value-space support. Tests establish no implicit rounding/underflow, not that every possible XSD number is accepted.
- `xml.rs:649–709` reads civil dates with complete supported timezone suffixes and character-boundary-safe splitting; nonempty invalid dates fail. This is the Agent policy, distinct from Adatkapcsolat's date-content leniency. Legacy date grammar/finite date range are not a general XSD validator.

No additional robustness finding survived the current controls and source comparison. There was no exhaustive XML conformance, memory/load, browser, TLS handshake or HTTP/2/3 interruption exercise. Entire response buffering plus copies has no explicit cap (`client.rs:387–403`), and ordinary completed parse failures do not return the full raw response. These remain capability/hardening boundaries, not demonstrated documented-exchange defects.

## Complete error-code/classification audit

All **43 named numeric codes** were compared with B5 and supplements/account observations. Mapping is `error.rs:214–317`; class selection `:375–423`; text normalization `:460–471`. `U` = Unknown, `R` = Rejected, `D` = DuplicateOrderNumber, `N` = NotFound. Only **1/55** have `is_retryable=true`; only **3/135/136/164** have `is_credential_error=true` (`:319–352`). Those are independent questions.

| Code | Rust variant | Class | Source and meaning |
|---|---|---|---|
| 1 | Maintenance | U | B5: maintenance/internal failure; wait minutes. |
| 3 | InvalidCredentials | R | B3/B5: login failure; credential code. |
| 7 | MissingData | N | C2/C3: unknown invoice selector; C6: missing email subject. Operation-dependent, not uniformly missing document. |
| 14 | StornoOfReversalInvoice | R | Account B5 observation: storno/credit original cannot itself be reversed/credited. |
| 53 | XmlNotAFile | R | B5: missing proper XML file upload. |
| 54 | EInvoiceNotEnabled | R | B5: e-invoice permission/certificate setup. |
| 55 | EInvoiceSigningFailed | U | B5: signing failed, expired certificate or inaccessible timestamp server; transient hint is not issuance proof. |
| 56 | InvoiceNotificationDeliveryFailed | U as error | P1/P2: numbered issuance warning; not observed live. |
| 57 | MalformedXml | R | B5: request XML read/XSD failure. |
| 71 | DuplicateOrderNumber | D | B5/E2: duplicate toggle refusal. |
| 73 | PrepaymentInvoiceNotIdentifiable | R | Account C6-4/5: prepayment not identifiable/already settled. |
| 135 | BrowserSessionActive | R | B5: log out of browser; credential/access classification. |
| 136 | LoginBlocked | R | B5: account/subscription/payment access blocked. |
| 152 | DuplicateOrderNumberNamed | D | B5/E2: 71 with order number in diagnostic. |
| 164 | MultipleAccounts | R | B3/B5: legacy user accesses multiple billing accounts. |
| 202 | UnregisteredPrefix | R | B5: empty/unregistered invoice prefix. |
| 221 | HasCorrectiveInvoice | R | Account B7: original with corrective cannot be stornoed. |
| 259 | NetValueMismatch | R | B5: net versus price × quantity. |
| 260 | VatValueMismatch | R | B5: VAT versus net × rate / 100. |
| 261 | GrossValueMismatch | R | B5/C7: gross versus net + VAT. |
| 262 | NetValueInvalid | R | B5: net check; EN says row number, HU product name; message retained. |
| 263 | VatValueInvalid | R | B5: VAT check naming row. |
| 264 | GrossValueInvalid | R | B5: gross check naming row. |
| 335 | ProformaNotFound | R | C4: absent/already-deleted proforma; not replayed deletion success. |
| 336 | ReceiptPrefixUsedForInvoices | R | C5: receipt prefix already used for invoices. |
| 337 | InvalidReceiptPrefix | R | C5: uppercase letters/numbers required. |
| 338 | DuplicateReceiptCallId | R | C5: reused call id refused; no original result recovered. |
| 339 | ReceiptNotFound | N | C5: nonexistent receipt number. |
| 340 | ReceiptPaymentMismatch | R | C5: tender sum differs from gross. |
| 352 | IssueDateMustBeToday | R | Account B3: storno issue date; not fulfillment date or an e-invoice-only rule. |
| 363 | ReceiptGrossNotWhole | R | B5/C7: HUF receipt whole gross. |
| 364 | ReceiptNetPrecision | R | B5/C7: HUF receipt net precision. |
| 365 | ReceiptVatPrecision | R | B5/C7: HUF receipt VAT precision. |
| 463 | PaymentOnReversedInvoice | R | Account D8: credit entry on reversed/reversing invoice, body-only. |
| 537 | ErasureCodeLimit | R | B5/C9: maximum 400 per item. |
| 538 | ErasureCodesUnavailable | R | B5: demo/test account restriction. |
| 539 | ErasureCodesDisabled | R | B5/C9: account setting disabled. |
| 551 | SimplifiedImageAccountIncompatible | R | B5/C8: OSS/non-Hungarian tax number, inherited final included. |
| 552 | SimplifiedImageItemLimit | R | B5/C8: maximum two items, four on final. |
| 553 | SimplifiedImageVatInvalid | R | B5/C8: prohibited VAT token. |
| 554 | SimplifiedImageCannotCorrect | R | B5/C8: cannot correct simplified original. |
| 555 | SimplifiedImagePrepaymentVatMismatch | R | B5/C8: final/prepayment VAT mismatch. |
| 556 | SimplifiedImageDocumentForbidden | R | B5/C8: delivery-note/corrective prohibition. |

Inventory cross-check: **30 general + 5 receipt supplement + 2 operation examples + 1 PHP warning + 5 account-observed = 43**. Observed five are 14/73/221/352/463, explicitly marked in the source. No missing mapping, wrong code round trip, or unjustified promotion of an unknown code to refusal was found.

Unknown numeric/text tokens, including above-u16 values, retain trimmed text; known padded tokens normalize (`007` to 7). Empty token maps to Absent. Unknown/Absent are U, noncredential, not affirmatively retryable. An unfamiliar NAV token stays unknown rather than inheriting a guessed Agent meaning. This is not an audit of NAV's complete error catalogue or third-party invoicing.

`ResponseError` HTTP/down/parse and `ClientError` transport/incomplete equivalents are U (`error.rs:743–761`; `client.rs:103–137`); deterministic local Request errors are R. Refusal describes **this exchange**, not an earlier interrupted send. Credential refusals are reasonably classified from B3/B5 meanings, while exact vendor sequencing is explicitly not established. The code no longer claims that repairing access guarantees the request will otherwise succeed.

## Excluded intentional deviations and unresolved evidence

| Topic | Evidence and disposition |
|---|---|
| Storno repeat and success-shaped no-op | `docs/szamlazz-hu-behaviour.md:79,86–89` records negative-total SS, repeat existing-SS echo, and unchanged proforma/delivery-note echo. Preserve. `CreatedInvoice::reverses` (`ops/envelope.rs:61–87`) correctly says heuristic; false means not established, not no reversal. Zero-original/negative-original acceptance remains unverified. |
| Storno external id | S1 describes an original reference, but recorded B6/XPRB (`behaviour.md:69–70`) attaches it to the new SS. Preserve the recorded behavior and verify original reference on reconciliation; do not alter transport/recovery based on the generic description. |
| Appearance/date behavior | P48/P73 observations (`behaviour.md:90–98`) include silently accepted mismatches and server date behavior. These are intentional account-derived semantics, not transport bugs. No mandatory positive/negative storno total is inferred from S2's illustrative sample. |
| Body-only errors and comma headers | Recorded query 7/credit 463 body-only (`behaviour.md:137–145`) and `100,01` header (`:160`) justify current handling. Generic optional header tables do not contradict this. |
| Numbered 56 | P2 gives specific first-party evidence despite I2/S2 generic “error headers omit number” prose. D6 did not trigger 56 (`behaviour.md:153,180–181`); E1 test email redirection weakens malformed buyer-email probes. Keep warning support but do not call it live-tested. |
| Timeout | 60-second policy is not mandated by docs; PHP chooses 30. A4d records ≥57 s stall and no issuance found (`behaviour.md:152`), not proven delayed issuance. `recovery.md:50–55` preserves uncertainty; elapsed time or immediate empty query is not proof of nonexecution. |
| Sparse data/number and metadata normalization | X1 makes payload optional; application requires a number for an issued result. Empty optional fields, auxiliary-id leniency, metadata precedence, finite Decimal domain and extension tolerance are explicit policies. No observed valid amount is shown lost. |
| Sample defects | Fresh I2/S2/C1 success examples contain unescaped URL `&`; I2/S2/C2 PDFs are abbreviated with `....`. `fixtures/SOURCES.md:142–170` records source preservation and labeled transformations. Refusing those literal invalid bytes is not a valid-interoperability defect. |
| Cookie revocation/precedence | B3 says key deletion is immediate; B4 discusses authenticated sessions but establishes neither key-versus-cookie precedence nor revocation of an already-open session. Fresh jars on account/key changes are ownership policy. Loopback tests prove mechanics, not vendor account selection or 90-minute expiry. |
| Text header ambiguities | Number/error URL encoding and raw totals/code are explicit. Exact payment-method/customer-URL/down encoding, literal plus, invalid escapes, repeated/conflicting header semantics need raw captures or vendor clarification. No double-decoding recommendation follows from PHP implementation details. |
| HTTP conflicts | No source specifies all combinations of status, repeated verdict headers, contradictory body/headers or incomplete transfer. Tests pin library policy, not vendor emission. |
| Browser/runtime | `lib.rs:45–70` explicitly bounds Fetch/CORS/cookie access and forbids deploying keys client-side. No browser/Cloudflare runtime or fresh wasm compilation was performed here. |
| Attachment validation | Five files and decimal 2,000,000-byte cap are conservative local policy; E1 says 2 MB without exact byte definition and allows individual omission server-side. This review found no reason to force silent partial attachment delivery instead of local rejection. Unusual filename rendering remains unverified. |

The behavior note bounds observations to one TEST account and September 3/6/7, and says the raw A–D logs are outside the repository (`:3–28,164–172`). Its historical design-consequence cells include superseded worker reasoning; those are not extra vendor evidence. Its reference to a current four-case `eszamla_semantics` test is stale: current `tests/live.rs:44–133` contains the invoice lifecycle, and `tests/probes.rs:15–67` contains the two mismatch investigations. Tests/probes were read, **not executed**. Their presence does not re-establish the account observations on this HEAD.

## Executed verification and coverage boundary

```sh
cargo test -p szamlazz-agent --locked --offline --features client-reqwest --lib \
  --test client --test custom_http_client --test response_headers \
  --test response_completion --test response_namespaces --test numeric_fidelity \
  --test error_classification --test upstream --test business_text

cargo tree -p szamlazz-agent --locked --offline --features client-reqwest -e features -i reqwest
```

| Target | Passed |
|---|---:|
| Library unit tests | 188 |
| client | 9 |
| custom_http_client | 1 |
| response_headers | 14 |
| response_completion | 4 |
| response_namespaces | 11 |
| numeric_fidelity | 6 |
| error_classification | 3 |
| upstream | 11 |
| business_text | 2 |
| **Total** | **249** |

No selected test failed or was ignored. Offline Cargo prevents dependency retrieval; loopback tests still intentionally use local HTTP. The upstream tests use the existing corpus, not fresh vendor calls. Request-outline equivalence omits empty containers/edge whitespace (`fixtures/SOURCES.md:230–240`), so it cannot prove byte equivalence or every absent/empty semantic. Success-example PDF substitutions prove local decoding, not real PDF validity.

Native client tests inject equivalent cookies/timeout/no-redirect settings without root certificates (`tests/client.rs:18–38`); they do not measure a full 60-second deadline or establish current vendor TLS trust. Default production settings were inspected directly. No new test or scratch source was needed to validate the old candidates because the current independent adverse controls covered them.

**Final recommendation:** retain the scoped implementation and current fixed regressions. Pursue raw evidence/vendor clarification for ambiguous header/status/56/session combinations before changing semantics. Keep the accepted live-account deviations separate from documented guarantees, and keep malformed-response robustness separate from valid documented interoperability. Business-field completeness, operation-specific models and the Restate worker remain outside this report's assigned scope.
