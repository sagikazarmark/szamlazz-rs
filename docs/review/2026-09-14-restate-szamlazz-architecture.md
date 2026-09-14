# restate-szamlazz: pre-adoption architecture review

Date: 2026-09-14. Reviewed commit: `1a3ea7619844f21ed4ca8e69f5a311a007dc4ad1`.

## Executive judgment

**Keep the protected Order protocol. Change the transient-failure lifecycle and simplify the contracts around it before broad adoption.**

Order contains necessary complexity: Számla Agent invoice creation lacks a provider idempotency key, external ids are non-unique discovery handles, and an interrupted exchange can have delayed financial effects. Removing durable admission and uncertainty retention would transfer those problems to every caller.

The architecture's largest mismatch with durable execution is earlier in the lifecycle: ordinary temporary failures can become **completed terminal faults**, requiring a new invocation instead of allowing the original business request to recover. Five attempts can be a sensible threshold for pausing; it is a different decision to complete the request permanently.

The worker is an opinionated invoice-issuance module, **not a complete Számla Agent facade**. Before adoption, decide what an Order identifies, whether automated credit-entry synchronization needs Order-like protection, and how documents are retrieved and delivered. Most additional fields and read operations are additive and can wait for concrete consumers.

### Recommended order of decisions

| Priority | Decision or change | Why before adoption |
|---|---|---|
| 1 | Retain business invocations across temporary prerequisite failures; decide retry versus pause deliberately | Changes caller retry identity, completion semantics and operational recovery |
| 2 | Decide the protection contract for credit-entry writes | Additive writes can duplicate; delayed replacements can overwrite newer intent |
| 3 | Fix misleading response distinctions and adjudicate final reissue-guard outcomes | JSON meanings and caller branching become persistent dependencies |
| 4 | Confirm Order cardinality, scope mapping and supported billing identity | Permanent external ids and lock partitions make migration expensive |
| 5 | Resolve positive-evidence assumptions for repeated reissue | Conditional safety concern; requires provider evidence or an explicit accepted premise |
| 6 | Reduce recovery/client coupling and repeated expert Gateway intent | Public storage and orchestration shapes become costly to evolve |
| 7 | Confirm required tax/money, delivery and networking capabilities | Missing fields are often additive; a workaround through custom orchestration is expensive |

The storno warning-loss defect should also be fixed promptly, although its implementation is localized and less adoption-sensitive.

## Method and confidence

Four independent subagents reviewed retry semantics, feature coverage, Order correctness, and interface complexity. Their reports were adjudicated against source; central retry, marker/permission, reissue, probe and warning-loss paths were independently re-read. Official Restate and vendor documentation were consulted; the retry reviewer also inspected pinned SDK/shared-core/server source.

This is an architecture review, not a formal proof. Findings distinguish:

- **Confirmed behavior/defect:** established by source, sometimes also by executed tests.
- **Contract decision:** intentional behavior worth changing before consumers depend on it.
- **Conditional concern:** a concrete failure sequence requiring an unverified provider/environment premise.
- **Feature gap:** absent capability, not automatically a bug.

The workspace was clean at the reviewed commit. Only this report was added. No vendor-live mutations were performed. Verification details are in §7.

## 1. Retry architecture

### 1.1 Three mechanisms must remain distinct

1. **Run retries:** Restate re-executes an unsuccessful operation closure. Explicit policy exhaustion produces a recorded terminal run failure in the pinned Rust SDK.
2. **Invocation retries/pause:** Restate retains unfinished execution. Paused invocations require resume; pause is not automatic eventual completion.
3. **Terminal faults:** a completed invocation replays its retained failure under the same `Idempotency-Key`. HTTP 503 does not make that failure retryable to Restate.

