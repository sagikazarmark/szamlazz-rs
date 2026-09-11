# Számla Agent review: non-create invoice mutations

**Reviewed:** 2026-09-11, `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.
**Result:** the initial review identified one P3 documentation defect and one P3 vendor-contract clarification, with no confirmed P0–P2 implementation defect. **Parent/adjudication update:** numberless acknowledgement is promoted to an actionable contract gap; actual vendor emission remains unobserved. Request field coverage and ordering match the freshly retrieved mutation schemas, including the deletion request download verified in the follow-up. Explicit clearing has recorded execution evidence.

## Scope and evidence discipline

- `HEAD` equalled the requested starting commit. `git diff 28dcec1456cc08d50089ed8f9c9d15f877c7c3d2...HEAD` and the corresponding commit range were empty. This is a full implementation-versus-protocol audit at that revision, not a claim about regressions introduced after it.
- Primary files: `crates/szamlazz-agent/src/ops/{storno,credit_entry,proforma,envelope}.rs`; supporting paths: `xml.rs`, `wire.rs`, `error.rs`, `client.rs`, `types.rs`, `recovery.md`. Line citations below refer to this revision and use these directory-relative names.
- Fresh public documentation GETs only. No authenticated requests, credentials, live tests, product edits, or delegation. The parent review owns the broad Cargo suite.
- Existing reviews, including the eight initially untracked `77d53c5` reports, were left untouched and were not read as evidence. Links to reviews inside the research documents were not followed. Their conclusions are not premises of this report.
- Recorded execution facts come from `docs/szamlazz-hu-behaviour.md` and the dated clearing record. Historical raw logs are explicitly outside the repository; those rows are bounded recorded observations, not independently replayed HTTP captures. Test source establishes implementation intent/coverage, never vendor execution.
- Fresh pages displayed documentation build **v202608271632**. An XSD admitting a shape does not prove that the server emits it on a successful operation; first-party PHP source is corroboration, not live evidence.

## Prioritized findings

### M-01 — P3, high confidence: shared code-56 documentation promises a success conversion that credit operations do not perform

**Code:** [`error.rs:369–374`](../../crates/szamlazz-agent/src/error.rs#L369) says “56 surfaces as an error only when the response carries no document number (with one, the parsers report success with `notification_delivery_failed` set).” That statement is not true across the crate's operations.

`RegisterCreditEntry::parse` uses `xml::valasz` ([`ops/credit_entry.rs:316–343`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L316)); clearing delegates to that parser (237–249). `xml::verdict_text` refuses header errors and a false body verdict before decoding the balance (`xml.rs:528–553`). `InvoiceBalance` has no notification flag (credit_entry.rs:252–272). Consequently a numbered code 56 is still `ResponseError::Api(InvoiceNotificationDeliveryFailed)` for both registration and clearing.

**Offline reproduction:** a complete HTTP-200 `xmlszamlavalasz` containing `sikeres=false`, `hibakod=56`, and `szamlaszam=SS-1` returned an API error for register/clear and a numbered warning-success for storno. No live credit operation producing 56 is established.

**Why it matters:** consumers reading the shared error documentation can incorrectly assume that every surfaced 56 lacks reported identity, or that the same success conversion applies to credit mutations. The actual `Unknown` classification is conservative and appropriate: an invoice number alone does not prove a credit mutation completed.

**Recommendation:** qualify the promise as applying to the issuing-envelope parser and its accepted complete/status/header shapes. Also avoid saying “only” about every parse failure: malformed identity or an HTTP-500 body-only 56 can remain errors even with number text present. Do not broaden the credit parser's success exception without operation-specific evidence.

### Q-01 — P3 clarification, high confidence in the gap; unproven production occurrence: successful credit replies require an undocumented nonblank echo

**Code:** [`ops/credit_entry.rs:319–322`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L319), shared by `ClearCreditEntries` through 247–249; reported identity resolution in `ops/envelope.rs:120–133,326–329`.

Both EN/HU response pages describe `szamlaszam` as optional and extra headers as data that “may” arrive. The inline response XSD requires only `sikeres`; a complete bare success passes both language schemas:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

With no number header, register and clear return `ParseError::Missing("szamlaszam")`. This was reproduced through the current public parser and independently validated against freshly extracted schemas.

**Initial review classification:** clarification, not a confirmed observed vendor-compatibility failure. The XSD covers successes and failures together. Official successful examples are numbered; D7 and both September 11 clearing executions were numbered. A schema-valid numberless success has not been recorded from szamlazz.hu. Conversely, those observations do not establish a universal success-specific echo guarantee. The unsent clarification draft explicitly asks for that guarantee (`docs/research/2026-09-11-agent-vendor-clarification.md:18–50,118–123`); it is not a vendor answer.

**Parent/adjudication final classification (follow-up):** promote numberless acknowledgement to an **actionable contract gap**: the parser imposes a success requirement absent from the published contract. This supersedes the initial clarification-only disposition, while preserving the distinction between a reproduced contract gap and unobserved actual vendor emission. The recorded numbered successes and pending vendor question remain relevant research evidence.

**Impact if the shape occurs:** a completed mutation appears uncertain to the caller. Blind additive repetition could duplicate entries; replace/clear could overwrite intervening entries. The error is conservatively `Unknown`, not proof of refusal. If the vendor confirms numberless success, model optional **reported** identity separately from the requested target; never manufacture an echo from the request.

The storno schema has the same optionality, and `parse_issued` likewise requires a number (`envelope.rs:275–279`). For storno, a number is also needed to distinguish an `SS` from the observed unchanged-document echo. A bare success is therefore rightly not promoted to a verified reversal. Neither shape is evidence that another send is safe.

## Exhaustive request coverage inventory

Notation: **R** = XSD `minOccurs=1`; **O** = `minOccurs=0`; all listed scalar maxima are one. Matching a schema establishes lexical/structural compatibility, not account-specific acceptance.

### Shared request transport and values

| Surface | Fresh source requirement | Current implementation and disposition |
|---|---|---|
| Endpoint/method/file | HTTPS POST `https://www.szamlazz.hu/szamla/`, multipart XML file, operation-specific action | `wire.rs:398–411`, `client.rs:374–405`; all three action constants match the EN/HU request pages. No application-level resend loop in `Client::send`. Custom transport retry behavior is documented separately in `recovery.md:4–9`. |
| XML declaration/root namespace | UTF-8, exact operation root and `http://www.szamlazz.hu/{root}` namespace; fixed field order | `xml.rs:157–178`; each writer uses the correct root/namespace. `xsi:schemaLocation` in examples is a hint, not a required request field. |
| Credentials | Optional XSD strings `felhasznalo`, `jelszo`, `szamlaagentkulcs`, in that order; authentication is operationally required | `xml.rs:628–637`: emits either the key or the username/password pair. Official authentication docs support both and legacy key-in-both-fields. Empty/incorrect credentials remain server refusals, not invented XSD restrictions. |
| Escaping/characters | XML text must be escaped and XML-1.0 representable | `xml.rs:586–603`, `wire.rs:405–432`: checked path rejects NUL and escapes caller text/credentials. `write_xml` is explicitly unchecked (`wire.rs:367–373`); bypassing `to_wire` also bypasses operation validation by contract. |
| Dates | `xs:date` | `xml.rs:19–35,616–625`: supported outbound domain is positive years 1–9999, civil dates without timezone. Year zero/negative refused before send. Positive boundary years tested offline. This deliberately supports a subset of XSD dates, not every XSD lexical spelling. |
| Identifiers/text | `xs:string`, no nonblank/length facet in the mutation XSDs | `types.rs:22–56`: `InvoiceNumber` wraps text; order selection is a String. No worker-specific 40-byte/NFC restriction imported. Empty target elements remain representable (the enum guarantees one selector element, not a nonempty value); no evidence that an empty selector means a wildcard. Not reported as a batch-deletion defect. |
| Money | `xs:double` on credit amounts; finite decimal output is valid | `xml.rs:611–614`: plain exact Decimal formatting; no floating-point conversion, grouping, or currency rounding invented for credit entries. NaN/infinity unavailable, a sensible money-domain subset. Zero/negative amounts remain representable; server acceptance is not established by that fact. |

