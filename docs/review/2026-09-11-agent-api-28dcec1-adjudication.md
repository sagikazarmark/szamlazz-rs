# Independent Számla Agent adjudication at `28dcec1`

**Reviewed:** 2026-09-11. **HEAD:** `28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`.

## Decision

**The specialists' blanket “no P2 defect” disposition is too dismissive for the requested API-completeness standard.** Credit/PDF reported identity, taxpayer validity, and explicit `fizetve=false` have demonstrable representation/acceptance gaps against the published contract. They deserve implementation work, with vendor occurrence and business consequences explicitly left unproven. Documenting a restriction in Rust does not establish a vendor exemption.

Conversely, **do not promote a numberless storno acknowledgement to a verified reversal, and do not call customer-URL corruption a captured vendor defect.** Fresh inspection found an important omission in the PHP comparison: the public customer-URL getter performs another decode after the cited `rawurldecode` assignment.

P2 below means normal-priority implementation work for contract completeness, not a demonstrated production incident. P3 means diagnostic/documentation work. Confidence in a code/schema mismatch is distinct from confidence in vendor emission or financial effect. No P0/P1 incident was established.

| ID | Adjudication | Priority / confidence | Demonstrated versus unknown |
|---|---|---|---|
| A1 | Numberless successful register/clear rejected: **confirmed published-contract acceptance gap** | P2; high for gap | Required reported number is local, not a published success-specific rule. Actual numberless vendor success unobserved. |
| A2 | Successful PDF plus no reported number rejected: **confirmed published-contract acceptance gap** | P2; high for gap | PDF retrieval need not manufacture reported identity. Actual vendor occurrence unobserved. |
| A3 | Numberless storno: **acknowledgement representation gap; conservative reversal decision justified** | P2 with response-model work; high for gap and need to retain uncertainty | Bare success is admitted by the envelope; it does not identify a reversal. Observed success-shaped no-ops prevent automatic promotion. |
| A4 | `OK` without taxpayer validity rejected: **confirmed schema-coverage gap, with a success-semantics qualification** | P2; high for code/cardinality, medium for intended successful omission | Both NAV schemas and the linked PDF table make validity optional. Narrative/examples normally supply true/false; actual Agent omission unobserved. |
| A5 | Explicit `paid=false` cannot be sent: **confirmed request capability omission** | P2 for requested completeness; high | No default/equivalence rule justifies collapsing absence and false. Wrong paid status or cash-specific effect unproven. |
| A6 | Taxpayer diagnostics omitted: **confirmed partial response projection** | P3; high | Header/software and successful result diagnostics are lost. Notification forwarding frequency is unknown; declared support should not depend on a capture. |
| A7 | Customer-URL header form decoding versus raw decoding: **unresolved encoding contract; conditional local corruption demonstrated** | P2 clarification; high for local behavior, low for an established vendor defect | PHP's initial raw decode is not its final public behavior. No exact vendor wire grammar or meaningful-plus capture establishes the intended result. |
| A8 | Broad code-56 promise: **confirmed documentation defect** | P3; high | Numbered credit/clear 56 remains an error; PDF drops the flag. Do not copy the issuance exception into unrelated operations. |
| A9 | Receipt provenance: **confirmed stale evidence summary** | P3; high | 337 and completed-create 338 were observed September 11, but only **337** is among the thirteen #195 additions. |
| A10 | `action-agent_ceg_mb`: **real documented capability, explicitly excluded scope** | Informational; high | Eleven mainstream built-in actions are not the entire documented surface. No requirement to add delegated onboarding under the current non-goal. |

## Basis and independence

Read all six `2026-09-11-agent-api-28dcec1-{invoices,queries,mutations,receipts,taxpayer,transport}.md` reports, then inspected the actual production paths and dated execution records. Older `77d53c5` reports were not used to adjudicate these conclusions. All preexisting reports remain intact.