All recognized worker `Fault` values become `TerminalError`, including `unavailable`: [`support.rs:282–303`](../../crates/restate-szamlazz/src/service/support.rs#L282). Explicit run exhaustion is mapped to a fault at [`support.rs:311–329`](../../crates/restate-szamlazz/src/service/support.rs#L311).

### 1.2 Current defaults

| Mechanism | Default | Exhaustion behavior |
|---|---|---|
| Ordinary `[read]` | 5 executions; 5s initial delay, doubling to 60s; 5m duration threshold | Terminal `unavailable` |
| Explicit `[query]` | Inherits configured read policy if absent; own read-like defaults if table is present | Terminal `unavailable` |
| `[resolve]` | No count cap; 1s doubling to 10s; 1m duration threshold | Terminal `unavailable` |
| Credential fetch | 3 local attempts, 200ms pauses, 10s per-call deadline | Terminal initialization failure on ordinary operations, bypassing read/issue policy |
| `[issue]` | Unmanaged storno only: 5 executions, 2m doubling to 10m, 1h duration threshold | Normally terminal `outcome_unknown` |
| Protected mutation run | One consumed permission; `max_attempts(1)` | Uncertainty as data, followed by read-only reconciliation |
| Protected reconciliation | Invocation retry policy | Order mutations default to 5 attempts, then pause |
| Ordinary read handler infrastructure failures | 3 invocation attempts, 10s initial delay, doubling | Kill |
| Operator recovery infrastructure failures | 3 invocation attempts, 10s initial delay, doubling | Pause |
| Agent credit-entry infrastructure failures | 2 invocation attempts | Kill; open run can have repeated its write |

Sources: [`config.rs:325–379`](../../crates/restate-szamlazz/src/config.rs#L325), [`prologue.rs:376–438`](../../crates/restate-szamlazz/src/service/prologue.rs#L376), [`handlers.rs`](../../crates/restate-szamlazz/src/service/handlers.rs), [`recovery.rs:387–432`](../../crates/restate-szamlazz/src/service/recovery.rs#L387).

These are exhaustion thresholds, not hard deadlines or durable wire-request budgets. For fast failures, five read executions have nominal delays totaling **75 seconds**, not five minutes. Three fast credential failures take approximately **400ms plus call time**. Protected mutation invocation delays nominally total about **24 minutes** before pause; infrastructure read delays about **30 seconds** before kill. Actual timing and effective policies depend on execution time, consumed retries and deployment overrides.

### R1 — High: temporary failures complete valid business requests permanently

**Confirmed behavior; architectural recommendation.**

Example:

```text
create_invoice accepted
  → first ownership lookup needs credentials
  → store returns unavailable three times
  → initialization emits terminal unavailable
  → invocation completes
  → store recovers one second later
  → same retained key still returns the completed failure
```

The terminal initialization deliberately occurs inside the executing run, preserving command replay, but bypasses its retry policy: [`support.rs:581–604`](../../crates/restate-szamlazz/src/service/support.rs#L581). Ordinary read and resolver exhaustion have the same lifecycle consequence.

This transfers durable request management back to the caller. Order mutation completions are retained under their idempotency keys for 30 days. A new key is a new invocation, and the caller must distinguish this failure from one that may have followed an earlier effective write.

**Recommendation:** define retained business execution separately from bounded interactive observation. Temporary prerequisite failures should retain the same business invocation, automatically retry while permitted, then pause for repair/intervention. Preserve bounded explicit queries where a caller intentionally wants an unavailable completion.

Keep credential acquisition inside executing operations, and keep secrets out of journals. Change the error treatment there; do not move initialization ahead of replayed operations.

**Implementation trap:** catching an exhausted run and returning a retryable handler error is insufficient. The exhausted completion is already journaled. Replay can encounter it forever without re-executing the dependency. Change the policy before terminal completion, or explicitly design a fresh durable retry phase.

The pinned SDK 0.12.0 maps explicit run exhaustion to `FailAsTerminal`; shared-core support for pause does not mean this Rust wrapper exposes it. Invocation-controlled retries, as already used by reconciliation, or a suitable SDK enhancement are concrete directions.

### R2 — High: answered maintenance is treated as settled failure, not temporary failure

**Confirmed behavior; classification defect relative to retained-execution intent.**

Code `1` means maintenance, with the vendor asking callers to try again in a few minutes. Yet generic vendor answers are recorded as `Api`: [`gateway.rs:2407–2434`](../../crates/restate-szamlazz/src/gateway.rs#L2407). Explicit query converts that into terminal `szamlazz_error`; Order prerequisite reads convert it into terminal `unavailable`.

The lower-level crate already identifies codes 1 and 55 as potentially retryable reads: [`error.rs:327–339`](../../crates/szamlazz-agent/src/error.rs#L327). Code 55 can need certificate repair, so it is not necessarily a self-healing outage.

**Recommendation:** classify read outcomes by meaning before recording completion. Known maintenance should retry/pause under the applicable contract; deterministic request refusal should complete. Credential/subscription problems may require repair-and-resume for retained business commands. An unknown code must remain unclassified.

Do not apply this rule indiscriminately after a mutation send: an answered maintenance/signature failure can still leave write uncertainty, requiring read-only reconciliation.

### R3 — High operational cost: a failed pre-send read can strand an unsent request

**Confirmed conservative design, not a duplicate-send bug.**

The permit is consumed before Gateway initialization and the final leading query. A transport failure in that query becomes unresolved state even though that live execution sent no mutation: [`recovery.rs:397–425`](../../crates/restate-szamlazz/src/service/recovery.rs#L397), [`gateway/recovery.rs:256–277`](../../crates/restate-szamlazz/src/gateway/recovery.rs#L256).

Later absence cannot settle it. Restoring connectivity and resuming the invocation may therefore never issue the first document. An interruption after acknowledged arm but before send deliberately has the same result.

**Recommendation:** retain the crash safety, but consider bounded retry of safe pre-send reads within the still-permitted execution, or a durably recorded not-sent result followed by an explicitly designed continuation. An unrecorded local observation must never regrant permission on replay. This is a protocol change, not a one-line retry adjustment.

### 1.3 What Restate and vendor guidance actually support

[Restate's official error guide](https://docs.restate.dev/guides/error-handling) treats network problems, overload and dependency unavailability as transient. It supports bounded invocation retries followed by **pause**, and explains that bounded run retries can terminate the run. Thus the user's expectation is directionally correct: durable execution should retain recoverable work. It does not imply every dependency should be called forever automatically.

[Számla Agent's official guidance](https://docs.szamlazz.hu/agent/basics/error-handling#retry-limit) says the same request may be sent unsuccessfully at most five times before stopping for operator intervention. It also explicitly warns against loops until success.

These fit together:

```text
retain invocation and intent
  → retry safe operations within permitted budget
  → stop provider sends / pause when intervention is required
  → repair and resume the same invocation
  → after an uncertain mutation, only read-only reconciliation
```

`max_attempts = 5` does **not** establish compliance with a strict five-request limit. A closure can make multiple requests; interrupted open runs can re-execute; the vendor does not precisely specify how mutation and reconciliation selectors are grouped. Multiple requests do not by themselves prove a violation, but a run threshold is not durable per-request accounting. Resolve the accounting scope before promising a strict cap.

Pinned source matters: this workspace uses SDK 0.12.0, shared core 7.0.3 and Restate server 1.7.8. Server 1.7.8 source defaults to 70 attempts then pause, with a 500ms initial interval; current unversioned documentation contains different timing examples. Repository handler overrides remain decisive.

## 2. Order correctness

### 2.1 The core protocol is well justified

[`Execution::protected_write`](../../crates/restate-szamlazz/src/service/recovery.rs#L336) performs:

```text
validate and read prerequisites
  → journal exact intent
  → set unresolved-write marker
  → execute arm closure, await durable acknowledgement
  → consume execution-local permit once
  → fresh guard and at most one mutation send
  → journal conclusive result OR reconcile read-only
  → clear marker only after settlement
```

Important properties validated by the reviews:

- Every exclusive business mutation checks the same marker; even unreadable/unknown state blocks before account resolution.
- Completed arm replay does not recreate the local permit. `max_attempts(1)` is not used as the sole one-shot guarantee.
- Kill can release the lock but leaves the marker for later invocations to encounter.
- Cancellation preserves uncertainty rather than implying rollback.
- Transport retries and redirects are disabled; client cookies are execution-local.
- Reissue echoes of the old number do not establish replacement.
- Corrective evidence checks the original base; storno evidence verifies both the reversal and a matching reversed original.
- Deletion absence cannot settle an uncertain deletion; deletion refreshes exact target identity before sending.
- Operator recovery compares exact retained intent, distinguishes attestation from queried evidence, and records a receipt before clearing.
- Completed observations replay without credentials.

**No unconditional replay path granting a second protected mutation send was found.** That is a scoped review conclusion, not proof against arbitrary administrative prefix restart, account misbinding, outside writers or undocumented provider behavior.

### C1 — Medium: reported storno notification failure is lost during deferred verification

**Source-confirmed defect; no crash required.**

1. Provider returns numbered reversal SS-1 and notification failure, but insufficient gross evidence for immediate acceptance.
2. Immediate verification is temporarily unsuccessful.
3. `Unconfirmed::StornoVerification` preserves the candidate number but drops the warning.
4. Later read-only reconciliation establishes that same SS-1.
5. The caller receives `reversed` with no warning.

Immediate verification preserves the issued metadata; deferred verification does not: [`gateway.rs:2074–2093`](../../crates/restate-szamlazz/src/gateway.rs#L2074), [`gateway/recovery.rs:123–151`](../../crates/restate-szamlazz/src/gateway/recovery.rs#L123), [`gateway/recovery.rs:379–385`](../../crates/restate-szamlazz/src/gateway/recovery.rs#L379).

Retain the warning associated with its exact candidate and restore it only when that candidate is verified. Do not attach it to a different fallback reversal. Empty warnings are not delivery proof, but that does not justify discarding a known failure.

### C2 — High contract decision: changed reissue target has timing-dependent meaning

**Confirmed behavior, with conflicting policy rationale; send protection remains intact.**

Caller asks to replace A. Earlier reads see A reversed. Before the final armed query, B becomes the matching external-id holder. At that query:

- B live becomes `Found`, then caller outcome `issued B`.
- B reversed becomes `reversed B`.
- Absence becomes `conflict{target_changed}`.

The same B observed earlier instead yields `target_changed`. See [`gateway.rs:1680–1683`](../../crates/restate-szamlazz/src/gateway.rs#L1680), [`gateway.rs:2545–2556`](../../crates/restate-szamlazz/src/gateway.rs#L2545), [`create.rs:130–138`](../../crates/restate-szamlazz/src/service/create.rs#L130).

The service comment attributes B to an earlier execution of this step. Under protected replay, an earlier permitted send cannot re-enter the leading query with permission, so that explanation no longer justifies this branch.

**Recommendation:** consistently report a changed expected holder as a pre-send conflict. If adopting B is intentional, define that separately and explicitly. The existing behavior is tested and partially documented; changing it is an adjudicated contract change, not merely fixing a typo.

### C3 — Adoption-critical assumption: can a historical holder falsely settle a later reissue?

**Conditional safety concern; provider regression has not been demonstrated.**

Reconciliation accepts a matching Order/kind/base whose number differs from the immediately expected old number: [`gateway/recovery.rs:475–489`](../../crates/restate-szamlazz/src/gateway/recovery.rs#L475).

Counterexample under regressing provider reads:

```text
A issued and reversed
  → replacement B issued and reversed
  → reissue of B sends C, answer lost / C may still be processing
  → provider query returns historical A
  → A matches Order/kind and A != B
  → worker settles as reversed A and clears C's marker
```

Another mutation can then enter while C may still act. This is false positive settlement, unlike conservative blocking on absence.

Ordinary probes support newest-holder behavior but do not establish a monotonic-read guarantee across failure/recovery. Treat this as a **provider-consistency premise to verify or explicitly accept**, not a proven production bug. Add the historical-holder scenario as a specification test. If stronger evidence is needed, design a discriminator with actual provider support; arbitrary delay or an assumed ordering of undocumented ids is not proof.

### 2.2 Accepted limits that adoption must understand

- Scope/account/credential mapping is trusted. A valid key for the wrong account can issue there. `check_account` is not a seller/account pin.
- External writers are not fenced by Order's lock. Final target checks do not refresh every journaled prerequisite, such as the current liveness of a corrective base.
- The order-number foreign-document hint is best effort and ignores generic answered non-credential codes: [`gateway.rs:1328–1335`](../../crates/restate-szamlazz/src/gateway.rs#L1328). It is not exhaustive exclusion of external documents.
- A paused exclusive invocation holds the lock. Independent exclusive recovery needs the owner stopped and queued/prospective mutations controlled, as the runbook specifies.
- Arbitrary restart from an old journal prefix is exceptional replay, not ordinary safe resume. State commands and prior external effects require review.

### C4 — Medium: `check_account` overclaims acceptance on arbitrary vendor answers

**Confirmed inference; provider validation ordering unverified.**

Every non-credential vendor answer, including maintenance or an unknown code, becomes `Accepted`: [`gateway.rs:1822–1855`](../../crates/restate-szamlazz/src/gateway.rs#L1822). The premise that credential checks precede every other answer is not established; the agent error documentation itself says exact processing order is unknown.

Report credential acceptance only from evidence establishing it, such as the expected authenticated sentinel miss or a valid document. Preserve inconclusive maintenance/open answers as such. This matters because the handler is an onboarding/deployment gate.

## 3. Restate surface and feature completeness

The worker registers ten Order handlers and five Agent handlers: [`handlers.rs`](../../crates/restate-szamlazz/src/service/handlers.rs). They reach six of the eleven lower-level Számla Agent operation families. Missing operation families are PDF query and all four receipt operations. Other gaps occur inside represented families.

| Capability | Worker support | Adoption significance |
|---|---|---|
| Ordinary invoice, proforma, prepayment, final | Protected Order handlers | Strong for one billing unit with one supported chain |
| Corrective invoices | Multiple correction IDs; same Order's ordinary/prepayment/final base | No general corrective inventory; caller retains IDs/numbers |
| Storno | Protected Order; weaker unkeyed Agent route | Distinct guarantees should remain explicit |
| Exact proforma deletion | Namespace-owned or named-target; expected number and credit-entry guard | Strong coverage; unassociated and bulk deletion absent |
| XML document query | Number/order/external-id selectors; reduced facts | Useful status/reconciliation read, not complete document readback |
| Taxpayer lookup | Validity, name, stem, VAT code, addresses | County code, group membership and other business facts omitted |
| Credit-entry append/replace | Unkeyed Agent handler | Major protection asymmetry; see below |
| Clear credit entries | Absent, although lower-level dedicated operation exists | Explicit additive operation needed for empty intended sets |
| PDF/artifact retrieval | Absent | Delivery/recovery capability needed by many customer-facing applications |
| Initial invoice email / storno recipient | Supported; warning on reported failure | No worker delivery-recovery path; never recover by reissuing |
| Receipt create/reverse/query/send | Absent | Separate durable design if receipt users are in scope |
| Delivery notes | Absent, including expert create admission | Additional kind/lifecycle design |
| Preview, attachments, waybill details | Absent | Usually deferrable |
| Inbound payment/document observations | Separate IPN/Adatkapcsolat crates | Caller owns orchestration/indexing |

### F1 — High: credit entries introduce a second, weaker financial-write protocol

`Agent.set_credit_entries` has no marker or consumed permission. After the vendor accepts the write but before run completion is recorded, interruption can cause another send. The existing real-Restate regression explicitly expects two sends in this scenario: [`tests/e2e/agent_writes.rs:200–267`](../../crates/restate-szamlazz/tests/e2e/agent_writes.rs#L200).

Additive registration can duplicate entries. Replacement can restore an older set after a newer set. This affects Order-issued invoices too. A per-invoice lock alone does not settle an already delayed vendor request.

**Decide before adoption:** either this is an expert/supervised operation with caller-owned settlement, or ordinary automated synchronization needs a protected per-document mutation design. Do not globally increase its retry count as a durability improvement.

Explicit clearing is a related gap. Empty replacement is deliberately rejected, while the lower-level crate has `ClearCreditEntries`: [`contract/agent.rs:500–524`](../../crates/restate-szamlazz/src/contract/agent.rs#L500), [`ops/credit_entry.rs:195–217`](../../crates/szamlazz-agent/src/ops/credit_entry.rs#L195). A zero-valued entry is not an empty set; clearing needs its own uncertainty contract.

### F2 — High identity decision: an Order is one billing unit, not a general document collection

Four regular kinds occupy permanent slots. A matching live holder returns `already_issued`; the worker does not compare the new buyer/items/monetary intent with the old document. One Order cannot hold several independently live ordinary invoices or repeated prepayment/final cycles.

See [`create.rs:225–241`](../../crates/restate-szamlazz/src/service/create.rs#L225), [`create.rs:490–524`](../../crates/restate-szamlazz/src/service/create.rs#L490), [`identity.rs:147–171`](../../crates/restate-szamlazz/src/identity.rs#L147).

If a business order means a subscription or multi-shipment purchase, define a separate billing-unit identity before issuing permanent external ids. Casually suffixing keys later also changes vendor order numbers, automatic linking, foreign-document detection and serialization.

Do not count unsupported aggregation of multiple prepayments into one final as a worker omission: [current vendor document-type guidance](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) itself disallows that relationship.

`get` is also not an inventory: it queries only four current slots, not all correctives, replacements or reversals. Persist historical document numbers and correction identities in the caller or a suitable archive.

### F3 — Capability decisions before onboarding affected sellers

**Tax and accounting:** worker inputs omit document-level EU VAT controls, margin VAT/base, simplified items, settlement-period/ledger fields and payable adjustment available below it. See [`ops/invoice.rs:176–236`](../../crates/szamlazz-agent/src/ops/invoice.rs#L176), [`gateway/build.rs:190–232`](../../crates/restate-szamlazz/src/gateway/build.rs#L190). VAT tokens or comments cannot substitute for structured document controls. Validate actual seller requirements against the supported profile.

**Money:** authoritative amounts support a deliberately bounded set of currencies/categories and calculation conventions; they are not a gateway for arbitrary vendor-valid arithmetic. Test actual approved pricing examples before committing to the contract. Keep preflight rather than discovering mismatches after issuance.

**Paid semantics:** worker `paid: false` omits `<fizetve>`; explicit false cannot be represented. This is confirmed representation loss, not demonstrated incorrect paid status. Resolve omission versus false before changing the meaning of existing JSON: [`document.rs:46–48`](../../crates/restate-szamlazz/src/contract/document.rs#L46), [`gateway/build.rs:200–201`](../../crates/restate-szamlazz/src/gateway/build.rs#L200).

**Seller profiles:** seller bank/email values are account-level. Multiple scopes for the same provider account are not a safe way to select per-document bank/language profiles; that violates no-fan-in and splits protection. Add per-document/profile selection if required.

**Delivery:** the worker requests no PDF and exposes no artifact read. Reconciled issuance may lack the original customer-account URL. The statement that this URL exists only in create replies is overbroad: [the PDF response schema](https://docs.szamlazz.hu/hu/agent/querying_pdf/response) also permits it, and the lower-level PDF operation parses it. Availability is still optional. Consider a by-number artifact read and deliberately choose whether bytes enter journals or external storage. No dedicated invoice-resend operation was established in the reviewed vendor operation set; receipt sending is different.

**Receipts:** vendor call identity prevents duplicate creation but does not replay the success result; duplicate code 338 still requires recovery. A receipt module should preserve that identity and use receipt-specific query/money/storno rules. It need not reshape today's invoice Order.

### F4 — Useful additive reads that can usually wait

Explicit query omits payment method, exchange rate/bank, appearance, detailed buyer identity, line items and accounting facts. Taxpayer lookup omits county code and VAT-group information, preventing full-number reconstruction from an eight-digit lookup even when NAV reported the parts.

Add needed scalar facts first. If detailed readback is required, use a separate projection rather than enlarging every mutation journal. Queried partner identity is current provider-returned data, not an immutable at-issuance snapshot.

## 4. Overengineering and interface depth

### 4.1 Complexity that earns its place

- Protected admission, exact expected target and retained uncertainty: deleting them recreates hard problems in callers.
- Separate account resolver and credential store: journaled routing and rotating secrets have genuinely different lifetimes.
- Found/issued/explicit-query projections: different information policies, not merely redundant structs.
- Bounded mutation identity versus broader provider read identity: different uses with different requirements.
- Closed financial requests and open response tokens: typo rejection and forward-compatible interpretation are valuable.
- Monetary preflight, centralized evidence rules and real-runtime interruption tests: high leverage and locality.

The number of Rust types or lines is not itself evidence of overengineering.

### A1 — High: response shapes force callers to reconstruct distinctions already known internally

**Deletion:** `{"deleted":true,"reason":"absent"}` describes absence, which can mean consumption rather than deletion. The source explicitly says it is not deletion proof: [`storno.rs:410–438`](../../crates/restate-szamlazz/src/contract/storno.rs#L410). Replace the boolean/reason convention with distinct open outcomes for deleted, absent, conflict and rejected; keep vendor answers separate from worker reasons.

**Observation:** `get` maps both absent and colliding holders to a null slot: [`status.rs:77–81`](../../crates/restate-szamlazz/src/service/status.rs#L77). Report collision as a state or additive diagnostic rather than requiring an attempted mutation or log search to explain it.

**Known create outcomes:** one response struct with many optional fields permits `issued` without a number and conflict without a reason. Prefer known-case payload requirements with an opaque unknown case, or a validated response view. Open token sets do not require erasing known-case invariants.

**Vocabulary:** `Order.get.credit_entries` is an array of amounts; explicit query's `credit_entries` is an array of records. Use `credit_entry_amounts` for the reduced projection, and align number/total/reference field names where semantics match.

These are worth changing before JSON shapes proliferate. They are more consequential than moving private helpers between files.

### A2 — Medium-high: recovery clients must echo the persisted storage representation

`RecoveryRequest.marker` is the full `UnresolvedWrite`, including version, account topology and operation. The service compares it in full: [`contract/recovery.rs:254`](../../crates/restate-szamlazz/src/contract/recovery.rs#L254), [`service/recovery.rs:112–124`](../../crates/restate-szamlazz/src/service/recovery.rs#L112).

Exact stale-observation refusal is necessary. Sharing the persisted representation with the operator request is optional. Consider an opaque exact-revision handle plus visible reviewed intent, with the server loading and checking the complete pinned marker. Keep evidence tied to that exact revision and preserve host authorization; a token is not authorization.

This is a design recommendation, not evidence current matching is unsafe. JSON clients can echo an opaque object today, which reduces their burden; typed clients and schema evolution remain coupled. A replacement needs its own state/replay design review.

### A3 — Medium: expert Gateway requires repeated intent and implementation choreography

Expert custom orchestration is an explicitly supported use case, so it should not be removed just because it is advanced. But callers assemble kind/order/external id/base for a builder, again for `CreateStepRequest`, and again around serialized recovery intent: [`tests/gateway/recovery.rs:19–42`](../../crates/restate-szamlazz/tests/gateway/recovery.rs#L19).

Prefer normal preparation returning the checked outbound request and complete serializable recovery intent together. Retain a checked constructor for hand-built Számla Agent requests. Keep durable admission explicit. Review public lookup helpers for actual expert needs versus Order implementation choreography.

Avoid elaborate typestate pretending to enforce durability: `CreatePermission` is useful explicit intent, not a durable capability. Removing the forwarding `Gateway::build_create` convenience is much lower priority than eliminating repeated identity assembly.

### A4 — Smaller public commitments worth deciding, not a rewrite agenda

- Static scope keys are narrower than Restate's accepted alphabet to support host environment merging, although the host owns that loader. Supporting valid dashed scopes would avoid unnecessary custom resolvers. Where scoped operation is already acceptable, a single-entry scoped deployment avoids a future unscoped-to-scoped identity migration.
- `RetryPolicyConfig<Table>` uses public sealed marker types largely to choose defaults. Plain policy values with field-specific defaults could be simpler. Retain validated configuration. `[issue]` should be named/documented unmistakably as unmanaged storno, not protected issuance.
- Arbitrary HTTP client customization exists on expert Gateway, while protected Order always uses its fixed constructor. Confirm custom TLS/proxy requirements; if real, offer narrow shared transport options while retaining ownership of cookie isolation, retries, redirects and deadlines.
- Promote the existing actual-resolver seller-verification example into reusable onboarding functionality if hosts duplicate it. It remains point-in-time verification, not an account pin.

**Defer:** file splitting, generic serialization cleanup, private helper consolidation, contracts-crate extraction and purely internal trait reshuffling. They do not address the expensive adoption commitments above.

## 5. Recommended target lifecycle

For ordinary application issuance:

```text
accepted with stable scope, Order and request identity
  → safely retry prerequisites while permitted
  → pause for repair/intervention rather than complete temporary failure
  → record admission and consume one send permission
  → receive settled result OR retain uncertainty
  → reconcile read-only / pause / exact operator recovery
  → return a result whose shape expresses what is actually established
```

For interactive reads, allow deliberately bounded unavailable completions. For expert financial writes, state the weaker contract prominently or implement appropriate protected admission. Avoid one global retry policy for all three cases.

## 6. Focused follow-up verification

Highest-value scenarios, rather than broad additional unit coverage:

1. Credential store and ownership query recover after today's terminal threshold; intended retained invocation then completes without a new key.
2. Maintenance code 1 transitions to successful read without becoming a completed fault.
3. Failed armed pre-send read with zero mutation sends follows the deliberately chosen liveness policy.
4. Protected storno code-56 warning survives deferred verification and pause/resume for the same candidate, but not a different fallback.
5. Changed live/reversed holder at final reissue guard returns the adjudicated outcome.
6. Historical A → B → uncertain C → A evidence tests the provider-consistency premise.
7. Interruption after marker clear but before invocation completion; actual runtime restart with committed arm/open write.
8. Cross-deployment unresolved state and a complete host-operated recovery drill with queued producers, authorization, pinned credentials and receipt retention.

## 7. Verification record and primary sources

Subagents reported **62 passing Rust test cases** in total:

- Correctness reviewer: 38 focused Gateway/create/recovery contract/unit tests, including existing immediate storno-warning preservation.
- Retry reviewer: 20 prologue/support unit tests and 4 real-Restate e2e tests on a local **1.7.8** server with mocked Számla Agent.

The four e2e test functions were:

```text
query_policy::e2e_query_policy_is_independent_and_retains_completed_results
unresolved::e2e_unresolved_exhaustion_retains_one_send
e2e_order_protocol
arm_ack::e2e_unresolved_arm_ack_loss_never_grants_replayed_send_permission
```

`e2e_order_protocol` was filtered to four scenarios: read execution/exhaustion, run-versus-invocation retry accounting, flaky resolution, and operation-local initialization failure. This was **not the full protocol suite**. Two e2e tests selected by unit-module filters remained ignored: completed-operation credential replay and namespace cancellation. Filtered/ignored tests are not included in the passing count. The newly identified defect sequences were source-traced, not all reproduced by new executable tests.

Focused commands included:

```sh
cargo test --offline --locked -p restate-szamlazz --test gateway recovery::
cargo test --offline --locked -p restate-szamlazz --test gateway create::
cargo test --offline --locked -p restate-szamlazz --test recovery_contract
cargo test --offline --locked -p restate-szamlazz --lib gateway::recovery::
cargo test --offline --locked -p restate-szamlazz --test gateway storno::immediate_reconciliation_preserves_a_reported_warning_for_the_same_storno
cargo test -p restate-szamlazz --lib service::prologue::tests:: --locked --offline
cargo test -p restate-szamlazz --lib service::support:: --locked --offline
```

Runtime tests selected the local server through `RESTATE_SERVER_BIN`, unset reuse URLs, and used `--ignored` with the named test filters. No production integration or full-suite acceptance is claimed.

Primary references:

- [Restate error handling](https://docs.restate.dev/guides/error-handling)
- [Restate invocation management](https://docs.restate.dev/services/invocation/managing-invocations)
- [Restate versioning](https://docs.restate.dev/services/versioning)
- [SDK 0.12.0 run policy](https://github.com/restatedev/sdk-rust/blob/v0.12.0/src/context/run.rs)
- [SDK 0.12.0 context implementation](https://github.com/restatedev/sdk-rust/blob/v0.12.0/src/endpoint/context.rs)
- [Shared core 7.0.3 retry policy](https://github.com/restatedev/sdk-shared-core/blob/v7.0.3/src/retries.rs)
- [Shared core 7.0.3 journal transitions](https://github.com/restatedev/sdk-shared-core/blob/v7.0.3/src/vm/transitions/journal.rs)
- [Server 1.7.8 invocation state machine](https://github.com/restatedev/restate/blob/v1.7.8/crates/invoker-impl/src/invocation_state_machine.rs)
- [Server 1.7.8 invocation configuration](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/config/invocation.rs)
- [Számla Agent error and retry guidance](https://docs.szamlazz.hu/agent/basics/error-handling)
- [Vendor document relationships](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types)
- [Vendor VAT controls](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates)
- [Vendor PDF response](https://docs.szamlazz.hu/hu/agent/querying_pdf/response)
- [Vendor receipt creation response](https://docs.szamlazz.hu/hu/agent/generating_receipt/response)

**Bottom line:** the expensive changes are invocation lifecycle, financial-write guarantees, permanent identity and response/recovery semantics. Keep the resilience; make its public contract clearer and let durable execution retain recoverable work.
