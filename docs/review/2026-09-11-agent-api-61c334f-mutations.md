# Számla Agent mutation fidelity review — 61c334f

**Source retrieval:** 2026-09-11. **Result: no confirmed operation implementation defect.** One concrete, reproduced response-contract question remains, ranked below. The three request writers cover the documented fields, types, cardinalities, ordering, namespaces and operation selectors. Storno, credit registration/clearing and deletion response interpretation were all reviewed, including the shared issuing envelope.

## Revision and review boundary

- Initial HEAD: `61c334f9508e8b63df2f3db182d6ca83f4feb8f0`. The worktree already contained user edits, including `crates/szamlazz-agent/src/wire.rs`.
- During the audit, another actor advanced HEAD to `5c6d5ead33a3587c4ea29cc973bedaefc3ddcb1f`. A comparison against `61c334f` confirmed that **the only change under `crates/szamlazz-agent/` was the already-present extraction/export of `wire::validate_xml_text`** (10 added lines, one removed). The three operations, `ops/envelope.rs`, their tests, the behavior notes, fixture provenance and `Cargo.lock` were unchanged. This report therefore reviews the requested operation code at `61c334f`, with tests against the current worktree and that disclosed shared-wire change.
- Full reads: `src/ops/{storno,credit_entry,proforma,envelope}.rs`; supporting XML verdict/writer/namespace and numeric/header paths; relevant shared types and client handoff; operation unit tests, response regression tests, upstream example tests, explicit-clear tests, current live/probe scenario source; all of `docs/szamlazz-hu-behaviour.md`.
- Paths below are relative to `crates/szamlazz-agent/` unless workspace-qualified. Line ranges refer to the inspected code. `wire.rs` ranges refer to the current worktree.
- This is a current-source API-fidelity audit, not a diff/style review. Historical review `docs/review/2026-09-11-agent-api-837dad0-mutations.md` was read after the initial direct comparison, as leads only. Its narrower exclusion of storno responses does **not** apply here.
- Only this report was written in the repository, through `apply_patch`. No source, fixture or user edit was modified; no credentials were read, no live Számla Agent operation was called, and no commit was made by this audit. Public documentation/XSD/PHP-package GETs and loopback mock HTTP were the only network activity.

## 1. Ranked actionable question

### Q1 — P3 clarification: successful credit acknowledgement requires an undocumented nonblank echo

**Potential impact:** P2 compatibility impact if the server emits this shape. **Confidence:** certain local behavior and XSD compatibility; success-specific vendor permission/occurrence unresolved. **Not counted as a confirmed production bug.**

**Code:** `src/ops/credit_entry.rs:252–258,311–317`; shared body/header lookup `src/ops/envelope.rs:120–133,326–329`; explicit clearing delegates to the same parser at `credit_entry.rs:247–249`.

**Fresh sources:** [credit response EN][C3], [HU][C3-HU]. They say additional headers **“may also arrive”** and **“Elements marked `minOccurs="0"` may not always be included.”** The operation-specific XSD has:

```xml
<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1"/>
<element name="szamlaszam" type="string" maxOccurs="1" minOccurs="0"/>
```

**Reproduced:** HTTP 200, no headers, and the following true acknowledgement passes the envelope verdict but fails with `ResponseError::Parse(ParseError::Missing("szamlaszam"))`:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
  <kintlevoseg>0</kintlevoseg>