Primary pages, NAV schemas/PDF and official PHP ZIP cited below were retrieved afresh in this adjudication. Site pages displayed `v202608271632`; that is a documentation build, not an execution date. A small independently written public-parser executable reproduced the disputed local behavior against a fresh workspace-locked library build. No additional agents, authenticated/live requests, product changes or broad test suite. The parent owns broad testing.

The operative distinction is:

1. **Contract support:** Does the type/writer/parser represent what the applicable published contract permits? A clear gap can be actionable without a production capture.
2. **Operational fact:** Does szamlazz.hu actually emit that shape, and what effect did the request have? XSD validity and synthetic tests alone cannot answer this.
3. **Evidence-backed deviation:** A bounded observation can justify different interpretation where the vendor contradicts its documentation. Observing two numbered successes cannot establish that all successes must be numbered; an existing Rust restriction cannot establish it either.

Schemas that cover success and failure together require semantic interpretation. They are not proof that every combinatorially valid body is a meaningful completed operation. Equally, “shared schema” is not evidence of an unstated conditional requirement. Where identity or validity is absent, a faithful response model can preserve the acknowledgement and the missing fact separately.

## A1–A4: acknowledgement, reported identity and validity

### Credit registration and explicit clearing

**Code:** `crates/szamlazz-agent/src/ops/credit_entry.rs:247–249` delegates clearing to registration; `:256–258` requires `InvoiceBalance.invoice_number`; `:316–322` accepts the shared verdict then fails if neither body nor header supplies a number. `ops/envelope.rs:120–133,326–329` resolves only nonblank reported identity, body first.

Fresh [credit response documentation][credit] says extra headers **may** arrive, describes structured version 2, and expressly warns that `minOccurs="0"` elements may be absent. Its operation-specific XSD requires `sikeres`, makes `szamlaszam` optional and has no conditional success-number rule. A complete HTTP-200 envelope containing only `<sikeres>true</sikeres>` returns `Parse(Missing("szamlaszam"))` in both current parsers.

**Uphold the mismatch; overturn mutation Q-01's clarification-only disposition.** Request identity already selects the target, but is not an echoed fact. Return an acknowledged registration/clear with optional reported number and optional balance metadata, or an equivalent explicit response variant. Never populate “reported number” from the request. Missing optional metadata alone should not turn a readable acknowledgement into an unparseable answer.

This does not prove a live mutation was misreported or authorize repeating an additive/replacing write. The [clearing record](../research/2026-09-11-credit-clearing-live.md):14–32,64–74 establishes two successful, numbered, verified clear operations; raw channels were not archived. Neither those observations nor numbered examples create a universal echo guarantee. A later acknowledgement also does not settle an unrelated earlier unresolved execution.

### PDF query

**Code:** `ops/query_pdf.rs:39–41,83–92` requires an invoice number via `parse_issued`; `ops/envelope.rs:211–216,275–279` recognizes an unnumbered successful envelope internally, then rejects it. The [PDF response page][pdf] defines version 2 as structured XML with base64 PDF, says headers may arrive, and makes the number optional with the same explicit cardinality warning.

**Uphold query A1's reproduction; upgrade from P3 clarification to P2 contract support.** Expose a fetched PDF with optional reported identity, retaining number/order/external-id request provenance separately. A read does not issue a numbered document; routing it through an issuance-only result imposed the extra condition. Missing PDF can still fail a result promising an artifact. The scratch `%PDF-` bytes isolate the identity gate, not PDF validity; the gate is independent of PDF contents, so a complete PDF would meet the same number requirement.

No numberless vendor PDF was captured. The action is justified by published response support, not a claim that tested downloads failed.

### Storno

**Code:** `ops/storno.rs:218–220` delegates to `parse_issued`; `ops/envelope.rs:275–279` refuses numberless success, and `:61–87` separately qualifies the reply-only reversal heuristic. The [storno response page][storno] also declares optional number and totals.

**Correct both extremes.** The shared schema gap is real here too; “rightly not promoted to a verified reversal” does not establish that discarding the acknowledgement/PDF/metadata is a complete raw API implementation. Preserve a successful but unnumbered acknowledgement distinctly from a numbered result. Do not weaken `CreatedInvoice`'s numbered invariant merely to return success, and do not label the unnumbered case `reversed` or `issued`.

