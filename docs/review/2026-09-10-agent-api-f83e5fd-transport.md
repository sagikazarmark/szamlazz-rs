# Independent Számla Agent transport audit — f83e5fd

**Snapshot:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`

**Audit / fresh official-source acquisition:** 2026-09-10

**Scope:** `crates/szamlazz-agent/src/{client,wire,credentials,error,lib}.rs`, crate
`README.md`, and the included `src/recovery.md`; supporting inspection of all
eleven operation entry points and shared XML/envelope helpers.

## Result

**No confirmed current runtime defect or violation of the reviewed Számla Agent
transport specification was established.** All eleven multipart routes match the
fresh official table. All 37 distinct numeric codes in the reviewed Agent basics,
operation-response examples and receipt supplement have named mappings. The
catalogue additionally names first-party-PHP-backed 56 and five recorded account
observations, for 43 named codes in total.

**The older interrupted-body evidence-loss finding is closed on this snapshot.**
The client now returns `IncompleteResponse` with status, raw repeated headers and
the reqwest source, while retaining `OutcomeClass::Unknown`. It neither invents
an empty completed body nor promotes provisional headers to a success/refusal.
This was verified through the current repository test and fresh loopback cases.

Confirmed findings: **0 P0 / 0 P1 / 0 P2 / 0 P3**. This is a bounded audit result,
not proof that all vendor behavior or all operation XML is covered. Published
ambiguities, local policies, evidence gaps and documentation qualifications are
listed separately below; they are not inflated into demonstrated defects.

Verification: **221 repository unit/integration tests, 8 doctests and 6 scratch
checks passed**. Native build and both core-only and reqwest-enabled
`wasm32-unknown-unknown` checks passed. No live Számla Agent call was made.

## Method and ownership

- HEAD resolved to the requested full SHA. Explicit `git diff --exit-code f83e5fd`
  checks of the Agent crate, `Cargo.toml`, `Cargo.lock`, `fixtures/SOURCES.md` and
  `docs/szamlazz-hu-behaviour.md` passed before and after verification. Citations
  below are current-snapshot line numbers, not copied old locations.
- This is a snapshot-versus-official-documentation audit, not a diff review of
  `f83e5fd...HEAD`. No subagents were used.
- Read the scoped production files and README in full; traced action, auth,
  response-version and parser entry points for every operation. Inspected the
  envelope, XML writer/verdict/structural checks, tests and locked reqwest source.
- Fresh public documentation GETs established the specification. The untracked
  `2026-09-10-agent-api-current-transport.md` (fbda137) and earlier
  `2026-09-10-agent-api-transport.md` (382cf761) supplied leads/closure checklists,
  not current-code or vendor evidence.
- The previous scratch source was read, its applicable assertions compared with
  the freshly fetched sources, and **recompiled against this snapshot**. Its
  obsolete evidence-loss assertion was explicitly excluded. A new scratch check
  was authored with `apply_patch` for raw-header retention and body timeouts.
- This report is the sole repository file written by this audit. Scratch source
  and compiled artifacts are under `/tmp/opencode`. No source, repository test,
  fixture, lockfile or other review was edited. Unrelated concurrent changes in
  `CONTEXT.md` and `restate-szamlazz` were left alone.
- Paths `src/…`, `tests/…` and `README.md` below are relative to
  `crates/szamlazz-agent/`; other repository paths are explicit.

## Fresh source inventory

Docs pages displayed **`v202608271632`**, a site-build label rather than a date for
each statement. All URLs below were retrieved in this audit. Short quotes identify
what is actually published; observations and deductions are labeled separately.

### Basics, authentication and network

| Ref | Official URL | Claim checked |
|---|---|---|
| B1 | <https://docs.szamlazz.hu/agent/basics/what-is> | Lists the eleven capabilities; proforma/delivery-note creation has “no separate endpoint.” |
| B2 | <https://docs.szamlazz.hu/agent/basics/how-does> | XML file in an “HTTP POST request to `https://www.szamlazz.hu/szamla/`”; PDF receipt and vendor email are separate delivery paths. |
| B3 | <https://docs.szamlazz.hu/agent/basics/sending-requests> | Function selected “using the name of the form field”; eleven exact fields; “HTTPS POST”; one document per XML; “Always validate your XML against the XSD before sending.” |
| B4 | <https://docs.szamlazz.hu/agent/basics/authentication> | Key in `szamlaagentkulcs`; legacy username/password, or the same key in both legacy fields; “only in lowercase”; “exactly one billing account.” |
| B4 security | Same URL | “do not include it in client-side code”; keys have “identical permissions,” no per-key scope, at most 17, no expiry until deletion; deletion “takes effect immediately.” |
| B5 | <https://docs.szamlazz.hu/agent/basics/session-cookie> | `JSESSIONID`; “inactive for 90 minutes”; without storage each request authenticates again; fresh session after company-data/email edits is “advised”; file persistence is “advisable.” |
| B6 | <https://docs.szamlazz.hu/agent/basics/security> | Separates HTTPS destination CIDRs from fixed outbound addresses “from which Számlázz.hu calls its partners.” |
| B7 | <https://docs.szamlazz.hu/agent/basics/error-handling> | Same request “at most five times”; stop for human intervention; no until-success loop; maximum 500 test invoices per ten minutes; general code table and version-1 `[ERR]` format. |
| B7-HU | <https://docs.szamlazz.hu/hu/agent/basics/error-handling> | “legfeljebb ötször” confirms five total sends, not five retries after the first; same general numeric catalogue. |
| B8 | <https://tudastar.szamlazz.hu/gyik/technologiai-valtozasok-2025> | August 1, 2025 infrastructure changes; “switching from Let's Encrypt to Google Trust Services”; old three destination IPs ceased. |

