# Protected Order write protocol (#216)

Implementation specification, following [the approved recovery decision](unresolved-order-writes.md).
The test seams are confirmed: public Gateway, contract serialization, and real Restate ingress/admin.

## Commands and permission

Every exclusive mutation validates its body/key, reads `unresolved-write`, and refuses any present value
before the prologue or prerequisites. Missing state alone permits ordinary validation and lookup.
Unrecognized or malformed state is blocked, never treated as missing.

After the existing prerequisite/expected-document checks, the write path is:

1. `prepare-write` run creates the marker's unique token and creation timestamp. Its other fields are
   deterministic: owner invocation, scope, Order key, namespace, external id, operation and recovery intent,
   pinned account id, endpoint and credential reference. No document body or credentials enter state.
2. Set `unresolved-write` to the versioned marker.
3. Await `arm-write` run. Only its executing closure grants an execution-local atomic one-use permit.
   Its journaled result is unit, never permission. The permit starts false on every execution.
4. The existing named write run consumes permission once. Without permission it records unresolved data
   and performs no external write. With permission it executes at most one write, including its fresh
   guards. Settled original-send evidence is data; uncertainty is data, not a retryable write error.
5. If unresolved, await `reconcile-write`, a read-only run without an explicit run retry policy. Absence,
   collisions, failed credentials and other inconclusive answers remain retryable errors of this read.
   The Order mutation's invocation policy pauses on exhaustion and retains the lock.
6. A journaled conclusive result precedes clearing `unresolved-write` and returning the operation result.

Cancellation at or after arming returns structured write uncertainty with `cause: cancelled`, retaining
the marker. Manual kill cannot clear it. A completed arm replay never grants permission; an open write
replay therefore only reconciles. An uncertain arm acknowledgement may block a request that never sent.
Interruption before marker commitment cannot have sent: permission is not consumed until the arm await.

Pinned ordering: Rust SDK 0.12.0 `endpoint/context.rs::RunFutureImpl` executes a closure only on
`AwaitResponse::ExecuteRun`, proposes its completion, then awaits its notification. Shared core 7.0.3
`vm/transitions/journal.rs::ProposeRunCompletion` caches protocol-v7 results without making them ready;
`vm/async_results_state.rs::enqueue_run_completion_ack` makes the cached result available only on the
server acknowledgement. Completed runs replay results without calling their closures. Real-Restate
interruption tests must verify the complete marker/arm/send ordering on server 1.7.8 before release.

`tests/e2e/arm_ack.rs` intercepts the actual protocol-v7 arm acknowledgement in a loopback HTTP/2 relay,
checks that arming is recorded while no send has occurred, then drops the connection and verifies replay
only reconciles. `write_commands.rs` checks marker preparation/guard/set, acknowledged arm, write/settlement
and clear ordering through real journals, including every create kind's reconciliation path. Recovery
interruption tests cover recorded authorization, document evidence and recovery receipt before clearance;
revoking admission affects new invocations, not an already journaled authorization.

The production Gateway must disable reqwest retries and redirects explicitly, keep a fresh cookie jar
per execution, and retain the Számla Agent request timeout. One permission authorizes one possibly
effective write exchange; transport reconnection that has not sent a request is not another write.
Supplied HTTP clients must meet the same contract.

## Recovery contract

`Szamlazz.Order.observe_unresolved` is shared and operator-only. It returns absent, a versioned marker,
or unreadable state. It does not resolve the current account or acquire credentials.

`Szamlazz.Order.recover` is exclusive and operator-only. Both handlers require the host's configured
recovery authorizer; the default denies all access. `authorize-recovery` journals the admitted operator
identity (or denial) before state access, so revocation cannot change an existing invocation's command
prefix on replay. The host authorizes each new invocation. Authorization uses trusted request metadata at the
host/ingress boundary, never a body flag. The host must prevent direct caller impersonation, including
internal SDK calls; runtime request identity alone authenticates Restate, not the operator.

