# Current Számla Agent review — independent adjudication

**Revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`

**Date:** 2026-09-10

**Scope:** only T1, CM1, CQ-1/CQ-2, receipt H1/H2, and numberless credit-entry success from the six current reviews.

## Decision

**No remaining candidate establishes a normal-wire P2 defect. Retain four bounded P3 concerns: T1, CM1, CQ-2 and H2.** Keep H1's whitespace-only PDF behavior as an optional P3 consistency improvement, not a missing download capability. Demote CQ-1 to a shared URL-normalization policy/fidelity note. Keep numberless credit-entry success as a source ambiguity, with no confirmed runtime-defect severity.

P3 here means a lower-priority, bounded improvement supported by a reproduction; it does not imply vendor emission or a release blocker. An XSD-permitted synthetic input, a malformed XML input and an interrupted transfer are different kinds of evidence.

| Candidate | Inclusion / demotion | Classification and practical consequence |
|---|---|---|
| **T1** | **Retain P3; reject the new P2 ranking.** Same behavior as prior consolidated F3. | Incomplete-transfer evidence loss. Received number/status/diagnostics disappear; caller may need additional reconciliation or operator investigation. `Unknown` remains correct. |
| **CM1** | **Retain P3.** Strongest local contract inconsistency. | Schema-invalid optional metadata erases a readable body-only number under code 56, contrary to the existing leniency promise. Numbered warning becomes uncertain error 56. |
| **CQ-1** | **Demote from a counted defect to a policy/fidelity note.** At most P3 if exact URL text preservation is selected. | XML boundary whitespace is removed. No broken usable vendor link demonstrated; URL policy is explicitly outside the README's general business-text preservation promise. One shared concern across four operations. |
| **CQ-2** | **Retain P3, narrowly as an XML lexical-conformance concern.** | Illegal characters reach recognized business strings; some malformed ignored markup passes. Does not establish a normal-response failure, namespace/identity bypass, or a need for full XSD validation. |
| **H1: missing requested PDF** | **Exclude as a mandatory runtime finding.** | `None` already exposes absence while retaining the receipt. Stronger requested-artifact diagnostics are an API choice. |
| **H1: whitespace-only PDF** | **Optional P3 consistency improvement; not a headline defect.** | Blank text produces `Some(Pdf(0 bytes))`, unlike empty text. Normalize absence locally if desired; no PDF-content validity guarantee currently exists. |
| **H2** | **Retain P3 semantic robustness concern.** | Empty, schema-invalid reversal text becomes a definite `false`. This can influence recovery decisions, but no receipt-specific empty-means-false evidence or live occurrence was found. |
| **Numberless credit success / CA1** | **Retain as vendor clarification, not a confirmed defect.** | A schema-permitted success is rejected for missing echoed identity. If actually emitted, a completed registration becomes uncertain to the caller. Success-specific omission remains unestablished. |

## Evidence and review boundary

Read all six `2026-09-10-agent-api-current-{invoices,mutations,queries,receipts,taxpayer,transport}.md` reports. The invoice/taxpayer reports supplied context and evidence limits; their other conclusions were not independently re-audited here. Read the prior consolidated report's T1/F3 adjudication, rather than inheriting the transport specialist's priority.

Checked the actual Agent implementation and relevant public promises at the pin. HEAD matched at the initial and subsequent checks; the scoped diff against the pin was empty for `crates/szamlazz-agent`, `fixtures/SOURCES.md`, `docs/szamlazz-hu-behaviour.md`, `Cargo.toml` and `Cargo.lock`. Concurrent Restate source/test/design work and other untracked reports were preserved.

Source paths below are relative to `crates/szamlazz-agent/` unless otherwise stated. These are current-code citations, not the prior revision's line numbers. Source rules were checked directly through the focused public GETs listed below, the cached first-party PHP source, and the recorded behavior notes. No live Számla Agent calls, credentials, delegation, source/test edits or scratch-source edits. This report is the only authored file; verification generated ordinary build artifacts in existing scratch targets.

## 1. T1 — confirmed evidence loss, still P3

**Code:** [`src/client.rs:338–364`](../../crates/szamlazz-agent/src/client.rs#L338) collects status and headers at 349–359, then exits on `response.bytes().await?` at 360 before constructing `RawResponse`. [`ClientError::Transport`, lines 54–65](../../crates/szamlazz-agent/src/client.rs#L54), contains the reqwest error alone; [`outcome_class`, lines 82–89](../../crates/szamlazz-agent/src/client.rs#L82), returns `Unknown`.

**Independent execution:** reran the inspected existing loopback test `interrupted_bodies_still_discard_all_header_evidence`. It reads the complete outbound multipart request before sending complete HTTP headers, declares 1,000 body bytes, sends one byte and closes. All four cases returned `Transport` / `Unknown`, with reqwest `Decode` → `Body` → `IncompleteBody`:

- code 3 and `login` message;
- `szlahu_down: maintenance`;
- code 56 and number `I-2`;
- number `I-2` alone.

The direct complete-`RawResponse` control with numbered-56 headers and empty body returned the numbered warning. It demonstrates the separate parser policy, **not permission to substitute an empty body for a failed transfer**.

**Source rule:** the [create-response page][create] identifies the number and error headers as meaningful channels. [First-party PHP documentation][php] and `InvoiceResponse.php:17,319–322` support numbered notification-failure handling. Neither specifies how an incomplete HTTP exchange must be promoted to a verdict. [`src/wire.rs:291–310`](../../crates/szamlazz-agent/src/wire.rs#L291) defines the crate's completed-response precedence, not an incomplete-transfer rule.

**Challenge to P2:** the new report supplies four reproducible header combinations, but no newly demonstrated misclassification, automatic resend, vendor incident, or inability of ordinary complete responses to succeed. The established consequence remains loss of useful evidence while conservatively retaining uncertainty. That is exactly the prior consolidated [F3 P3 rationale](2026-09-10-agent-api.md#f3--retain-received-headers-on-incomplete-transfers). Broader reproduction does not itself raise severity. Transfer interruption can affect an otherwise ordinary vendor reply; it does not require the vendor to generate malformed XML, making this more operationally plausible than CM1, but its frequency and actual recovery cost are unmeasured.

**Bounded action:** retain received status/header evidence with the transfer cause and an explicit incomplete-body condition. A future operation-aware policy may interpret that evidence. Do not turn a bare number into settled success or remove the valid-body refusal precedence. Number-plus-56 identifies an issued document under the supported policy; for storno, that alone is still not proof of the intended reversal (`CreatedInvoice::reverses`, `src/ops/envelope.rs:61–87`).

## 2. CM1 — retain P3 for the existing leniency contract

**Code:** [`src/ops/envelope.rs:104–118`](../../crates/szamlazz-agent/src/ops/envelope.rs#L104) puts number and metadata in the same `Body`. [`203–217`](../../crates/szamlazz-agent/src/ops/envelope.rs#L203) replaces a failed payload decode with `Body::default()` under 56, then searches that default body and headers for a number. [`281–287`](../../crates/szamlazz-agent/src/ops/envelope.rs#L281) separately retains the verdict, but not identity.

Complete, correctly namespaced body; HTTP 200; no headers:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <szamlaszam>I-2</szamlaszam>
  <szamlabrutto><bad/></szamlabrutto>
</xmlszamlavalasz>
```