### Every operation's response and supplement

| Ref | Official URL | Claim checked |
|---|---|---|
| I1 | <https://docs.szamlazz.hu/agent/generating_invoice/response> | Version 2: `xmlszamlavalasz`, optional base64 PDF. Number/error text “URL encoded”; net/gross/code “not URL encoded”; payment method and customer URL also listed. |
| I2 | <https://docs.szamlazz.hu/agent/reversing_invoice/response> | Same version/header contract; number is that of the storno invoice; structured success/error examples are present today. |
| C1 | <https://docs.szamlazz.hu/agent/credit_entry/response> | Version 2 envelope, invoice/balance headers; `sikeres=false`, code and message on failure. |
| Q1 | <https://docs.szamlazz.hu/agent/querying_pdf/response> | Version 2 base64 PDF; unknown number/order/external identifier: “error code 7.” |
| Q2 | <https://docs.szamlazz.hu/agent/querying_xml/response> | Success: full `szamla`; error: `xmlszamlavalasz`; unknown selector gives 7. |
| D1 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response> | `xmlszamladbkdelvalasz`; 335 example; “On critical error, a plain text/html error message may be returned instead.” |
| R1 | <https://docs.szamlazz.hu/agent/generating_receipt/response> | `xmlnyugtavalasz`; unique call id, otherwise call unsuccessful; repeating it “will not duplicate an existing receipt”; supplemental codes 336–340. |
| R2 | <https://docs.szamlazz.hu/agent/reversing_receipt/response> | Same envelope, “storno receipt, not the original receipt,” type `SN`; absent, already reversed and storno-receipt targets are refusals. No numeric mapping for those three messages is supplied here. |
| R3 | <https://docs.szamlazz.hu/agent/querying_receipt/response> | Result “matches the response when generating new receipts.” |
| R4 | <https://docs.szamlazz.hu/agent/sending_receipt/response> | `xmlnyugtasendvalasz`, boolean verdict; 7 example is `Hiányzó adat: emailtargy elem.`—missing subject, not missing receipt. |
| N1 | <https://docs.szamlazz.hu/agent/querying_taxpayer/response> | NAV `QueryTaxpayerResponse`; examples dated 2020-11-04 use NAV 2.0; numeric Agent 57 inside `result`; `OK` + false validity is data. Current schema link points to NAV v3.0 documentation. |

The error-list links on these response pages lead to B7's general inline table.
The receipt supplement and the 7/335 examples add distinct codes. No separate
complete error download was linked from these reviewed pages.

### Request, recovery and first-party corroboration

| Ref | Fresh source | Claim checked |
|---|---|---|
| A1 | <https://docs.szamlazz.hu/agent/generating_invoice/request> | `multipart/form-data`, main file `action-xmlagentxmlfile`, attachments `attachfile1`…`attachfile5`. |
| A2 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification> | At most five attachments, “2 MB” each; invalid attachments may be omitted while valid ones are emailed. Test-account notifications go to the configured account email, not the XML buyer email. |
| O1 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number> | Per-type repetition toggle, storno/corrective exemption, reversal frees order; successful replay requires matching buyer/gross/three dates and earlier issuance “within the last 2 days.” |
| O2 | <https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number> | Receipt toggle is separate; restriction prevents a new receipt with a previously used order number. |
| O3 | <https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency> | Explicit refusal conditions 551–556; OSS **or** non-Hungarian seller; item/VAT restrictions and final-invoice inheritance. |
| X1 | <https://docs.szamlazz.hu/agent/querying_pdf/xml> | Root-level credentials, `valaszVerzio`, then external selector. |
| X2 | <https://docs.szamlazz.hu/agent/querying_xml/xml> | Root-level credentials; order query returns the last invoice; external id is queryable, with no uniqueness promise. |
| X3 | <https://docs.szamlazz.hu/agent/sending_receipt/xml> | Omitted email block sends no email; present block without details asks for previous email; children individually optional. |
| X4 | <https://docs.szamlazz.hu/agent/generating_invoice/xml>, <https://docs.szamlazz.hu/agent/reversing_invoice/xml>, <https://docs.szamlazz.hu/agent/credit_entry/xml> | Fresh auth/settings blocks and version ordering for the other three versioned operations. |
| X5 | <https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml>, <https://docs.szamlazz.hu/agent/generating_receipt/xml>, <https://docs.szamlazz.hu/agent/reversing_receipt/xml>, <https://docs.szamlazz.hu/agent/querying_receipt/xml>, <https://docs.szamlazz.hu/agent/querying_taxpayer/xml> | Fresh request-root/auth blocks: credentials inside `beallitasok`, with no response-version selector. |
| P1 | <https://docs.szamlazz.hu/php/valasz-feldolgozas> | Invoice can be “successfully issued” despite notification failure; separate notification-error accessor. |
| P2 | <https://docs.szamlazz.hu/php/nyugta-lekerdezes> | Number/order lookup; order returns “last matching document.” |
| P3 | <https://docs.szamlazz.hu/php/dijbekero-torles> | “multiple proforma invoices can be deleted” by order; member failure causes rollback. |
| P4 | <https://docs.szamlazz.hu/php/> | Current offered package: 2.12.4, 2026-08-12. |
| P5 | <https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip> | Fresh download, inspected in memory; exact number-plus-56 and down-header logic below. |
| T1 | <https://docs.szamlazz.hu/third-party-invoicing/>, <https://docs.szamlazz.hu/third-party-invoicing/issue-invoices-as-delegate> | Corroborates `credentials.rs`'s legacy-auth use case: “Nothing changes in the XML”; delegated calls require dedicated username/password; account key does not carry delegated relation. Also exposes a catalogue boundary discussed below. |

