# Recovering an unresolved Order write

Use the deployed `Szamlazz.Order` operator endpoints through the authenticated ingress. The host must install
`RecoveryAuthorizer`; access defaults to 403. Keep the original scope, Order key and invocation identity.
Recovery is not permission to reissue. Empty queries, elapsed time, cancellation and kill do not settle a write.

## Observe

Call the shared `observe_unresolved` with a fresh invocation/key. It returns `absent`, `unresolved` with the
marker, or `unreadable`. Preserve the exact marker for recovery. For unreadable state use a compatible deployment;
do not replace or delete the state to make a mutation proceed.

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

## Submit independent evidence

An exclusive `recover` cannot run behind a paused exclusive owner. Quiesce all producers (including internal SDK
calls and delayed sends), inspect/cancel queued mutations, and deliberately stop the owner before recovery.
Kill releases its lock, not its external effects or marker. Submit a new recovery invocation with this body:

```json
{"marker": "REPLACE WITH THE EXACT OBSERVED MARKER OBJECT", "evidence": {
  "type":"document", "number":"SS-1"
}}
```

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

Attestations are operator assertions, not worker-verified vendor facts. Keep the referenced evidence durably in
the operator's incident system. Record vendor support's confirmation of the exact request where applicable;
an observation that a document is missing or a statement that enough time has passed does not suffice.
Without sufficient evidence, keep the order blocked.

Recovery records the operator, token and submitted evidence before clearing the marker. Preserve its receipt;
Restate's recovery journal retention is 30 days. A stale token, wrong target, mismatched operation or unknown
evidence is refused. Observe again with a fresh invocation. A subsequent business change still requires its
ordinary checks, the originally intended expected document and a deliberate new Idempotency-Key.

## Effective retry controls

| Control | Governs |
|---|---|
| `WorkerConfig.issue` | The run policy of unmanaged `Szamlazz.Agent.storno` |
| `WorkerConfig.read` | Ordinary read runs and operator document verification |
| `WorkerConfig.resolve` | Account resolution in the prologue |
| Order mutation invocation policy | Retained read-only reconciliation and infrastructure failures; default 5 executions, 2m → 10m doubling, then pause |

The protected write run consumes one permit; no setting or resume grants a second one. The Rust host can override
handler invocation policy through SDK `ServiceOptions` / `HandlerOptions` before binding the service definition.
Apply overrides to each intended handler and retain pause-on-exhaustion. Check effective settings through Restate
service discovery after registration. On server 1.7.8 these retry policies are deployment settings, not a live
admin policy patch. Execution counts/durations are exhaustion thresholds, not hard deadlines or vendor fences.

Unkeyed Agent credit entries do not use Order's marker. Before renewal, independently settle earlier registration
and exclude delayed execution, then query again and submit only still-required entries or the current replacement.
A per-invoice caller lock alone cannot stop a request already processing at the vendor.
