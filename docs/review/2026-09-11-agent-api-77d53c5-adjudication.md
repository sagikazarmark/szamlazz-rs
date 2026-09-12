# Independent adjudication of the six Számla Agent reviews

**Date:** 2026-09-11. **Baseline:** `77d53c553c9ecdc86d5fa72ca932c636256ae807`.

## Recommended aggregate verdict

**Qualified pass for implemented operation/business-data coverage, not an unconditional conformance certification.** No demonstrated ordinary, semantically established documented exchange is newly shown to fail at runtime. There are **two confirmed P3 public-documentation defects**, a **confirmed taxpayer diagnostic-metadata capability omission**, and unresolved response-contract differences that must remain visible: numberless success and customer-URL header decoding. Neither a passing suite nor README disclosure settles these differences.

All eleven vendor actions are implemented, including an explicit clear operation sharing the credit-entry action. The six reports substantiate broad request coverage and current invoice/receipt business-field coverage. Their overlapping test totals must not be added as independent coverage. “No confirmed runtime defect” is supportable; “complete response fidelity” or “all documented successful responses are supported” is not.

Read all six `2026-09-11-agent-api-77d53c5-{invoices,queries,mutations,receipts,taxpayer,transport}.md` reports. Independently checked the contested production paths and refreshed the primary sources below. HEAD was `370ff2e5398e9ff4a3aff3b6008ab82e38b9d058`; the baseline diff for `crates/szamlazz-agent`, `Cargo.lock` and both September 11 execution records was empty. No implementation edits, live calls, probes, `.env` access or delegation. No suites rerun.

Code references below are relative to `crates/szamlazz-agent/` and describe the baseline.

## 1. Firm findings and disagreements

### A-D1 — P3: receipt evidence summary is stale (uphold receipt D1)

`src/error.rs:359–374` says observations are only on variants explicitly marked observed and that none of the thirteen receipt/simplified-image additions were observed. `src/recovery.md:57–62` similarly contrasts their documentation provenance with live observations.

The separate test-account record explicitly records **337** (`docs/research/2026-09-11-receipts-live.md:22,29–37`) and completed-create repetition yielding **338**, with a subsequent query of the same original (`:49–59`). `src/error.rs:157–158` itself already mentions September 11's observed prefix limit. The historical account's identity is not established as the same account (`receipts-live.md:5–10`): the correction must name date/account scope, not rewrite all historical observations as one corpus.

**Qualification to the receipt report:** 337 alone disproves the blanket statement about the thirteen additions. 338 corroborates duplicate-call behavior, but is not one of those thirteen listed in `README.md:478` (their lists total thirteen without 338). Likewise, “sourced from documentation” can remain true as historical provenance; the defect is presenting that as the current absence of execution evidence.

**Impact/disposition:** misleading public evidence provenance, not wrong wire behavior. Correct the aggregate summary, preserving uncertainty for the remaining codes, concurrency and retention. The transport report's “no actionable P0–P3 finding” is too broad in this shared-documentation scope.

### A-D2 — P3: the public code-56 rule overstates its operation scope

`src/error.rs:369–372`: “56 surfaces as an error only when the response carries no document number (with one, the parsers report success …).” This is false across the public parsers. `RegisterCreditEntry::parse` uses ordinary `xml::valasz` (`src/ops/credit_entry.rs:316–322`); clearing delegates to it (`:247–249`). A numbered error header 56 remains `Api(InvoiceNotificationDeliveryFailed)`. Existing `tests/response_headers.rs:203–223` explicitly asserts that distinction; the adjudication control reproduced it at HTTP 200 as well.

The transport report notices this at its line 139, then asks readers to interpret the blanket wording as the issuing-parser rule. That does not repair the public statement. `ErrorCode::InvoiceNotificationDeliveryFailed`'s own narrower wording (`src/error.rs:71–74`) is better. Fresh PHP [S8] corroborates the issuing exception, not credit/receipt/taxpayer promotion.

**Impact/disposition:** incorrect expectation when handling a numbered error; amend the rustdoc to scope the exception. Preserve operation-specific runtime behavior. No claim that a vendor credit operation actually emits 56 is needed to establish that the library's statement about its parsers is false.