P5 SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`.
Under `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`:

- `Response/InvoiceResponse.php:17`: `INVOICE_NOTIFICATION_SEND_FAILED = 56`.
- `:319–322`: `if ($this->hasInvoiceNumber() && $this->hasInvoiceNotificationSendError())`
  clears the error verdict; its comment expressly says invoice issuance succeeded.
- `:136–137,347–348`: customer URL goes through `rawurldecode` at ingestion and
  `urldecode` at its getter. This is first-party implementation evidence, not an
  instruction to copy double decoding into Rust.
- `Response/SzamlaAgentResponse.php:147–152`: nonblank `szlahu_down` checked first.

Web pages were fetched with `webfetch`; X4/X5 were additionally fetched with
Python `urllib.request` and an `HTMLParser` printing the relevant example/auth
blocks. The ZIP was downloaded with `urllib.request`, hashed, and inspected with
`zipfile` in memory. No vendor package code was executed.

## Coverage inventory: all eleven routes

The common target is `https://www.szamlazz.hu/szamla/` (`src/wire.rs:7–14`).
`Client::send` builds the wire request and sends its bytes with POST and its
generated Content-Type (`src/client.rs:374–383`). All action fields below match
B3 exactly. Every root uses `http://www.szamlazz.hu/{root}` as its namespace.

`B` = credentials inside `beallitasok`; `R` = directly under root. `—` means no
version selector, not version 1. Version 2 is the single constant
`src/ops.rs:27–31`, used by exactly four writers.

| Operation | Multipart form field | XML root | Auth / version | Current source (writer / parser) |
|---|---|---|---|---|
| Invoice family | `action-xmlagentxmlfile` | `xmlszamla` | B / 2 | `ops/invoice.rs:679,737–755,927–948` |
| Invoice storno | `action-szamla_agent_st` | `xmlszamlast` | B / 2 | `ops/storno.rs:163–183,211–213` |
| Credit entry | `action-szamla_agent_kifiz` | `xmlszamlakifiz` | B / 2 | `ops/credit_entry.rs:214–251` |
| Invoice PDF | `action-szamla_agent_pdf` | `xmlszamlapdf` | R / 2 | `ops/query_pdf.rs:59–93` |
| Invoice XML | `action-szamla_agent_xml` | `xmlszamlaxml` | R / — | `ops/query_xml.rs:536–588` |
| Proforma deletion | `action-szamla_agent_dijbekero_torlese` | `xmlszamladbkdel` | B / — | `ops/proforma.rs:61–87` |
| Receipt creation | `action-szamla_agent_nyugta_create` | `xmlnyugtacreate` | B / — | `ops/receipt.rs:190,223–231,293–295` |
| Receipt storno | `action-szamla_agent_nyugta_storno` | `xmlnyugtast` | B / — | `ops/receipt.rs:341–365` |
| Receipt query | `action-szamla_agent_nyugta_get` | `xmlnyugtaget` | B / — | `ops/receipt.rs:421–450` |
| Receipt email | `action-szamla_agent_nyugta_send` | `xmlnyugtasend` | B / — | `ops/receipt.rs:503–532` |
| Taxpayer | `action-szamla_agent_taxpayer` | `xmltaxpayer` | B / — | `ops/taxpayer.rs:266–285` |

All paths above are under `src/`. Recompiled scratch checks exercised every row
with both authentication variants, XML escaping of `&`/`<`, main-file disposition,
filename, declaration, namespace, auth placement, response-version presence and
closing boundary. They also exercised every parser's header/down/status and
body-error channels. These are local contract checks, not evidence that the vendor
emits each synthetic code on every route.

### Shared transport assessment