</xmlszamlavalasz>
```

The same result occurs with `kintlevoseg` omitted. Tested with valid populated replace and additive requests and `ClearCreditEntries`; adding only `szlahu_szamlaszam: I-1` makes each succeed. No request identity is substituted by the parser.

**Impact if emitted:** an acknowledged registration/clear and any balance data become an uncertain parse failure. A caller incorrectly repeating additive registration could duplicate entries; repeating replacement/clear could overwrite intervening entries. The parser performs no resend and conservatively reports uncertainty, so duplicate execution is not demonstrated.

**Why unresolved:** the XSD describes successes and failures together. Both current EN/HU success examples include a number, their error examples omit it, and behavior notes `:145` record numbered successful credit replies. No numberless success is recorded. Conversely, the mandatory Rust result field is not evidence of a vendor guarantee. Version 1's bare `xmlagentresponse=DONE` does not establish version 2 behavior.

**Fix/next action:** obtain a success-specific echo guarantee. If omission is supported, represent an acknowledgement with optional **reported** identity, or distinguish requested identity from returned identity; retain balances without manufacturing an echo. If every successful version-2 reply guarantees a nonblank number in one channel, document that source and retain the present requirement. Apply the resolution to both registration and clearing. A parser test alone cannot settle the vendor question.

**Storno comparison:** its numberless reply is also refused (`envelope.rs:268–279`, test `storno.rs:360–369`). The result deliberately requires the issued document's number, which cannot be replaced with the original's number. This is conservative unknown-outcome handling, not proof that no reversal occurred; do not turn it into a create-style preview.

## 2. Complete request coverage

Notation: `?` means optional (`minOccurs=0`); every listed singleton has `maxOccurs=1`. Arrows are wire order. XSDs specify no `default` attributes for these request fields: constructor defaults and the response-version prose must be distinguished from schema defaults.

### Shared serialization and dispatch

[Sending requests][B2] says the service **“decides which function to perform using the name of the form field containing the XML file”**. All three request pages require POST to `https://www.szamlazz.hu/szamla/`, `multipart/form-data`, one XML file. Exact action constants are checked below. `wire.rs:7–14,66–99,402–408` supplies endpoint, file disposition/filename, `text/xml` part, multipart boundary and checked serialization; `client.rs:374–405` POSTs it and supplies HTTP status/headers/body to the operation parser.

`xml.rs:139–160,568–620` writes XML 1.0, UTF-8, default namespace, escaped text, true/false booleans, plain Decimal text and ISO civil dates. Namespace identifiers remain **HTTP**, independently of the HTTPS endpoint. `None` omits optional scalars; `Some("")` writes empty content. Omission is not assumed equivalent to empty text. `xsi:schemaLocation` in examples is a validation hint, not a missing business field.

[Authentication][B1] permits **“either an Agent key … or a username and password.”** All three settings blocks emit `szamlaagentkulcs`, or `felhasznalo → jelszo`, in their schema slots. Deletion examples show both alternatives simultaneously; the library's choice of one is supported, not an omission. No credential value was inspected.

### Invoice reversal — [request][S1], [XML/example/inline XSD][S2], [download][S4]

| Sequence / field | Rust surface, type/default and assessment |
|---|---|
| `action-szamla_agent_st`, `xmlszamlast`, namespace `http://www.szamlazz.hu/xmlszamlast` | Exact at `storno.rs:162–170` |
| `beallitasok → fejlec → elado? → vevo?` | Exact at `:171–206`; seller/buyer containers are always emitted, empty by default; both shapes are schema-valid |
| `felhasznalo? → jelszo? → szamlaagentkulcs?` | Shared credential writer, before all settings |
| `eszamla` boolean | `e_invoice: bool`, always written, constructor false; not automatic inheritance (`:59–73,144,173`) |
| `szamlaLetoltes` boolean | `download_pdf: bool`, always written, constructor false (`:74–76,145,174`) |
| `szamlaLetoltesPld?` int | `download_copies: Option<u8>`, absent by default (`:77–81,175–177`); narrower than int but current S2 explicitly says **“our system no longer processes it”**, so no useful missing range established |
| `aggregator?` string → `guardian?` boolean | Optional string/bool, both absent by default, present in correct order (`:82–85,178–181`); no entitlement/default invented |
| `valaszVerzio?` int → `szamlaKulsoAzon?` string | Always shared `ops::RESPONSE_VERSION = "2"`, then optional external id (`:182–183`); field meaning conflict adjudicated in §4 |
| Header `szamlaszam` string | Required original number (`:56–58,186`), not order/external-id-only selection |
| `keltDatum?` date → `teljesitesDatum?` date | Optional `jiff::civil::Date`, omitted by default (`:99–121,187–188`); live-tested omission/explicit-value behavior below |
| `megjegyzes?` string → `tipus?` string | Optional comment, then fixed `SS` (`:122–123,189–190`), matching example; arbitrary document-type changes are not this operation |
| `szamlaSablon?` string | Optional open `InvoiceTemplate` (`:124–125,191–193`); all six annotated tokens plus `Other(String)` |
| Seller `emailReplyto? → emailTargy? → emailSzoveg?` | Three independently optional strings in `SellerEmail` (`:195–201`; `types.rs:1022–1032`); content/BBCode passed through with XML escaping |
| Buyer `email? → adoszam? → adoszamEU?` | Three optional strings in exact order (`:129–135,202–206`); schema annotation describes supplying a tax number **missing from the original**, not editing an already-present one |