There is relevant live evidence for a conservative *domain decision*: `docs/szamlazz-hu-behaviour.md:105–106` records repeat storno echoing an existing SS, and storno of proforma/delivery note returning `sikeres=true` with the unchanged original number and no reversal. It proves that the success flag alone is insufficient, **not that a number is universally present**. Numberless acknowledgement requires reconciliation of the exact original and matching reversal evidence; it is never a preview merely because the internal `Reply::Unnumbered` comment currently describes previews. No resend permission follows.

### Taxpayer validity

**Code:** `ops/taxpayer.rs:188–190` exposes a `bool`; internal validity is already optional at `:294`; `:567–578` parses the four boolean tokens; `:594–600` requires validity on `OK`. The independent executable reproduced `Missing("taxpayerValidity")` in both supported namespace layouts.

The [Agent response page][taxpayer] says the response matches NAV's taxpayer response type and explicitly links the [NAV 3.0 PDF][navpdf]. Fresh NAV 2.0 [`invoiceApi.xsd`][nav2]:1668–1697 (validity at 1682) and NAV 3.0 [`invoiceApi.xsd`][nav3]:1552–1581 (1566) both specify `minOccurs="0"`, without default or conditional assertion. The PDF's printed **p. 67** likewise says validity is not mandatory (`nem`).

**Counterevidence matters:** printed **p. 68**, rule 1, says true for an existing tax number and false for invalid/nonexistent numbers. Agent's true/false examples include the element. This describes normal semantics; it supplies no example of meaningful successful omission and prevents interpreting absence as “not found.” It does not erase the explicit optionality. The appropriate disposition is an actionable schema-coverage gap with a success-semantics clarification, not “documented local policy, therefore no defect.”

Represent unreported validity explicitly (`Option<bool>` or a named unknown state) while preserving `OK`, data and diagnostics. Keep absent/invalid `funcCode` distinct from `OK` without an optional fact; keep malformed boolean text distinct from omission. Never coerce omission to false or silently treat unknown validity as verified for invoicing. An application requiring a definite validity can gate after parsing. No live missing-validity Agent reply is established.

## A5: explicit unpaid request capability

**Code:** `ops/invoice.rs:178–180,230,252` uses/defaults a false `bool`; `:823–825` writes only true. `src/ops.rs:18–22` discloses the PHP policy and unverified equivalence. Fresh [EN][invoice-en] and [HU][invoice-hu] inline schemas and the [download][invoice-xsd] (line 124) all declare optional boolean `fizetve`, **without a default**. Official PHP `Header/InvoiceHeader.php:393` also writes only true.

**The invoice specialist correctly identifies a capability omission and correctly refuses to invent its financial effect; its wait-for-semantics recommendation is too weak for the requested completeness.** Expose omission, explicit false and true independently. Preserve omission as the default if that is the chosen migration policy. Supporting explicit false does not require guessing what omission does or switching all existing false values to emitted false.

No recorded omission-versus-false comparison supports equivalence, a cash override, or incorrect outstanding amounts. PHP corroborates a wrapper choice, not a server exception to the wire capability. This is a request expressiveness issue, not demonstrated financial corruption.

## A6: taxpayer diagnostic projection

**Code:** `ops/taxpayer.rs:340–397` recognizes only selected fields; root header/software and result notifications are excluded. `:563–566` reads result code/message, but `:596–610` discards them on OK; `:612–622` reduces non-OK to `ApiError { code, message }`. `TaxpayerInfo:188–225` has no exchange metadata. `client.rs:403–405` returns the projection without the complete `RawResponse`.

Agent's own [success example][taxpayer] contains header request ID/timestamp/version and software metadata. NAV [Common 1.0][common]:616–647 declares result code, optional message and notifications; `:668–701` declares repeated notification code/text. These are within the supported taxpayer response's inherited structure, unlike arbitrary direct NAV error roots.