| Area | Current code / result |
|---|---|
| File framing | `wire.rs:66–100`: XML is a **file** part (`filename` present), `Content-Type: text/xml`, CRLF part boundaries and final `--boundary--`. Matches A1/B3 and code 53. Routing is by form name; no `.xml` filename extension requirement was found. |
| Attachments | `ops/invoice.rs:405–500,938–948`: at most five, 2,000,000 bytes each, `attachfile1` onward, original content bytes. Decimal interpretation of “2 MB” is explicit policy. Existing exact multipart/count/size tests passed. |
| Framing safety | `wire.rs:102–125`: chooses a boundary absent from XML/file contents; numeric suffix stays under 70 characters. Attachment disposition strips CR/LF and escapes quote/backslash; MIME value strips CR/LF (`:80–92`). Built-in actions are trusted constants. Custom `AgentRequest` implementations own their action/structure. |
| XML/auth bytes | `xml.rs:19–40,443–453,484–494`: UTF-8/XML declaration, escaped values, exactly one credential variant, correct element order. `wire.rs:402–429` performs validation then refuses forbidden XML characters/encoding before HTTP. It is not an XSD validator. |
| Credential policy/privacy | `credentials.rs:5–24,45–98`: lowercase documented, no silent normalization, invalid/empty keys delegated to vendor auth. Debug hides key/password; legacy username visible. `WireRequest` hides body (`wire.rs:43–50`). B4/T1 support both auth variants. No key lifecycle/scope restrictions are invented. |
| Endpoint | `client.rs:182–190,242–257`: parses with transport URL parser, accepts HTTP(S) with host, default remains HTTPS. Custom HTTP/mock/proxy and userinfo support are deliberate. URL userinfo removed from builder/client/build-error diagnostics (`:149–172,267–274,339–346`). Scratch connection-refused diagnostics also showed no userinfo/key. Query/path secrets are not covered. |
| TLS/network | Manifest `:24–25` enables reqwest rustls/platform verification. Default client has no verification bypass, certificate pin or IP pin. B6/B8 outbound partner addresses do not authenticate Agent responses; firewall policy is the deployment's. No live TLS test. |
| Sessions | `client.rs:193–212,293–331`: native default in-memory jar, clone shares it, new default client gets fresh jar; injected jar ownership explicit. Fresh session after company/email edits matches B5. Fresh jar on account/key changes is correctly labeled caller policy. No disk-persistence or automatic-refresh requirement invented. |
| Cookie helper | `wire.rs:313–340`: first exact case-sensitive `JSESSIONID`, repeated headers scanned, malformed/nonmatching entries skipped; preserves later `=`, allows empty value, strips HTTP SP/HTAB. Jar owns attributes/lifetime/domain/path; default native client does not use this lossy-attribute helper. Current tests passed. |
| Redirects | `client.rs:303–307`: native default `Policy::none()`. Fresh actual-default 302/307/308 loopback checks returned `HttpStatus`; redirect target received zero connections. Covers both method-changing and body-preserving redirects. Injected clients/Fetch own their behavior. |
| Deadlines | `client.rs:280–311`: 60-second native request timeout, no corresponding default on wasm. Fresh injected 30-ms timeout before headers became `Transport/Unknown`; injected 200-ms body timeout after headers became `IncompleteResponse/Unknown`. Serialization/parsing CPU is outside the HTTP deadline. No vendor-mandated exact duration found. |
| Retries | No application retry loop in `send`. Injected retry policy remains active (`client.rs:199–202`, `recovery.md:4–9`). Locked reqwest 0.13.4 `src/retry.rs:9–17,195–203,273–317` describes default protocol-NACK retries, not arbitrary timeout/status resends. The isolated native build enabled cookies/rustls, not HTTP/2 or HTTP/3; unified downstream features can change this. No unconditional one-send/one-transmission guarantee is made. |
| Buffering | `client.rs:385–403`: buffers complete body and copies to `Vec`; no response-size cap or streaming. Timeout is not a memory bound. Capability/hardening limitation, not a measured failure or documented-protocol violation. |
| Header capture | Complete response: name/value pairs, lossy UTF-8 value conversion (`client.rs:394–402`). Incomplete response: original reqwest `HeaderMap`, raw bytes/repetitions retained (`:385–393`). `RawResponse` lowercases names and expressly takes first matching value (`wire.rs:186–199,227–237`). No published conflicting-duplicate rule found. |
| Text encoding | `wire.rs:239–249,343–351`: one form-style decode, `+` → space, `%2B` → plus; code/id/totals stay raw. Invalid percent-UTF-8 returns plus-adjusted original input. `ops/envelope.rs:321–326` also decodes payment method. Number/error encoding explicit in I1/I2/C1; other textual headers less specified. |
| Money and URL fallback | `ops/envelope.rs:120–168,334–374`: body before headers; decimal-comma headers; malformed nonblank XML amount not hidden by header. XML customer URL is entity-decoded only. Auxiliary id malformed/negative → None. `response_headers` tests passed. |
| HTTP/in-band policy | `wire.rs:291–310`: nonblank down, nonblank error code, known non-2xx, then body. Header 56 judged by operation. Bare success-number/id does not defeat non-2xx. Body-only 7 is API error at 200, `HttpStatus/Unknown` at 500. Status origin is not inferred. |
| Version/response bodies | Four versioned operations request 2, so no general version-1 PDF/text decoder needed. XML query recognizes document vs error envelope; receipt operations share receipt envelope; delete/email use own verdict roots; taxpayer uses NAV result. Header-only error handling is not substituted for body errors. |
| Numbered 56 | `ops/envelope.rs:179–257,288–318`: a readable number required; optional invalid metadata may be dropped. Complete body refusal beats provisional 56; malformed XML cannot use plain-notification fallback. Unique body-only identity survives optional payload structural failure. Credit does not tolerate 56 as success; PDF query still requires PDF (`query_pdf.rs:83–93`). |
| XML completion | `xml.rs:63–194`: validates completion through EOF, root/namespace, declaration and lexical grammar; DTD refused. `:196–255` filters foreign subtrees without concatenating separated scalar text. New malformed-XML-under-56 tests passed. Full XML/XSD conformance is outside this transport audit. |
| Logging | `RawResponse` Debug hides Set-Cookie values and body, leaves business headers (`wire.rs:155–179`). Incomplete-response Debug lists header names only (`client.rs:93–100`). `README.md:367` correctly limits bounded excerpts to HttpStatus/UnexpectedBody; API/parser text is not universally sanitized/truncated. |