### A-C1 — Confirmed diagnostic capability omission, not a taxpayer-verdict bug

The fresh official success example [S4] contains `header/{requestId,timestamp,requestVersion}` and `software` metadata. These are genuinely documented response data, not speculative NAV-only extensions. `TaxpayerInfo` (`src/ops/taxpayer.rs:184–225`) exposes neither. Its recognized-path projection (`:343–397`) also omits notifications; `into_info` discards `result/message` on success (`:592–610`). NAV Common [S9], `BasicHeaderType`, `BasicResultType` and `NotificationType`, defines correlation/version data and informational notifications.

**Agree** with taxpayer T-M1 that all inspected taxpayer business fields are mapped and this does not change validity/name/address interpretation. **Disagree with treating that as complete API response capability:** retaining vendor correlation ids, software provenance and informational messages through the built-in request's typed result is unavailable. `Client::send` returns only that result (`src/client.rs:374–405`); a custom `AgentRequest`/transport can preserve raw bytes, but that is additional caller work, not existing `TaxpayerInfo` exposure.

This is a low-priority, concrete diagnostic feature gap under the user's completeness criterion, not a demonstrated billing/lookup correctness failure or a missing vendor action. The actual type is a taxpayer projection, not a declared lossless NAV response. That scope justifies its design, but does not make the omitted capability disappear. README `:480` discusses exposure of the newly added business fields to worker/CLI surfaces; it does **not** establish that this omitted metadata was explicitly adjudicated there. Agent forwarding of NAV notifications remains uncaptured; the official example already establishes header/software omission without that assumption.

## 2. Numberless success: formal incompatibility established, emission unresolved

Fresh credit, PDF and storno response pages [S1–S3] all say optional elements “may not always be included,” make `szamlaszam` optional, and describe headers as additional data that may arrive. The schemas contain no conditional rule requiring identity when `sikeres=true`. Consequently the following distinctions are mandatory:

| Operation | Established code behavior | Adjudication |
|---|---|---|
| Credit registration / clear | HTTP 200, no headers, `<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>` → `Parse(Missing("szamlaszam"))`; `credit_entry.rs:319–322`, clear `:247–249` | **Confirmed formal schema/parser difference.** A useful successful acknowledgement need not conceptually identify a new document: the request already targets one. There is no semantic necessity to discard its positive verdict. The requested identity must nevertheless not be fabricated as a reported echo. |
| PDF query | Success plus decodable PDF but no number → the same error; `query_pdf.rs:83–93`, `envelope.rs:275–279` | **Confirmed formal difference and conditional artifact loss.** The purpose is fetching an artifact, not issuing a numbered document. `InvoicePdf.invoice_number` being nonoptional is a local constraint, not vendor proof. |
| Storno | Bare success → the same error; `envelope.rs:211–217,275–279` | Same formal difference, with a stronger recovery rationale: it cannot name/verify the reversal. Keep a positive wire verdict distinct from established reversal identity; do not turn this into preview or nonexecution. |
| Normal invoice create | Unnumbered reply accepted as preview only for requested preview with PDF; `invoice.rs:948–960` | The shared optional-number schema also covers non-issuing preview. It therefore cannot establish that ordinary issuance may omit identity. |

**Why not promote these into proven emitted-response failures?** Success/failure share each schema, the successful examples are numbered, and the prose does not say whether the optionality applies *within success*. The schema also permits false without an error code despite prose describing one. Receipt [S5] demonstrates the distinction explicitly: `nyugta` is optional in its envelope XSD, but prose requires it for every successful call. Thus XSD validity alone is insufficient to infer every semantic variant actually belongs to the operation. Conversely, none of [S1–S3] supplies receipt-like success-only identity wording; they do not establish Rust's stricter rule either.

The live clear record (`docs/research/2026-09-11-credit-clearing-live.md:14–24,64–74`) establishes two numbered successes, not a universal guarantee or which channel supplied the number. There is **no evidenced live-deviation exemption** resolving this mismatch. Existing unanswered clarification is not an authority for conformance.