**Uphold T-1's local result, but record a genuine partial response surface rather than dismissing it because it is not a “business field.”** Add typed exchange/result metadata and notification preservation, or a response wrapper providing that information alongside `TaxpayerInfo`; preserve relevant diagnostics on failures as well. This is lower priority than rejecting a whole acknowledged operation. Supporting declared optional diagnostics need not await a live occurrence. The synthetic 3.0 control proves their loss if supplied; it does not prove current Agent forwarding. The 2.0 notification control is extension tolerance only, not a 2.0 declaration.

**Do not upgrade T-2 into a confirmed defect:** Agent promises `QueryTaxpayerResponse`, including its own error example. Direct NAV `GeneralErrorResponse`/`GeneralExceptionResponse` forwarding through Agent is not established by that promise or by the existence of direct NAV schemas. Supporting them may be robustness work; first obtain evidence that they belong to this intermediary's contract. Header/software omission in Agent's own error/false examples also means new preservation must not impose their NAV-required cardinalities on every Agent reply.

## A7: customer URL — the PHP comparison was incomplete

**Rust:** `wire.rs:239–249,343–351` applies form decoding (`+` → space, then percent decoding); `ops/envelope.rs:137–142` uses it for `szlahu_vevoifiokurl`. Body URLs get XML decoding and take precedence. This reaches creation/storno, credit/clear and PDF results through their shared projection.

The independent control supplied the header:

```text
https://example.test/a+b?q=%2B&escaped=%252B
```

Rust returned `https://example.test/a b?q=+&escaped=%2B`. Putting the same URL in XML with `&amp;` instead preserves `a+b`, `%2B` and `%252B`. Those transformations are demonstrated.

**Fresh PHP 2.12.4 source changes the evidential conclusion:**

- `Response/InvoiceResponse.php:136–137`: stores `rawurldecode($headers['szlahu_vevoifiokurl'])`.
- `:354–355`: setter merely assigns the value.
- **`:347–348`: public `getUserAccountUrl()` returns `urldecode($this->userAccountUrl)`**.

Thus the complete PHP header-to-getter path is two decodes, the second form-style. A raw literal `+` becomes a space there too; `%2B` becomes `+` in storage and space through the getter; `%252B` becomes `%2B` then `+`. This is source-traced behavior, not executed PHP or vendor evidence. It defeats any claim that the reference client's *public result* proves preservation of literal plus. Do not copy its double decoding either.

The fresh [invoice][invoice-response], [credit][credit] and [storno][storno] header tables explicitly label number/error encoding, but call this field only “Customer account URL (if enabled).” They do not specify form versus percent-only outer encoding or the number of encoding layers. PHP therefore suggests competing implementation choices, not an authoritative grammar.

**Disposition:** retain an actionable encoding clarification. Ask for the raw grammar and examples distinguishing `+`, `%2B`, `%20` and `%252B`, including path and query components. If percent-only decoding is confirmed, introduce a URL-specific decoder; do not change invoice-number/error-message decoding globally. A form decoder certainly alters a literal-plus URL if that is what the header means, but neither that antecedent nor a real broken customer link is established. The transport report was right not to call this a captured incident, but its `rawurldecode` comparison was incomplete. Current Rust documentation is likewise not proof the form interpretation is correct.

## A8–A9: documentation defects and precise receipt provenance

### Code 56

`error.rs:369–372` says 56 is an error **only** without a document number, and README `:414–415` repeats the broad numbered-success promise. This contradicts `credit_entry.rs:316–317` and `xml.rs:528–553`: registration/clear retain numbered 56 as an API error. The scratch check confirms this, while storno yields `notification_delivery_failed=true` for the same complete numbered envelope. PDF also uses the tolerant parser but projects no flag (`query_pdf.rs:83–92`). A missing PDF can still fail PDF retrieval; malformed identity or a body-only 56 behind a non-2xx status can also remain an error despite number text.