**Reproduced:** `StornoInvoice::parse` returns API error 56, without the number. Duplicate `<pdf>` metadata produces the same result. A malformed scalar total/PDF instead retains `I-2` and the warning; adding a number header rescues the structurally malformed case. The old opposite-direction bug remains closed: a readable body refusal 3 survives malformed metadata beneath header 56 on both create and storno.

**Why this is more than arbitrary strictness:** [`envelope.rs:173–177`](../../crates/szamlazz-agent/src/ops/envelope.rs#L173) explicitly says a malformed optional total/PDF must not hide numbered notification-failure issuance. The implementation distinguishes lexical from structural metadata failures without documenting that limit. The first-party number-conditioned rule supports preserving the usable number, but does **not** prove the vendor emits nested or duplicated metadata. The XSD permits neither nested children in `double` nor duplicate singleton PDF elements. CM1 is therefore a bounded malformed-response inconsistency, not a compliant-response failure.

**Scope and action:** one shared create/storno envelope finding. Preserve an independently validated, unique body identity before optional-metadata decoding, while retaining whole-document, namespace and refusal checks. Do not salvage an arbitrary number from duplicate/structurally invalid identity. Credit registration uses a different verdict path and does not promote 56. PDF query inherits the helper mechanically, but there is no source evidence of notification-failure 56 on a PDF read; do not count an additional demonstrated query incident.

The consequence is unnecessary uncertainty/reconciliation. Error 56 is already `OutcomeClass::Unknown`; the library does not automatically reissue. With a caller-owned `RawResponse`, the original bytes also remain accessible. The built-in `Client::send` exposes only the typed result/error.

## 3. CQ-1 — shared URL fidelity, with a material policy counterargument

**Behavior confirmed:** `<vevoifiokurl>&#160;opaque:x&#160;</vevoifiokurl>` becomes `Some("opaque:x")`. The same happens with ordinary boundary spaces and with NBSP around a synthetic HTTPS URL. [`src/xml.rs:559–571`](../../crates/szamlazz-agent/src/xml.rs#L559) calls Unicode `trim`; [`envelope.rs:114–115`](../../crates/szamlazz-agent/src/ops/envelope.rs#L114) selects that helper.

The [response schema][create] declares `string`, not `token` or a custom whitespace-normalizing restriction; [XSD string/whiteSpace rules][datatypes] do not require this trim. [`InvoicePdf::customer_account_url`, lines 50–53](../../crates/szamlazz-agent/src/ops/query_pdf.rs#L50), calls the value opaque. These support a low-priority exact-text fidelity argument.

**Counterevidence that the query report underweights:** [`README.md:237–244`](../../crates/szamlazz-agent/README.md#L237) explicitly exempts **URLs**, alongside envelope numbers, numeric/verdict parsing and base64, from the general business-text preservation policy. “Opaque” can mean callers must not interpret the URL's structure; it does not, by itself, settle handling of presentation padding. The code does not parse/rebuild the URL, percent-decode its XML value, or alter percent-encoded boundary characters. Independent controls retained literal `%20` and `%2B` at the end of HTTPS query values. No ordinary vendor URL whose useful target/token changes under this trim has been demonstrated.

**Decision:** document as a compatibility/normalization choice with a possible fidelity improvement, rather than a normal-wire defect or a breach of the general business-text promise. The schema-compatible NBSP marker establishes character loss; it does not establish a broken customer link. If the intended contract is exact decoded nonblank URL text, change the shared field adapter and explicitly test both channels. No blanket URI normalization or extra decoding follows.

### Count the shared scope once

All four consumers read the same `Body::vevoifiokurl` and [`Body::customer_account_url`, lines 135–143](../../crates/szamlazz-agent/src/ops/envelope.rs#L135):

| Consumer | Current path |
|---|---|
| Invoice create | `src/ops/invoice.rs:927–935` → `parse_reply` → `envelope.rs:245` |
| Invoice storno | `src/ops/storno.rs:211–213` → `parse_issued` → same field |
| Credit-entry registration | `src/ops/credit_entry.rs:250–276` → shared `Body` and URL accessor |
| PDF query | `src/ops/query_pdf.rs:83–92` → `parse_issued`, then projection |

Decoded **header** URLs are filtered for empty strings but not trimmed by this accessor. Body-before-header selection can therefore produce channel-dependent fidelity. This is one shared XML-field concern, not four findings, and not a full `<szamla>` query-model defect. Runtime reproduction here exercised the PDF entry point; the other three scopes follow directly from the shared call paths.

## 4. CQ-2 — retain the lexical boundary, without claiming a normal-wire defect

**Code:** [`src/xml.rs:63–167`](../../crates/szamlazz-agent/src/xml.rs#L63) scans the original document through EOF and checks root/namespace, nesting, comments and selected prolog/epilog conditions. Ordinary text/CDATA and general references inside the root have no comprehensive lexical validation there. Attribute iteration checks prefix resolution but does not establish every XML syntax rule. Namespace projection at [`174–229`](../../crates/szamlazz-agent/src/xml.rs#L174) and query deserialization at `src/ops/query_xml.rs:588–608` then consume selected content.

**Reproduced through `QueryInvoiceXml`:** literal NUL and `&#x1;` survive in `supplier.name`; ordinary `A]]>B` also succeeds. An unknown entity inside an ignored extension, adjacent quoted attributes without a separating space, literal `<` in an attribute, and an element name starting with a digit all pass. Python ElementTree independently refused the seven corresponding minimal cases. [XML 1.0 productions][xml] `Char`, `CharData`, `STag` and `AttValue` directly disallow the relevant spellings.

**Strongest consequence:** illegal characters reach a recognized public business string. A later XML export or persistence layer may refuse it. That consequence is conditional; no such downstream failure was executed. Ignored malformed extensions are weaker supporting evidence that parse success is not XML certification. Neither demonstrates that foreign fields can supply a valid identity/verdict or that well-formed vendor records are misread.

**Decision:** retain one P3 shared lexical-conformance concern, not one finding per grammar production or operation. Full XSD validation is a different capability: unknown well-formed extensions and sparse content can remain supported while XML lexical rules are checked. Conversely, the current README's specific EOF/root guarantees are not an explicit promise of exhaustive XML certification. If complete well-formedness is not an intended boundary, accurately describe the limitation rather than claim every accepted body is well-formed. Do not use this finding to impose XSD business requirements or broaden into generic parser hardening.

Existing namespace/completion tests passed independently. They close their specific failure cases, not every lexical gap. Taxpayer extraction has additional entity handling, so a shared root helper does not establish identical acceptance of every CQ-2 specimen on every operation.

## 5. Receipt H1 — split artifact absence from blank representation

**Code:** [`src/ops/receipt.rs:690–701`](../../crates/szamlazz-agent/src/ops/receipt.rs#L690) filters only `s.is_empty()`; [`src/types.rs:104–117`](../../crates/szamlazz-agent/src/types.rs#L104) removes whitespace and decodes standard base64. `Pdf` contains bytes and validates base64, not PDF document structure. Create/storno/query share this parser (`receipt.rs:293–295,364–366,449–451`), without passing `download_pdf`.

**Reproduced:** all three entry points with `download_pdf=true` preserve `NY-1` and return `None` for absent/empty `nyugtaPdf`, but `Some(0 bytes)` for whitespace-only text. Nonempty invalid base64 errors. Wrapped base64 decodes to `%PDF-`; that control is not a renderable PDF. The shared-operation matrix uses one synthetic `NY` body; the separate `SN`/original-reference control passed and does not establish live storno behavior.

**Source qualification:** [receipt-create prose][receipt] associates the PDF with the requested flag; [receipt-storno prose][receipt-storno] explicitly promises it when true. The [receipt XSD][receipt-xsd], however, types `nyugtaPdf` as optional **string**, not `base64Binary`. Whitespace-only text is therefore not itself malformed XML or an XSD lexical violation; even standard base64 can denote zero bytes. It fails the semantic expectation of a useful PDF. Calling all of H1 “malformed-response” without this distinction overstates the schema evidence.

**Decision:** preserve the earlier adjudication of missing PDF: `None` exposes the incomplete artifact without losing issuance identity. Returning an optional artifact is a coherent library contract; extra diagnostics or a flag-aware result are optional. For whitespace-only content, a localized P3 consistency improvement is reasonable because `is_some()` currently differs from the empty case and `save_to` can write an empty file. That is not evidence of normal downloads failing and does not justify a general PDF validator or a new fatal write error. Any artifact recovery should query the known receipt number, not create again.

## 6. Receipt H2 — retain P3, acknowledge the compatibility change

**Code:** [`Receipt::reversed`, `receipt.rs:555–558`](../../crates/szamlazz-agent/src/ops/receipt.rs#L555), is a definite bool. [`AlapXml`, lines 764–765](../../crates/szamlazz-agent/src/ops/receipt.rs#L764), uses [`xml::de::flexible_bool`, lines 587–598](../../crates/szamlazz-agent/src/xml.rs#L587), whose explicit empty-string arm returns false. This is existing leniency, not a new regression or an accidental missing-field default.

**Reproduced:** self-closing `<stornozott/>` returns `reversed=false`; removing the element or putting it in a foreign namespace instead fails for missing `stornozott`. [The XSD][receipt-xsd] requires a boolean; [XSD boolean][datatypes] permits `true`, `false`, `1`, `0`, not empty text. Neither the current receipt example nor the inspected evidence establishes an empty-reversal compatibility requirement.

**Why retain:** unlike the weaker URL padding concern, this maps an unknown business fact to a definite negative. The public recovery example actually tests `!receipt.reversed` at [`README.md:186–190`](../../crates/szamlazz-agent/README.md#L186). A malformed empty value could therefore satisfy that part of its live-receipt check. The rest of the identity checks remain; this is not proof that the library automatically issues a duplicate.

**Bounded action:** prefer a receipt-specific refusal of empty reversal text, or deliberately model unknown. Retain the four valid forms and account for existing callers that may rely on empty-as-false. Do not globally change the shared helper: the same empty-to-false policy on an envelope verdict refuses success, a different consequence. There is no receipt live capture in the supplied evidence. P3 semantic robustness is justified; P2 or a normal-wire noncompliance claim is not.

## 7. Numberless credit-entry success — source ambiguity remains

**Code/reproduction:** [`src/ops/credit_entry.rs:250–256`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L250) checks the verdict then requires a body/header number to construct `InvoiceBalance.invoice_number`. HTTP 200, no number header, and a complete namespaced `<xmlszamlavalasz><sikeres>true</sikeres></xmlszamlavalasz>` returns `Parse(Missing("szamlaszam"))`. Adding a number header succeeds. This was independently rerun, not inferred from the DTO.

**The case for a defect is real but incomplete:** [current EN][credit] and [HU][credit-hu] response pages explicitly say headers “may” arrive and `minOccurs="0"` elements are not always present. Only `sikeres` is mandatory in their shared success/error schema. Numberless success is therefore schema-valid; it must not be dismissed as malformed. The operation also knows its requested number, and version 1's documented `xmlagentresponse=DONE` demonstrates that mutation acknowledgement is conceptually possible without a new number.

**What that does not establish:** the schema combines successful and failed responses, whose number omission is explicitly documented. Both current successful examples include a number. `docs/szamlazz-hu-behaviour.md:145` records success number headers on create/storno/credit, with no numberless-success observation; those account notes are bounded and their raw logs are not available here. Version 1 does not define version 2's success-specific field guarantee. The Rust DTO is a chosen stronger result shape, not proof that the vendor promises that shape.

**Decision:** retain CA1 as a targeted clarification: does a successful version-2 registration always echo a nonblank number in at least one channel? Do not assign P2 to the hypothetical downstream mutation risk before settling that premise, and do not claim the echo is schema-required. If omission is supported, choose a successful acknowledgement with optional reported identity or distinguish requested identity from reported identity. Silently filling the current reported-number field from the request would invent evidence.

If such a response occurs, parsing failure does not mean the registration failed. The current [operation recovery rule, `src/recovery.md:18`](../../crates/szamlazz-agent/src/recovery.md#L18), correctly requires inspecting entries/balance before deliberately repeating additive or replacing registration. This candidate does not reopen create/storno's need to obtain their newly assigned number.

## Verification performed in this adjudication

All Cargo invocations were `--locked --offline`. Standalone scratch crates use their existing independent lockfiles and current path dependency; that is distinct from the workspace lock. No scratch source was changed. Inspected sources before execution; none of the executed Rust scratch paths calls the vendor.

```sh
cargo test --locked --offline --manifest-path /tmp/opencode/current-mutations-fbda137/Cargo.toml --test probe body_only_numbered_56_loses_identity_on_structural_metadata_failure -- --exact --nocapture
cargo test --locked --offline --manifest-path /tmp/opencode/current-mutations-fbda137/Cargo.toml --test probe schema_valid_credit_success_without_echo_is_rejected -- --exact --nocapture
cargo test --locked --offline --manifest-path /tmp/opencode/current-mutations-fbda137/Cargo.toml --test probe closed_body_refusal_survives_bad_payload_across_create_and_storno -- --exact --nocapture
cargo run --locked --offline --manifest-path /tmp/opencode/current-receipts-fbda137/Cargo.toml
cargo run --locked --offline --manifest-path /tmp/opencode/query-current-fbda137/Cargo.toml
/tmp/opencode/current-transport-fbda137 interrupted_bodies_still_discard_all_header_evidence --exact --nocapture
cargo test --locked --offline -p szamlazz-agent --features client-reqwest --target-dir /tmp/opencode/current-transport-target --test response_headers --test response_completion --test response_namespaces --test business_text
```

- **20 existing repository tests passed:** headers 10, completion 2, namespaces 6, business text 2.
- **Three selected mutation scratch tests and one transport scratch test passed.** Their assertions confirm the problematic behavior plus controls; pass does not mean repaired.
- Receipt/query scratch executables completed successfully and printed the behaviors above. Some cases are printed observations, not individual assertion tests; no inflated test count is claimed. Incidental arithmetic/date/writer output did not expand this adjudication's findings.
- The transport executable was the existing workspace-linked binary, with its source inspected and current behavior checked against the unchanged implementation. It was rerun, not freshly compiled by this adjudication. Its prior build provenance is recorded in the current transport report.
- In-memory Python/ElementTree checks rejected seven minimal CQ-2 cases. Three additional bodies sent through the existing query executable's stdin verified URL `%20`/`%2B` preservation and NBSP trimming. The HTTPS URL strings were payload data only, never fetched.
- Fresh unauthenticated source GETs: create response; EN/HU credit response; receipt create/storno responses; receipt reply XSD; PHP response-handling page; W3C XSD datatypes and XML 1.0 Fifth Edition productions. PHP 2.12.4 `InvoiceResponse.php` was read from the existing scratch extraction, not redownloaded or executed. A first in-memory XML-production extraction command had a Python syntax error; the corrected command succeeded. No finding depends on the failed command.
- No full XSD validator, PDF renderer, live call, browser execution or downstream database/export test was run. Source optionality is read directly from declarations, not presented as an executed schema-validation result. No new vendor capture establishes code 56, padded URLs, malformed receipt reversal/PDF, or numberless successful credit registration.

## Strongest findings and order

1. **CM1** has the clearest bounded discrepancy with an existing explicit policy; identity preservation can be addressed at its current envelope boundary.
2. **T1** has the most plausible ordinary operational trigger and loses useful reconciliation evidence, but needs an incomplete-transfer evidence API, not a more optimistic verdict. **P3 remains appropriate.**
3. **H2** has a concrete business-state consequence on malformed data; changing receipt-specific tolerance requires an intentional compatibility decision.
4. **CQ-2** is a real lexical-conformance limitation. Lead with illegal characters in recognized fields; avoid turning the ignored-extension examples into a generic hardening project.

H1's blank artifact normalization and CQ-1's exact URL fidelity are smaller consistency/policy choices. CA1 needs a success-path answer from the vendor. None of the supplied evidence supports a remaining normal-wire P0/P1/P2 headline, a live duplicate-issuance claim, or counting a shared parser behavior once per consuming operation.

[create]: https://docs.szamlazz.hu/agent/generating_invoice/response
[credit]: https://docs.szamlazz.hu/agent/credit_entry/response
[credit-hu]: https://docs.szamlazz.hu/hu/agent/credit_entry/response
[receipt]: https://docs.szamlazz.hu/agent/generating_receipt/response
[receipt-storno]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[receipt-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[php]: https://docs.szamlazz.hu/php/valasz-feldolgozas
[datatypes]: https://www.w3.org/TR/xmlschema-2/
[xml]: https://www.w3.org/TR/2008/REC-xml-20081126/