Recovery names the exact marker token, scope, account id, endpoint, namespace and operation. It accepts:

- **Document evidence:** query on the marker's pinned account; validate external-id ownership, kind,
  order and corrective base, and exclude the expected old reversed number for reissue. Storno evidence
  verifies the reversal's original reference and the original's reversal. Deletion absence is ambiguous
  and cannot alone clear a marker.
- **Non-execution attestation:** the authorized operator supplies an audit reference and explicitly
  attests that this exact request did not execute and cannot execute later. This is an operator assertion,
  never vendor proof. The recovery journal records the operator, marker and evidence before clearing.
- **Audited positive settlement (ADR 0013):** the authorized operator establishes that the exact write
  completed and cannot execute later, naming the issued document, reversal document or deleted proforma
  and an audit reference. This is operator evidence, not worker-verified vendor proof. The completion
  must match the operation; deletion repeats the pinned number, storno excludes its original number,
  and issuance excludes the old reissue target. Independent evidence must establish the remaining intent.

Storno document evidence may be verified directly by number: idempotent repeat storno does not attach the
request's external id. Automatic reconciliation may obtain a candidate from the order hint when that id is
absent, and still validates candidate identity, order, original reference and the original's reversal.
Every worker query requires a nonblank document number, and a by-number query must return that exact number
before any ownership or mutation decision. Storno evidence must name a reversal distinct from its original;
the queried original must be stornoable and reversed. Missing or contradictory identity retains uncertainty.
These are worker evidence requirements, not restrictions on the Agent crate's permissive wire model or the
length of vendor-reported document numbers. XML-invalid caller query selectors are `invalid_input` before the prologue.
Protected create retains a conclusive 71/152 refusal even when its optional diagnostic query fails.
Unresolved write results retain a safe diagnostic and a candidate storno number where available; retained
read failures name both the original cause and latest reconciliation reason without copying vendor free text.

An original-send rejection is handled automatically only by its owning invocation's recorded write
result, with no earlier unresolved send. Recovery does not accept an arbitrary vendor-code string as
proof of a rejection. Unknown evidence, stale tokens and mismatched identity refuse without clearing.
There is no forget operation, TTL, elapsed-time clearance, or recovery send.

Operator document verification uses the configured read policy for unanswered reads; answered credential
and vendor codes retain their structured faults and the marker. Recovery pins a three-execution
invocation policy (10 seconds to one minute, pause on exhaustion), four-minute inactivity and three-minute
abort timeouts, and 30-day journal retention. Recovery logs use the ordinary execution span with the
pinned account; they never re-resolve the scope.

A paused owner retains the exclusive lock. Prefer resuming it for read-only reconciliation. Exclusive
recovery requires quiescing producers, inspecting/cancelling queued mutations, deliberately stopping
the owner, then submitting evidence. Recovery clears uncertainty; a new business operation still needs
fresh ordinary checks and its original expected-document intent.

## State and deployment

State uses one stable key and an explicitly versioned, closed schema. Unknown versions, extra fields,
invalid identity and malformed encoding fail closed. No automatic state migration or backfill infers
absence of uncertainty. Immutable invocation routing does not isolate cross-invocation state: old code
that ignores the marker must not overlap the protected deployment on the same scope/key.

Resolver-owned account ids and credential references remain opaque strings, including empty values accepted
by `Account`. Marker decoding uses the same domain as marker production; these values are not document identity.

Before switching, quiesce every producer (including SDK callers and delayed sends), settle old unmarked
uncertainty and external in-flight work, and review any exceptional replay prefix. Draining Restate is
not evidence of vendor completion. Keep the pinned credential reference usable for recovery across
rotation; changed scope configuration cannot redirect a marker's queries.

Monitor marker age, paused owners, orphaned markers and queued mutations; #45 owns runnable operational
SQL and broader alerting. State deletion, incompatible old code, scope remapping, unkeyed Agent writes
and vendor UI writes remain outside the protection boundary.