All optional fields default absent (`storno.rs:138–159`). EN/HU inline, legacy `/xsd` and downloaded request schemas agree on field structure/order. `InvoiceTemplate` maps `Most/Default/NoEnvelope/EightCentimeter/Continuous/DeliveryNote` to `SzlaMost/SzlaAlap/SzlaNoEnv/Szla8cm/SzlaTomb/SzlaFuvarlevelesAlap` (`types.rs:983–1019`). `Default` selects a named layout, not omission; labels conflict across vendor pages (§5).

No storno request schema field for `sendEmail`, invoice language, line items, `simpleItems`, or arbitrary attachments was found. Create's email settings page is not proof those controls apply to storno. The simplified-image page explicitly says reversal **“inherits the state of the original document”**; no missing storno `simpleItems` switch. The specialized aggregator/guardian operational contracts are not explained by these sources.

### Credit registration and explicit clearing — [request][C1], [XML/example/XSD][C2], [download][C4]

| Sequence / field | Rust surface, type/default and assessment |
|---|---|
| `action-szamla_agent_kifiz`, `xmlszamlakifiz`, namespace `http://www.szamlazz.hu/xmlszamlakifiz` | Exact at `credit_entry.rs:274–289`; clear reuses the action/writer (`:237–244`) |
| `beallitasok → kifizetes{0..5}` | Exact at `:291–306`; settings required; zero through five entries in schema |
| Credentials → `szamlaszam` string | Required number (`:292–293`) |
| `adoszam?` string | Optional `issuer_tax_number`, default absent (`:161–164,185,294`); HU explicitly identifies the issuer |
| `additiv` boolean | Always written, constructor false (`:165–169,186,295`); false replaces, true retains and appends |
| `aggregator?` string → `valaszVerzio?` int | Optional aggregator, then shared version 2 (`:296–297`) |
| Entry `datum` date → `jogcim` string → `osszeg` double → `leiras?` string | `CreditEntry { date, title, amount, description }`, exact order (`:21–45,299–305`); date/title/amount required by constructor; description absent by default |
| Five-entry maximum | Private `CreditEntries(Vec<_>)`; push, vector conversion and serde enforce ≤5 (`:49–142`); slice access cannot grow the collection |
| Empty replacement intent | Ordinary unfinished registration fails validation (`:278–281`); explicit `ClearCreditEntries` sends false with no entries (`:193–249`) |

C2: **“If true, former credit entries are retained; otherwise they are replaced.”** The PHP wrapper's documented additive default true is not an Agent-wire default: `additiv` is required, and this crate always sends its explicit choice. Similarly PHP's today/transfer/0.0 defaults do not require Rust to default date/title/amount.

`PaymentMethod::Other(String)` preserves arbitrary `jogcim` text (`types.rs:588–687`). Decimal is a finite, exact supported subset of `double`, without an invented positive-only or two-decimal request rule. Civil dates are a supported date subset; no timezone-bearing request constructor is offered. The XSD supplies no sign/scale or nonempty-string facets here.

**New relative to the prior review:** zero-entry replacement is now explicitly expressible through `ClearCreditEntries`. Its rustdoc honestly says live zero-entry behavior is not established and recommends querying afterward. Its closed serde shape refuses accidental `entries`/`additive` fields. The older behavior note `:211–215` saying clear-all is “not offered” is stale capability prose; its assertion that the operation was never probed remains unresolved, not disproved by the presence of a probe test.

### Proforma deletion — [request][D1], [XML/examples/XSD][D2], [download][D4]

| Sequence / field | Rust surface and assessment |
|---|---|
| `action-szamla_agent_dijbekero_torlese`, `xmlszamladbkdel`, namespace `http://www.szamlazz.hu/xmlszamladbkdel` | Exact at `proforma.rs:60–67` |
| `beallitasok → fejlec` | Required blocks, exact order (`:69–77`) |
| Settings: `felhasznalo? → jelszo? → szamlaagentkulcs?` | Credentials only; no invented PDF/version field |
| Header `szamlaszam?` string → `rendelesszam?` string | `ProformaSelector` writes exactly one (`:14–30,72–77`); lowercase `rendelesszam`, not query's `rendelesSzam` |
| Number versus order | Number targets one proforma; order targets **all matches**, explicitly documented (`:25–27,35–43`) |

HU D2: **“Ha azonos rendelésszámmal több díjbekérő is van a számlázási fiókban, akkor a törlés az összes díjbekérőre vonatkozik.”** Translation: when several proformas share the order number, deletion applies to all of them. [PHP deletion][P3] corroborates multiple deletion and says **“If deleting any … fails, we perform a rollback.”** The crate sends one operation; it does not narrow to the latest queried match or promise a count, deleted-number list, or live-verified rollback.

