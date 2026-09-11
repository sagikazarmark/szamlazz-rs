# Számla Agent conformance review — transport and invoice replies

**Reviewed tree:** `837dad024300e2a202c2b6351fcba73df82a7744`  
**Review / fresh official-source acquisition:** 2026-09-11

## Result

**No actionable current conformance defect was established in the owned scope.**
Finding count: **0 P0 / 0 P1 / 0 P2 / 0 P3**. The previously reported legacy-key
diagnostic leak, numbered-56 identity fallback defect, and overstated HTTP/auth
claims are fixed at this HEAD. This conclusion follows an independent current-tree
inspection and fresh official-source comparison; historical reports were read
afterward as leads, not used as the specification or carried forward as findings.

**248 repository unit/integration tests passed**, with no failures or ignored
tests in the selected targets. No live Számla Agent call, account query, credential
access, or further delegation was performed. This audit added only this report;
source, tests and fixtures were not modified.

The result is bounded: passing offline cases does not establish actual vendor
emission of contradictory responses, code 56, session revocation, or every XML
Schema value. Accepted policies and evidence gaps below are not counted as bugs.

## Scope and method

Full production inspection of `crates/szamlazz-agent/src/client.rs`,
`credentials.rs`, `wire.rs`, `xml.rs`, `error.rs`, and `ops/envelope.rs`;
invoice/storno **response** entry points in `ops/invoice.rs` and `ops/storno.rs`.
Supporting inspection included `number.rs`, `types.rs`'s PDF implementation,
`recovery.md`, platform/README parsing promises, the manifest, operation action
constants, focused tests, `docs/szamlazz-hu-behaviour.md`, and `fixtures/SOURCES.md`.
Invoice/storno business request fields and complete operation-specific models are
outside this slice. Multipart framing and response-version selection are included.

Code citations below are HEAD line ranges relative to
`crates/szamlazz-agent/src/`, unless a repository path is written explicitly.
HEAD was checked at entry; the worktree was clean. After testing,
`git diff --exit-code 837dad024300e2a202c2b6351fcba73df82a7744 -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock`
passed. Final verification retained the same HEAD and unchanged reviewed paths;
an independently authored untracked `2026-09-11-agent-api-837dad0-invoices.md`
appeared alongside this report and was not inspected or edited. This is a full
scoped-tree audit, not an empty HEAD-to-HEAD diff review.

## Fresh official source inventory

The following sources were retrieved during this review. The documentation site
displayed **`v202608271632`**, a build label, not the acquisition date of each
example. Quoted phrases identify published requirements separately from inference.