**Disposition:** uphold mutations Q1/Q2 and queries Q-A1 as unresolved semantic-contract questions, but reject an unqualified “conforms” or “parser is correct” conclusion based merely on the existing result type/README. Credit and PDF are the strongest candidates for accepting an acknowledgement/artifact with optional reported identity. Vendor confirmation of a nonblank success echo would settle the narrower contract; confirmation that omission is legitimate establishes a runtime interoperability defect. Until then, record the formal gap explicitly and do not infer permission to resend from the parse failure.

## 3. Customer-URL header: actual decoder disagreement, no captured affected exchange

Fresh official PHP 2.12.4 [S8], `Response/InvoiceResponse.php:136–137`, calls `rawurldecode` specifically for `szlahu_vevoifiokurl`; error text at `:152–154` uses `urldecode`. PHP's primary manual [S10] explicitly says raw decoding preserves literal `+`. Rust `src/ops/envelope.rs:135–143` instead calls `RawResponse::szlahu`, whose `src/wire.rs:343–350` replaces every literal `+` with space before percent decoding.

Targeted control, body success with header number `I-1` and no body URL:

```text
szlahu_vevoifiokurl: https%3A%2F%2Fexample.test%2Fa+b%3Ftoken%3Dc+d
PHP raw-decode semantics: https://example.test/a+b?token=c+d
Rust InvoiceBalance URL: https://example.test/a b?token=c d
```

**Firm fact:** decoder parity fails and a literal-plus URL would be corrupted in create/storno/credit/clear/PDF header fallback. This is not merely stylistic. Existing `tests/response_headers.rs:226–249` uses outer `%2B`, on which both decoders agree; it cannot settle the disputed literal-plus case. XML-body URLs bypass this decoder and are unaffected.

**Remaining uncertainty:** [S1/S3/S7] call it a customer account URL without specifying outer encoding or exhibiting a literal-plus header. PHP is a first-party consumer, not the producer's grammar; if the producer outer-encodes all pluses as `%2B`, both clients agree on every emitted URL. Our synthetic input is legal header text and meaningful under PHP's reader, not a captured or expressly illustrated Számla Agent exchange. Therefore uphold the transport review's uncertainty about actual interoperability, while giving the first-party discrepancy substantially more weight than arbitrary hardening. Prefer field-specific percent-only decoding if adopting PHP parity; do not change error/number decoding globally. A raw affected header or explicit encoding guarantee would settle severity. README disclosure of form decoding is no exemption.

## 4. Codes 7 and 338: challenge rejected after checking the complete public contract

- **7 → `NotFound`:** [S6] explicitly gives receipt send's `Hiányzó adat: emailtargy elem.` This is not evidence of an absent receipt. The name is misleading in isolation, but `src/error.rs:43–48,451–454` explicitly defines the class as missing data **or** missing document, interpreted per operation. Code and message are preserved; dispatch at `:386` implements that broader declared class. The runtime therefore does not assert receipt absence. Calling this an unconditional wrong classification would ignore the actual contract. A future operation-aware classifier would improve ergonomics; callers must not read the variant as a universal existence verdict.
- **338 → `Rejected`:** [S5] says duplicate call identity makes the call unsuccessful and prevents another receipt. The completed-repeat record (`receipts-live.md:49–59`) corroborates this. `src/error.rs:439–442` explicitly scopes rejection to this exchange and singles out 338 as not recovering the prior result; `src/recovery.md:15` forbids switching call id while unresolved. This is consistent, not a demonstrated unsafe rejection classification. The introductory “may a document exist?” shorthand (`error.rs:355–356,427–435`) must be read with that scope: 338 certainly does not mean no receipt exists from a prior send. No original identity or concurrency/retention guarantee is inferred.

## 5. Other reviewer conclusions