The raw schema permits both/neither selectors; the enum intentionally supports the documented alternatives. Empty strings remain representable because the wire type is plain string; no successful empty-target semantics are claimed. No external-id deletion is documented. Paid-state retention is caller-owned, consistent with the observed fully paid deletion.

## 3. Complete response coverage and outcomes

### Version and response shapes

- [Storno][S3]: version 1/omitted is `xmlagentresponse=DONE;{invoice_number}` or raw PDF according to `szamlaLetoltes`; version 2 is `xmlszamlavalasz` with optional base64 PDF. This crate always requests 2 and returns `CreatedInvoice` via `parse_issued` (`storno.rs:211–212`).
- [Credit][C3]: version 1/omitted is `xmlagentresponse=DONE`; version 2 is `xmlszamlavalasz`. This crate always requests 2 and returns `InvoiceBalance` (`credit_entry.rs:311–338`), including for clear.
- [Deletion][D3]: always `xmlszamladbkdelvalasz`; critical errors may be text/HTML. Success is `()` after the dedicated verdict (`proforma.rs:82–87`). No version/PDF selection exists.
- Rejecting version-1 success/text under a version-2 request is not missing version-2 support. Unexpected text/HTML is bounded diagnostic evidence and an unknown outcome, never deletion success.

### Every declared body/header field

S3's sequence is `sikeres → hibakod? → hibauzenet? → szamlaszam? → szamlanetto? → szamlabrutto? → kintlevoseg? → vevoifiokurl? → pdf?`. C3 has the same sequence without `pdf`. D3 has only the first three fields. All singletons have maximum one; only `sikeres` is required in the XSD.

| Field/channel | Handling and code |
|---|---|
| Root/namespace | Storno/credit require `xmlszamlavalasz` / `http://www.szamlazz.hu/xmlszamlavalasz`; deletion requires its own root and namespace. Complete UTF-8/XML, root and namespace checks precede deserialization (`xml.rs:167–359`) |
| `sikeres` boolean | Shared unique scalar fact; true/false/1/0 supported, whitespace accepted. Missing/invalid fails. Legacy empty reads false, never success; not a claim that empty is XSD-valid (`xml.rs:460–481,750–782`) |
| `hibakod?` | Storno/credit XSD string; deletion XSD int. Shared open reader retains unknown tokens, normalizes known numeric spellings, false with absent/blank code becomes `ErrorCode::Absent` (`xml.rs:484–504`, `error.rs:25–35,214–317`) |
| `hibauzenet?` string | CDATA/XML decoded; readable refusal survives malformed/nested/duplicate optional diagnostic with message unavailable (`xml.rs:460–481`); never fabricates a message/code |
| `szamlaszam?` / encoded `szlahu_szamlaszam` | Trimmed nonblank body first, once-decoded header fallback (`envelope.rs:120–133,326–329`); mandatory for issued/credit result, Q1 |
| `szamlanetto?` double / raw `szlahu_nettovegosszeg` | Optional Decimal, body first then header (`envelope.rs:158–168,226–231`; `credit_entry.rs:318–323`) |
| `szamlabrutto?` double / raw `szlahu_bruttovegosszeg` | Same (`envelope.rs:232–237`; `credit_entry.rs:324–329`) |
| `kintlevoseg?` double / raw `szlahu_kintlevoseg` | Same (`envelope.rs:238–243`; `credit_entry.rs:330–335`); header is observed although not listed in current operation header tables |
| `vevoifiokurl?` string / `szlahu_vevoifiokurl` | Body first then once-decoded header (`envelope.rs:135–143,244`; `credit_entry.rs:337`); body receives XML entity decoding only, no URL decoding |
| `szlahu_fizetesmod` | Once-decoded open `PaymentMethod`, now exposed on **both** issued and credit results (`envelope.rs:49–53,245,318–324`; `credit_entry.rs:336`); no XML counterpart is defined |
| `pdf?` base64Binary, storno only | Optional decoded PDF, XML whitespace accepted by `Pdf`; ordinary malformed PDF fails. Absence remains `None` even when requested, consistent with the optional response field. No raw-version-1-PDF parsing under version 2 (`envelope.rs:145–148,246`) |
| `szlahu_id` observed header | `CreatedInvoice.document_id`, optional nonnegative i64; unusable auxiliary id becomes None (`envelope.rs:30–38,331–342`). Credit's projection omits it; C3 does not require a document-id field |
| `szlahu_error_code`, `szlahu_error` | Raw code, once-decoded message; body-only errors supported after header/status checks (`wire.rs:251–310`, `xml.rs:510–546`) |
| Deletion result | True becomes unit; false/335 remains `Api(ProformaNotFound)`, not replayed success. No result count/list is specified (`proforma.rs:82–87,119–153`) |