**Uphold mutation M-01; make it an explicit documentation fix rather than incidental maintenance.** Scope the warning-success rule to the appropriate issuance interpretation and accepted response shapes. Preserve `Unknown` for surfaced 56; a number alone does not prove a credit effect. Fresh PHP `InvoiceResponse.php:319–323,427–431` corroborates numbered invoice notification failure, not every operation or every HTTP/body shape. No executed 56 response was established. Query-specific 56 semantics remain an unresolved shared-parser design question, not evidence to change all parsers.

### Receipt observations

`error.rs:373–374` says none of the thirteen receipt/simplified-image additions was observed; `recovery.md:57–62` describes their documentation origin without the subsequent observations. The [receipt record](../research/2026-09-11-receipts-live.md):29–37 documents direct 337 for the seven-character prefix; `:49–59` records a completed-create duplicate returning 338. These are transcribed executions, not raw HTTP captures, on an operator-confirmed test account without asserted continuity with the historical account.

**Precision correction to the receipt specialist:** 338 is real observed receipt evidence, but **not one of the thirteen #195 additions**. `tests/error_classification.rs:62–135` lists the thirteen: 336, 337, 339, 340, 363–365, 551–556. `git show 76c8107 -- crates/szamlazz-agent/src/error.rs` independently confirms that 338 already existed. Therefore **337 alone directly falsifies the thirteen-additions claim**; mention 338 separately when updating the overall receipt evidence summary. The transport specialist's narrower 337 correction is accurate.

Retain their documentation-derived origin, add the dated 337 exception and separate completed-duplicate 338 observation, and leave the other twelve additions and 55/56 unobserved. Source origin and later corroboration are different facts. No classification change or claim about concurrent deduplication, retention, raw channels or production accounts follows. Receipt email 153 remains a separate bounded observation, not one of the thirteen.

## A10: excluded delegated onboarding action

Fresh [/agent/self_billing][onboarding-old] explicitly supplies `action-agent_ceg_mb` and `XmlCegMb`. The [current dedicated request reference][onboarding] confirms POST/multipart at the same endpoint for creating a company account or requesting connection to an existing one. It also permits an Agent key for identifying the calling delegate account, unlike the old inline schema's mandatory login/password fields.

No `action-agent_ceg_mb` or `XmlCegMb` implementation occurs in `crates/szamlazz-agent`; `src/ops.rs:34–43` exposes the existing operation modules. **Uphold taxpayer O-1:** this is an explicitly excluded wider capability (`CONTEXT.md:146–147`), not a taxpayer defect. Describe coverage as the eleven mainstream actions with delegated onboarding excluded, never “every documented action.” `AgentRequest` extensibility is not built-in implementation. Onboarding with an Agent key must not be confused with issuing later as the dedicated delegate user. No new onboarding implementation is required by this adjudication.

## Prioritized actions and corrections

1. **P2 — Make responses faithful to optional facts:** support numberless acknowledged credit/clear, unnumbered fetched PDF, and an unnumbered storno acknowledgement that remains unverified; model unreported taxpayer validity explicitly. Preserve request provenance separately and keep malformed facts/refusals distinguishable. Vendor clarification can refine guarantees, but a missing capture does not close these contract gaps.
2. **P2 — Expose explicit `fizetve=false` independently of omission.** Preserve the chosen default and document that its server-side distinction is unverified; do not claim a paid-state incident.
3. **P2 clarification — Resolve customer-URL outer encoding.** Correct the incomplete PHP premise now; make a URL-specific implementation change only against an established decoding contract. Never copy the PHP double decode.
4. **P3 — Preserve taxpayer exchange/result diagnostics**, including declared 3.0 notifications. Keep generic direct NAV roots as a separate unproven forwarding question.
5. **P3 — Fix code-56 scope and evidence prose**, including README, and update receipt provenance precisely: 337 is the newly observed #195 addition, 338 is separately observed preexisting functionality.
6. **Scope statement — Name the delegated onboarding exclusion.** Eleven built-in mainstream actions do not exhaust the vendor documentation.

