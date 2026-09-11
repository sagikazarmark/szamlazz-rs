# Round 2 — reviewer B: errors and receipts, followed by a judging pass

**Date:** 2026-09-09. **Code:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`; no worktree changes under `crates/szamlazz-agent` at verification. Read `REVIEW.md`, `ADJUDICATION.md`, raw reports 03/05, the actual implementation, and the complete `docs/szamlazz-hu-behaviour.md`. Their prior verdicts were inputs to challenge, not authority.

## Result

**Retain F2, D3 and D4, with narrower implementation boundaries.** F2 is missing semantics in an advertised classifier; D3/D4 are documentation fixes, not a mandate to implement receipt recovery or arithmetic validation. Retain five residual documentation recommendations, **E01–E05**, mapped separately below. No P0/P1 issue established.

Fresh first-party evidence materially changes the previous uncertainty list:

1. **Receipt automatic MNB lookup has receipt-specific first-party support:** PHP 2.12.4's `ReceiptHeader` documentation **and receipt creation example** explicitly describe it. It is not merely an invoice inference. Preserve the capability; qualify its provenance and the differing general XML guidance.
2. **Receipt order lookup has a documented “last matching document” rule:** both English and Hungarian PHP documentation say so. It remains untested live here; the definition of “last” and treatment of an SN after reversal are not established.
3. **Query call-ID meaning is only partly clarifiable:** the PHP model calls its ID a *creation* identifier, but the query serializer deliberately does not emit it. This does not establish the server semantics of the optional query XML field, nor call-ID-only lookup. Rust's “as supplied at creation” overstates what is known about the request field.
4. **More simplified-image errors are relevant without a Rust `simpleItems` field:** 554 is not the only such path. A final inherits its prepayment's setting; current docs explicitly describe inherited 551, and 555 is the final/prepayment VAT mismatch. These are documented reachability, not observed failures.

Priority here: **P2** normal functional classifier work; **P3** documentation/provenance work. Confidence in a **gap** is separate from confidence in the **preferred solution**, and neither means a live incident occurred.

## Evidence ledger

All public URLs below were freshly fetched during this pass on 2026-09-09. Rendered pages report `v202608271632`; that is the site build, not independent proof of every paragraph's freshness. Only public documentation/package GETs were made.

| Ref | First-party source | What it establishes |
|---|---|---|
| S1 | [Error handling, EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | General code meanings; **five total sends**, stop after five unsuccessful attempts; 55 has expired-certificate and unreachable-timestamp-server causes |
| S2 | [Receipt create response](https://docs.szamlazz.hu/agent/generating_receipt/response) | 336–340; repeated creation call ID prevents a duplicate rather than replaying success; foreign receipt bank/rate requirement |
| S3 | [Receipt query request](https://docs.szamlazz.hu/agent/querying_receipt/request), [XML/XSD](https://docs.szamlazz.hu/agent/querying_receipt/xml) | Number/order selectors; optional `hivasAzonosito`, without a defined filtering/lookup role |
| S4 | [Receipt amounts](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) | HUF/Ft: integral item gross; net/VAT ≤2 decimals; exact sum; small 2-HUF tolerance on the other two arithmetic checks |
| S5 | [Receipt sending XML, HU](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml), [response](https://docs.szamlazz.hu/agent/sending_receipt/response), [PHP sending](https://docs.szamlazz.hu/php/nyugta-kuldes) | No email details → previous email; absent block → no email; optional children; example error 7 for missing subject |
| S6 | [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) | General requirement to supply foreign receipt bank and rate |
| S7 | [Receipt order rule](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number) | Separate receipt repetition toggle; multiple receipts per order possible; returned order value |
| S8 | [Receipt storno response](https://docs.szamlazz.hu/agent/reversing_receipt/response) | Returns SN; already reversed and reversing an SN are errors; numeric codes for those cases not specified |
| S9 | [Simplified image](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Per-document option, restrictions, **final/storno inheritance**, explicit inherited 551 case, 554 without the field |
| S10 | [PHP receipt query, EN](https://docs.szamlazz.hu/php/nyugta-lekerdezes), [HU](https://docs.szamlazz.hu/hu/php/nyugta-lekerdezes) | “same ‘last matching document’ behaviour as invoice queries”; HU: “utolsó bizonylat” |
| S11 | [PHP package page](https://docs.szamlazz.hu/php/), [official 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), [receipt generation](https://docs.szamlazz.hu/php/nyugta-generalas) | First-party client source, examples and operation coverage |

The ZIP was downloaded and inspected in memory, without extraction or execution. SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. Paths below are under `PHPApiAgent-2.12.4/szamlaagent/`:

- `src/szamlaagent/Header/ReceiptHeader.php:24–29`: `callId` is “A létrehozás egyedi azonosítója, megakadályozza a nyugta duplikált létrehozását” — unique creation identifier preventing duplicate receipt creation. `182–194`: create emits it, storno emits it, **query emits only `nyugtaszam`, `pdfSablon`, `rendelesSzam`**. `228–238` maps these wire fields directly.
- Same file `61–79`: `MNB` with an omitted rate uses the current MNB rate; the rate comment also mentions omitted/zero rate and currency availability in MNB. Some wording says invoice, but the bank comment says document and this is the receipt class. Independently, `examples/document/receipt/create_receipt_with_custom_data.php:38–44` sets EUR/MNB and explicitly says that when the rate is omitted the system uses MNB's current daily rate. Its actual example supplies 300.0, so it is **documentation, not a successful omitted-rate execution**.
- `src/szamlaagent/SzamlaAgent.php:480–488`: `getReceiptData` selects only number/order; it sets no call ID. This supports omitting that field, not a claim that the server ignores it if sent.
- `src/szamlaagent/Document/Receipt/Receipt.php:203–220,265–277`: the email writer sends nonblank values and omits the whole block when none are present. It supplies no per-field default-merge algorithm and no recipient-list parser. Do not copy this omission over Rust's intentionally present empty resend block.
- `src/szamlaagent/Response/InvoiceResponse.php:14–17,314–323`: 56 is notification failure; **number plus 56** counts as successful issuance. No analogous 55 success rule found in the inspected paths.

### Live evidence boundary

`docs/szamlazz-hu-behaviour.md:1–28,164–181,249–263` bounds observations to one test account and specific invoice/proforma operations. **There is no recorded live receipt operation, none of the thirteen F2 codes was observed, and no 55 outcome was reproduced.** 56 was not triggered (`153,180–181`); credential codes were explicitly unobserved (`255–261`). The actual rounding record is EUR invoices (`160–161`), with fractional HUF rounding explicitly unresolved (`249–254`).

Keep intentional, evidenced behavior: body-only errors/header-free deletion success; repeat invoice storno and success-shaped no-ops; nonunique/latest invoice external IDs; observed EUR rounding and HUF invoice tolerances. None justifies imposing invoice semantics on receipts. No request to change those implementations is made here.

## Part I — independent review

### F2 — Complete thirteen known-code mappings, not thirteen separate incidents

**Gap: High confidence.** `src/error.rs:220–253` omits all thirteen; `314–348` makes each `Unknown`. The module advertises typed documented codes and meaningful outcome classification (`3–13,22–28`), rather than an explicitly arbitrary catalogue subset. S1/S2/S9 document settled meanings. Local probes confirmed both body and header paths preserve messages/codes but classify them as unknown. Conservative fallback avoids false rejection, yet loses the advertised known semantics and can trigger unnecessary reconciliation.

**Preferred solution: High confidence; P2.** Add thirteen named `ErrorCode` variants, `known`/`code` mappings and explicit classes, using the existing open enum. Proposed names below are a concrete single naming proposal; numeric semantics, not spelling, are the compatibility-important part.

| Code | Proposed variant | Class | Exact meaning and operation implication |
|---|---|---|---|
| 336 | `ReceiptPrefixUsedForInvoices` | `Rejected` | Receipt creation prefix belongs to invoices; choose a receipt-only prefix |
| 337 | `InvalidReceiptPrefix` | `Rejected` | Receipt prefix format: uppercase letters/digits |
| 339 | `ReceiptNotFound` | `NotFound` | Receipt number not found; S2 explicitly lists query/send/“delete”. Do not invent a receipt-delete operation or assign this code to every storno failure |
| 340 | `ReceiptPaymentTotalMismatch` | `Rejected` | Supplied tender total differs from gross; correct receipt payment amounts |
| 363 | `ReceiptGrossMustBeWhole` | `Rejected` | HUF receipt **item** gross must be integral |
| 364 | `ReceiptNetPrecisionExceeded` | `Rejected` | HUF receipt item net has >2 decimals |
| 365 | `ReceiptVatPrecisionExceeded` | `Rejected` | HUF receipt item VAT has >2 decimals |
| 551 | `SimplifiedInvoiceAccountIncompatible` | `Rejected` | OSS enabled **or** non-Hungarian seller tax number; also inherited final after account changes |
| 552 | `SimplifiedInvoiceItemLimit` | `Rejected` | At most two items; final up to four (two negative + two new); checked before VAT-rate restriction |
| 553 | `SimplifiedInvoiceVatRateInvalid` | `Rejected` | Only 0/5/18/27/TAM/AAM/K.AFA/F.AFA; one disallowed rate rejects the document |
| 554 | `SimplifiedInvoiceCannotBeCorrected` | `Rejected` | Original uses simplified image; corrective refused even without `simpleItems` in its request |
| 555 | `SimplifiedInvoicePrepaymentVatMismatch` | `Rejected` | Final item VAT rates must match prepayment; item order irrelevant |
| 556 | `SimplifiedInvoiceDocumentTypeUnsupported` | `Rejected` | Simplified image forbidden on corrective/delivery note |

All thirteen: **`is_retryable=false`, `is_credential_error=false`**. 551 is an account-feature refusal, not an authentication code. 339 joins 7 in the existing `NotFound` documentation; keep 335 as its existing deletion refusal, and 338 as duplicate-call refusal. A `NotFound` class on code 7 still has its documented operation dependence: on `SendReceipt`, 7 can be **missing subject**, not missing receipt. A caller must inspect operation/code/message; do not remap every 7 to document absence.

**Current reach:** all seven receipt codes concern supported operations/constructible data. 554 can arise correcting an externally/UI-created original. S9 says finals inherit prepayment settings, explicitly including 551 if OSS changed; 555 likewise does not require the caller to set `simpleItems`. 552/553 constraints can matter to inherited simplified finals; do not call these independently live-proven numeric paths. Direct 556 requests require the currently missing flag. Catalogue completeness need not wait for the separate C1 feature.

**Logical request versus this send:** `Rejected` describes the exchange that returned that refusal. It does not prove an earlier send of the same logical issuance failed. After a lost first reply, a subsequent 338 proves no duplicate from the repeat, not that the logical issuance is absent; after account/input changes, even a new ordinary refusal cannot erase an earlier success. Extend the class docs to say this expressly. 339 is current lookup/refusal information, not a historical nonexistence guarantee.

**Rejected alternatives:** keep raw strings only (duplicates the classifier in every caller); classify numeric ranges as rejected (future success-with-warning codes must stay unknown); add names without class changes (leaves the real gap); introduce an operation-aware outcome hierarchy just for 339 (unnecessary API expansion given current 7 semantics); add local arithmetic/account gates (different scope, can mis-model server rules).

**Code/API impact:** additive non-exhaustive enum variants; existing `Unknown("339")` becomes named, and thirteen classifier answers change intentionally. Add a release note for consumers matching unknown strings. No request writer change and no automatic retry. Downstream issuing code using the helper may now settle a documented refusal rather than enter an unknown-outcome path; that is consequential behavior even though tokens/messages remain intact.

**Focused regression criteria:** independent source-derived table of these thirteen numeric tokens, expected variants/classes/retry flags, padded numeric spelling and reverse tokens. Feed actual failed `xmlnyugtavalasz` bodies for receipt codes, `xmlszamlavalasz` for 551–556, and `xmlnyugtasendvalasz` for 339; representative header 363/554/339 paths. Assert original Hungarian message survives. Controls: 338 remains an error, 7 send/missing-subject remains distinguishable by code/message, 335 remains rejected, 55 and unnumbered 56 remain unknown, and future numeric/NAV textual/absent codes remain open. Do not make enum-list self-consistency the only completeness check.

**Live occurrence:** none established for these thirteen; operational impact is conditional on receiving them and relying on the classifier. This is one coverage defect, not thirteen demonstrated incidents.

### D3 — Replace universal external-ID advice with operation-specific recovery guidance

**Gap: High confidence; solution: High confidence; P3.** `error.rs:9–13,256–270,353–379`, `client.rs:63–72`, README:228 give invoice external-ID recovery to all operations. Receipts have no `szamlaKulsoAzon` field; S2/S3 provide a different identity mechanism. Existing `CreateReceipt` docs already correctly distinguish 338 from successful replay (`receipt.rs:89–109`); the common docs contradict that usable local guidance.

**One preferred solution:** put a short operation table in the public error/recovery docs and link the shared helpers to it:

| Operation | Meaningful next action after an unanswered exchange |
|---|---|
| Invoice creation / invoice storno | Reconcile the operation's external ID if one was supplied, verifying what it found and accounting for an earlier send still in flight. The worker is one stronger implementation, not a generic guarantee that an immediate empty query authorizes resending |
| Receipt creation | Assign and persist one unique `call_id` **before the first send**, retaining it for resends of that same logical issuance. 338 refuses a duplicate; it supplies no original number/PDF. Query a known receipt number or a deliberately managed order number. Check the returned receipt's order, creation call ID where present, type and reversal state before adopting it. If neither identifies the original conclusively, stop for reconciliation rather than generate a new call ID |
| Receipt storno | Query the known original number and inspect reversal state; a reversed original does not by itself recover the SN number/PDF. Keep any original storno call ID on a repeated logical storno, but do not promise create's full 338 semantics or invoice-style successful storno replay: S8 specifies an error for already reversed originals |
| Invoice/receipt queries and taxpayer lookup | Read-only retry decision; an unanswered read is not evidence of an issuance by that read. No receipt call-ID generation needed |
| Credit-entry registration / proforma deletion | Reconcile the relevant document/credit entries; an external-ID existence query cannot tell whether the mutation landed. In particular additive credit-entry repeats may add twice; replacement can overwrite intervening changes |
| Receipt email send | Receipt existence says nothing about whether email was sent. A repeat may send another email; `OutcomeClass` is not email-delivery deduplication |

An intended new issuance is a new logical operation; a network resend of the old issuance is not. Never rotate the receipt call ID just to bypass 338. No automatic random ID generation by the low-level client is needed. Apply the exact vendor send ceiling in E01, without treating it as permission to resend unsafe writes.

S10 now supports describing order lookup as **vendor-documented last matching document**, rather than entirely unspecified. This makes identity verification more important when receipt order repetition is allowed (S7). It does not supply arbitrary older-receipt recovery or settle SN selection after reversal. E03 owns the query-field qualification itself.

**Rejected alternatives:** implement a client retry/recovery loop (requires caller-owned identity, persistence and business decisions); expose an invented call-ID selector; translate 338 into success; impose receipt order uniqueness locally; prescribe five unconditional sends.

**Impact:** rustdoc/README only. **Regression criteria:** compile a short example using real receipt number/order `ReceiptSelector` variants; show the same persisted create ID on a resend, with 338 handled as unresolved original-result recovery. Inspect rendered shared docs for operation-qualified links. A mock that merely assumes server deduplication is not proof of it and is not needed for a prose-only change. **Live occurrence:** no observed failed recovery/duplicate receipt; impossible external-ID instructions are evident from the interface and vendor schema.

### D4 — Scope invoice rounding observations and document receipt amount constraints

**Gap: High confidence; solution: High confidence; P3.** `item.rs:20–26` and README:215/217 generalize observed invoice storage behavior to shared line items. S4 specifically requires already rounded HUF/Ft receipt values. The scratch probe confirmed `Scale(2)`, quantity/price 1 at 27%, produces `1 / 0.27 / 1.27` and passes local receipt validation; S4 says this gross draws 363. That is deliberate raw financial input, not a calculator bug.

**One preferred solution:** retain `Exact`, `Scale`, `minor_unit` and asserted `LineItem::new` unchanged. Label independent two-decimal storage rounding as the **recorded EUR invoice** behavior, then add the S4 receipt rules to `CreateReceipt::items`/type docs with a cross-link from `Rounding`. Explain that a successful calculation guarantees its arithmetic/rounding policy, not server acceptance. HUF minor-unit calculation gives integral net/VAT/gross and exact sum for ordinary supported rates, but callers with externally fixed gross may instead need explicitly computed fractional net/VAT.

Use S4's accepted `787.40 / 212.60 / 1000` shape to avoid implying net and VAT must be whole. `Scale(2)` alone does not guarantee integral gross. Do not extend the HUF-specific rules to EUR or infer foreign receipt storage precision. Scope README:217's “what is sent is what szamlazz.hu stores” to the actual tested HUF/EUR invoice cases.

**Rejected alternatives:** silently round asserted receipt values (changes caller totals); require whole net/VAT (rejects the vendor example); forbid `Exact` on receipts (exactly computed values can already comply); add a receipt calculator or duplicate all server checks as part of a documentation correction.

**Impact:** documentation only. **Regression criteria:** a compiling receipt example using explicit `787.40/212.60/1000` remains representable, plus a minor-unit example with a fractional unit price and whole gross/exact sum. Existing arithmetic tests cover calculation; do not add tests asserting that a mock server enforces S4. **Live occurrence:** no receipt rejection observed; the invoice observation is retained, not disputed.

### E01 — Exact five-send ceiling and honest code-55 retry guidance

**Gap: High confidence; solution: High confidence; P3.** `error.rs:256–270` says “at most ~5 times,” attributes retrying both 1 and 55 to the vendor, and calls 55 “issued, signing failed.” S1 permits **at most five sends of the same request, including the initial send**; it is not five retries after the first. It prescribes later retry for maintenance, but 55 only describes signing failure with two causes. The code does not prove a document exists, nor that waiting repairs an expired certificate.

**One preferred solution:** correct the prose while retaining `55 → Unknown` and `is_retryable(55) == true` as a **potentially transient hint**, explicitly not a guaranteed self-healing condition or an instruction to retry a write. Say timestamp connectivity may recover; certificate expiry requires operator action; no specific successful retry interval for 55 is established. Make the ceiling five sends, then stop/escalate if unsuccessful. A new process invocation or fresh receipt call ID is not an allowance to restart an unresolved request's budget. Reconciliation reads are different requests, not free permission for additional writes, and repeated identical reads are subject to the same vendor ceiling.

**Rejected alternatives:** 55 → `Rejected` (no proof nothing landed); 55 → successful document (no number-backed rule); removing 55 from the hint just because one cause is persistent (loses the documented transient cause); enforcing a global counter in a stateless transport (logical identity and previous sends belong to the caller). **Impact:** documentation only, with existing tests preserving classifications. **Regression criteria:** rendered docs say “five total sends,” never “five retries”/“~5”; no 55 issuance assertion; maintain explicit true/Unknown controls for 55. **Live occurrence:** no 55 observed and no excessive-retry incident demonstrated in this crate.

### E02 — Separate documented, first-party-client and live-observed provenance

**Gap: High confidence; solution: High confidence; P3.** `error.rs:299,854–857` says the classification table was verified/observed on szamlazz.hu, while the behavior record explicitly excludes credential codes/56 and supplies no 55 probe. README:229 also omits the **number condition** on 56. `client.rs:201–204` says a minute-long stall “still issue[d]”; the supplied behavior evidence at line 152 instead records the stalled send **issued nothing**. The broader in-flight risk is valid, but that particular observation is not supported by this record.

**One preferred solution:** a compact provenance note for the error table distinguishing vendor-documented refusals, named observed additions (14/73/221/352/463), and the PHP-source-backed number-dependent 56 rule. Update test comments to “documented and observed evidence,” not “every code observed.” Add the number condition to README:229. Rephrase the timeout comment as observed stalls plus the conservative fact that client timeout does not establish server completion; retain the timeout and reconciliation caution.

**Rejected alternatives:** weaken credential refusal classes or remove 56 because unobserved locally; turn conservative uncertainty into a claimed live fact; change timeout/retry policies from a provenance correction. **Impact:** prose/comments only. **Regression criteria:** review each “observed” assertion against its named evidence; retain the existing numbered/unnumbered 56 parser controls. **Live occurrence:** documentary contradiction established; no runtime defect or evidence that the timeout itself is wrong. This item does not edit the worker or its policies.

### E03 — Qualify query call ID; incorporate documented order lookup without inventing selectors

**Gap: Medium confidence; solution: High confidence; P3.** `receipt.rs:368` says query `hivasAzonosito` is “as supplied at creation.” S3 calls it an optional unique call identifier, with no stated query filtering meaning. The shared PHP `ReceiptHeader` names a creation ID, but excludes it entirely from query XML. The response field is explicitly a creation ID in S2; that does not prove the same semantics for this request field. The current Rust prose may be right, but its asserted interpretation remains unsupported.

**One preferred solution:** keep the optional low-level field and serialization; document it as an optional wire call identifier whose **query behavior is not specified**, recommend leaving it absent for documented number/order lookups, and remove the asserted creation-filter meaning. On `ReceiptSelector::OrderNumber`, cite S10's “last matching document,” explicitly not a way to select a particular older receipt. Note unverified ordering criterion/SN behavior. No `CallId` selector added.

**Rejected alternatives:** remove the field because PHP omits it (current XML schema includes it); claim it is a query invocation id, creation filter, or ignored (none established); allow call-ID-only queries because XSD selectors are optional (business prose requires number/order); retain the old blanket “order selection undocumented” conclusion (S10 contradicts it).

**Impact:** documentation only; runtime ambiguity remains openly stated. **Regression criteria:** existing query XML tests preserve each selector and optional call-ID emission; a docs example works with `call_id=None`. No synthetic test purporting to prove which server record is selected. **Live occurrence:** none; confidence concerns unjustified specificity, not proof the old interpretation fails.

### E04 — Narrow receipt email default and recipient-list promises

**Gap: Medium confidence; solution: High confidence; P3.** `receipt.rs:422–428` promises independent fallback for every `None` field and comma-separated recipients. S5 specifies reusing previous email when details are absent, but not arbitrary partial-field merging, empty-string clearing, or recipient-list syntax. S11's receipt writer only sends nonblank supplied fields; that does not establish server merging. A first send with missing subject can fail with 7 despite schema optionality.

**One preferred solution:** describe `None` literally as omitting that XML child; document the whole-details-absent resend behavior separately. Remove the comma-list guarantee, using one recipient in examples; mark partial overrides/empty values as unspecified server behavior. Retain the optional string API and the intentionally present empty `<emailKuldes>` block used by `SendReceipt::new` to request resend. A fully supplied first-send example is useful without declaring every field globally required.

**Rejected alternatives:** require all fields in the Rust type (breaks documented resend); implement local merge/cache or email validation; treat absent block as equivalent to empty block; silently split and send multiple emails (new side effects). **Impact:** docs only. **Regression criteria:** verify default send emits a present empty block, `None` child omission differs from `Some("")`, and supplied fields remain serialized in order; most are writer-policy checks, not new server-behavior tests. **Live occurrence:** none; no evidence of actual partial-default or multiple-address failure. The defect is the strength of the promise, not proof the server rejects those forms.

### E05 — Receipt automatic MNB: retain capability, state the source conflict precisely

**Gap: Medium confidence for unqualified provenance, not a demonstrated functional gap. Solution: High confidence; P3.** `types.rs:932–962`, `receipt.rs:117–120,189–199,948–959` accept/document bank MNB with an omitted rate. S6/S2's general rule asks for both. Contrary to raw 03 Q-A and the first adjudication, S11 explicitly describes the omission behavior in receipt-specific source and a receipt example. This is positive first-party evidence, stronger than merely XSD optionality, although not a live test or an unambiguous current wire-page guarantee.

**One preferred solution:** retain `ExchangeRate::automatic_mnb()` and the receipt validator/serializer behavior. Add a concise receipt note citing the PHP 2.12.4 receipt-specific guidance, distinguishing it from the XML page's general bank-plus-rate requirement, and noting the lack of a recorded receipt probe. Describe the local test as proving emission/acceptance **by this crate**, not server validity; document currency availability in MNB as a condition, without inventing a local supported-currency table.

**Rejected alternatives:** require an explicit receipt rate (removes first-party-supported capability without observed failure); add a separate receipt exchange-rate type; substitute rate zero (additional semantic choice); label the capability wholly undocumented; claim it was live-verified. **Impact:** documentation/test description only. **Regression criteria:** retain the omitted-`devizaarf`, MNB-bank wire check and an explicit-rate check. Neither is a live acceptance test. **Live occurrence:** no failure or success observed for receipt MNB lookup. Escalation to a functional change requires contradictory vendor confirmation or separately authorized evidence.

## Part II — separate judging pass

This is a second pass by the same reviewer, not a claim that another independent agent adjudicated it. It tests the proposed changes against three questions: does the low-level contract promise the missing behavior; is the proposed change supported without a live inference; and can a smaller change solve the real problem?

| Item | Judge disposition | Why this solution survives; scope limit |
|---|---|---|
| F2 | **Retain P2, High gap / High solution** | Known settled meanings belong in the advertised classifier. Add names **and** mappings, twelve rejected/339 not-found. No business-state validation or retry machinery. Corrective 554 and inherited finals make deferral behind C1 unjustified |
| D3 | **Retain P3, High / High** | An invoice-only query cannot recover a receipt. One operation-specific guide is sufficient; no new API. Keep exchange outcome distinct from accumulated logical-request outcome |
| D4 | **Retain P3, High / High** | Source contradicts a generalized server-rounding expectation, not the intentional caller-supplied arithmetic surface. Document constraints and examples; preserve raw/exact amounts |
| E01 | **Retain P3, High / High** | Ceiling is exactly five sends. 55 remains uncertain/potentially transient. No basis for changing its class or promising retry success |
| E02 | **Retain P3, High / High** | Mixed evidence is mislabeled as all-live. Correct assertions and numbered-56 condition, not tested runtime behavior |
| E03 | **Retain P3, Medium / High** | Source cannot prove query call-ID interpretation; neutral docs solve overstatement. S10 partially settles order lookup and must replace the old all-unverified claim |
| E04 | **Retain P3, Medium / High** | Missing specification is not proof of a broken email implementation. Remove specific unsupported guarantees while retaining documented resend and optional wire fields |
| E05 | **Retain only P3 provenance clarification, Medium / High** | **Reject a receipt-MNB functionality defect or explicit-rate gate.** Receipt-specific PHP evidence supports preserving the implementation; acknowledge the wire-doc discrepancy |

### Judge's rejected expansions and corrections to the first round

- **No automatic receipt recovery defect:** the client explicitly sends once and leaves identity/persistence to callers. D3 repairs bad instructions; it does not require a durable workflow in this crate.
- **No thirteen observed production failures:** F2 is one source-derived classifier gap. Header probes show parser capability, not evidence of header emission for each operation/code.
- **No arbitrary code reassignment from messages:** do not label all receipt storno “missing data” cases 339 or 7; the storno page gives messages, not numeric assignments. Likewise S2's “delete” wording does not add a twelfth operation.
- **No evidence-free stronger 338 guarantee for storno:** the field is emitted by the first-party storno writer, but uniqueness scope, retention and cross-operation collisions remain unspecified. D3 should remove categorical repetition semantics from `StornoReceipt::call_id:296–298` unless separately sourced; preserve the field and use a distinct ID per logical operation.
- **Receipt queries are not call-ID-only lookups:** response identity, a shared PHP creation property and an optional query schema field do not establish that selector. Its exact server behavior remains unresolved after source inspection.
- **Order-query uncertainty is narrower now:** the vendor documents last matching document; no need to ask whether it is wholly unspecified. Still no established ID/date tie-break, SN inclusion/inheritance behavior, normalization, or live validation.
- **Receipt MNB is no longer merely an invoice extrapolation:** two receipt-specific PHP comments corroborate it. A source-backed feature does not need a local live probe to remain supported, but cannot be presented as observed. E05 is a small provenance repair.
- **No false simulator evidence:** a scripted fake server returning 338 or applying partial email defaults would demonstrate only its script. Focused regressions test the crate's classification and wire contract; vendor behavioral questions remain source/live-evidence questions.

## Verification performed

Fresh standalone scratch program: `/tmp/opencode/round2-reviewer-b`, depending on this checkout's `szamlazz-agent` with no HTTP-client feature; Decimal 1.43 resolved offline. Executed successfully:

```sh
cargo run --offline --quiet --manifest-path /tmp/opencode/round2-reviewer-b/Cargo.toml
```

It checked **68 synthetic error parsing paths**: seven receipt codes × four receipt operations × body/header, plus six invoice codes × body/header. Every one currently returns `Api`, preserving token/message and classifying `Unknown`/not retryable. Feeding a code through an operation is a parser check, **not an assertion that the service emits it on that operation**. Controls checked 338 rejected, 7 not-found, 55/56/future textual and numeric codes unknown, and 55's retry hint. It also checked local acceptance of HUF gross `1.27`, receipt MNB omission and default send's empty email block.

This pass did not rerun the first round's test suites or count them as new verification. No production edits, account calls, live tests, subagents, commits or changes to concurrent user work. Scratch files are only under `/tmp/opencode`; the sole repository deliverable authored by this pass is this report.
