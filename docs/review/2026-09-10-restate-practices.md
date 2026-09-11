# `restate-szamlazz`: alignment with Restate

Date: 2026-09-10. Reviewed the current working tree with parallel reviews of durable execution,
SDK integration/naming, deployment/replay, and independent primary-source research. Baseline:
Rust SDK 0.12.0, shared core 7.0.3, tested server 1.7.8.

This is a source review, not a runtime reproduction. No implementation changes or tests were run.
Primary sources and version qualifications are collected in
[the research note](../research/2026-09-10-restate-practices.md). Existing ADRs were evaluated as
design intent, not as independent evidence that their guarantees hold.

## Verdict

The core architecture works with Restate: a stateless Virtual Object serializes an order's writes,
external reconciliation stays inside the write closure, and normal upgrades use immutable deployments.
The strongest departures are narrower: fallible execution initialization before replay, erasing
cancellation into an application outage, and releasing uncertain writes while relying on a delay that
only protects retries of the same invocation.

Keep the domain protocol. Move execution-time dependencies behind the durable operation boundary,
preserve native lifecycle signals, and narrow several guarantees in the documentation.

## Findings requiring implementation or operational decisions

### R1 — High: eager credential failure can diverge from the recorded journal

Locations: `crates/restate-szamlazz/src/service/prologue.rs:103–123,342–375,407–415`;
`src/service/support.rs:273–293` in that crate.

An execution can record `namespace → account → lookup-invoice → … → create-invoice`, then be
re-executed. The prologue replays its two runs, fetches credentials outside a run, and returns a terminal
fault if the store fails. The SDK then writes an output command where the journal expects an operation
run command. Shared core rejects the command-type difference as a journal mismatch. Gateway construction
failure can take the same path. A recorded unfinished run is sufficient; the external write need not
have completed.

This contradicts the promised structured `unavailable` on replay in `README.md:182–189`.
Completed operation results also unnecessarily depend on the credential store being available to replay.

**Recommendation:** obtain credentials and lazily open an execution-local gateway inside an operation
closure that actually executes. Return only the operation outcome; using credentials inside a run does
not serialize them. Keep account/defaults available outside it for deterministic decisions. An interim
retryable initialization failure avoids writing a divergent terminal output, but still obstructs replay.

**Evidence:** SDK `endpoint/context.rs:832–860` writes terminal handler results through `sys_write_output`;
shared-core `vm/transitions/journal.rs:907–974` checks that output against the next recorded command.
See research R3/R4 and D6. A live replay regression test is the next verification step.

### R2 — High-impact SDK interaction: child execution span can suppress fresh logs after replay

Locations: `crates/restate-szamlazz/src/service/prologue.rs:59–65,141–149`;
`src/lib.rs:63–73`.

SDK 0.12 updates `restate.sdk.is_replaying` on `Span::current()`, but `ReplayAwareFilter` reads it
specifically from the `restate_sdk_endpoint_handle` ancestor. The crate's `execution` span surrounds
context operations. When replay transitions to new work, recording `false` on this child does not update
the ancestor; the child does not even declare the field. The ancestor can remain marked as replaying,
suppressing fresh gateway events and credential-rejection warnings.

**Recommendation:** reproduce a replay-to-fresh-work transition with the recommended subscriber;
prefer an upstream fix retaining the SDK span explicitly. This is an SDK defect exposed by ordinary
instrumentation, not a reason to abandon application correlation fields. Merely adding the replay field
to the child does not fix the stock filter's ancestor selection.

**Evidence:** SDK `endpoint/context.rs:69–76`, `filter.rs:53–85` (research R3/R9).
Source-confirmed interaction; no runtime reproduction in this review.

### R3 — Medium: terminal uncertainty releases protection against a still-processing send

Locations: `crates/restate-szamlazz/src/service/create.rs:926–962`;
`src/config.rs:472–477,506–527`; `src/service/handlers.rs:49–60`.

The issue-policy floor protects re-execution of the same write. It does not protect the next invocation
after the current one terminates. A valid `max_attempts = 1` policy can send, lose the connection,
re-query absence, complete as `outcome_unknown`, and let a queued invocation query/send immediately.
The same conceptual gap exists when an invocation is killed. If szamlazz.hu is still processing the
first request, the second lookup is too early. Correctives and cross-kind creates deserve particular
attention because ordinary duplicate-order-number protection is insufficient there.

**Recommendation:** revisit terminal/kill-on-exhaustion for unresolved Order writes. Retaining the
invocation through reconciliation, or recording an unresolved-write fence that later invocations honor,
would preserve protection. Durable waiting can enforce the existing timing assumption, but a strict
external exactly-once guarantee still requires a credible processing/visibility bound or vendor-side
deduplication. A pause/reconciliation policy is a reasonable use of Restate here, with operator costs.