| Ref | Official source | Relevant evidence |
|---|---|---|
| B0 | [Basics index](https://docs.szamlazz.hu/agent/category/basics), [what it is](https://docs.szamlazz.hu/agent/basics/what-is), [how it works](https://docs.szamlazz.hu/agent/basics/how-does) | Seven basics pages; eleven operations; proforma/delivery-note creation has “no separate endpoint”; XML POST and PDF/email are distinct paths. |
| B1 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | “same URL every time: `https://www.szamlazz.hu/szamla/`”; function selected using the “name of the form field”; “HTTPS POST”; XML file; case-sensitive tags; one document per creation XML. |
| B2 | [Authentication](https://docs.szamlazz.hu/agent/basics/authentication) | Key in `szamlaagentkulcs`; legacy key in both username/password fields supported; keys “only in lowercase”; legacy user accesses “exactly one billing account”; keep key secret, not client-side. |
| B3 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | `JSESSIONID` reuse; expiry when “inactive for 90 minutes”; no-cookie requests authenticate again; fresh session after company/email edits is “advised”; file persistence is “advisable.” |
| B4 | [Network and security](https://docs.szamlazz.hu/agent/basics/security) | Destination CIDRs versus outbound partner-call IPs. No exact client timeout, HTTP-status matrix, or browser CORS guarantee stated. |
| B5 | [Error table EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Same request “at most five times” / “legfeljebb ötször”; stop for human intervention, no until-success loop; 500 test invoices/10 minutes; v1 text-error format; 30 general numeric codes. |
| I1 | [Create request](https://docs.szamlazz.hu/agent/generating_invoice/request) | POST `multipart/form-data`, main file `action-xmlagentxmlfile`, optional `attachfile1`…`attachfile5`. |
| I2 | [Create response](https://docs.szamlazz.hu/agent/generating_invoice/response) | Version 2: “Structured `xmlszamlavalasz` with optional base64 PDF”; number/error text URL encoded, amounts/code not URL encoded; payment-method/customer-URL headers; structured success/refusal. |
| S1 | [Storno request](https://docs.szamlazz.hu/agent/reversing_invoice/request) | Same endpoint/method/content type; `action-szamla_agent_st`. |
| S2 | [Storno response](https://docs.szamlazz.hu/agent/reversing_invoice/response) | Same envelope/header contract; number is the storno invoice's; structured examples now present. |
| X1 | [Downloaded reply XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | `elementFormDefault="qualified"`; required singleton boolean `sikeres`; optional singleton strings, `double` totals and `base64Binary` PDF. Agrees structurally with I2/S2's inline reply schemas. |
| E1 | [Invoice email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | Five attachments, 2 MB each; bad files may be omitted while valid ones are sent. Test notification goes to the account-configured email, not XML buyer email. |
| O1 | [Order-number rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Duplicate toggle; per-type checks; storno/corrective exemption; reversal frees order. Successful replay requires matching buyer/gross/three dates and issuance “within the last 2 days.” |
| C1 | [Receipt response supplement](https://docs.szamlazz.hu/agent/generating_receipt/response) | Additional 336–340; repeated call id refuses duplicate issuance, not replayed success. Inspected for shared error catalogue only. |
| C2 | [Invoice XML response](https://docs.szamlazz.hu/agent/querying_xml/response) | Unknown number/order/external selector: code 7; error envelope `xmlszamlavalasz`. |
| C3 | [Proforma deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) | Code 335 example; critical failure may return plain text/HTML. |
| P1 | [PHP index](https://docs.szamlazz.hu/php/), [basics](https://docs.szamlazz.hu/php/alapok), [response processing](https://docs.szamlazz.hu/php/valasz-feldolgozas) | Offered version 2.12.4; UTF-8 default; separate session per instance; invoice can be “successfully issued” despite notification-delivery failure. |
| P2 | [Official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Fresh download, inspected in memory with Python `urllib.request`/`zipfile`; no PHP executed or archive extracted. SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. |

P2 locations, relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `SzamlaAgentRequest.php:480–499`: POST and `CURLFile` with `text/xml`, routed by
  the file-field name; `:507–555`: cookie handling, numbered attachments, timeouts.
  `:27–30` gives its **30-second** default, not a vendor-mandated timeout.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank `szlahu_down` precedes body
  interpretation. Its exception code 500 does not establish wire HTTP status 500.
- `Response/InvoiceResponse.php:314–323,427–431`: a number plus notification-send
  error makes issuance successful. This corroborates the narrow warning policy,
  not arbitrary contradictory-body acceptance.
- `Response/InvoiceResponse.php:128–158,347–348`: header fields; raw amounts;
  `urldecode` on error text; customer URL uses `rawurldecode` on ingestion and
  `urldecode` in its getter. Copying its double decoding is not a fidelity rule.

## Transport, multipart and authentication

| Area | HEAD implementation | Assessment |
|---|---|---|
| Endpoint and verb | `wire.rs:7–14`; `client.rs:374–383` | Exact documented HTTPS endpoint, POST, generated multipart Content-Type and complete bytes. |
| XML file part | `wire.rs:66–100` | Both `name` and `filename`, `text/xml`, CRLF framing, blank line before payload and final boundary. Avoids B5 code 53's “not sent as a file” case. Filename need not end in `.xml`; HTML submit-button fields are not required selectors. |
| Boundary collision | `wire.rs:109–131` | Checks XML and every attachment's bytes; chooses bounded decimal suffix until absent. Determinism is not a collision defect. |
| Attachment framing | `wire.rs:78–107`; `ops/invoice.rs:938–948` | Raw file bytes, numbered fields; CR/LF removed from part metadata and quote/backslash escaped in disposition values. Unusual recipient filename rendering remains unverified. |
| XML writing | `xml.rs:139–161,568–620`; `wire.rs:402–429` | UTF-8 XML 1.0, escaped text, credential injection, forbidden-character refusal before HTTP. Not a general XSD validator. Custom `AgentRequest` implementations own their XML structure and action constant. |
| Credential forms | `credentials.rs:5–24,45–81`; `xml.rs:610–619` | Exact key or legacy username/password; no invented HTTP auth requirement, no trimming/lowercasing of supplied secrets. Lowercase rule is documented; invalid credentials remain vendor-owned refusals. |
| Credential diagnostics | `credentials.rs:27–30,90–99`; `client.rs:149–172,339–346` | Key and **both** legacy fields redacted, including official key/key alias. Endpoint userinfo is removed from builder/client/build-error diagnostics. Arbitrary URL path/query secrets are not promised general redaction. |
| TLS and override | `client.rs:242–257,300–312`; crate `Cargo.toml:24–25` | Default rustls/platform verification; no hardcoded IP/certificate pin or verification bypass. Explicit HTTP overrides support mocks/proxies; not a default downgrade. Vendor outbound receiver IPs are not relevant to authenticating these replies. |
| Session lifetime | `client.rs:193–212,293–331`; `wire.rs:313–340` | Native default jar; clone shares jar; fresh client gets fresh jar. Refresh advice matches B3. Exact case-sensitive `JSESSIONID` helper scans repeated Set-Cookie fields and preserves later `=`; attributes/expiry belong to the transport jar. |
| Redirects | `client.rs:293–307` | Default native redirects disabled; comments correctly distinguish method-changing redirects from credential-body forwarding. Supplied client/Fetch owns its policy. |
| Timeout | `client.rs:280–312` | Native total HTTP timeout 60 seconds; no separate connect timeout. Body transfer is included, synchronous serialization/parsing is not. No official exact timeout requirement was found; 60 rather than PHP's 30 is accepted local policy. |
| Retry behavior | `client.rs:193–207,374–405`; `recovery.md:4–9,36–55` | No application recovery loop. Injected retry policies remain active. Five-total-send guidance accurately reflects B5 and does not invent a combined write/query budget. |

All eleven built-in action constants match B1, checked directly at:
`ops/invoice.rs:679`, `storno.rs:163`, `credit_entry.rs:214`, `query_pdf.rs:59`,
`query_xml.rs:536`, `proforma.rs:61`, `receipt.rs:190,341,421,503`,
`taxpayer.rs:265`. Both owned issuing writers select `ops::RESPONSE_VERSION`
(`ops.rs:31`) at `invoice.rs:746` and `storno.rs:182`. Credit/PDF writers also use
that constant. No version-1 decoder is required for requests that always select 2.

The locked reqwest **0.13.4** source was inspected at `src/retry.rs:9–17,195–202,273–317`:
default retries are low-level protocol cases, not a general 5xx/application-code
loop. The implementation permits two additional protocol retries; relevant branches
are feature-gated. Consequently, “no application retry loop” is not an absolute
physical-transmission-count guarantee under every downstream feature composition.
No unsafe default resend was established in this audit.

Browser limits are explicit at `lib.rs:45–70`: Fetch owns cookies/redirects,
Set-Cookie is hidden, CORS/header exposure is external, and this client's requests
retain same-origin credentials. Account keys must remain server-side. No browser
runtime/CORS or fresh wasm compilation check was performed in this review.

## Completed and incomplete response interpretation

| Input/evidence | Actual HEAD policy | Assessment / reproducer coverage |
|---|---|---|
| Body transfer fails after headers | `client.rs:385–393`: `IncompleteResponse` with status, original HeaderMap and source; class `Unknown` (`:117–126`) | Preserves evidence without inventing a completed body. `tests/client.rs:232–294` uses truncated loopback HTTP with code 3, down, numbered 56 and plain-number headers; all pass. |
| Nonblank down header | `wire.rs:291–297` | `ServiceUnavailable` before code/status/body; P2 corroborates precedence. Blank down is absent. |
| Nonblank error-code header | `wire.rs:262–270,298–300`; `ops/envelope.rs:179–186` | Raw code; decoded message. Ordinary error wins before HTTP/body; 56 is judged by issuing parser. Blank code is absent. |
| Non-2xx with no preceding verdict header | `wire.rs:301–308` | `HttpStatus` before body; bare number/id/other headers do not bypass it. Does not claim which server produced the status. |
| HTTP 200 body-only error | `xml.rs:455–504`; `ops/envelope.rs:188–203` | Reads `sikeres`, code and diagnostic; body-only refusals supported. Same XML at 500 without a verdict header stays uncertain HTTP failure. |
| Ordinary successful envelope | `ops/envelope.rs:209–248` | Number required for issued result; optional totals, URL, method, id and PDF. Body before header for number/totals/URL. |
| Unnumbered create success | `ops/invoice.rs:923–935` | Preview only when requested and a decoded PDF exists; otherwise missing-number error. A numbered answer remains Issued even if preview was requested. |
| Unnumbered storno success | `ops/envelope.rs:268–279`; `ops/storno.rs:211–213` | Missing-number error, never an unnumbered reversal. |
| Numbered header/body 56 | `ops/envelope.rs:188–248,285–315` | Notification flag, optional malformed metadata can become None; unique body identity retained. Invalid/duplicate body identity remains an uncertain parse failure even with header fallback. |
| Header 56 and readable non-56 body refusal | `ops/envelope.rs:197–203` | Refusal wins; optional malformed payload/diagnostic cannot erase it. |
| Header 56 and plain or empty body | `ops/envelope.rs:188–196,251–254` | Narrow plain-notification fallback; requires number. Malformed XML does not use the fallback. |
| 56 without any usable number | `ops/envelope.rs:211–216` | API error 56 / Unknown, never settled refusal or fabricated issuance. |

The ordering above is **library policy**, not a complete conflict-resolution matrix
specified by the vendor. I2/S2 say headers and XML carry the same data, and ordinary
error headers omit number/totals. P2 establishes the specific numbered-warning
exception. No source establishes repeated conflicting verdict headers, different
header/body invoice numbers, or all non-2xx combinations. First matching header
(`wire.rs:227–237`) and body-first payload selection are therefore recorded as
defined policies, not unsupported claims that the server emits these conflicts.

Complete native headers become strings using lossy UTF-8 (`client.rs:394–403`);
incomplete native responses retain original bytes/repeated values. Documented
URL-encoded/ASCII headers fit this interface. Browser Fetch's filtering precedes
either representation; native raw-byte fidelity is not unfiltered browser access.

### Header and PDF fidelity

- `wire.rs:239–249,343–351` performs one form-style textual decode: `+` → space,
  `%2B` → literal plus, `%252B` → `%2B`. Invalid percent-UTF-8 falls back to the
  plus-adjusted input. Codes/id/money use raw lookup, so `%33` is **not** code 3
  and the `+` sign in `+1.5` is not stripped. I2/S2 explicitly specify encoding for
  number/error and no encoding for totals/code; payment-method/customer-URL/down
  exact encoding has weaker normative support. No evidence justifies double decoding.
- `ops/envelope.rs:120–167,318–370` reads body amounts before headers; nonblank bad
  XML money fails rather than silently choosing a good header. Empty XML values
  permit fallback. Header grammar permits ungrouped decimal dot/comma, sign,
  exponent and outer HTTP SP/HTAB. `100,01` is specifically observed in the behavior
  note P60-E1 (`docs/szamlazz-hu-behaviour.md:160`). `1,234` means 1.234, not grouping.
- Body URLs receive XML entity decoding only: `+`/`%2B` stay unchanged. Envelope
  number/URL trimming is deliberately separate from queried business-text fidelity
  (crate README `:262–269`). Auxiliary `szlahu_id` is optional/nonnegative i64;
  malformed values are None (`ops/envelope.rs:331–342`), not issuance failure.
- `types.rs:104–117` decodes standard base64 after whitespace removal; the result
  exposes raw bytes. No PDF-signature/page-count validator is promised. Normal bad
  base64 is a parse failure; numbered-56 optional PDF failure is None. Invoice/storno
  PDF remains optional even if requested, avoiding an invented second issuance.
- `tests/response_headers.rs` exercises one-pass decoding, body precedence, signs,
  commas/exponents, grouping refusal, header/body 56, and malformed identity controls.
  These are synthetic protocol cases, not vendor captures.

## XML, namespace, numeric and lexical assessment

`xml.rs:167–283` checks UTF-8, expected expanded root, completion through EOF,
declaration, prolog/epilog and XML lexical legality. It refuses truncation, extra
roots, outside text/CDATA/references, DTDs and malformed ignored extensions. It is
not XSD validation. Namespace declarations are normalized before binding checks;
reserved names, undeclared element/attribute prefixes and duplicate expanded
attributes are checked (`xml.rs:19–137`).

`xml.rs:285–359` projects protocol-namespace elements into canonical local names
for serde. Foreign subtrees cannot contribute identity/verdict fields even if a
descendant re-enters the protocol namespace. An ignored foreign child leaves a
placeholder, so `tr<foreign/>ue` cannot become true. Prefix aliases and interleaved
extensions preserve repeated rows; recognized singleton duplicates/scalar children
are rejected. The complete-current namespace and completion suites passed.

The shared verdict requires a present `sikeres`; normal boolean tokens true/false
and 1/0 are accepted. Its legacy flexible helper reads empty as false
(`xml.rs:741–782`), while receipt reversal uses the stricter helper. Empty false
without a code remains Absent/Unknown. This tolerance is not claimed as an XSD
lexical form and no concrete issuance-success defect follows from it. A malformed
optional diagnostic is None independently of readable verdict/code
(`xml.rs:455–481`); error messages otherwise retain decoded text.

Financial parsing uses `number.rs:61–140`, reached by `xml.rs:629–647` and envelope
money readers. Finite exponent forms are accepted without f64 conversion; values
outside Decimal's exact domain are refused rather than rounded or underflowed to
zero. Tests cover `1e-29`, overprecision, overflow, equivalent representable
spellings and retained scale. XSD `double` also admits nonfinite values and a wider
range; the finite exact-money domain is an explicit policy, not complete XSD
value-space support. No observed monetary value was shown to be lost by it.

Civil dates (`xml.rs:649–709`) keep the printed date and discard only recognized
complete timezone suffixes, validate offset bounds and avoid byte-boundary panics.
The documented legacy finite-date grammar is broader than strict XSD grammar;
optional empty dates are None and invalid nonempty dates fail. No UTC conversion,
unconditional suffix truncation, or silent numeric precision loss was found.
The Agent's policy must not be replaced with Adatkapcsolat's different content
leniency merely because the wire elements overlap.

## Error catalogue and outcome classification

Compared the full general EN/HU table and the inspected supplements with
`error.rs:38–211,214–317,319–423,457–471`:

| Source group | Codes | HEAD classification |
|---|---|---|
| General table B5 (30) | 1, 3, 53, 54, 55, 57, 71, 135, 136, 152, 164, 202, 259–264, 363–365, 537–539, 551–556 | 1/55 Unknown; 71/152 DuplicateOrderNumber; remaining codes Rejected. |
| Receipt supplement C1 (5) | 336–340 | 339 NotFound; others Rejected. 338 refuses duplicate call id without returning prior success. |
| Operation examples C2/C3 (2) | 7, 335 | 7 operation-dependent NotFound/missing data; 335 Rejected. |
| First-party PHP warning | 56 | Unknown as an error; numbered issuing result is successful with notification flag. |
| Recorded test-account observations (5) | 14, 73, 221, 352, 463 | Rejected; each named and explicitly marked observed. |

**43 named codes**, plus Unknown and Absent; no missing general-table mapping or
wrong numeric round trip established. This is not an exhaustive catalogue of NAV
or third-party-invoicing errors. Unknown text/numeric tokens, including values
above u16, retain their trimmed string and conservative Unknown classification.
Known padded numeric forms normalize (`007` → 7); `%33` stays unknown. Unknown
tokens are not treated as refusals just because most current codes are refusals.

Only 1/55 have a true potentially-transient read hint; certificate expiry still
needs remediation. Only 3/135/136/164 are credential codes. Their classification
as per-exchange access refusals is reasonable from B2/B5; code now explicitly says
exact processing order is unestablished (`error.rs:334–342`). A later refusal does
not settle an earlier uncertain send. Transport/HTTP/parse/down/incomplete failures
remain Unknown (`client.rs:103–126`; `error.rs:746–763`). No conservative uncertainty
case was promoted to a defect merely because a more optimistic parser could return
success. B5's 262 EN/HU wording differs (row number versus product name); preserving
the vendor message and neutral variant avoids dependence on that translation.

## Accepted deviations and evidence strength

| Behavior or apparent deviation | Evidence strength and disposition |
|---|---|
| Number plus code 56 despite generic “error headers omit number” prose | P1/P2 first-party documentation/source support, not live observation. Preserve the narrow exception. D6 failed to trigger 56 (`behaviour.md:153,171,180–181`); E1's test-email redirection explains why malformed buyer email is a weak trigger. |
| Repeat storno echoes existing SS | Recorded B4 test-account observation (`behaviour.md:86`); current live lifecycle checks same returned number (`tests/live.rs:105–129`). Accept as bounded evidence, not freshly executed or a guarantee for every account. |
| Storno of proforma/delivery note is success-shaped no-op | Recorded B5 (`behaviour.md:87`); response parser correctly does not invent an SS type. `CreatedInvoice::reverses` (`ops/envelope.rs:61–87`) is explicitly heuristic and false means unestablished. |
| Positive or absent storno gross | S2 example shows positive gross, but is not a matched original/reversal exchange. Parser retains it; helper says query type/reference. Negative-original/zero-original server acceptance remains unverified; zero comparison is synthetic policy. |
| Body-only query/credit errors | Recorded operation-specific observations (`behaviour.md:137–145`); absent headers must not imply success. Generic header tables are optional channels, not mandatory field presence. |
| Comma monetary headers | Specific P60 test-account observation (`behaviour.md:160`), supported by passing offline fallback cases. Preserve it even though XML money uses a dot. |
| 60-second timeout versus PHP's 30 | Local policy with a recorded ≥57-second stalled create (`behaviour.md:152`). Its query found no issuance; no linked raw proof here establishes delayed issuance. `recovery.md:50–55` accurately qualifies this. Neither time nor empty query proves nonexecution. |
| Sparse optional reply fields, unknown extensions, auxiliary-id leniency | Intentional parser policy; X1 makes all envelope payload fields optional. Required application identity is enforced separately. No blanket strict-XSD gate is warranted. |
| Broken published success samples | Fresh I2/S2 contain raw `&` in URL and abbreviated `....` base64. `fixtures/SOURCES.md:142–170` records these defects. Refusal of those exact bytes is not a conformance bug; repairs used in tests are synthetic transformations, not reconstructed live replies. |
| Cookie isolation/refresh | Direct B3/P1 guidance and native loopback mechanics. Existing-session key revocation, mismatched cookie/key precedence, actual 90-minute expiry and concurrent account-switch behavior remain unverified. |

The behavior note explicitly says raw A–D logs are outside the repository
(`:3–28`) and bounds findings to one TEST account on the recorded September dates
(`:164–172`). Its older design-consequence cells are not independent vendor facts:
some still refer to removed worker pins or imply that a delay prevents in-flight
resend. This review uses the observation columns and current `recovery.md`, not
those historical worker conclusions, as the relevant evidence.

Current `tests/live.rs` has taxpayer, HUF invoice lifecycle and proforma lifecycle;
`tests/probes.rs` has two mismatching-appearance cases. The behavior note's references
to a current `eszamla_semantics` four-case test are stale (`:22–24,304`). That is an
evidence-navigation caveat, not a production response defect. The lifecycle queries
original and reversal identity/reference, which is stronger than only asserting a
negative reply total. None of these tests was run in this audit; merely finding a
test in the tree is not evidence that it passed on this HEAD against the vendor.

`fixtures/SOURCES.md:23–39,126–140,142–228` distinguishes project synthetic/golden
data, July acquisitions, a project-modified invoice request XSD, September samples,
and inline/download disagreements. The fresh **reply** XSD X1 was inspected
separately; request-XSD disagreements are not imported into this reply assessment.
The upstream request-outline tests discard empty containers/edge whitespace and
cannot establish exact lexical or absent-versus-empty equivalence. Passing PDF
substitution tests establishes local decoding of substituted bytes, not validity
of a complete real invoice PDF.

## Historical-lead disposition at this HEAD

Read only after independent production/docs/test inspection:
`2026-09-11-agent-api-transport.md` (2ba5fb8), and the September 10 original,
current, and f83e5fd transport reports in this directory.

| Earlier lead | HEAD disposition and direct evidence |
|---|---|
| Legacy key/key alias visible in Debug | Closed: both fields redacted (`credentials.rs:90–99`); key/key unit case and client/builder regression pass (`credentials.rs:109–118`, `client.rs:414–420`). |
| Numbered-56 payload fallback hides duplicate/nested identity | Closed: `payload_result?` propagates identity failure (`ops/envelope.rs:205–209`); identity-only salvage remains confined to optional metadata (`:295–315`). Number-header-present duplicate/nested controls pass (`tests/response_headers.rs:539–585`). |
| Incomplete body erases status/headers | Closed: `client.rs:66–100,385–393`; four truncated HTTP cases pass. Remains Unknown, as required by its evidence-only promise. |
| “Never via HTTP status” / exact auth processing order / guaranteed success after repair | Closed in public error docs (`error.rs:3–7,334–342`). No new runtime classifier change needed. |
| Cookie-prefix match, comma amount loss, URL userinfo, misleading redirect rationale | Current code has exact cookie match, comma reader, URL redaction and method/body-aware rationale; relevant unit/integration cases pass. |
| Overbroad logging, browser-key, replay promises | README `:330–348,388–396`, `error.rs:78–96`, and platform docs scope these appropriately. General message/body evidence is not universally redacted or retained. |

## Verification and remaining coverage boundaries

Executed from the reviewed workspace:

```sh
cargo test -p szamlazz-agent --features client-reqwest --lib \
  --test client --test custom_http_client --test response_headers \
  --test response_completion --test response_namespaces --test numeric_fidelity \
  --test error_classification --test upstream --test business_text
```

| Target | Passed |
|---|---:|
| Library unit tests | 188 |
| client | 8 |
| custom_http_client | 1 |
| response_headers | 14 |
| response_completion | 4 |
| response_namespaces | 11 |
| numeric_fidelity | 6 |
| error_classification | 3 |
| upstream | 11 |
| business_text | 2 |
| **Total** | **248** |

No selected test failed or was ignored. Upstream corpus-dependent cases ran with
the workspace corpus present; source-level expected-failure controls are not real
vendor calls. The command did not pass `--locked`, but the subsequent explicit
comparison confirmed Cargo.lock and reviewed files unchanged. Compiled versions
included reqwest 0.13.4. An initial PHP inspection command used unavailable
`python`; repeating with `python3` succeeded and produced the cited artifact hash.

Native integration cases use loopback and mostly inject equivalent transport
settings without production root certificates (`tests/client.rs:18–38`). They
exercise `Client::send`, not live TLS/root-store connectivity or a measured full
60-second deadline. The default settings were source-inspected. No new tests or
scratch reproduction source were necessary after the candidate historical defects
were covered by passing current regressions.

Remaining limits: no live auth/session-expiry/revocation/55/56 tests; no browser
execution, HTTP/2/3 interruption matrix, exhaustive XML conformance corpus, memory
load test, real-PDF validation or complete business-field audit. The client buffers
and copies the complete response with no size cap (`client.rs:387–403`); arbitrary
complete parse failures do not return the full raw response. These are capability
and hardening boundaries without a demonstrated vendor violation here. Unknown
header encodings, contradictory channels and session precedence warrant raw
captures or vendor clarification before altering the current policies.

**Recommendation:** preserve the current transport/reply behavior and its fixed
identity/diagnostic regressions. No source change is justified by this review's
evidence. Keep live-tested exceptions bounded to their recorded observations and
keep unresolved protocol questions distinct from confirmed defects.