## Closed prior finding: interrupted-body evidence retention

**Prior severity:** P2 robustness/evidence loss (R1 in the 382cf761 report, T1 in
the fbda137 report). **Current disposition: closed; high confidence.**

Current code locations:

- `src/client.rs:66–100`: public `IncompleteResponse { status, headers, source }`,
  with a source chain and value-redacting Debug.
- `:117–126`: classification explicitly remains `Unknown`.
- `:385–393`: capture status/headers before `.bytes().await`, retaining them on
  body failure rather than returning bare `Transport`.
- `README.md:364,374–377`: evidence semantics and breaking error-variant change.

**Fresh reproduction, not an inference from the commit message:**

1. Ephemeral loopback TCP listener consumes the entire multipart POST according
   to its Content-Length.
2. Sends a complete HTTP header section claiming a 1000-byte body, with one body
   byte, then closes; a separate case keeps the body open beyond an injected
   200-ms deadline.
3. Current repository test `tests/client.rs:233–293` checks four independent
   header sets: credential code 3, down, numbered 56, and bare number. It passed.
4. New `/tmp/opencode/f83e5fd-transport-evidence.rs` checks actual-default native
   truncation at HTTP 200 and 503, plus the injected body-timeout case. Each sends
   repeated number headers `I%2B2` / `I-3`, two Set-Cookie headers and `x-opaque`
   containing byte `0xff`.
5. All three returned `IncompleteResponse/Unknown`, exact status, both repeated
   values, undecoded `%2B`, byte-exact `0xff`, source-chain access and expected
   timeout classification. Debug/Display exposed neither cookie secret nor
   document-number values. The default cases required no injected HTTP client.

I1/I2 establish number/error header channels; P1/P5 establish the numbered-56
exception for interpreted responses. **None specifies interrupted HTTP bodies.**
Retaining evidence without declaring a verdict is therefore a well-defined local
policy, not a new vendor guarantee. No partial body is retained, by documented
design. Before-header failures still have no header evidence to retain. Ordinary
complete-body parse failures likewise do not generally return the raw response;
this fix is specifically the interrupted-transfer boundary.

### Other prior findings and recent regression leads

| Lead | Current disposition / independently checked evidence |
|---|---|
| URL userinfo leaks through diagnostics | Closed for reported surfaces: `client.rs:149–172,242–274,339–346`; unit plus freshly recompiled build/transport-error redaction checks passed. |
| Universal bounded/log-ready error promise | Closed by corrected scope at `README.md:367`; scratch API Display was 23,021 bytes, unexpected-root Display 459 bytes. Current prose explicitly permits full API/parser text. |
| Browser discussion omits key-deployment prohibition | Closed: `lib.rs:67–70`, `README.md:319` match B4. Actual wasm compilation now checked in this audit too; not a CORS/runtime test. |
| Unqualified identical-resend/replay advice | Closed: `client.rs:56–61` says “can issue”; `error.rs:85–90` includes matching conditions/two-day window. O1 independently corroborates the qualification. |
| Optional payload failure hides a body refusal beneath 56 | Closed: `ops/envelope.rs:188–212`, `tests/response_headers.rs:431–450` passed. |
| Malformed XML promoted by numbered-header-56 fallback | Closed for exercised cases: `ops/envelope.rs:188–196,254–257`; `tests/response_headers.rs:452–471` passed. |
| Body-only number lost on numbered-56 optional structural failure | Closed for unique readable identity: `ops/envelope.rs:298–315`, `tests/response_headers.rs:473–502` passed; duplicate/nested/foreign identity controls remain errors without header fallback identity. |
| Cookie-prefix matching, comma totals, HTTP precedence | Current implementation and `response_headers`/unit checks confirm exact cookie matching, comma parsing and down/code/status/body order. No old finding count carried forward. |

## Error catalogue and classifications

Current tables: `src/error.rs:217–316`; retry/credential flags `:318–350`;
classification `:352–452`; parsing/canonicalization `:455–493`.

The independently enumerated source groups are:

| Source | Distinct codes | Current classification |
|---|---|---|
| B7/B7-HU general table (30) | 1, 3, 53, 54, 55, 57, 71, 135, 136, 152, 164, 202, 259, 260, 261, 262, 263, 264, 363, 364, 365, 537, 538, 539, 551, 552, 553, 554, 555, 556 | 1/55 Unknown; 71/152 DuplicateOrderNumber; remaining codes Rejected. |
| R1 receipt supplement (5) | 336, 337, 338, 339, 340 | 339 NotFound; others Rejected. 338 refuses another issuance without replaying prior success. |
| Q1/Q2/R4 and D1 examples (2) | 7, 335 | 7 NotFound with operation-dependent meaning; 335 Rejected. |
| P1/P5 (1) | 56 | Unknown when surfaced as an error; invoice-number-conditioned warning exception in issuing parsers. |
| Recorded account observations (5) | 14, 73, 221, 352, 463 | Rejected; sources below. |

All **43** have named variants and canonical reverse tokens. Recompiled scratch
enumeration checked each, padded numeric spelling, flags and classification.
Only 1/55 are potentially transient read hints; only 3/135/136/164 are credential
codes. Unknown text/numeric tokens, numbers above `u16` and `%33` remain Unknown;
blank is Absent/Unknown. Numeric `007` normalizes to 7. Unknown codes are not
silently assumed to be refusals. `ClientError::Request` is Rejected because it
precedes HTTP; transport, incomplete, parse, down and HTTP-status failures are
Unknown (`client.rs:117–126`, `error.rs:744–761`).

Named meanings were checked against the sources, including 551's **OR** condition,
552's final-invoice exception, 553's whole-document refusal, 554/556 restrictions,
555's matching VAT rates, receipt-only 363–365, and 537–539 erasure restrictions.
Code 7 is deliberately contextual: the receipt-email example cannot establish
absence of a receipt. Code 335 does not supply a successful delete replay. Code
55 establishes failed signing, not that a number was or was not allocated.

The five observed additions are traceable in `docs/szamlazz-hu-behaviour.md`:
14 at `:88`, 73 at `:119`, 221 at `:89`, 352 at `:90–91`, 463 at `:135`.
No new execution evidence for any of them was acquired here.

**Catalogue boundary:** the 37-code count is complete for the reviewed **Agent
basics + eleven response pages + receipt supplement**, not every page on the
vendor's site. T1 also documents delegated-invoice prefix refusals 354/356/357 and
follow-up restrictions 358/359/493/494, not named by this crate. They remain
conservative Unknown, not misreported success/refusal. Delegated onboarding,
recurring operations and the full delegated/NAV catalogues are separate surfaces;
the worker explicitly excludes delegated invoicing. The low-level credential
type's delegated-use note is valid but should not be read as comprehensive typed
coverage of that product. This is a coverage/capability boundary, not a missing
code concealed by the 37-code claim.

## Recovery advice assessment

`src/recovery.md:4–62` and `README.md:77–217,299–319,355–367` distinguish the
operations rather than treating existence of an invoice as proof of every effect.

| Operation / question | Assessment |
|---|---|
| Invoice create | Persist order/external id; serialize logical issuance; inspect newest holder's identity/type/reversal. Nonunique external ids are explicit. Empty immediate query does not authorize another send. README example sends once, leaves inconclusive observation unresolved. |
| Invoice storno | Verify original and matching SS/reference. Repeat echo and proforma/delivery-note no-op are qualified observations, not a universal replay mechanism. `CreatedInvoice::reverses` is explicitly heuristic (`ops/envelope.rs:61–86`). |
| Receipt create | Persist and retain call id before send. 338 prevents a second issue but does not recover the old number/PDF. Query known number or managed order and verify identity/type; no new call id while unresolved. R1/P2/O2 support this. |
| Receipt storno | Query original reversal; that alone does not recover SN identity or who reversed. R2 documents repeat refusal, not invoice-style success. Storno-specific call-id/338 behavior remains explicitly unestablished. |
| Reads | New observation, not an external document creation. Absence is selector/operation-dependent and does not settle an earlier send. |
| Credit entries | Query current entries/balance. Additive resend can double amounts; replacing resend can overwrite intervening state. Intent must be reconciled. |
| Proforma deletion | Order selection may delete multiple matches, including newer ones on a repeat; query intended numbers, distinguish consumption from deletion where possible. P3 supports batch scope. |
| Receipt email | Receipt existence does not prove delivery; another send may duplicate email. X3 distinguishes omitted block from empty-present resend; partial-field merging is not promised. |
| Budget / timeout | Five total sends including first, human intervention, no tight loop. Vendor wording does not define a combined write-plus-query budget. Timeouts do not cancel work; elapsed time alone is not proof. |

Neither error classification nor fixed delays settle earlier uncertain writes.
`recovery.md:50–55` correctly distinguishes the detailed stalled/no-issuance
observation from the broader delayed-issuance assertion lacking a linked probe.
The library makes no durable-storage, cross-invocation lock, unresolved-write-marker
or global rate-limit guarantee. Those are caller concerns, not advertised missing
transport features.

## Documentation ambiguities, policies and evidence gaps

