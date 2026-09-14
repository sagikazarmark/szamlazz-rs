# Explicit Order execution and RequestResponse replay risk

Accepted 2026-09-14, #247 / #249. This consolidates the initial ordinary-invoice
experiment, its operation extensions and the production-interface decision.

## Decision

`WorkerConfig.order_execution` explicitly selects `config::OrderExecution` on
either host. It is a financial execution setting, separate from SDK protocol mode
and never inferred from the compilation target:

| Setting | Contract | Hosting |
|---|---|---|
| `protected` (default) | Acknowledged execution-local send permission; before arming, resumed work may continue toward its first send; after arming, interrupted or inconclusive writes reconcile read-only | Bidirectional hosting is needed for normal write progress |
| `replay_enabled` | An unfinished, unrecorded write may resubmit after fresh operation-specific checks; recorded uncertainty remains read-only | Native or Workers, including buffered RequestResponse |

Protected execution remains supported, including correctives. Replay-enabled
execution supports all four create kinds, exact-target reissue, pinned conversion
and chain references, proforma deletion, Order storno, reads and recovery.
Corrective issuance is refused with `invalid_input` before account/provider
operations in replay-enabled execution: vendor overlap produced two correctives,
and that duplicate risk has not been accepted.

Agent has no execution selector. Its existing unmanaged storno, credit-entry and
read contracts run on either host. Execution selection needs no `test-util`;
that feature retains unchecked test configuration and interruption observers only.
The pre-release experimental builder methods were removed.

