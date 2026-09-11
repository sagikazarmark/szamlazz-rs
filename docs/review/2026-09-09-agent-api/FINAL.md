# Final review: real Számla Agent gaps and recommended solutions

**Date:** 2026-09-09 · **Baseline:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`

**This report supersedes the first-round conclusions.** Five fresh specialist subagents re-reviewed the findings and residual notes; a separate judge then checked their evidence, challenged their remedies and resolved disagreements. The [complete judgment](round-2/JUDGE.md) contains detailed acceptance criteria, citations, and a disposition for every original finding and all 44 grouped residual-review entries.

## Executive conclusion

**The crate's operation coverage is strong, but several real implementation and API-exposure gaps remain.** The review retains **28 deduplicated actionable findings**:

| Category | Count | What the count means |
|---|---:|---|
| Agent implementation defects | 6 | Date/header parsing, known-error classification, XML structure, string fidelity and cookie-name matching |
| Consequential worker interpretation defect | 1 | Treating an inconclusive storno helper result as a confirmed no-op; a separately owned follow-up |
| Capability additions | 4 | `simpleItems`, two PDF result fields, five taxpayer fields, create/storno payment method |
| Documentation corrections | 16 | Specific semantic, recovery, protocol and evidence claims; most need no runtime change |
| Evidence/test-methodology package | 1 | Source provenance, semantic assertions and accurate coverage claims |

There are also **five housekeeping/source notes**, excluded from that count. **No P0/P1 finding was established.** Seven findings are P2; the rest are P3. These are work priorities, not 28 production incidents. The [implementation packages](#recommended-implementation-packages) group the work into eleven coherent changes.

**Intentional, live-backed differences remain supported**, including external-ID behavior, invoice-storno echoes/no-ops, appearance codes, sparse responses, body-only errors, and the actual invoice rounding observations. None is removed merely to match an older example or conflicting XSD.

## Confidence: how to read the findings

Every retained finding below has two ratings:

- **Gap confidence:** certainty that the precisely described problem or missing capability exists.
- **Solution confidence:** certainty that the proposed remedy is the right bounded approach, before implementation verification.

**High** means direct source/code evidence or a reproduced counterexample with a clear remedy. **Medium** indicates conflicting sources, an unsupported-but-not-disproven claim, or integration work still needing proof. Live occurrence is separate: an offline reproduction proves parser behavior, not the frequency with which szamlazz.hu emits the triggering response. No new live-account operation ran.

## 1. Implementation defects

| ID | Priority | Real gap and impact | Best solution | Gap / solution confidence |
|---|---|---|---|---|
| **F1** | P2 | Valid XSD date suffixes such as `2026-09-09Z` or `+02:00` reject an entire invoice query/receipt response. Required credit-entry and receipt dates also reject XML whitespace padding. | Add private required/optional date adapters at all eleven invoice positions and the receipt date. Validate the entire new timezone form, preserve the printed civil date, and retain existing compatible date-domain behavior. No UTC shift, arbitrary suffix truncation or new CE-year restriction. | **High / High** — schemas and offline cases establish the gap; a private adapter preserves public types. |
| **F2** | P2 | Thirteen documented codes become `Unknown` outcomes, losing known settled semantics and potentially causing unnecessary reconciliation. | Add explicit named mappings: **339 → NotFound**; **336, 337, 340, 363–365, 551–556 → Rejected**. Preserve future-code uncertainty, 338 duplicate refusal and the distinction between this exchange and earlier sends. Test against a source-derived table. | **High / High** — current official meanings and missing mappings are direct evidence. |
| **F3** | P2 | Monetary-header fallback rejects recorded comma-decimal format `100,01` when its body amount is absent/empty/blank. A completed write can therefore become a parse failure. | One header-specific ungrouped decimal reader: sign, dot/comma mantissa, exponent, HTTP space/tab padding; Decimal conversion without floating point. `1,234` means `1.234`; reject mixed/repeated separators and underscores. Keep XML grammar, body precedence and numbered-56 softness separate. | **High / High** — recorded format plus reproduced fallback failure; explicit grammar resolves the ambiguity. |
| **R1** | P3 | Taxpayer parsing accepts incomplete XML, lets unrelated/duplicate fields overwrite verdicts/data, and deletes unknown entities. Shared XML parsing also accepts trailing documents/content. | Shared single-complete-document checking, then versioned NAV parent-path/expanded-name extraction. Reject ambiguous recognized singletons and undefined entities; skip unknown subtree extraction. Preserve sparse data and actual 2.0/3.0 namespaces. No full XSD validator or general XML engine rewrite. | **High / Medium** — failures independently reproduced; shared integration and the NAV reader need regression proof. |
| **R2** | P3 | Generic optional-scalar parsing removes meaningful surrounding characters from business strings, including identifiers and NBSP-only text. | Separate optional business text from scalar/token parsing: absent/empty/XML-whitespace-only → None; otherwise preserve decoded characters. Coordinate invoice, receipt and taxpayer readers; retain deliberate envelope-number/worker normalization. | **High / High** — direct reproduction and string semantics; helper separation addresses the cause. |
| **E.E1** | P3 | `session_cookie()` matches `JSESSIONIDOTHER` as `JSESSIONID` and accepts a bare name without `=`. Sans-I/O callers can receive the wrong cookie pair. | Parse the first cookie pair, require `=`, match **exact case-sensitive name** `JSESSIONID`, preserve the value and continue past nonmatches. Keep full cookie lifetime/domain/path handling in the HTTP transport. | **High / High** — independently reproduced public-helper mismatch; small exact-name fix. |

**Live qualifications:** F1, R1 and R2 have no recorded live triggering incident. For F3, comma create-net headers are recorded, but the missing-body-plus-comma combination is not; normal valid body totals mask it. None of F2's thirteen codes was observed in the account record. E.E1's competing-cookie input is valid HTTP, but vendor emission is unobserved; native reqwest's cookie jar is unaffected.

**Code boundaries:** F1 `src/xml.rs:269–283`, `src/ops/query_xml.rs` date readers, `src/ops/receipt.rs:735`; F2 `src/error.rs:220–253,314–348`; F3 `src/ops/envelope.rs:146–155,285–303`; R1 `src/xml.rs:63–108`, `src/ops/taxpayer.rs:202–338`; R2 the optional helpers and taxpayer leaf extraction; E.E1 `src/wire.rs:308–325`. Here `src/` is under `crates/szamlazz-agent/`.

Detailed evidence, sources and positive/negative acceptance cases: [judge §2](round-2/JUDGE.md#2-implementation-defects).

### E.E5a · P2 — Inconclusive storno metadata must not become “no-op”

**Gap confidence: High. Solution confidence: Medium overall; High for the agent documentation correction.**

`CreatedInvoice::gross_total` is optional. `CreatedInvoice::reverses()` returns false when it is missing, but `restate-szamlazz` currently turns **every false result** into `NotStornoable`, logging that the storno was a no-op. The absent auxiliary total cannot establish that nothing was reversed. This is a concrete invalid inference even without proving that a negative original produces a positive-gross storno.

**Best solution:** document the boolean as a heuristic whose false answer is inconclusive. In a separately owned worker change, introduce an internal decision for ambiguous changed-number replies and query the returned document. Require the queried storno type and reference to the original; the existing `FoundDocument::is_storno_of` expresses this identity check. Preserve the observed fast paths. Verification after a send belongs inside the storno step; an unanswered or inconclusive verification must remain unknown, not become settled no-op.

**Acceptance:** changed number with missing/positive gross takes verification; matching SS/reference settles reversal; mismatched, absent or unanswered verification never silently settles no-op. Review the worker's durable boundary and post-send error mapping. The low-level helper must not perform I/O.

**Live boundary:** negative-original, zero-total and missing-gross compound outcomes were not observed. Positive-original negative stornos and same-number proforma/delivery-note echoes were. This recommendation preserves those facts.

Locations: `crates/szamlazz-agent/src/ops/envelope.rs:56–75`; `crates/restate-szamlazz/src/gateway.rs:1607–1615`. [Full judgment and rejected alternatives](round-2/JUDGE.md#ee5a--a-false-storno-heuristic-is-inconclusive).

## 2. Capability decisions

These are missing built-in capabilities, not claims that every SDK must expose every upstream field.

| ID | Priority | Preferred public surface and solution | Gap / solution confidence | Evidence limit |
|---|---|---|---|---|
| **C1** | P2 | Add `InvoiceHeader::simple_items: Option<bool>`, default None. Serialize template → preview → simple items. Keep full item amounts and server-owned business validation; document inheritance and restrictions. | **High / Medium overall** — field/default is High; PHP/download order has stronger implementation evidence but conflicts with inline schemas. | No live simple-items/combined-preview observation. The simultaneous-field server ordering remains unresolved. |
| **C2** | P3 | Add `InvoicePdf::{outstanding: Option<Decimal>, customer_account_url: Option<String>}`. Carry already-parsed values through; retain required PDF/number and body/header precedence. | **High / High** — both fields are in this operation's own schema and visibly discarded. | Optional PDF-specific emission unverified. Decline PDF id/payment-method/notification-flag expansion on present evidence. |
| **C3** | P3 | Add taxpayer `short_name`, `county_code`, `vat_group_membership`, `incorporation`, `info_date`. Four textual optionals plus an open `Incorporation` enum. **`info_date: Option<String>` preserves source text**, including unspecified timezone and precision. | **High / High for surface**, **Medium for integration** through R1's redesigned extraction. | `infoDate` is in the official dated example; current forwarding of all five remains unverified. NAV permits client-selected subsets. |
| **E.E4** | P3 | Add `CreatedInvoice::payment_method: Option<PaymentMethod>`, sharing the existing credit-entry header reader. Preserve unknown tokens and old-JSON defaults; no invented body element. | **High / High for exposure**, **Medium for broader encoding claims** beyond the existing policy. | Create/storno pages explicitly document the header; no fresh capture establishes every emission/encoding case. |

For C3, `infoDate` means last change of taxpayer data, **not lookup time or cache expiry**. A missing timezone is not UTC. Raw source text avoids a new optional metadata field rejecting otherwise useful results. Do not automatically expand the worker journal/contract or CLI output alongside these agent additions.

For C1, the judge traced **first-party PHP 2.12.4's actual writer** and found preview before simple items, agreeing with the downloadable XSD. Both inline language schemas reverse those two fields. Choose the documented implementation policy, preserve original sources separately, and expose the conflict honestly. Never drop preview, automatically retry the other order, or turn a locally merged schema into purported vendor proof.

**Release effects:** C1 adds a field to an exhaustive request struct and is Rust source-breaking. C2/C3/E.E4 extend non-exhaustive responses; old JSON should decode with defaults, while serialized output grows. [Exact fields, source citations, compatibility and acceptance](round-2/JUDGE.md#3-bounded-capability-decisions).

## 3. Retained documentation corrections

These sixteen findings should be implemented as coherent documentation patches. **No new runtime validation is implied.** Render rustdoc, verify links and compile meaningful examples; comment-string tests cannot validate these semantics.

| ID | Priority | Real gap → best correction | Gap / solution confidence |
|---|---|---|---|
| **D1** | P2 | TAHK is incorrectly described as TAM's exemption → “outside the subject-matter scope of VAT”; preserve its token. | **High / High** — Hungarian list and vendor VAT table directly distinguish them. |
| **D2** | P2 | `eusAfa` omits the processing effect → explain accepted true suppresses NAV submission, seller prerequisites, item VAT obligation and no retroactive submission. | **High / High** — explicit vendor field semantics. |
| **D3** | P3 | Universal invoice-external-ID recovery advice cannot apply to receipts or arbitrary mutations → one operation-specific recovery table; stable receipt create IDs, 338 as duplicate prevention, mutation-specific reconciliation. | **High / High** — the recommended receipt selector does not exist; bounded operation guidance is supported. |
| **D4** | P3 | Shared rounding guidance overgeneralizes invoice observations → separate local arithmetic, currency policy and server behavior; HUF receipt gross whole, net/VAT ≤2 places, exact sum. | **High / High** — explicit receipt rules differ from generalized storage prose. |
| **D5** | P3 | Bank account described as recipient → sender when known, otherwise the account printed on the invoice; no source discriminator. | **High / High** — direct annotation of the same queried XML field. |
| **D6** | P3 | “Unpaid proforma” suggests protection absent from the operation → “existing proforma,” with bounded paid-deletion observation. | **High / High** — live test-account counterexample and no client guard. |
| **D7** | P3 | Buyer identifier guidance omits partner update/account-link consequences → explain account-local partner identity and distinguish internal numeric id. Merge qualified queried-buyer mutability guidance. | **High / High** for identifier semantics; **Medium / High** for ambiguous temporal wording. |
| **D9** | P3 | Erasure-code prerequisites/errors are bundled incorrectly → 537 count, 538 demo/test restriction, 539 setting; invoice template separately; stock/vendor-supplied codes. | **High / High** — catalogue and first-party feature guidance. |
| **D12** | P3 | Three reference-bearing kinds are attributed to an XSD restriction it does not encode → attribute that boundary to the crate model. | **High / High** — independent XSD flags/reference elements. |
| **D13** | P3 | Waybill/layout guidance hides supported scope and precedence → compatible invoice layout, forced delivery-note template, barcode fallback, unused destination, named template versus omission. | **High / High** for each constituent — writer behavior and schema annotations. |
| **B.E01** | P3 | “~5” and “55 means issued” overstate the contract → five total sends, uncertain signing outcome, potentially transient hint versus certificate remediation. | **High / High** — current error guidance and conservative classifier semantics. |
| **B.E02** | P3 | Mixed documented/observed/PHP evidence is called all verified; 56's number condition is omitted → explicit provenance and conditional numbered-56 wording. Qualify conflicting stall claims. | **High / High** for main corrections; **Medium / High** for incomplete stall history. |
| **B.E03** | P3 | Query call-ID meaning is asserted without proof; last-match receipt guidance omitted → neutral optional wire field, normal number/order lookups, vendor-documented last matching document. | **Medium / High** for unsupported call-ID interpretation; **High / High** for last-match addition. |
| **B.E04** | P3 | Receipt email promises independent field merging/comma recipients without established support → describe child omission and documented whole-details-absent resend; leave partial behavior unspecified. | **Medium / High** — stronger promises unestablished, not disproven. |
| **E.E2** | P3 | HTTP/decoding prose differs from code → actual down/error header → status → body policy; textual decoding only where applicable; no certain proxy-origin claim. | **High / High** — source plus 200/500 counterexample. Keep runtime policy. |
| **E.E3** | P3 | Session refresh and injected-jar ownership guidance incomplete → fresh jar versus clone, account ownership, inactivity expiry, native/browser distinction. | **High / High** for jar/refresh guidance; **Medium / High** for misleading browser breadth. |

**Live qualifications:** D6's paid deletion and D7's possibility of later buyer-data mutation have bounded test-account evidence. D4's actual EUR invoice storage/selected HUF tolerance observations stand. The other corrections do not establish live incidents involving VAT misuse, receipt recovery/email/defaults, erasure codes, carrier rendering, 55/56, cookie-account selection or browser access. B.E02's minute-stall issuance claim has conflicting repository provenance; neither assert it proven nor rewrite it as disproven.

The [judge's documentation decisions](round-2/JUDGE.md#4-documentation-decisions-and-coherent-work-packages) give locations, sources, individual qualifications and acceptance. [Reviewer D](round-2/04-domain-docs.md) supplies exact proposed rustdoc text for domain corrections; use the judge's final scope where reviewer proposals differ.

## 4. E.E6 · P3 — Evidence and tests must establish the claimed property

**Gap confidence: High overall. Solution confidence: High.** One package with six traceable constituents; these are evidence defects, not production failures.

| Constituent | Best solution | Confidence qualification |
|---|---|---|
| Historical source descriptions | Preserve July acquisition facts; append dated September source observations and new fixtures. | **High / High** — current pages differ; history should not be relabeled. |
| Receipt-create provenance | Record cached `torloKod` versus current download discrepancy; recover acquisition/transform record or mark it unknown. | **High** that files differ; **Low** that historical acquisition was wrong; **High** remedy confidence. |
| Conflicting schemas | Keep unmodified source snapshots, chosen writer expectations and any project transformations visibly separate. | **High / High** for evidence handling; actual server schema remains unresolved. |
| Empty receipt-email block | Fix the outline comparator's universal empty=omitted claim; add an independent generated-output assertion that default send contains exactly one present empty `emailKuldes`. | **High / High** — vendor docs distinguish absent block/no-send from empty-present/resend. |
| Live-test descriptions | Describe scenarios actually implemented; distinguish conservative local pacing from the current vendor rate limit. | **High / High** — test code/source comparison, no live run needed. |
| Independent expectations | Add source-derived error table, actual mixed NAV namespaces, lexical probes and feature/order cases under their implementation owners. | **High / High** — self-round-trip lists and lossy outlines cannot detect those omissions. |

[Detailed package and test acceptance](round-2/JUDGE.md#6-ee6--evidence-corpus-and-verification-methodology).

## 5. What was demoted, rejected or left open

### Five notes, not additional defects

- **D8 — currency count:** High confidence in stale count and remedy; remove “37,” retain the open type and current-source link. No missing currency capability.
- **D10 — credential placement prose:** High/High; two query operations place credentials directly at root. Built-in writers are correct.
- **D11 — false defaults called absent:** High/High; clarify explicit flags and empty seller container. No default behavior change.
- **B.E05 — receipt automatic MNB:** High confidence in **receipt-specific first-party support** from PHP class/example documentation. Keep the feature. A provenance/source-conflict note is reasonable, not an explicit-rate gate. No live receipt rate result was observed.
- **N.AF — `afalevon` unit:** Medium confidence that “percentage” is unsupported specificity; Low that it is actually false. High-confidence remedy: neutral reported-integer wording until the unit is established. No 0–100 gate.

### Explicitly rejected expansions

- Additional PDF id/payment-method/notification-warning fields on present evidence; C2 retains only its two operation-schema fields. Create/storno payment-method exposure has its own direct source and stays.
- A new strict date range, full XSD validator, arbitrary unknown-code rejection, automatic retry/recovery engine, locally inferred OSS/account eligibility, or receipt arithmetic correction.
- Removing receipt MNB support because it lacks local live testing.
- Treating positive-gross storno failure as an observed incident, or making missing gross a required-response error to avoid the worker decision problem.
- Blind schema replacement or a project-edited XSD presented as upstream confirmation.

### Remaining vendor questions

1. Which `simpleItems`/preview order(s) does the server accept, and does the combination preserve non-issuing preview?
2. What does query `hivasAzonosito` do? What is receipt call-ID scope/retention and storno reuse behavior? Receipt **last match is documented**; exact ordering and selection after reversal remain open.
3. How do partial receipt email overrides, empty strings and multiple recipients behave?
4. What wins when XML credentials and an existing session disagree, and what changes invalidate sessions? What direct browser/CORS access is supported?
5. What are the unprobed storage/paid-flag/storno/reference behaviors beyond the account-bound observations, and which optional PDF/NAV fields are emitted today?

These questions do not block the bounded repairs above. The [complete 44-row disposition register](round-2/JUDGE.md#all-44-residual-register-rows) retains their precise evidence requirements and all accepted no-change boundaries.

## Recommended implementation packages

| Order | Package | Findings / dependency |
|---:|---|---|
| 1 | Invoice/domain guidance | D1/D2 first; D5/D6/D7/D9/D12/D13 and housekeeping in the same coherent sweep |
| 2 | Response lexical interoperability | F1/F3; preserve public types, error precedence and explicit compatibility decisions |
| 3 | Known refusals | F2; independent of `simpleItems` because some codes already affect inherited/existing documents |
| 4 | Storno evidence decision | E.E5a; agent guidance plus separately owned worker/durable-step follow-up |
| 5 | Simplified-image request | C1; next appropriate breaking request release; retain source conflict |
| 6 | Response boundaries and text | R1/R2; genuine version fixtures, scoped extraction and decoded text |
| 7 | Document response exposure | C2/E.E4; optional fields and shared header reader |
| 8 | Taxpayer business record | C3; build on package 6, keep worker/CLI projections separate |
| 9 | Transport helper/contract | E.E1/E.E2/E.E3; exact cookie match and accurate HTTP/jar/platform guidance |
| 10 | Recovery/receipt guidance | D3/D4/B.E01–B.E04, plus MNB provenance note |
| 11 | Evidence corpus closure | E.E6; provenance rules apply from the first fixture change, not only at the end |

F2 intentionally changes classification of previously unknown codes. R2 changes returned characters. R1 rejects malformed/ambiguous shapes previously accepted. F3's explicit header grammar rejects prior underscore/extra-whitespace extensions. These effects belong in release notes alongside C1's source break and response JSON additions.

## Review record and verification

The five fresh reviewers inspected code and primary sources, ran targeted offline probes where useful, and provided preferred solutions and rejected alternatives. The independent judge read all five, inspected critical code/source claims, traced the PHP writer, and reran parser/cookie/storno counterexamples. The coordinating pass checked the newly elevated storno call site and final report scope.

This round did **not** rerun the earlier full targeted test suites or relabel their passing counts as fresh results. Proposed fixes remain unimplemented. No production edits or live-account operations were made by this review. Concurrent changes elsewhere—including a partial agent README HTTP-policy correction—remain separate; the corresponding source-documentation finding is not wholly closed.

- **[Complete independent judgment](round-2/JUDGE.md)** — authoritative detailed decisions, all confidence ratings, code/source citations, acceptance criteria and alias ledger.
- [Parser reviewer](round-2/01-parsers.md)
- [Error/receipt reviewer](round-2/02-errors-receipts.md)
- [Capability reviewer](round-2/03-capabilities.md)
- [Domain-documentation reviewer](round-2/04-domain-docs.md)
- [Skeptical scope/completeness reviewer](round-2/05-scope-challenge.md)

Where reviewer recommendations disagree, the judge's decision and this summary take precedence. All original 17 findings were revisited: 16 retained, currency-count D8 demoted. Additional retained items came from explicitly reviewing the previously unranked notes—not from counting duplicate reviews twice.