These are **not confirmed runtime findings**. Each would need clarification,
additional evidence or an explicit policy change before being promoted to one.

| Topic | Exact boundary / potential documentation qualification |
|---|---|
| Credential sequencing | `error.rs:335–338` says codes arrive “before it looks at the request (its documentation…)” and “the same request succeeds once the account is fixed.” B4/B7 establish authentication failures, not that exact internal sequence or that unrelated request validation must then succeed. Treat pre-action as authentication-semantics inference; account repair permits processing but cannot guarantee success. Behavior notes `:255–261` expressly lack live credential-code evidence. No contrary execution was demonstrated; per-exchange Rejected remains reasonable. |
| Absolute HTTP wording | `error.rs:3–5` says “never via HTTP status codes.” Better scoped as **application errors are in-band**. Current code/README already handle HTTP failures conservatively. The fetched docs do not establish an absolute all-status guarantee. |
| Redirect rationale | `client.rs:296–298` says endpoint “never redirects” and following one converts POST to GET. No such universal vendor guarantee was found; 307/308 preserve method/body. Default refusal is correct and tested, including those statuses; the rationale could mention credential/body forwarding as well as method conversion. |
| Number-plus-error tension | I1/I2 say “If error codes are present… invoice number and amounts are omitted”; P5's more specific number-plus-56 exception contradicts that generalization. Keep the narrow exception. No account probe triggered 56 (`behaviour.md:153,180–181`). |
| Textual header encoding | Number/error URL encoding is explicit; payment-method, down/customer-URL exact encoding and literal-plus semantics are not equally specified. PHP's mixed decoding is not proof of a universal double-encoding contract. Rust's one-pass policy has local tests; need raw captures for a stronger claim. |
| Header/status conflicts | Down/code/status/body precedence, first duplicate-header value and body-before-header payload selection are explicit local policies. No vendor rule for contradictory channels/duplicates was found. Synthetic contradiction tests do not establish real vendor emission. |
| Incomplete evidence on wasm | `IncompleteResponse` retains the HeaderMap **reqwest exposes**. Browser Fetch filters/combines response headers and hides Set-Cookie before Rust sees them (`reqwest src/wasm/client.rs:259–279`). “Original bytes/repeated values” at `client.rs:85–86` is physically established for native HTTP, not unfiltered wire access in browsers; general platform docs correctly explain the boundary. |
| Key deletion versus session | B4 says deletion is immediate; B5 says cookie reuse skips authentication. Existing-session invalidation and conflicting cookie/key precedence are unspecified. Fresh jar is caller isolation policy, not proof that the vendor invalidates old cookies. |
| Browser / server wasm | Both configurations compiled. No browser/Cloudflare Worker runtime, CORS, exposed-header or cross-origin credential test was performed. `lib.rs:59–70` and README `:317–319` correctly separate compilation from feasibility and prohibit shipping account keys to browser code. |
| Large bodies and complete parse errors | Full buffering/no cap is explicit implementation scope. Retaining every complete parse-failure response could improve reconciliation diagnostics, but is not what the interrupted-transfer change promises. General payload validation still rejects malformed non-56 metadata; do not claim every failure retains a number/body. |
| Strictness | B3 advises XSD validation; the crate checks selected request rules and XML representability, not a complete XSD. Optional content/unknown extensions and stricter lexical checking are deliberate parser policies. This audit does not certify every XML name/namespace constraint or operation field. |
| Local code 7/receipt storno | R1 gives 339 for unknown receipt number, while R2 describes three “Missing data” storno messages without codes. The tests' synthetic 339 storno case establishes parsing, not a vendor mapping for those specific messages. |
| Receipt identity | Call-id scope/retention, query-call-id semantics, precise “last” ordering and storno-specific deduplication remain unknown. Recovery prose avoids asserting them. |

### Recorded live evidence and source provenance

`docs/szamlazz-hu-behaviour.md` was read through its end. Its `:3–28,168–172`
bounds observations to one test account and September 3/6/7; original raw logs are
not in the repository (`:11–12`). This audit uses it as a recorded observation,
not an authenticated capture or fresh vendor execution.

- `:63–71`: nonunique external ids, newest holder, non-echo, no attachment on a
  replay; these justify cautious identity-based recovery, not an idempotency key.
- `:141–145`: error headers differ by operation; query 7 and credit 463 were
  body-only, while create/storno/delete examples had headers. This supports
  parsing both channels, not categorical absence on every future response.
- `:86–98`: storno echo/no-op/date/appearance behavior is account evidence;
  `:176–181` still does not establish zero-original storno or actual code-56 shape.
- `:133–135`: replacement/additive credit behavior observed; empty replace not
  probed. `:160` records decimal-comma header `100,01`.
- `:152`: ≥57-second stalled create followed by no issuance found; no raw proof
  here that a timed-out create later landed. Do not infer a safe negative-settlement
  time or transplant historical worker retry claims into this client.
- Several **design-consequence** cells are historical: `:63` still mentions
  checking `teszt`; `:133,152` claim old worker delay protections. They are not
  observations of a vendor guarantee. Current recovery guidance is more careful.