**Confidence:** absence of cross-invocation delay is confirmed; an actual duplicate is conditional on
external processing after connection loss. This review did not establish that vendor behavior. Restate's
kill documentation explicitly does not guarantee external consistency (research D5).

### R4 — Medium: the paid-proforma guard replays an old authorization to delete

Location: `crates/restate-szamlazz/src/service/delete.rs:35–58,104–109`.

The lookup journals an unpaid proforma; a later closure deletes it. If execution is interrupted between
those points and credit entries arrive before replay, `force = false` still deletes using the old unpaid
result. The vendor itself does not protect paid proformas from deletion.

**Recommendation:** repeat the relevant checks against the pinned document number inside the deletion
closure before sending. Do not reselect a replacement proforma by external id. This removes the potentially
long replay-induced gap, although it cannot make a vendor query/delete atomic against external writers.
The need is the same durable-boundary reasoning already correctly applied to create.

### R5 — Medium: read cancellation becomes an indistinguishable outage

Locations: `crates/restate-szamlazz/src/service/support.rs:297–316,579–582`;
`src/service/prologue.rs:293–308`; `src/contract.rs:183–228`.

SDK cancellation becomes `503 unavailable`; only English prose says it was cancelled. The public
classifier groups this with uncertain/transient failures. Best-effort reads already propagate native
cancellation unchanged, making lifecycle semantics depend on where cancellation arrives.

**Recommendation:** propagate native cancellation on read paths, or preserve it in a machine-readable
fault code/cause if a uniform structured contract is required. For writes, retain both cancellation and
the possibility that the external write landed. Keeping exactly seven application fault codes is not
a sufficient reason to erase cancellation. See research D3/D5/R6.

### R6 — Medium: old mutation intent can act on a replacement document after retention

Locations: `crates/restate-szamlazz/src/contract/create.rs:50–55`;
`src/service/create.rs:478–495`; `src/service/delete.rs:31–47`;
`src/service/handlers.rs:89–92`.

A `reissue: true` invocation replaces reversed A with B. After its idempotency retention expires and B
is reversed, a delayed duplicate of that original request can now issue C. The boolean authorizes the
current reversed holder rather than identifying A. An old order-addressed deletion can similarly select
a replacement proforma. External-id discovery surviving journal expiry does not imply historical command
deduplication survives it.

**Recommendation:** explicitly bound caller retry horizons, or add an expected-document precondition
for destructive/reissue intent. Persistent native Object state for processed business commands is another
option if indefinite command deduplication is required. State need not mirror the external invoice ledger.
See research D4/D5/D7.

### R7 — Medium, external-protocol classification: unknown one-shot answers are treated as refusals

Location: `crates/restate-szamlazz/src/gateway.rs:823–835,874–883`.

Deletion and credit-entry registration classify every noncredential code as rejection, except deletion's
explicit 335. This includes codes the crate does not know. A response from the vendor is not necessarily
proof of no effect; the Számla Agent crate already makes that distinction for creation outcomes.

**Recommendation:** require operation-specific evidence for settled refusals. Preserve inconclusive
answers as data and report `outcome_unknown`, without enabling automatic one-shot retries. The classification
is confirmed; no observed post-action error for these two operations was established in this review.
This is domain correctness rather than a Restate naming or SDK-convention violation.

## Guarantees and runbooks to correct