The source-conflict findings about invoice preview/simple-items order, missing downloadable declarations, and the Hungarian PDF-request schema are not overturned here. Multiple official definitions disagree; a schema pass cannot identify which the server applies. This adjudication does not repeat those full matrices, nor turn a proposed order swap or malformed illustrative PDF into a proven fix. The explicit-clearing and receipt lifecycle/MNB/email records remain valid bounded execution evidence; they do not establish unrelated response cardinalities.

## Fresh sources and focused verification

All links below were fetched on **2026-09-11 by this adjudication**, rather than treating the reports' acquisition as fresh evidence. Inline paths/clauses and numbered downloadable source lines above are the durable locators.

| Artifact | Fresh SHA-256 |
|---|---|
| Official PHP 2.12.4 ZIP | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| NAV 3.0 API XSD at pinned revision | `268c923298fea89832699c509d57fbe3b28d1b2956322294cffc9840dd78e656` |
| NAV 2.0 API XSD at pinned revision | `eb765a8642979b215992b66176459f8c205c565923e6075cb31f7117014bdb88` |
| NAV Common 1.0 XSD | `0ad7a99292d9b5c967d0cf1f37ceafd9945ac456b534963c7c72a6e7bb42971c` |
| Linked NAV 3.0 PDF | `54fbc97f110a6c26348d1da5abc7047f12b94de140b21559afff40ad988048f2` |
| Downloaded invoice request XSD | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |

Executed successfully:

```sh
python3 /tmp/opencode/adjudication-28dcec1-sources.py
python3 /tmp/opencode/adjudication-28dcec1-nav.py
cargo build -p szamlazz-agent --locked --offline --lib
rustc --edition=2024 /tmp/opencode/adjudication-28dcec1-check.rs --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -L dependency=/home/laborant/szamlazz-rs/target/debug/deps -o /tmp/opencode/adjudication-28dcec1-check
/tmp/opencode/adjudication-28dcec1-check
```

The acquisition scripts fetched unauthenticated public documentation, inspected the PHP ZIP in memory and extracted the linked PDF via `pdftotext`. The Rust executable constructed synthetic HTTP-200 `RawResponse`s and asserted four numberless failures, header/body URL behavior, the credit/clear-versus-storno 56 distinction, both namespace layouts' missing validity, and successful diagnostic loss. It contains no HTTP client or credentials. Its minimal NAV controls isolate parser decisions and are not claimed as full schema-valid envelopes. No new full XSD-validation matrix, PHP execution, PDF rendering or vendor execution is claimed. The source/cardinality checks are independent of the specialists' test counts.

Final verification found HEAD unchanged and no tracked diff from the requested commit. The new-report whitespace check produced no diagnostics. A concurrently added `2026-09-11-final-project-28dcec1.md` was left untouched, alongside all preexisting reports. This adjudication is the only workspace file authored here.

[credit]: https://docs.szamlazz.hu/agent/credit_entry/response
[pdf]: https://docs.szamlazz.hu/agent/querying_pdf/response
[storno]: https://docs.szamlazz.hu/agent/reversing_invoice/response
[invoice-response]: https://docs.szamlazz.hu/agent/generating_invoice/response
[taxpayer]: https://docs.szamlazz.hu/agent/querying_taxpayer/response
[invoice-en]: https://docs.szamlazz.hu/agent/generating_invoice/xml
[invoice-hu]: https://docs.szamlazz.hu/hu/agent/generating_invoice/xml
[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd
[navpdf]: https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf
[nav3]: https://github.com/nav-gov-hu/Online-Invoice/blob/cc7a775d6dce361311e409abb9934eb755f2749c/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[nav2]: https://github.com/nav-gov-hu/Online-Invoice/blob/84442e64bc2cd7feb368fedb8199645188962b23/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[common]: https://github.com/nav-gov-hu/Common/blob/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd
[onboarding-old]: https://docs.szamlazz.hu/agent/self_billing
[onboarding]: https://docs.szamlazz.hu/third-party-invoicing/szamla-agent/request

PHP source archive: <https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip>; member paths above are relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`.