### Storno (`xmlszamlast`)

Writer: [`ops/storno.rs:162–219`](../../crates/szamlazz-agent/src/ops/storno.rs#L162). Public fields/defaults: 55–158.

| Block/field, in wire order | Requirement/type | Coverage and interpretation |
|---|---|---|
| `beallitasok` | R container | Always emitted; credentials first. |
| `eszamla` | R boolean | `e_invoice`, defaults false; both values supported. Request flag is not the queried integer appearance code. P73 establishes the request controls the storno's form even on mismatch. |
| `szamlaLetoltes` | R boolean | `download_pdf`, false by default; v2 PDF is base64 in XML. |
| `szamlaLetoltesPld` | O int | `download_copies: Option<u8>`; narrower than signed int but no meaningful current loss: both language inline schemas say deprecated/ignored. Download retains field without the deprecation comment. |
| `aggregator` | O string | `Option<String>`, correctly before guardian/version. Contracted-integration effects unverified. |
| `guardian` | O boolean | `Option<bool>`, preserves explicit false vs omission. Contracted-integration effects unverified. |
| `valaszVerzio` | O int | Always shared constant `RESPONSE_VERSION=2`; no unsupported v1 parse fallback needed for normal requests. |
| `szamlaKulsoAzon` | O string | `external_id`; correct schema position. Recorded B6/XPRB behavior assigns it to the created `SS`, not a lookup of the original when number is present. See conflicts below. |
| `fejlec` | R container | Always emitted after settings. |
| `szamlaszam` | R string | Required Rust field/constructor target; always emitted. External-id-only storno is not exposed or established. |
| `keltDatum` | O date | `issue_date`; default omitted. Only positive-year validation locally. Recorded B3 rejects non-today with 352; no local clock-dependent today gate. |
| `teljesitesDatum` | O date | `fulfillment_date`; omission and explicit date both supported. P48 establishes default-original and acceptance even of mismatches. Rustdoc instructs use of the original date; raw client does not query to enforce it. |
| `megjegyzes` | O string | `comment`, after fulfillment, before type. |
| `tipus` | O string | Always `SS`; operation-specific constant is appropriate; caller cannot request another document type through storno. |
| `szamlaSablon` | O string | `template`, after type. Six documented tokens plus open `Other` (`types.rs:989–1019`). PHP table says int, but Agent schemas and examples are strings; string writer is correct. |
| `elado` | O container | Always emitted, possibly empty; valid because all three children are optional. |
| `emailReplyto`, `emailTargy`, `emailSzoveg` | O strings | `seller_email.reply_to/subject/body`, in schema order. Newlines/BBCode remain text; no local email-format gate. |
| `vevo` | O container | Always emitted, possibly empty. |
| `email`, `adoszam`, `adoszamEU` | O strings | Buyer email, Hungarian tax number, EU tax number, in order. Inline schema describes adding a tax number missing from the original; it does not establish arbitrary tax-number edits. |

**Absences checked:** storno XSD has no `sendEmail`, language, `simpleItems`, line items, order selector, payment block, or original appearance/date verification fields. Do not copy invoice-create fields into this writer. The tour-operator rule expressly says storno inherits the original's simplified-image setting. The create email rule documents attachments and `sendEmail`, but the reviewed storno-specific pages do not establish these controls for storno; their absence is not promoted to a confirmed storno feature defect. Exact storno email delivery/content and template override interactions lack recorded execution evidence.

### Credit registration, add, replace, explicit clearing (`xmlszamlakifiz`)

Writer/parser: [`ops/credit_entry.rs:274–344`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L274); explicit clear: 193–249; collection: 49–143.

| Block/field, in wire order | Requirement/type | Coverage and interpretation |
|---|---|---|
| `beallitasok` | R container | Credentials first. |
| `szamlaszam` | R string | `invoice_number`, required; no order/external-id selector on this operation. |
| `adoszam` | O string | `issuer_tax_number`; HU says issuer tax number matches the incoming credit to the corresponding invoice. Correctly not modeled as the buyer's tax number. |
| `additiv` | R boolean | Always emitted. False replaces prior entries, true retains/appends. Rust constructor defaults false and documents it; PHP's wrapper default true is not the wire default, since XSD requires the tag. D7 confirms both explicit modes. |
| `aggregator` | O string | Available on register and clear, before version; not silently dropped. |
| `valaszVerzio` | O int | Always 2, documented by both Agent response pages; PHP 2.12.4's forced v1 is a wrapper limitation, not evidence against v2. |
| `kifizetes` | 0–5 repeated blocks | `CreditEntries` enforces maximum through push, conversion, and deserialization; no mutable Vec escape. Five accepted in recorded D7; sixth only refused locally, server code unknown. |
| `datum` | R date per entry | `CreditEntry.date`, positive-year checked. |
| `jogcim` | R string | `title: PaymentMethod`, including unknown tokens through `Other`; no invented closed vocabulary. |
| `osszeg` | R double per entry | `amount: Decimal`, finite exact serialized value. No local positive-only or outstanding-balance cap unsupported by the XSD. |
| `leiras` | O string per entry | `description`, optional free text; correct position after amount. |

Mode inventory:

- **Populated replace:** allowed for 1–5 entries; false emitted. D7 `[100]` followed by replace `[200]` yielded `[200]`.
- **Populated add:** allowed for 1–5 entries; true emitted. D7 append 50 yielded `[200,50]`, outstanding 1020.
- **Empty registration replace:** rejected at checked boundary with `EmptyCreditEntryReplace` (278–286). This is intentional protection from an unfinished constructor, not an assertion that the wire forbids clearing.
- **Empty additive:** admitted, true plus no blocks. Schema-valid and consistent with retain-and-add-nothing; no independent recorded live execution of this exact shape found. Unit-test prose calling it harmless is not used as live evidence.
- **Explicit clear:** separate `ClearCreditEntries`, false plus zero blocks; preserves issuer/aggregator and shared parsing. It intentionally bypasses only the unfinished-replace guard, with XML character validation still applied by `to_wire`. `deny_unknown_fields` prevents a registration object with `entries`/`additive` silently becoming a clear.
- **Clear populated/already-empty:** both actually executed September 11; detailed evidence below. No raw HTTP capture identifies which channel provided the parsed number. No concurrency, delayed-send, all-account, or universal identity guarantee follows.
- No credit-entry idempotency key, currency selector, requested output PDF, bank-account write field, or per-entry deletion selector is declared by the reviewed request schema. Query-only credit metadata is not a missing request field.

### Proforma deletion (`xmlszamladbkdel`)

Writer/parser: [`ops/proforma.rs:60–88`](../../crates/szamlazz-agent/src/ops/proforma.rs#L60); target contract/rustdoc: 1–49.

| Block/field | Requirement/type | Coverage and interpretation |
|---|---|---|
| `beallitasok` | R; three O credential strings | Shared credentials only. No response-version, aggregator, guardian or external-id declaration omitted by the writer. |
| `fejlec` | R | After settings. |
| `szamlaszam` | O string | `ProformaSelector::InvoiceNumber`; exactly one target number element. |
| `rendelesszam` | O string | `ProformaSelector::OrderNumber`; exactly one order element, **all matching proformas**. |

The schema uses two optional sequential elements, not an XSD choice. Restricting the public request to one of the two documented alternatives is deliberate and consistent with examples. There is no reason to expose ambiguous both/neither-element requests. Empty strings are not ruled out by the schema; operational rejection is left to the server.

Paid-state checks are caller policy: D3 records deletion of a fully paid proforma and its credit history. Order selection is explicitly documented as batch scope in current Rustdoc, including that a latest-match query does not narrow it and repetition can target newly created matches. The PHP EN/HU page additionally promises rollback if a member fails; this is vendor documentation only, not independently recorded batch-atomicity evidence. The crate sends one request and makes no stronger atomicity promise.

## Exhaustive response, identity and failure inventory

| Surface/shape | Current behavior and assessment |
|---|---|
| Storno root/version | `StornoInvoice::parse → parse_issued`; correct v2 `xmlszamlavalasz` and namespace (`envelope.rs:18–21,275–295`). A v1 DONE/raw-PDF reply is not accepted as normal v2 success. |
| Credit/clear root/version | Correct same envelope through `xml::valasz`; no conversion of the requested number into a reported fact. Number required locally: Q-01. |
| Deletion root | Correct `xmlszamladbkdelvalasz`, dedicated namespace; `Response=()` accurately models success with no count/number list. No credit/invoice envelope fallback. |
| `sikeres` | Required unique scalar boolean (`xml.rs:478–499`); XML `true/false/1/0` handled. Missing/malformed/duplicate verdict is not defaulted to success. |
| `hibakod`, `hibauzenet` | False verdict maps code/message to API error; absent/empty code is `ErrorCode::Absent`, not a fabricated 0. Diagnostic decoding failure does not erase a valid refusal. Code is string in invoice/credit XSD, int in deletion XSD; open text preservation on deletion is a conservative extension. |
| Success with a body error code | `Verdict::api_error` treats `sikeres=true` as authoritative (`xml.rs:506–516`) and ignores a co-present code. Offline `true + 56 + number` therefore has no warning flag. Docs put error fields on failure; no live contradictory-success shape is recorded. Retained as an unproven edge, not a confirmed defect. |
| `szamlaszam` | Optional schema string; nonblank body first, decoded nonblank header fallback, trimmed (`envelope.rs:120–133,326–329`). Body/header disagreement uses body; request/response number equality is not enforced for credit. These are observable choices, not reported vendor guarantees. A storno number must differ from the original to establish even heuristic reversal. |
| `szamlanetto`, `szamlabrutto`, `kintlevoseg` | Optional totals; body first, respective raw header fallback; exact decimal/exponent parse. Comma-decimal HTTP totals normalized; XML numeric text not URL-decoded (`envelope.rs:151–167,344–370`, credit_entry.rs:323–340). Missing totals remain None. Present invalid totals are parse errors on ordinary success, dropped under accepted storno 56. |
| `vevoifiokurl` | Optional XML string, otherwise decoded `szlahu_vevoifiokurl`; no double URL decoding of XML (`envelope.rs:135–143`, credit_entry.rs:342). |
| `pdf` | Optional storno base64 decoded with whitespace tolerance (`types.rs:104–116`); absence allowed even when requested. Invalid base64 on ordinary success is uncertainty, under accepted 56 becomes None. Credit response schema has no PDF; balance exposes none. |
| `szlahu_id` | Optional storno auxiliary id, header only; missing/malformed/negative becomes None (`envelope.rs:331–342`). Recorded header evidence supports it although the response-page header table omits it. Credit balance deliberately does not expose this auxiliary field. |
| `szlahu_fizetesmod` | Optional decoded header → open PaymentMethod on both storno and balance. No same-named XML payment-method field in these response schemas; correctly not invented. |
| `szlahu_szamlaszam`, totals, URL encoding | Header names case-insensitive; text decoded once, numeric/code/id headers read raw (`wire.rs:227–270`). Literal plus must be percent-encoded on textual headers. Optional header tables do not establish mandatory presence. |
| Header/body precedence | Nonblank `szlahu_down` → unavailable; nonblank error-code header next; non-2xx without header code → HTTP error; then body (`wire.rs:291–310`). Header refusal takes precedence over body success. Body-only 463 is recognized. Error-message-only header is not a code verdict. |
| Non-2xx body-only API response | Body not parsed after HTTP failure without error header; offline HTTP-500 body-only numbered 56 returned HttpStatus. Classified Unknown. No recorded such vendor status/channel combination; no unsupported claim that every body refusal is retained under every status. |
| Code 56, storno | Accepted when header/body says 56 and a valid reported number exists. Lenient optional metadata, flag true, no reissue. Header 56 may accompany plain notification text/empty body and usable number header; malformed XML is not rescued as success. Conflicting body refusal (e.g. 221) wins over header 56; malformed/duplicate identity cannot become header-only success (`envelope.rs:179–254,285–315`). |
| Code 56, register/clear/delete | No issuing-success exception. Error remains Unknown; an existing invoice number does not establish a credit effect. M-01 is the documentation problem, not grounds to copy storno success logic into credit. |
| Storno `CreatedInvoice::reverses` | Different number plus known gross ≤0, explicitly a heuristic (`envelope.rs:61–87`). False is inconclusive for changed-number missing/positive gross; query `tipus=SS` and original reference. Zero is synthetic policy, not observed zero-total acceptance. |
| Storno success/repeat/no-op | Parsed CreatedInvoice is a wire result, not proof “created now.” B4 repeats existing SS; B5 proforma/delivery-note replies echo original unchanged. Public operation Rustdoc documents these caveats despite the shared result type's generic “issued” description. |
| Delete success/335 | Bare true → unit; 335 → ProformaNotFound, not success. Missing/deleted/consumed target are not distinguished by this refusal. Current caller recovery doc says reconcile rather than translate all 335s into replayed success. |
| Critical text/HTML, malformed XML, wrong namespace | Errors, not success; bounded excerpts. Entire XML checked for UTF-8, complete structure, root/namespace, lexical well-formedness; foreign namespace subtrees cannot supply protocol facts (`xml.rs:201–377`). No runtime XSD validation is claimed. |
| Known/unknown errors | 1,55,56,unknown/absent → Unknown; 71/152 distinct duplicate class; 7 operation-dependent missing data; 14,53,54,57,221,335,352,463 and credential codes are refusals of this exchange (`error.rs:320–423`). Unknown code/message retained. Parse/HTTP/unavailable errors stay uncertain. |
| Retry semantics | `is_retryable` is a query hint, not write authorization. `recovery.md:13–19,36–55` distinguishes original-specific storno evidence, credit current state, order-based batch deletion, and five total same-request sends. An earlier uncertain write is not settled by a later refusal, absence, or elapsed time. |

No mandatory response field other than `sikeres` is omitted from the parser's protocol decision. Optional fields exposed by the mutation-specific XML schemas are covered, with the deliberate reported-number requirement discussed separately. The response parser is not a full XSD validator: order-insensitive reads and ignored extension fields are not outbound-order defects.

## Fresh source conflicts and their resolution

1. **Storno external id:** both EN/HU request pages describe an optional reference to the original's pre-existing external id. Both inline examples instead describe a later queryable identifier, while XSD only declares a string. B6/XPRB records assignment to the newly created `SS`, retention of the original's own id, and repeat-storno dropping a new id. `storno.rs:86–98` explicitly documents this conflict and bounds its observation to calls carrying a number. Preserve this behavior; external-id-only lookup is not proved.
2. **Order deletion scope:** EN Agent XML prose describes a proforma “with the given order number” and omits the batch sentence. HU explicitly says all proformas with that order are deleted. EN/HU PHP pages both corroborate multiple deletion and document rollback. Current code/rustdoc follows the more explicit scope, correctly.
3. **Broken example locations; working deletion request download:** the exact schema locations printed in request/response examples return 404 when fetched over HTTPS: `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd` and `.../xmlszamladbkdelvalasz.xsd`. Adding `/szamla` also returns 404; guessed `/szamla/docs/xsds/agentdbkdel/` variants return 404. The parent subsequently identified a working location from fixture provenance: **https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd**. It was freshly fetched, independently hashed and compared: declarations, types, occurrence bounds and ordering match the EN/HU inline request schemas, apart from comments/formatting. All four existing generated deletion requests pass it (number/order × key/password). The relevant response sibling `https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdelvalasz.xsd` returned 404; response validation therefore still uses the available EN/HU inline schemas. This corrects the initial blanket missing-download limitation. The examples print HTTP; the fetch tool upgrades HTTP to HTTPS, so no claim about the HTTP-only route is made.
4. **Working mutation downloads:** storno and credit request download declarations match current inline fields, types, occurrence bounds and sequence. HU credit uses a named `szamlaKifizTipus` instead of the EN/download anonymous root type: same accepted instance shape. Comment/formatting differences are not mismatches. Storno download lacks the inline deprecation annotation for copies.
5. **Shared response schema:** downloaded `agent/xmlszamlavalasz.xsd` includes optional PDF, as storno's inline schema does. Credit EN/HU inline responses intentionally omit PDF; using a shared internal Body does not imply credit returns one. All make the number optional; none supplies a success-specific guarantee (Q-01).
6. **Published successful response examples are illustrative, not valid raw fixtures:** all four EN/HU storno/credit success examples contain raw `&partguid`/`&szfejguid` in the URL, failing XML parsing. Storno examples also visibly abbreviate base64 with `....` and show positive totals, which are not evidence of the observed positive-original/negative-storno case. Do not loosen XML/base64 parsing to accept them. The request examples, error examples, and deletion examples passed independent checks.
7. **56 absent from general error table:** neither fresh EN nor HU general error table lists it. Fresh PHP 2.12.4 source defines 56 as notification failure and explicitly treats it as issuance success only with an invoice number. This supports the storno policy despite response-page prose saying error headers omit numbers/totals. No recorded probe triggered 56; specific status/header/body shapes remain synthetic interoperability policy, not live proof.
8. **PHP credit defaults/version differ:** EN/HU PHP docs default additive to true; PHP source `SzamlaAgent.php:368–374` forces text response v1. Agent EN/HU schemas require explicit additiv and explicitly allow v2. Rust's documented false default and v2 pin are supported; no product change indicated.
9. **PHP types/examples are not mutation XSDs:** PHP storno table calls template int, while all relevant Agent XSDs declare string tokens. Historical hard-coded issue dates in request/PHP examples are schema-valid but not currently usable dates under recorded B3 behavior. Use the schema for type/order, account evidence for that operational rule.
10. **Create rules versus storno rules:** notification page's `sendEmail`/attachments apply to the documented create surface, not automatically to an operation whose XSD omits the switch. Tour-operator rules explicitly cover storno inheritance. No additional editable simplified-image flag is missing from storno.

## Recorded behavior deviations and verification boundaries

| Recorded execution | What it establishes here | Limits / current implementation |
|---|---|---|
| B4/B4x, P48-P6 (`behaviour.md:88,105,114`) | Repeat by original number returns existing SS, does not assign a newly supplied external id or change fulfillment date | Bounded account observations. `storno.rs:24–36,86–98` documents repeat behavior. The short “re-sending … is safe” sentence should be read with that observed scope and the explicit reconciliation guidance in `recovery.md:14`, not as proof of every in-flight/concurrent outcome. |
| B5 (`behaviour.md:106–108`) | Proforma/delivery-note storno succeeds as unchanged echo; SS target gives 14; corrected original gives 221 | No-op explicitly documented in operation; heuristic is qualified. Other invoice families and zero/negative originals were not established by these executions. |
| B8 (`behaviour.md:98–99`) | Reversal removes original queried credit entries; SS outstanding is full negative gross | Supports public warning; no assumption that earlier payments are netted into SS. |
| B3/P48 (`behaviour.md:109–115`) | Non-today storno issue date refused; omitted fulfillment uses original; correct and incorrect explicit fulfillment dates accepted silently | Supports keeping optional dates rather than claiming local validation ensures NAV-correct reversal. NAV/legal correctness is distinct from vendor acceptance; no fresh legal audit performed. |
| P73 (`behaviour.md:116–117`) | All four original/storno paper/electronic combinations accepted; resulting appearance follows request | Supports caller-settable boolean on raw client and documented advice to query original. Current probe source alone is not a rerun of P73. |
| D1–D4 (`behaviour.md:124,128–129`) | Bare deletion success/no success headers; repeat/unknown/consumed →335; order deletion works; fully paid proforma deletable | Does not establish multi-match rollback execution. Current code does not add a paid-state rejection or claim 335 proves prior deletion. |
| D7/D8 (`behaviour.md:152,154–155`) | Replace/add effect, five accepted entries, query ordering not submission ordering, reversed target 463 body-only | Sixth not sent. SS-target 463 is implied by message but untested. No temporal or concurrent-update guarantee. |
| CLEAR-populated/CLEAR-empty | September 11 `CTEST-2026-13` and `CTEST-2026-15` were verified test invoices. Clear removed a queried 100 HUF entry on the first and succeeded on the already-empty second; both parsed expected number/outstanding 3136; post-clear queries empty; cleanup SS `14`/`16` verified by type/reference | Detailed dated record includes run labels, commands, captured output and successful cleanup. Raw HTTP not archived; account continuity with September 3/6/7 unknown; universal number echo/concurrent behavior not proved. |
| D6 (`behaviour.md:173,191,200–201`) | Attempts to trigger 56 did not do so | No 56 response shape can be called live-verified. Fresh notification docs say test-account email routes to account settings; old speculation that tests do not send email is not an established rule. |

Explicit clearing evidence: [`docs/research/2026-09-11-credit-clearing-live.md:12–32,34–74`](../research/2026-09-11-credit-clearing-live.md#L12). Historical evidence scope and unavailable raw logs: [`docs/szamlazz-hu-behaviour.md:3–33,45–47`](../szamlazz-hu-behaviour.md#L3). The vendor-question draft remains **unsent/no answer**, not a reason to dismiss either the source ambiguity or the successful executions.

## Primary URL inventory

Every EN/HU link below was freshly fetched during this review. XML/response pages include their displayed examples and inline schemas; no previous local schema copy was used as fresh evidence.

| Topic | EN | HU |
|---|---|---|
| Storno request | https://docs.szamlazz.hu/agent/reversing_invoice/request | https://docs.szamlazz.hu/hu/agent/reversing_invoice/request |
| Storno XML/XSD | https://docs.szamlazz.hu/agent/reversing_invoice/xml | https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml |
| Storno response | https://docs.szamlazz.hu/agent/reversing_invoice/response | https://docs.szamlazz.hu/hu/agent/reversing_invoice/response |
| Credit request | https://docs.szamlazz.hu/agent/credit_entry/request | https://docs.szamlazz.hu/hu/agent/credit_entry/request |
| Credit XML/XSD | https://docs.szamlazz.hu/agent/credit_entry/xml | https://docs.szamlazz.hu/hu/agent/credit_entry/xml |
| Credit response | https://docs.szamlazz.hu/agent/credit_entry/response | https://docs.szamlazz.hu/hu/agent/credit_entry/response |
| Credit IPN note | https://docs.szamlazz.hu/agent/credit_entry/other | https://docs.szamlazz.hu/hu/agent/credit_entry/other |
| Delete request | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request | https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/request |
| Delete XML/XSD | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml | https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml |
| Delete response | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response | https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/response |
| Authentication | https://docs.szamlazz.hu/agent/basics/authentication | https://docs.szamlazz.hu/hu/agent/basics/authentication |
| Sending/validation | https://docs.szamlazz.hu/agent/basics/sending-requests | https://docs.szamlazz.hu/hu/agent/basics/sending-requests |
| Errors/retry limit | https://docs.szamlazz.hu/agent/basics/error-handling | https://docs.szamlazz.hu/hu/agent/basics/error-handling |
| Template rules | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template |
| Notification rules | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification |
| Storno simplified-image inheritance | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency |
| PHP credit | https://docs.szamlazz.hu/php/jovairas | https://docs.szamlazz.hu/hu/php/jovairas |
| PHP deletion/rollback | https://docs.szamlazz.hu/php/dijbekero-torles | https://docs.szamlazz.hu/hu/php/dijbekero-torles |

Additional first-party sources:

- https://docs.szamlazz.hu/php/sztorno-szamla-generalas (PHP storno example/table, EN).
- https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd — fresh SHA-256 `6f9d5beb6efd205f0509e7413e15fb96660ee89c9f169d27c4972d3b24127a3a`.
- https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd — fresh SHA-256 `9637a242df2f55f87ecfff1b07dd74ef0b09bdaff47e39bb4d9890d4748a73de`.
- https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd — follow-up fresh SHA-256 `076b4d98c3cf599a5b5ab30e5e9ff3e522d9c0642d806fb502e8a560ace08641`, matching the provenance hash supplied by the parent. The response sibling at the same directory returned 404.
- https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd — shared downloaded response schema, freshly read.
- https://docs.szamlazz.hu/php/ links the current **2.12.4** download: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip — SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Read in memory, never executed. Archive path `szamlaagent/src/szamlaagent/Response/InvoiceResponse.php:14–17,314–323,427–431` establishes the numbered-56 policy; `szamlaagent/src/szamlaagent/SzamlaAgent.php:368–374` establishes the PHP credit v1 override.
- Failed deletion URLs are listed explicitly in conflict 3. A guessed `https://docs.szamlazz.hu/agent/category/settings-and-rules` route returned 403; the actual individual rules URLs above were available and read. No conclusions depend on that failed route.

IPN is separately delivered payment-status information, not a per-entry acknowledgement or a completion barrier for these mutations. Its HU page explicitly includes proformas while EN introductory prose says invoices. No IPN receiver implementation is reviewed here.

## Verification performed

Scratch artifacts, created for this review only:

- `/tmp/opencode/mutations-28dcec1-check/Cargo.toml`
- `/tmp/opencode/mutations-28dcec1-check/src/main.rs`
- `/tmp/opencode/mutations-28dcec1-check/validate.py`

Commands:

```sh
cargo run --offline --manifest-path /tmp/opencode/mutations-28dcec1-check/Cargo.toml --target-dir /tmp/opencode/mutations-28dcec1-check/target
python3 /tmp/opencode/mutations-28dcec1-check/validate.py
```

The scratch Rust package uses the current source as a path dependency with no HTTP-client feature, dummy credentials, its own target and lockfile. Relevant resolved runtime versions match the workspace lock: quick-xml 0.42.0, xmlparser 0.13.6, jiff 0.2.35, rust_decimal 1.43.0, serde 1.0.229, serde_json 1.0.151. This is a focused downstream reproduction, not a substitute for the parent's locked workspace suite or exact workspace feature unification. The fresh schema check uses installed `libxml2.so.2` through Python ctypes; neither lxml nor xmllint was installed. No package installation was needed.

Results:

1. **22 generated requests / 66 schema-instance pairs passed cumulatively**: the initial 62 pairs plus four focused follow-up deletion/download checks. Shapes: minimal/all-option storno; one/five entries in add/replace modes; empty add; minimal/full clear; number/order deletion; each under agent-key and username/password credentials. All three request operations are now checked against fresh EN, HU and download. Included escaping, unknown payment-method token, explicit false guardian, boundary positive years, negative/zero/fractional credit money.
2. **All six EN/HU response schemas accept bare `sikeres=true`.** Public parsers require reported number for storno/register/clear and return unit for deletion. This independently establishes Q-01's structural gap, not real server occurrence.
3. **Eight official request examples passed their available schemas.** Six failure examples (including deletion's two failures) plus two deletion successes yielded eight valid response examples; four storno/credit success examples failed XML parsing on raw URL ampersands. Storno base64 abbreviation separately inspected as illustrative text.
4. Public-parser controls exercised header-only identity with XML boolean `1`, scientific totals, body-only 463, numbered/numberless 56, conflicting header-56/body-221, malformed optional metadata under 56, malformed identity despite number header, body-only 56 under HTTP 500, body/header identity disagreement, and true-plus-body-56. Storno's lenient optional-metadata path preserves numbered issuance without rescuing malformed identity. Credit/clear preserve code 56 as uncertainty. Deletion true/false-335/missing-verdict/numeric-boolean paths behaved as inventoried.
5. Checked-boundary assertions passed for accidental empty replacement refusal, explicit-clear NUL refusal, six-entry collection refusal, and storno year-zero refusal.
6. Existing test source was inspected for the coverage map (`tests/schema_requests.rs:420–505`, `tests/clear_credit_entries.rs:6–55`, primary operation/envelope tests). It was not relabeled as execution evidence. The broad suite and live probes were not run in this review.
7. **Focused deletion-download follow-up:** a Python in-memory GET/hash/libxml2 check freshly acquired the `dijbekerodel` request XSD, asserted the parent-supplied SHA-256, invoked the already-built scratch executable to regenerate its unchanged request shapes, and selected only the four deletion rows for schema validation. Results: `PASS delete-number key`, `PASS delete-number password`, `PASS delete-order key`, `PASS delete-order password`. No rebuild, broad Cargo suite, or full schema matrix rerun. The original `validate.py` remains the initial 62-pair script; the four added checks were run separately. Only this report was amended.

## Standards assessment and completion

The task's explicit protocol sources and repository vocabulary were the review standards. No additional AGENTS.md, CONTRIBUTING, or named coding-standards file was found. Hand-written order-preserving writers, open wire tokens, explicit clearing intent, and shared response parsing follow the surrounding design. No independent standards/architecture defect is asserted beyond M-01's inaccurate contract prose; schema/tooling-compatible choices are not style findings.

**Disposition:** correct the scope of code-56 documentation and address the parent/adjudication's actionable success-identity contract gap, retaining the pending vendor clarification and unobserved-emission caveat. Preserve the recorded storno/proforma deviations with their account/date limits. No product modifications were made. This report is the sole workspace file added and amended by this review.