| Topic | Finding and recommendation |
|---|---|
| Run duration | `src/config.rs:420–423` calls `max_duration` a hard bound. SDK 0.12 explicitly allows duration and execution-count overshoot. Describe an exhaustion threshold, not a deadline; retain real per-call timeouts. Research R2–R4. |
| Shared `get` | `src/service/handlers.rs:276–298` says “right now” and “nothing to replay.” `src/service/status.rs:24–38` performs four separately journaled reads concurrently with writes. It can mix observation times and replay ages. Keep the shared reader, document non-atomic observations, and use a fresh invocation/key for each poll. Native attach/output answers whether a particular invocation finished. |
| Credential rotation | `README.md:182–184` promises all in-flight executions pick up rotation. `src/account/static_resolver.rs:492–506` clones startup credentials; an old immutable deployment retains its old key. Dynamic stores must expose rotation to retained deployments, or static deployment credentials must be updated operationally. |
| Flag-day drain | `docs/design/restate-szamlazz.md:840–847` uses private → drain → switch. Private services still accept internal SDK calls. Quiesce internal producers and pending delayed sends, or state that the procedure assumes ingress-only producers. Research D5 and [private services](https://docs.restate.dev/services/security#private-services). |
| Exceptional replay | `README.md:946–951` and ADR 0009 should cover prefix restarts with a changed deployment as well as pause/resume. A step-name table is a regression signal, not proof that old branch logic, exact commands and inputs replay compatibly. Research D6/S4/S5. |
| Fault discovery | `Fault` deriving `JsonSchema` does not place it in handler error schemas. Provide a compiling generated-client fault-decoding example or a small helper, preserving native/raw errors when inner decoding fails. Research R6/R10/S8. |

All `src/` and README paths in this table refer to `crates/restate-szamlazz` unless qualified.

## Justified choices to retain

| Choice | Assessment |
|---|---|
| Stateless Order Virtual Object | Explicitly demonstrated by Restate's database guide: an Object can serialize external access without K/V state. A Workflow is not a better match for repeated lifetime commands. |
| Shared external-state reader | A useful way to inspect external state while an exclusive write is running or paused; it is not a transaction or invocation-completion barrier. |
| Query-first write closure | Essential because the vendor does not atomically commit with Restate's journal or enforce uniqueness of the external id. `ctx.run` alone cannot replace this protocol. |
| Deterministic external ids | New invocations months later need the same order/kind identity. An invocation id or generated UUID answers a different question. |
| Explicit issue/read/resolve policies | These configure native Restate run retries rather than reinventing them. The SDK's unspecified, `new()`, and `default()` policies are materially different. |
| One-shot lost answers as data | Correctly avoids deliberate run retries. Still not at-most-once external execution across a crash before completion is journaled. |
| Journaled account resolution and secret-free projections | Dynamic defaults affect replay; secrets and unnecessary document data should stay out of persisted results. The `Journaled` registry is a guardrail, not proof that arbitrary string contents are safe. |
| `Body<T>` | A small supported custom-decoding boundary for uniform malformed-body faults; SDK 0.12 offers no equivalent typed error-format hook. |
| `RunCtx` | Addresses actual SDK generic `Send`/metadata ergonomics. Its immediate-await discipline follows SDK 0.12 guidance. |
| Structured `Fault` in terminal message | Pragmatic given Rust SDK 0.12's code/string error channel. Keep the domain contract, but make consuming it easier. |
| Plain Rust Gateway shared by both services | Reusing external-system protocol locally does not require another durable service or RPC boundary. |
| Immutable deployments | Officially recommended. Dropping blanket cross-release journal compatibility is justified for ordinary pinned execution, with exceptional replay controlled explicitly. |

## Naming: largely aligned; do not rename for appearances

- **`Szamlazz.Order` / `Szamlazz.Agent`: keep.** Explicit service names and a library namespace are
  supported; no official rule found requires removing the prefix.
- **Snake-case handlers: keep.** They follow the Rust SDK's default. Other-language camelCase examples
  do not establish a Rust naming requirement.
- **Domain names (`storno`, credit entries, issued/found documents): keep.** Restate supplies execution
  vocabulary, not the vendor's financial vocabulary.
- **`Fault`, `Execution`, `Prologue`: defensible.** Local concepts with a documented mapping to SDK terms;
  renaming them buys little.
- **Kebab-case run names: fine, but project policy.** Restate describes names primarily as observability
  labels, not business deduplication keys. The ownership lookup and full lookup both being `lookup-{kind}`
  makes a create trace less clear (`src/service/support.rs:423–436`). Distinguish them if that improves
  diagnosis; one universal verb rule is not a Restate requirement. Names still participate in replay checks.
- **`run_once`: qualify or reconsider the name.** Its comment at `src/service/support.rs:503–507` promises
  at most one execution per journal entry, which `max_attempts(1)` does not guarantee across crashes.
  It currently wraps a pure namespace pin, so the defect is explanatory rather than a duplicate-write path.

Two low-risk simplifications use capabilities already in the SDK:

1. Put common handler settings on the service/object and override exceptions. The same write attributes
   repeat seven times in `src/service/handlers.rs`; assert effective inherited configuration in discovery tests.
2. Collapse a forwarding layer in `Order/Agent → Parts → prologue` (`src/service.rs:85–92,130–137,184–191`).
   Preserve the useful execution boundary; remove navigation that adds no behavior.

## Recommended order

1. Reproduce and fix R1's credential-on-replay boundary.
2. Reproduce the SDK logging interaction and prepare an upstream fix/report.
3. Decide the unresolved-write exhaustion policy; add a delayed-visibility/crash scenario before claiming safety.
4. Refresh deletion guards inside the write closure and preserve cancellation structurally.
5. Correct freshness, rotation, retention, timeout and exceptional-replay promises.
6. Consolidate SDK attributes and improve trace labels only where useful; retain the core domain architecture.