XML numeric parsing accepts exact finite decimal/exponent forms; header parsing additionally accepts ungrouped decimal comma and HTTP SP/HTAB padding (`envelope.rs:344–370`, `number.rs:63–140`). Blank body optional values permit fallback; malformed nonblank body numbers fail rather than silently use headers. Invalid/blank present monetary headers fail. Precision overflow/underflow is rejected rather than rounded. NaN/infinity and values beyond Decimal are outside the supported money domain; no useful account emission requiring them was established.

Response ordering is not XSD-validated: the reader tolerates reordered known fields and well-formed extensions, while rejecting malformed XML, wrong roots/namespaces and duplicate/nested scalar identity. Foreign subtrees cannot supply a protocol verdict or number. This tolerance does not alter request ordering requirements.

### Storno issuance, warning and no-op semantics

`parse_reply` (`envelope.rs:179–249,285–315`) distinguishes the true envelope verdict from optional payload decoding. **Only numbered code 56** is promoted to a document with `notification_delivery_failed=true`; ordinary errors are retained. A unique body number survives malformed optional metadata on this warning path. A missing body number may fall back to a header; malformed/duplicate body identity may not. Header 56 may use plain notification text/empty body, but not malformed XML concealing a body refusal. A non-56 body refusal wins over header 56. Numberless 56 stays an API error with unknown outcome.

This exception is freshly corroborated by [PHP response guidance][P4] and the [linked official PHP 2.12.4 package][PHP-ZIP]: `szamlaagent/src/szamlaagent/Response/InvoiceResponse.php:17` defines `INVOICE_NOTIFICATION_SEND_FAILED = 56`; `:319–321` says and implements `hasInvoiceNumber() && hasInvoiceNotificationSendError()` as no issuance error. This was read from a fresh in-memory ZIP acquisition; PHP was not executed. It is first-party implementation evidence, **not a newly observed storno reply**. Neither credit nor deletion receives this issuance exception merely because a number is present.

`CreatedInvoice::reverses` (`envelope.rs:61–87`) is explicitly a **heuristic**, not identity verification: changed number and gross ≤0. Changed number with absent or positive gross still parses successfully, but the predicate is false/inconclusive and rustdoc directs a query of type/original reference. No-op same-number echoes remain parse successes as observed. Zero is a synthetic comparison policy, not live proof of zero-total original acceptance. The current docs correctly avoid interpreting false as “nothing reversed.”

Known observed storno codes 14/221/352 and credit code 463 are typed; deletion 335 is documented and typed. Maintenance 1, signing 55, numberless 56, absent/unknown codes, parse/transport/status failures remain uncertain rather than automatic resend permission. Code 7 is a general missing-data/not-found class, not proof of a particular selector or earlier operation's non-execution. `src/recovery.md:4–19,36–55` correctly distinguishes the exchange from earlier sends and distinguishes credit state from invoice existence.

## 4. Accepted live deviations and evidence boundaries

Behavior evidence is the repository's existing record for **one TEST account**, September 3/6/7, with raw historical logs outside the repository (`docs/szamlazz-hu-behaviour.md:3–28`). No fresh live verification is claimed.

