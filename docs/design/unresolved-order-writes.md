# Unresolved Order writes: investigation and implementation brief (#205)

Decision approved by the owner on 2026-09-10: **retain the invocation for reconciliation, plus a durable
unresolved-write marker**. [ADR 0004](../adr/0004-kill-not-pause-on-exhausted-retries.md#unresolved-order-writes-205-2026-09-10)
records the trade-off. This file specifies the bounded follow-up. #216 implements the protected protocol in
[the command specification](order-write-protocol.md); the baseline reproduction below is historical evidence.

## Verified reproduction

Baseline: worker `7098b5c6bcc6a0447348e5152b01177d1307ab8c`, Rust SDK **0.12.0**, shared core **7.0.3**,
Restate server **1.7.8**, vqueues/protocol v7/scoped Virtual Objects enabled.

```sh
RESTATE_SERVER_BIN=/path/to/restate-server \
  cargo test -p restate-szamlazz --all-features --test e2e e2e_unresolved -- --ignored --nocapture
```

`crates/restate-szamlazz/tests/e2e/unresolved.rs` runs two independent servers through the existing server gate.
It is also selected by CI's workspace-wide `--ignored e2e_` command. No vendor account is contacted.

**Historical baseline behavior (7098b5c, superseded by #216):** the real Order runs with a **validated** `issue.max_attempts = 1`; the default two-minute
initial delay remains configured. The mock accepts a first send into pending work, returns HTTP 500 after four
seconds (modeling a lost answer through an intermediary), and keeps the document invisible until the test
explicitly publishes it. During that window a second invocation with a different Idempotency-Key is submitted
and observed queued, with no runs executed. The first invocation's immediate re-query sees code 7, exhaustion
returns structured `outcome_unknown`, and the queued invocation sends while the first document is still invisible.

Observed on 2026-09-10, all three scenarios passed with **two sends before visibility**:

| First invocation | Queued invocation | Why this case matters |
|---|---|---|
| `create_invoice` | `create_invoice` | No delay between terminal exhaustion and the next invocation's send |
| `correct_invoice`, correction ID `c1` | Same correction ID `c1` | Correctives have no ordinary duplicate-order-number guard |
| `create_invoice` | `create_prepayment` | Exclusivity queries cannot see the still-processing invoice |

The first run deliberately asserted “one send” and failed with **actual 2, expected 1**. The retained regression
test at that baseline asserted the two-send behavior; #216 replaced it with protected-protocol regressions.
The mock intentionally supplies no vendor deduplication. It proves a worker interleaving, **not a live duplicate**.

**Protection probe:** a test-only `RetainedWrite` Virtual Object journals an uncertain send as data and uses an
ordinary retryable read run under invocation-policy `on_max_attempts = "pause"`. Observed: first invocation
paused, second queued, the original Idempotency-Key attached to the original id, shared observation answered
absence, resume while absent paused again, and resume after visibility completed both invocations with one send.
A durable pre-send marker survived manual kill; the queued mutation then failed before any run or HTTP send.
This is an executable feasibility probe, not proof of the production state machine, all crash windows, or vendor
identity verification. Its boolean mock query stands only for independently verified positive reconciliation.

## Evidence boundary

- [Behavior record](../szamlazz-hu-behaviour.md), A1-q0/q2/q10/q60: external-id visibility 771 ms **after a
  successful create reply**, then at +2/+10/+60 s. It says nothing about a create whose reply never arrived.
- The same record, A4d-2/A4d-q: one send stalled at least 57 s, and an order-number query found no issuance.
  Older ADR 0004 #61/#114 and client/glossary wording said a minute-long stall still issued/answered. That stronger
  history has conflicting repository provenance; the named probe neither establishes it nor disproves every
  possible delayed issuance. #195 owns recovering/qualifying the wider Számla Agent evidence.
- No verified vendor maximum processing duration or read-visibility bound, external fencing token, request-status
  endpoint or atomic “not executed and cannot execute later” answer was established. The 60 s client timeout,
  90 s delay floor and two-minute retry interval are operational choices, not such guarantees.
- Sequential identical-request replay and ordinary order-number refusals do not establish concurrent-send
  deduplication for every kind. Correctives and cross-kind chains must be protected by the worker's own rule.

## Pinned controls, checked against source

| Control | Available behavior |
|---|---|
| Rust [`RunRetryPolicy`](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/struct.RunRetryPolicy.html) | Count/duration exhaustion thresholds may overshoot; no pause option |
| SDK [`RunFutureImpl::retry_policy`](https://docs.rs/crate/restate-sdk/0.12.0/source/src/endpoint/context.rs) | Explicit policies map to shared-core `OnMaxAttempts::FailAsTerminal` |
| SDK [handler configuration](https://docs.rs/restate-sdk/0.12.0/restate_sdk/configuration/index.html) | `invocation_retry_policy(..., on_max_attempts = "pause")` is exposed; the probe exercises it |
| Server [`handle_task_error`](https://github.com/restatedev/restate/blob/v1.7.8/crates/invoker-impl/src/invocation_state_machine.rs) | SDK-requested run delays take precedence and do not advance the invocation-policy iterator; ordinary retryable errors can exhaust into pause |
| Server [`manual_resume`](https://github.com/restatedev/restate/blob/v1.7.8/crates/worker/src/partition/state_machine/lifecycle/manual_resume.rs) | `PATCH /invocations/{id}/resume` keeps the pinned deployment unless explicitly repinned; protocol compatibility is not application replay compatibility |
| Server [`ModifyServiceRequest`](https://github.com/restatedev/restate/blob/v1.7.8/crates/admin-rest-model/src/services.rs) | No live invocation-retry-policy patch fields; this is a deployment setting |

Do not specify another SDK's pause-on-run-exhaustion API. One available design is to complete the uncertain
send run as data, then use a separate **read-only** run with no explicit run policy: ordinary failures consume
the invocation policy and eventually pause. Alternatively, catch bounded reconciliation-run exhaustion and
return an ordinary retryable handler error, with replay returning to an open read rather than a completed
terminal read. The follow-up must choose and test one command sequence; merely changing the handler attribute
cannot prevent existing explicit run exhaustion from completing the invocation.

## Failure and recovery matrix

“Retained” means Restate keeps the exclusive Order lock. An unresolved marker is separate from that lock.

| Event | Historical baseline behavior (7098b5c) | Approved target / permitted recovery (implemented by #216) |
|---|---|---|
| Issue **run-policy exhaustion** after possible send | Handler completes `outcome_unknown`; releases lock | Persist uncertainty, continue read-only reconciliation in the original invocation, then pause. Never send because a retry count reset. |
| **Invocation-policy exhaustion** from endpoint/worker failures | Automatic kill, no handler compensation; releases lock | Pause and retain lock. Resume on the pinned deployment into read-only recovery if a send permit may have been consumed. |
| **Cancellation** at a Restate await | Cooperative structured write uncertainty; completion releases lock | Preserve marker before returning ADR 0011's `outcome_unknown` with `cause: cancelled`. Queued writes are refused; cancellation is never rollback or resend evidence. |
| **Endpoint interruption**, process crash or forced abort | Invocation remains unfinished during retry; open create run can send again after an empty query | Retain lock during retry/pause; replay of an armed send must default to read-only reconciliation. Completed runs need no credentials (#200). |
| **Manual kill** | Releases lock; cannot run compensation or stop vendor processing | Marker remains and later cooperative Order mutations refuse. Quiesce producers and inspect queued work before kill/recovery. Killing is not resolution. |
| Authoritative positive reconciliation | Found document is validated before use | Complete the original intent without another send; clear only its marker once the evidence is recorded. New invocations observe external facts afresh. |
| Authoritative negative settlement | No general vendor mechanism verified | Operator recovery only: record evidence that the earlier request did not execute **and cannot execute later**, then deliberately authorize a new operation if still intended. If evidence is unavailable, remain blocked. |

Neither elapsed time, repeated code 7, a credential refusal on a later query, a completed/killed Restate invocation,
nor changing the Idempotency-Key is negative-settlement evidence. A rejection of the original send can settle
that send only when no earlier send remains unresolved. Reversal of a positively identified issued document is
settlement of issuance, not permission to reissue; explicit `reissue` still applies.

**Caller identity:** while running, backing off or paused, retain the original Idempotency-Key or use native
attach/output. A completed uncertain/cancelled/killed invocation has a retained failure; the same key reads it.
A new key is used only for an explicitly authorized operation after reconciliation, not as a recovery probe.
Fresh `get` calls are non-atomic observations, not completion or negative-settlement barriers. Correctives need
their external-id/by-number reconciliation: `get` only reads the four ordinary kinds.

**Historical runtime wording mismatch at the #205 baseline:** create/storno exhaustion
and read/credential fault constructors emitted “retry with a new Idempotency-Key” (notably
`service/create.rs::create_outcome_unknown`, `service/storno.rs` and `service/support.rs`). That message is not
permission to bypass this evidence rule. #216 implemented the recovery boundary and revised the guidance;
the current contract requires settlement before deliberate renewal, with the original expected-document intent.

## Bounded implementation follow-up

The remaining sections preserve the approved #205 implementation brief. #216 supplied the command
sequence and recovery surface; [the implemented protocol](order-write-protocol.md) is authoritative
where this historical brief lists alternatives or work to do. ADR 0014 subsequently added the shared
public Gateway reconciliation interface on 2026-09-12 without transferring Order durability to it.

Deliver one recovery protocol for **Order mutations**: create/proforma/prepayment/final, corrective, storno and
proforma deletion. Keep `Szamlazz.Agent`'s unkeyed writes outside the protection claim; coordinate #201/#203 and
document that by-number calls and the vendor UI can bypass Order. Keep document status authoritative at szamlazz.hu.

### Invariant and minimal state

At most one unresolved write per `(scope, Order key)`. Its marker records operation identity (including correction
ID or original/pinned number where relevant), external id, owner invocation, pinned account identity/endpoint
and namespace, and the minimal evidence needed for recovery. No credentials, raw XML, PDF or buyer/line-item
ledger. The marker is written **before** any possibly effective send; a crash even before the send may therefore
leave a conservatively blocked order. No TTL, automatic clearing, generation change, scope remapping, or ordinary
`reissue` flag may discard it. Every exclusive mutation checks it before prerequisites, across kinds. Recovery
uses the pinned account; it never silently resolves the marker under changed scope configuration.

Checking the marker at handler entry is insufficient: a replay reuses that entry's old “absent” result. A send
run with `max_attempts(1)` is also insufficient: its open closure can execute again after a crash. Use a
**durable arming boundary and an execution-local, one-use send permit** (or another source-verified equivalent).
The probe demonstrates the candidate shape: set marker → await named arm run, whose *executing closure* grants
a volatile permit → send run consumes permit → journal result → reconcile. Replay of the completed arm grants
no permit; an interrupted send is skipped and reconciled. Require protocol-v7 ordering/ack verification and
interruption tests before adopting it. An uncertain arm completion may deny a send that never occurred: accept
that availability cost rather than infer permission from absence. Never serialize the permit as `true` for replay.

### Recovery surface and progress

Add an operator-only **shared observation** of the marker (separate from `get`'s four document observations), and
an exclusive, evidence-carrying recovery action after the owner completes or is deliberately stopped. A queued
exclusive recovery action cannot run behind a paused owner: normally resume that owner for read-only recovery;
otherwise quiesce, inspect/cancel queued mutations, kill deliberately, then recover the retained marker. The
operator boundary must be enforced by the host/ingress; a body field is not authorization.

The implementation PR must fix the public request/response shape and evidence rules: positive document identity,
terminal original-send rejection with no older uncertainty, or an audited operator attestation tied to the exact
marker that the request cannot still execute. An attestation is operator responsibility, not a vendor fact the
worker independently proved. A stale marker token, another scope/account, or a different correction must refuse.
Do not offer a generic `forget` or “wait N minutes then clear” endpoint. Resume never itself authorizes a send.

Positive create evidence validates order/kind and the intended reissue/corrective identity; storno evidence
validates original reference and reversal (#196). Deletion reconciles the pinned proforma number, with the
operation-specific ambiguity of #201, not the latest holder alone. A missing document that might be consumed,
deleted or hidden is not by itself negative settlement of issuance. Collisions remain blocked for investigation.

### Acceptance scenarios at the agreed seams

1. Port the three current-behavior scripts above to the protected Order. Keep the first send invisible across
   run exhaustion, repeated fresh observations and resume; **one send**, original retained, later mutation queued.
   Publish matching positive evidence; original completes and the queued call performs its normal live checks.
2. Inject interruptions before marker commit, after marker/arm completion but before send, after send receipt
   but before result recording, and during reconciliation. Verify a durable marker precedes every actual send,
   no replay grants a second permit, and false-positive uncertainty is inspectable and recoverable.
3. Force invocation-policy exhaustion and endpoint unavailability: pause retains lock; shared observation works;
   same Idempotency-Key attaches; resume with absence cannot send, with positive evidence completes.
4. Cancel mid-send and during reconciliation: ADR 0011 structured cancellation remains; marker survives;
   already queued same-kind, corrective and cross-kind mutations cannot send. Manual kill before result recording
   must have the same marker protection, without claiming compensation ran.
5. Validate positive, negative-attested, stale-token, wrong-account, collision, reversed-target and unknown-evidence
   recovery; reject unauthorized recovery and no-evidence/elapsed-time-only clearance. Inspect/send counts through
   HTTP, state through the recovery observation, and exact commands through the existing journal table.
6. One-shot deletion preserves its pinned number and never auto-repeats; unmanaged Agent writes remain explicitly
   outside the Order guard. Credentials unavailable during reconciliation preserve uncertainty; completed replay
   does not fetch credentials. No secrets or unnecessary document content in marker or journal scans.

### Deployment, journal and operational implications

This is a breaking command-sequence/state change, not an attribute-only retry adjustment. Extend the step-name
table for the marker read/write/clear commands and candidate `arm-write-{operation}`, `reconcile-{operation}`
boundaries, retaining bounded parameters and no fixed/parametrized-prefix overlap. Review whether the current
`create-{kind}`/`storno-{number}` combined query/send run must split; specify exact commands before coding. Add
marker state to privacy scans; replace the Order “no state” invariant with “only the unresolved marker”.

ADR 0009 immutable deployments still apply, but **state crosses invocations and deployments**: unlike invocation
results, marker schema needs an explicit migration/fail-closed decoding rule. Drain old writes with producers
quiesced, reconcile every old uncertain completion and in-flight external send, then switch. Draining Restate
alone does not prove vendor completion. Old unmarked invocations and old code ignoring markers cannot overlap
the protected deployment on the same scope/key. Retain old code for its pinned invocations; exceptional replay
requires actual-prefix review, never just a matching step-name pattern. No automatic backfill can infer absent
old uncertainty.

Require alerts and operator ownership for unresolved age, paused owner, marker without an active owner, and
queued mutations. #45 owns runnable SQL and the broader runbook, including vqueue scheduler observations. A
paused order may block refunds/stornos/corrections indefinitely. Shared reads still work. Kill can release the
lock but cannot erase the marker or restore permission to mutate. State deletion, incompatible old code, scope
remapping, external writers and administrative override remain outside the guarantee and need deliberate
reconciliation. #206's stale intent after retention is separate; the marker does not authorize old intent.