- **Uphold invoice request assessment:** no new missing business request capability identified. Inline/download conflicts over preview/simpleItems order, group id and erasure count remain source conflicts, not proved server failures. These were exhaustively investigated in the invoice report; this adjudication does not claim to have repeated its schema matrix. Matching one official schema does not establish combined-preview runtime behavior.
- **Uphold receipt execution corrections:** automatic MNB and order queries have receipt-specific evidence; first-send and delayed empty-block resend have acknowledged delivery (`receipts-live.md:61–73,93–118`). The initially throttled email probe is not a subsequently rerun passing suite. NAV reporting, erasure allocation, rendering and partial email overrides remain open. Do not restore older “receipts untested” conclusions.
- **Uphold explicit clearing coverage:** refusing an empty replacing `RegisterCreditEntry` is not missing zero-entry protocol support when `ClearCreditEntries` expresses that intent and was executed on populated/empty invoices.
- **Uphold taxpayer generic-root uncertainty:** [S4] promises `QueryTaxpayerResponse`; NAV's direct generic errors do not establish Agent forwarding. Refusing those roots is a conditional compatibility gap, not a demonstrated valid Agent error misparse.
- **Uphold ordinary scalar/projection limits with scope:** finite exact money/civil dates and absence of raw archival are not full XSD-domain coverage. No ordinary valid monetary/date value was demonstrated lost here. Malformed examples with raw URL ampersands or placeholder PDF text are not valid-exchange counterexamples. Documenting a limit is disclosure, not independent proof it is acceptable for every consumer.
- **Storno retry wording deserves alignment:** mutations H2 correctly notes `src/ops/storno.rs:30–36` says resending after transport failure is safe, while `src/recovery.md:14` says reconcile. The local paragraph is explicitly bounded to observed test-account behavior (`storno.rs:24–28`); it proves neither all-account behavior nor concurrent repeat safety. Keep the bounded observation, and avoid presenting it as universal recovery permission.

## Fresh primary sources and verification

All sources below fetched independently on 2026-09-11; Agent pages report build `v202608271632`.

| Ref | Exact source / section used |
|---|---|
| S1 | https://docs.szamlazz.hu/agent/credit_entry/response — headers, success example, XSD `szamlaszam` |
| S2 | https://docs.szamlazz.hu/agent/querying_pdf/response — v2 artifact, headers, XSD |
| S3 | https://docs.szamlazz.hu/agent/reversing_invoice/response — numbered example, optional-number XSD |
| S4 | https://docs.szamlazz.hu/agent/querying_taxpayer/response — promised root, dated success/failure examples |
| S5 | https://docs.szamlazz.hu/agent/generating_receipt/response — success-only `nyugta` requirement, 337/338 |
| S6 | https://docs.szamlazz.hu/agent/sending_receipt/response — failure example, missing-subject 7 |
| S7 | https://docs.szamlazz.hu/agent/generating_invoice/response — textual header descriptions and shared XSD |
| S8 | https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip — `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/Response/InvoiceResponse.php:128–158,319–323`; fresh SHA-256 `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| S9 | https://raw.githubusercontent.com/nav-gov-hu/Common/common-1.0.0/schemas/src/main/resources/xsd/hu/gov/nav/schemas/NTCA/1.0/common/common.xsd — `BasicHeaderType`, `BasicResultType`, `NotificationType`, generic roots |
| S10 | https://www.php.net/manual/en/function.rawurldecode.php — Description and Notes (literal-plus preservation) |

Fresh PHP retrieval used the inspected retrieval-only `/tmp/opencode/transport-77d53c5-php.py`: archive inspected in memory, no PHP executed. A single throwaway public-parser program `/tmp/opencode/adjudication-77d53c5.rs` was compiled against the existing Agent artifact and run. It confirmed numberless credit/clear/storno/PDF refusal, literal-plus URL corruption under PHP semantics, numbered credit 56 remaining an error, and 7/338 classes. The PDF control used `%PDF-` bytes solely to isolate identity handling, not to claim a valid rendered artifact; the query specialist's stronger PDF/schema reproduction remains separately attributed to that report.

```sh
python3 /tmp/opencode/transport-77d53c5-php.py
rustc --edition=2024 /tmp/opencode/adjudication-77d53c5.rs --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -L dependency=/home/laborant/szamlazz-rs/target/debug/deps -o /tmp/opencode/adjudication-77d53c5
/tmp/opencode/adjudication-77d53c5
```

**Bottom line:** retain the runtime coverage verdict with explicit limits; correct the two public-documentation defects; record taxpayer diagnostics as omitted capability; keep numberless success and URL encoding open with precise evidence requirements. No newly demonstrated production/runtime defect justifies inventing a severity or overriding evidenced live behavior.