| Behavior / apparent discrepancy | Evidence and disposition |
|---|---|
| Storno external id belongs to created SS with original number supplied, repeat ignores a new id | Behavior `:69–70,95`, B6/B4x/XPRB-P4/P48-P6. Preserve `storno.rs:86–98,183` despite request-page wording suggesting original lookup |
| Non-today issue date refused with 352, including paper | `:90`, B3. Preserve omission/default recommendation (`storno.rs:99–107`); historical sample date is not a server acceptance guarantee |
| Omitted fulfillment inherits original; explicit matching or mismatching dates accepted | `:92–96`, P48. Preserve optional date and original-date recommendation (`storno.rs:108–121`). An explicit value does not create a general server validation guarantee |
| Storno appearance follows its request even when original differs | `:97–98`, P73. Preserve explicit bool and original-appearance advice (`storno.rs:59–73`); false default is paper, not inherited |
| Repeat storno echoes existing SS; D/SL storno echoes original unchanged | `:86–87`, B4/B5. Preserve response interpretation and qualified heuristic; fresh positive-total documentation example proves no original/reversal relationship |
| Credit entries removed from original by reversal; storno outstanding is full negative gross | `:79–80`, B1/B8. Preserve documentation; no credit-carryover or paid-netting inference |
| Paid D deletes; success has no headers; missing/repeated/consumed D returns 335 | `:105,109–110`, D1–D4. Preserve caller-owned paid guard and dedicated deletion verdict; 335 is not replayed success |
| Replace versus additive and at most five submitted entries; queried ordering differs | `:133–134`, D7. Preserve explicit false and bound; returned entry order is not submission identity |
| Reversed-invoice credit error is body-only 463 | `:135,141`, D8. Preserve body verdict handling; no header-only error detection |
| Comma money in headers | `:160`, P60. Correctly supported without accepting comma in XML amounts |
| Empty replace previously not offered | `:211–215` is stale about capability after `ClearCreditEntries`; no execution evidence in this note establishes clearing. Current rustdoc and investigative tests explicitly preserve that uncertainty |

Current live source was read, not run: `tests/live.rs:44–134` covers populated replace/additive, queried balances, matching paper storno date/appearance, external id and repeat; `:138–169` covers number-selected proforma lifecycle. `tests/probes.rs:15–66` contains appearance-mismatch scenarios; `:69–145` contains populated/already-empty clearing probes. Their existence is not a passing execution record. The behavior note's former `eszamla_semantics` test-name references are stale.

Other unverified facts remain appropriately bounded: actual notification delivery/code-56 shapes, zero/negative-total originals, buyer tax-number supplementation, issuer-tax-number matching, multiple-match rollback, arbitrary aggregator/guardian combinations, live-account date/appearance rules, and empty-additive effects. No synthetic control settles these.

## 5. Source conflicts and non-findings

1. **Storno external-id wording:** [EN request][S1] and [HU request][S1-HU] require the original number yet allow referencing it by external id. [XML example][S2] says the field allows later queries without clearly naming original versus SS. The observed assignment is decisive only for the documented number-present path. Do not add an external-id-only selector or reuse the original's id on the strength of this prose.
2. **Malformed current examples:** fresh S3/C3 EN/HU success bodies contain raw `&` in `vevoifiokurl`; S3 also abbreviates base64 with `....`. Refusing literal samples is correct. `tests/upstream.rs:445–510` retains original defects, then explicitly escapes the URL and substitutes a synthetic PDF to test remaining fields. These are not reconstructed real exchanges. Both pages now have structured examples; historical “only text example” leads are obsolete.
3. **Metadata parity:** response prose says **“the same data is also in the XML body”**, but no payment-method element exists in either response XSD. Header-only payment-method support matches the concrete field definition. Header URL/method encoding is less explicit than invoice-number/error encoding; decode-once behavior is tested library policy, not a captured universal guarantee.
4. **Template labels:** Agent [template table][T] says `SzlaAlap` = traditional and `SzlaNoEnv` = envelope-friendly. Its linked [knowledge base][T-KB] states the reverse and also shows inconsistent tag/token casing. Preserve exact XSD/Agent tokens; rendering was not examined, so no token swap is recommended. This affects the shared type, including storno.
5. **Broken deletion schema links:** example request and response `schemaLocation` URLs return 404 when fetched over HTTPS. The alternative request download D4 works and agrees with inline/legacy schemas. The adjacent response candidate also returns 404; the complete inline response XSD is the reviewed source. No successful response download is claimed.
6. **PHP versus XML defaults:** PHP credit additive=true and its date/title/amount defaults are wrapper choices, not missing Rust defaults. PHP storno's template `int` is its wrapper interface, while the Agent XML template is a string token. HU credit's issuer-tax-number explanation overrides no wire field: it clarifies the EN example's misleading “incoming receipt” translation.
7. **Batch deletion versus generic one-document language:** the specific HU order-deletion note/PHP page defines all-match deletion. The generic sending page's “one XML per document” is not evidence that order deletion targets only one match.
8. **Deliberate supported subsets:** finite Decimal/civil dates, ignored copies as u8, version 2, fixed SS, exactly one deletion selector and an explicit clear-intent type are not defects merely because XSD admits a larger structural/value domain. No useful omitted operation was established from those differences.

## 6. Shared-transport coordination

Relevant concerns to retain in the shared review, without asserting an additional operation defect:

- **Status/header/body precedence:** `wire.rs:291–310` applies nonblank down header, nonblank error-code header, known non-2xx status, then body. Consequently body-only 463 at HTTP 200 is typed, but the same XML at 500 without an error header is `HttpStatus`. Numbered header 56 is judged by the storno parser before status. The vendor pages do not define contradictory-channel precedence; current regression tests explicitly label these combinations as synthetic policy (`tests/response_headers.rs:148–224`). Do not import an older different precedence finding as current behavior.
- **Incomplete response evidence:** `client.rs:385–405` retains status/headers in `IncompleteResponse` if body reception fails; operation parsing runs only after full receipt. This can leave numbered storno evidence in an uncertain transport result. Retaining evidence for reconciliation is not proof the envelope completed; do not silently promote an interrupted body to issuance success.
- **Retry semantics:** operation modules have no retry loop. [General errors][B3] limits the same request to five sends and forbids retry-until-success loops; this is separate from five credit entries. Custom HTTP retry policy can resend before operation reconciliation (`src/recovery.md:4–9`). Especially preserve uncertainty for additive/replace/clear and future order-deletion matches. Timeout or a query returning absent is not rollback evidence.
- **Optional payload/error preservation:** current numbered-56 and malformed-diagnostic regressions cover the earlier leads: ordinary body refusal is not hidden behind broken optional metadata; body identity is not replaced by a header when structurally ambiguous. No new failure was reproduced. Credit's shared payload declares `pdf` even though C3 does not: malformed nested/repeated unexpected PDF can fail payload decoding, an extension-tolerance boundary rather than a missing documented credit field.

No independent complete transport, cookie, browser/wasm, retry-budget or parser-security audit is claimed here.

## 7. Executed offline verification and remaining test gaps

1. `cargo test --offline --locked -p szamlazz-agent --lib --test clear_credit_entries --test response_headers --test response_completion --test response_namespaces --test numeric_fidelity --test upstream`
   - **233 passed, zero failed/ignored/filtered:** library 185; clear 2; headers 14; completion 4; namespaces 11; numeric fidelity 6; upstream 11.
   - Includes 11 storno, 12 credit, 7 deletion unit tests, shared envelope cases, completion/namespace controls and the existing September response-example tests. The upstream corpus was present; corpus checks ran.
2. `cargo build --offline --locked -p szamlazz-agent` — passed.
3. `cargo test --offline --locked -p szamlazz-agent --features client-reqwest --test client clears_credit_entries_only_through_explicit_request` — **1 passed**, eight unrelated tests filtered. Loopback mock verifies unfinished registration sends nothing, explicit clear POSTs once with the correct action, issuer/aggregator/order/version and no entries, and parses its balance. No vendor connection.
4. A scratch Rust program compiled from standard input with `rustc --edition=2024 -`, linked against `target/debug/libszamlazz_agent.rlib`, produced `/tmp/opencode/mutations-61c334f-check`. **All assertions passed:** Q1's populated replace/additive and explicit-clear numberless-refusal/header-success controls; storno changed-number missing/positive/negative gross parsing and heuristic; deletion true/1 versus false/0 verdicts. No credential object, request send or source file was required.
5. `git diff --check` passed. `git diff --stat 61c334f -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.lock` showed only the disclosed shared-wire helper extraction.

**Coverage limits:** manual complete field/type/cardinality/order comparison against freshly fetched schemas; no full XSD validator was executed. Golden tests assert selected serializer outputs, while upstream outline comparison deliberately ignores empty containers and trims text (`tests/upstream.rs:1206–1234`), so it is not proof of omission/empty equivalence. Optional storno fulfillment/external-id/buyer-tax fields were inspected in the writer, but there is no single exhaustive all-options XSD execution in the tests run. Clearing is covered by offline intent/wire/error tests, not successful vendor probes. Body/header conflicts, malformed metadata and numberless success controls are synthetic, not observations of vendor emission.

No live/probe target, account configuration, secret file, email-delivery check, template-rendering PDF, or mutation-recovery worker test was run. Broader unit tests passing does not expand this report's semantic audit beyond the requested mutations and their supporting paths.

## 8. Fresh source register

All linked sources below were fetched during this audit. Current docs displayed `v202608271632`; legacy `/xsd` pages displayed `v202606031507`. These are site-build labels, not assertion publication dates. Source quotes above refer to named sections/code blocks rather than unstable rendered line numbers.