- A2's freshly fetched test-email routing explains why a bad XML buyer email need
  not trigger 56 on a test account. This is stronger published context than the
  alternatives conjectured at behavior notes `:171`, but it is not a new live probe.
- `fixtures/SOURCES.md:23–39,126–140,142–228` separates synthetic/golden data,
  historical acquisitions, a project-modified invoice XSD, September examples and
  schema disagreements. No cached fixture was treated as freshly downloaded.
- Fresh I1/I2/C1 examples still contain raw `&` in customer URLs; invoice/storno/PDF
  examples abbreviate base64. These are example defects, not a reason to accept
  malformed XML as ordinary success. Structured storno/credit examples now exist;
  historical “only text example” statements describe July, not today.
- No full downloadable-XSD refresh/comparison or exhaustive NAV/delegated-product
  error audit was performed. N1's NAV v3.0 link does not update its dated 2.0 examples.

## Verification actually executed

```sh
git status --short
git rev-parse HEAD
git log -5 --oneline
git diff --exit-code f83e5fd -- crates/szamlazz-agent docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock fixtures/SOURCES.md

cargo test -p szamlazz-agent --features client-reqwest --locked --offline --target-dir /tmp/opencode/current-transport-target --lib --test client --test response_headers --test error_classification --test custom_http_client --test response_completion --test response_namespaces
cargo test -p szamlazz-agent --features client-reqwest --locked --offline --target-dir /tmp/opencode/current-transport-target --doc
cargo build -p szamlazz-agent --features client-reqwest --locked --offline --target-dir /tmp/opencode/current-transport-target --message-format=json

rustc --edition=2024 --test /tmp/opencode/current-transport-fbda137.rs -L dependency=/tmp/opencode/current-transport-target/debug/deps --extern szamlazz_agent=/tmp/opencode/current-transport-target/debug/libszamlazz_agent.rlib --extern tokio=/tmp/opencode/current-transport-target/debug/deps/libtokio-6eb55a1a52deff24.rlib -o /tmp/opencode/f83e5fd-transport-checks
/tmp/opencode/f83e5fd-transport-checks --nocapture --skip interrupted_bodies_still_discard_all_header_evidence

rustc --edition=2024 --test /tmp/opencode/f83e5fd-transport-evidence.rs -L dependency=/tmp/opencode/current-transport-target/debug/deps --extern szamlazz_agent=/tmp/opencode/current-transport-target/debug/libszamlazz_agent.rlib --extern tokio=/tmp/opencode/current-transport-target/debug/deps/libtokio-6eb55a1a52deff24.rlib -o /tmp/opencode/f83e5fd-transport-evidence
/tmp/opencode/f83e5fd-transport-evidence --nocapture default_client_retains_raw_evidence_on_truncation_and_body_timeout

cargo check -p szamlazz-agent --features client-reqwest --locked --offline --target wasm32-unknown-unknown --target-dir /tmp/opencode/current-transport-target
cargo check -p szamlazz-agent --no-default-features --locked --offline --target wasm32-unknown-unknown --target-dir /tmp/opencode/current-transport-target

git diff --exit-code f83e5fd -- crates/szamlazz-agent docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock fixtures/SOURCES.md
git rev-parse HEAD
git status --short
```

| Suite / check | Result |
|---|---|
| Library unit tests | 187 passed |
| Client integration | 8 passed, including four interrupted-response header scenarios |
| Custom HTTP client integration | 1 passed |
| Error classification integration | 3 passed |
| Response completion integration | 4 passed |
| Response header integration | 12 passed |
| Response namespace integration | 6 passed |
| Doctests (including README) | 8 passed; no vendor network functions invoked |
| Recompiled prior scratch controls | 5 passed; obsolete loss assertion filtered out, not run/reported as passing |
| New scratch evidence check | 1 passed, three loopback scenarios; six imported tests filtered out |
| Native build / wasm core / wasm client | All passed |

The recompiled five scratch controls cover all eleven route/auth/error entry
points, the 43-code catalogue, endpoint diagnostics, actual-default redirect
refusal and an injected pre-header timeout, plus scoped diagnostic bounds.
They were not rewritten to imitate the current implementation. The new retention
check adds raw byte/repetition evidence and a post-header timeout. No selected
repository test failed or was ignored. An initial `rustup target list --installed`
attempt failed because `rustup` is unavailable; direct Cargo cross-target checks
subsequently succeeded, so this was a tooling-discovery limitation, not a wasm
compilation failure.

Locked dependencies actually compiled include reqwest **0.13.4**, quick-xml
**0.42.0**, xmlparser **0.13.6**, rust_decimal **1.43.0**, tokio **1.53.1** and jiff
**0.2.35**. Native-only crate build artifacts were identified from Cargo output;
old rlibs/binaries were not silently used as current evidence.

**Not executed:** live tests or account queries (including read-only ones), full
workspace tests, real 60-second timeout, live auth/revocation/session-expiry/TLS
probes, browser/Worker runtime tests, malformed-transfer cases for every HTTP
version, memory-load testing, exhaustive operation-field/XSD/NAV validation.
Public documentation shows what is published now; offline tests show how this
snapshot handles the supplied inputs. Neither substitutes for those missing
execution observations.