The [outcome and marker-clearance table](../design/request-response-outcomes.md)
defines operation-specific execution and settlement. The
[transition procedure](../operations/order-execution-transition.md) defines release
and mode changes. These are current rules; the
[#247 implementation record](../design/request-response-issuance-brief.md) summarizes
delivery and the [research archive](../research/2026-09-14-request-response/README.md)
preserves the original evidence.

## Why the replay contract exists

The protected protocol writes an unresolved marker, executes an arm closure granting
a volatile permit, awaits acknowledgement, then consumes the permit in the write
run. The real-Restate prototype observed the actual Order pausing with **zero
sends**, without injected failure, under buffered RequestResponse on server 1.7.8 /
Rust SDK 0.12.0 / shared core 7.0.3. Receiving the arm completion requires another
execution; completed-arm replay grants no permission. This is a progress failure.

The narrow shell progressed across two endpoint instances. It recovered a visible
first document without resending, but sent twice while that document was invisible.
Recorded uncertainty remained read-only after resume. A surviving old execution
could send after its replacement completed. These were controlled native-host,
fake-provider observations, not multi-process or leader-election evidence.

The [vendor overlap findings](../research/2026-09-14-request-response/VENDOR.md)
then established operation-specific observations:

- Overlapping identical ordinary requests through independent fresh sessions both
  returned `CTEST-2026-28`, id `930038478`.
- Overlapping identical corrective requests produced `CTEST-2026-31` and
  `CTEST-2026-32`, ids `930038529` and `930038532`, referencing `CTEST-2026-30`.
  Their shared external id returned only the latter.
- Ordinary cleanup was verified. Corrective cleanup returned inconclusive code 13;
  the three linked test documents remain for operator review. The vendor lifecycle
  did not pass cleanup; archiving its evidence does not settle that request.

The provider [documents duplicate-order protection after timeout resubmission](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number).
Checking is an account setting and is per document type, with corrective/storno
exceptions and reuse after reversal. One ordinary pair supports that behavior,
not universal atomicity, a processing deadline or a measured duplicate rate.

## Accepted execution and evidence boundaries

Duplicate-order checking must be enabled on the intended account and independently
confirmed by its operator. `check_account` verifies neither that setting nor seller
mapping. The go-live account verification remains required. Ordinary overlap is
not proof of proforma/prepayment/final atomic deduplication.

Validation, stable scope/account/namespace and document identity, monetary preflight,
ownership, collision and exclusivity checks remain. Unresolved intent is committed
before possible provider activity through a RequestResponse-compatible durability
barrier. No replayed result contains a reusable send permit. Completed prerequisites
pin references; the executing open write checks the target and applicable guards
afresh before any resubmission. Reissue requires the exact old holder still reversed,
never an absent holder. Positive target evidence takes precedence over subsequently
consumed or reversed prerequisites. Issuance does not independently verify linkage
or VAT allocation; final deduction lines remain caller-owned.

Recorded uncertainty permits read-only reconciliation and pause only. A later
refusal, changed guard, failed initialization or empty query cannot settle an earlier
interrupted send, even on an apparently first execution. Cancellation, kill, elapsed
time and a fresh ingress key neither clear markers nor authorize renewal. There is
no timed resend or automatic resume loop.

Matching positive issuance can complete under accepted replay risk. It establishes
a matching document, **not uniqueness or exclusion of a delayed old effect**.
Deletion requires recorded acknowledgement or audited operator settlement; absence
and code 335 cannot establish deletion of an admitted target. Order storno pins the
original provider id, fulfillment date, appearance and derived e-invoice flag;
same-number echoes and unnumbered success remain uncertain. Reversal evidence and
candidate-associated notification warnings retain their existing rules. Observed
repeat-storno behavior is not a universal exactly-once guarantee.

Agent storno retains its unmanaged query-first issue policy and no-Order guard.
Unkeyed credit-entry registration retains additive-duplication and replacement-
overwrite exposure after an unrecorded interruption, its existing uncertainty
faults and caller-owned settlement. It gains no Order marker or recovery protocol.

## State and deployment boundary

The public `ReplayExecutionContract` names the operation-specific marker contracts.
Their serialized `request_response_*_v1` tokens, the `unresolved-write` state key
and existing durable command names remain stable, including the original
ordinary-named create commands now shared by the four create kinds.

Known protected and replay-enabled markers remain recoverable regardless of the
configured mode. Exact-marker comparison preserves omitted versus explicit null
members. Unknown contracts remain inspectable and blocking; recovery never sends.
The distinct prepare-result wrapper requires a valid discriminator/operation pair;
a legacy prepare result cannot become replay permission. Marker decoding
compatibility is **not** journal replay permission.

Every release, including same-mode Workers releases, needs a new immutable endpoint
with old code/config retained. Quiesce all producers, settle old work, inventory
markers and unfinished invocations, then switch registration according to the
[transition procedure](../operations/order-execution-transition.md). Review actual
journal prefixes before exceptional replay; never replace `arm-write` semantics in
place or resume an old armed invocation into resend permission. Draining invocations
alone does not establish empty marker state or settled external effects.

This qualifies the arm-dependent parts of ADRs 0004 and 0018 for explicit
replay-enabled execution. The [protected protocol](../design/order-write-protocol.md)
still defines the default. ADR 0012's exact-target intent, ADR 0014's direct Gateway
permission obligations, host-owned recovery authorization and ADR 0009's
exceptional-replay requirements remain.

## Host and transport

Both hosts use `restate-szamlazz → szamlazz-agent → reqwest`. The initial private
Fetch implementation and comma-header refusal were removed. A synthetic 307 exposed
reqwest's WASM defaults; the vendor does not return redirects, and the user accepted
reqwest's WASM redirect/header behavior for this endpoint. Shared parsing owns
response interpretation; native HTTP configuration is unchanged.

The WASM request deadline covers the full response, dropped reqwest exchanges abort,
and Workers reauthenticates through XML without persisting sessions. The worker's
host adaptation covers JS-future affinity, execution timers and SDK hosting. Exact
money, query facts, credential privacy and execution/account isolation remain.

The [Workers example](../../examples/workers/README.md) pins the interim SDK fix
and builds without library test utilities. Maintained native and signed/scoped
workerd suites exercise the actual services, interruptions, recorded uncertainty,
pause/resume, cancellation, kill and recovery. Fake-provider runtime tests do not
establish new live-account behavior.

## Alternatives considered

- Protected arming remains the default for consumers needing its conservative send
  contract, but cannot satisfy normal buffered RequestResponse write progress.
- A separate durable send gate adds claim-response uncertainty, delayed-claim
  closure, tombstones and recovery machinery; it was not selected.
- Waiting and automatically resending recorded uncertainty would treat elapsed time
  as settlement; it was not selected.
- One retry premise for all document types is contradicted by corrective overlap.
  Replay-enabled corrective support requires a separate decision.