| ID | Source and content inspected |
|---|---|
| S1 | [Storno request EN][S1], [HU][S1-HU]: full requirements and HTML form example |
| S2 | [Storno XML EN][S2], [HU][S2-HU]: full request example and inline XSD, including optional settings |
| S3 | [Storno response EN][S3], [HU][S3-HU]: both versions, headers, text-error example, structured success/error examples and complete inline response XSD |
| S4 | [Storno request download][S4], [legacy XSD page][S4-L]: full schemas |
| C1 | [Credit request][C1]: full requirements/form |
| C2 | [Credit XML EN][C2], [HU][C2-HU]: full examples/inline schemas; HU's named root type is structurally equivalent |
| C3 | [Credit response EN][C3], [HU][C3-HU]: full versions, header table, examples and operation-specific PDF-free response schema |
| C4 | [Credit request download][C4], [legacy XSD page][C4-L]: full schemas |
| C5 | [Credit IPN page][C5]: payment-status trigger, body form encoding, account configuration and delivery policy; distinct from synchronous credit result, receiver implementation outside scope |
| D1 | [Deletion request][D1]: full requirements/form |
| D2 | [Deletion XML EN][D2], [HU][D2-HU]: both selector examples, inline request XSD and HU all-matches note |
| D3 | [Deletion response][D3]: full successful/335 examples and inline response XSD, critical text/HTML behavior |
| D4 | [Working deletion request download][D4], [legacy XSD page][D4-L]: full schemas |
| X | [Shared invoice-response XSD download][X]: full schema including optional PDF |
| B1–B3 | [Authentication][B1], [sending requests][B2], [errors][B3]: credential alternatives, dispatch/validation, known errors and retry limit |
| T/E/I | [Template/language][T], [linked template knowledge base][T-KB], [notification][E], [simplified image][I]: relevant shared settings and conflicts; no template preview PDF fetched |
| P1–P4 | [PHP storno][P1], [credit][P2], [deletion][P3], [response handling][P4]: displayed examples, wrapper defaults, batch/notification semantics |
| PHP | [PHP index][PHP], [linked 2.12.4 ZIP][PHP-ZIP]: download provenance; fresh in-memory read of notification constant/error handling source, no PHP execution |

Failed public downloads: `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd`, `https://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd` (the examples' HTTP locations fetched over HTTPS), and adjacent candidate `https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdelvalasz.xsd` — all **404**. Request replacement D4 succeeded; response schema evidence is inline D3.

**Bottom line:** preserve the current request writers, current storno evidence handling and accepted test-account deviations. Resolve Q1's success-specific credit echo contract; do not mistake source conflicts or synthetic parser controls for newly verified vendor behavior.

[S1]: https://docs.szamlazz.hu/agent/reversing_invoice/request
[S1-HU]: https://docs.szamlazz.hu/hu/agent/reversing_invoice/request
[S2]: https://docs.szamlazz.hu/agent/reversing_invoice/xml
[S2-HU]: https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml
[S3]: https://docs.szamlazz.hu/agent/reversing_invoice/response
[S3-HU]: https://docs.szamlazz.hu/hu/agent/reversing_invoice/response
[S4]: https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd
[S4-L]: https://docs.szamlazz.hu/agent/reversing_invoice/xsd
[C1]: https://docs.szamlazz.hu/agent/credit_entry/request
[C2]: https://docs.szamlazz.hu/agent/credit_entry/xml
[C2-HU]: https://docs.szamlazz.hu/hu/agent/credit_entry/xml
[C3]: https://docs.szamlazz.hu/agent/credit_entry/response
[C3-HU]: https://docs.szamlazz.hu/hu/agent/credit_entry/response
[C4]: https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd
[C4-L]: https://docs.szamlazz.hu/agent/credit_entry/xsd
[C5]: https://docs.szamlazz.hu/agent/credit_entry/other
[D1]: https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request
[D2]: https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml
[D2-HU]: https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml
[D3]: https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response
[D4]: https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd
[D4-L]: https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd
[X]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[B1]: https://docs.szamlazz.hu/agent/basics/authentication
[B2]: https://docs.szamlazz.hu/agent/basics/sending-requests
[B3]: https://docs.szamlazz.hu/agent/basics/error-handling
[T]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template
[T-KB]: https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok
[E]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification
[I]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency
[P1]: https://docs.szamlazz.hu/php/sztorno-szamla-generalas
[P2]: https://docs.szamlazz.hu/php/jovairas
[P3]: https://docs.szamlazz.hu/php/dijbekero-torles
[P4]: https://docs.szamlazz.hu/php/valasz-feldolgozas
[PHP]: https://docs.szamlazz.hu/php/
[PHP-ZIP]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
