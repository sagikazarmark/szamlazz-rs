# Recovering an unresolved Order write

Use the deployed `Szamlazz.Order` operator endpoints through the host application's authenticated and authorized
access path. The host controls access to both `observe_unresolved` and `recover`, including internal SDK calls;
the worker performs no caller authorization. Keep the original scope, Order key and invocation identity.
Recovery is not permission to reissue. Empty queries, elapsed time, cancellation and kill do not settle a write.

## Storno notification recovery

A known reversal with `warnings: ["notification_delivery_failed"]` remains
reversed. Never repeat storno to retry notification or change its recipient.
Settle reversal uncertainty through the read-only or audited recovery procedure,
then use szamlazz.hu's notification action for the existing document. Queries and
recovery cannot reconstruct notification history; an empty warning list is not
mailbox-delivery evidence. See the [recipient contract](../../crates/restate-szamlazz/README.md#storno-notification-recipient).

## Observe

Recovery currently takes the **full exact marker**. Preserve it as opaque JSON, including all values/fields
and integer precision, rather than reconstructing identity fields; key ordering need not be preserved.
This deliberate schema coupling remains for now. Future marker versions need an explicit compatibility and
state migration decision; use compatible code for recovery, never strip unknown fields to make it decode.

Call the shared `observe_unresolved` with a fresh invocation/key. It returns `absent`, `unresolved` with the
marker, or `unreadable`. Preserve the exact marker for recovery. For unreadable state use a compatible deployment;
do not replace or delete the state to make a mutation proceed.
An `unresolved` observation always carries `marker`; newer marker versions and unfamiliar state strings remain
inspectable but grant no recovery permission. `observe_unresolved` declares no retry, timeout or retention
overrides of its own: inspect the effective inherited settings, and use a fresh invocation for a new observation.

Inspect the owner in Restate's UI/admin API. Its named write run retains a safe reason and candidate document
number when available; the open `reconcile-write` failure names the original cause and latest reconciliation
reason. A crash before recording the write result can leave only conservative uncertainty. No failure text is
proof that the vendor did not act. Monitor marker age, paused owners, owners already completed/killed and queued
mutations; assign an operator to each unresolved incident.

## Resume first

Repair credentials or connectivity and resume the owner on its pinned deployment. Completed arming replay does
not grant another send; recovery only reads. Matching positive evidence completes the operation. Further absence
or inconclusive evidence eventually pauses again. Keep the original Idempotency-Key while unfinished; it attaches
to that invocation. `get` observes four ordinary kinds and does not include correctives or prove completion.

An Order can also pause **before arming**, during account resolution or a required prerequisite read.
Those transient failures, including document-query codes 1/55 and sanitized credential initialization
failure, remain retryable in the original run. Repair the dependency and resume that same invocation/key;
it can continue toward its first send. Completed prerequisite observations replay rather than refresh.
The prerequisite phase precedes marker preparation. Inspect the actual prefix: marker commitment itself
precedes the arm acknowledgement, so an interruption during arming can already retain a marker.
Optional best-effort hints, shared `get` and Agent calls retain their bounded policies.

### Suspected historical-holder regression

The worker accepts provider newest-holder/non-regression behavior as an assumption, not a guarantee.
If an external-id query returns an older document after a newer holder was observed, quiesce mutations for
the affected scope/Orders, stop automatic resume, preserve exact requests/numbers/observations and contact
provider support. Establish the exact pending write's effect and exclude delayed execution before proceeding.
By-number queries aid investigation; an older matching holder is not evidence of the new reissue.
Keep any marker until authorized evidence settles the exact request. If false settlement already cleared it,
maintain operational exclusion: the worker cannot recreate that missing uncertainty. The
[evidence inventory and accepted risk](../adr/0018-retained-order-execution-and-evidence-boundaries.md#provider-newest-holdernon-regression-assumption)
describe the A→B→uncertain-C counterexample.

## Submit independent evidence

An exclusive `recover` cannot run behind a paused exclusive owner. Quiesce all producers (including internal SDK
calls and delayed sends), inspect/cancel queued mutations, and deliberately stop the owner before recovery.
Kill releases its lock, not its external effects or marker. Submit a new recovery invocation with this body:

```json
{"operator":"support:alice", "marker": "REPLACE WITH THE EXACT OBSERVED MARKER OBJECT", "evidence": {
  "type":"document", "number":"SS-1"
}}
```

The host supplies the required, nonblank `operator` from the authenticated identity after authorization,
replacing any caller-supplied attribution. The worker records the string exactly as submitted; it does not
authenticate it. Missing or blank attribution is refused before marker access.

Evidence choices:

Copy candidate, issued and reversal numbers exactly as the vendor reports them. These recovery evidence numbers
accept nonblank XML 1.0 text without the mutation-input length bound, including whitespace and `:`; they are not
trimmed or normalized. A deleted number remains the bounded mutation target and must match the marker exactly.

- **Document:** the worker queries the pinned account and verifies the precise intent. Create evidence must hold
  the external id and match order/kind, corrective base and reissue constraints. Storno evidence can be verified
  directly by number, with its original reference and the original's reversal and order. Deletion cannot be
  proved merely by querying for an absent document.
- **Non-execution:** `{"type":"not_executed","audit_reference":"INC-301",
  "did_not_execute_and_cannot_execute_later":true}`. Independent evidence establishes both non-execution and
  impossibility of delayed execution of this exact request.
- **Audited positive settlement:** `{"type":"completed","audit_reference":"SUPPORT-301",
  "completion":{"type":"deleted","number":"D-1"},"completed_and_cannot_execute_later":true}`.
  Independent evidence establishes completed execution and impossibility of delayed execution. Use `issued`
  with the issued number or `reversed` with the storno number for those operations. The audit record must tie the
  result to the exact pinned account/order/intent; a completed deletion names exactly the pinned proforma.
  This also covers named-target deletion: the marker's proforma `external_id` is correlation, not evidence
  that the target occupied that slot. Use `operation.number`, never a coexisting namespace holder. `get`
  does not inventory manual/legacy documents, and absence by either selector cannot settle deletion.

Attestations are operator assertions, not worker-verified vendor facts. Keep the referenced evidence durably in
the operator's incident system. Record vendor support's confirmation of the exact request where applicable;
an observation that a document is missing or a statement that enough time has passed does not suffice.
Without sufficient evidence, keep the order blocked.

Recovery records the operator, token and submitted evidence before clearing the marker. Preserve its receipt;
Restate's recovery journal and idempotency retention are both 30 days, including keyed recovery calls.
A stale token, wrong target, mismatched operation or unknown
evidence is refused. Observe again with a fresh invocation. A subsequent business change still requires its
ordinary checks, the originally intended expected document and a deliberate new Idempotency-Key.

## Scope-migration inventory

Before a single→multi flag day or a scope mapping change, quiesce all producers,
including SDK calls and delayed sends. At the ingress gateway block ordinary business calls while retaining
authorized operator access under the old mapping. Resume paused owners or deliberately stop and recover them;
settle external uncertainty. Only then make both services private and finish draining. Service-wide privacy blocks
operator ingress too. Run from the repository root against the canonical Restate admin URL (redirects are refused):

```sh
cargo xtask check-order-migration --admin-url "$RESTATE_ADMIN_URL"
# Optional admin bearer credential is read from RESTATE_ADMIN_TOKEN, not argv.
```

The script exits 1 for any unfinished invocation or any Order state row, across **all scopes**;
unknown keys and unreadable values block too. Exit 2 means inspection failed and blocks the switch.
It prints scope/order/key identifiers without exposing marker values or credentials. A completed or
killed owner can retain state after invocation drain. Observe and recover that marker under its
**original scope**; never delete state or move it to make the check pass. Exit 0 is only an empty
inventory: independently settle unmarked, unkeyed and external in-flight uncertainty too. Keep producers
quiesced through both observations and the switch; neither query is a global completion barrier.
If the final inventory finds state, keep the old mapping and business calls blocked at the gateway. Set
`Szamlazz.Order` to `public: true` to restore authorized operator recovery ingress; Agent can remain private.
After recovery make Order private again and repeat drain/inventory before registering the switched mapping.

## Effective retry controls

| Control | Governs |
|---|---|
| `WorkerConfig.issue` | The run policy of unmanaged `Szamlazz.Agent.storno` |
| `WorkerConfig.read` | Shared/Agent reads, optional hints (including inside Order), and dedicated operator `verify-recovery` |
| `WorkerConfig.query` | Explicit document-query runs when configured; otherwise inherits `read` |
| `WorkerConfig.resolve` | Shared/Agent account resolution in the prologue |
| Order mutation invocation policy | Exclusive resolution, required prerequisite reads, retained reconciliation and infrastructure failures; default 5 executions, 2m → 10m doubling, then pause |
| Recovery invocation policy | Recovery infrastructure failures; default 3 executions, 10s → 1m doubling, then pause; journal/idempotency retention 30d. `verify-recovery` has its own bounded read policy and terminal initialization failure |

The protected write run consumes one permit; no setting or resume grants a second one. The Rust host can override
handler invocation policy through SDK `ServiceOptions` / `HandlerOptions` before binding the service definition.
Apply overrides to each intended handler and retain pause-on-exhaustion. The server-wide maximum-attempts ceiling
can cap the handler's requested value. Check effective service/handler settings after registration, not only the
SDK discovery manifest or deployment-registration reply. On server 1.7.8 these retry policies are deployment
settings, not a live admin policy patch. Existing invocations remain pinned to their deployment; registering new
settings does not retune them. Resume the owner on that pinned deployment; changing deployment requires ADR 0009's
exceptional-replay review. Execution counts/durations are exhaustion thresholds, not hard deadlines or vendor fences.

The provider's [five-attempt intervention rule](https://docs.szamlazz.hu/agent/basics/error-handling#retry-limit)
requires stopping after five unsuccessful sends of the same request and human repair before continuing.
Never automatically resume paused invocations or rotate keys to reset the budget. One run may make several
queries, and interrupted open reads can repeat: five executions are not a durable five-request wire cap.
The provider does not specify how distinct reconciliation selectors are grouped; clarify that scope before
promising strict accounting. The worker has no durable traffic admission counter. Stop repeated unsuccessful
traffic for intervention; no elapsed delay or human resume grants a new protected send or settles uncertainty.

Only `storno_invoice` uses 6m inactivity / 3m abort: its retained reconciliation may perform five sequential
60-second reads (candidate/original, then fallback discovery/candidate/original), plus bounded credential
initialization (three ten-second fetches and two 200 ms pauses). Other protected mutations and `recover` retain
4m / 3m; unmanaged Agent storno retains 4m / 3m. Inactivity requests suspension after lack of progress;
abort bounds the subsequent wait, not a concurrent timer or proof that the external request stopped.

Unkeyed Agent credit entries do not use Order's marker. Before renewal, independently settle earlier registration
and exclude delayed execution, then query again and submit only still-required entries or the current replacement.
A per-invoice caller lock alone cannot stop a request already processing at the vendor.
Credit registration also returns `outcome_unknown` when an otherwise successful acknowledgement explicitly
names a different invoice. The worker keeps a sanitized mismatch diagnostic and does not repeat the send;
neither that contradiction nor a later empty query establishes non-execution on the requested invoice.
