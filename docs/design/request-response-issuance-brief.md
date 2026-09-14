# #247 — RequestResponse ordinary issuance and Workers embedding

## Problem Statement

A downstream Rust application needs the actual Order and Agent services embedded
in Cloudflare Workers, with Restate Cloud providing durability in RequestResponse
mode. The current protected write protocol cannot progress normally in that mode,
and the library additionally has native transport/runtime assumptions. Ordinary
vendor duplicate checking offers a practical alternative, but corrective overlap
has now produced two distinct documents and cannot inherit that assumption.

## Solution

Implement a bounded, experimental actual-service vertical slice for initial
ordinary invoice issuance and reads under RequestResponse, following ADR 0019.
Use fresh discovery before unfinished-write replay while retaining recorded
uncertainty, pause and recovery. Then supply the host-compatible transport/runtime
implementation and workers-rs embedding example, verified with real Restate and
workerd. Promotion to a supported production release requires explicit operation,
settlement and migration decisions; this issue does not authorize a blanket
replacement of the shared protected-write function for every mutation.

## User Stories

1. As an embedder, I want actual Order and Agent execution on Workers, so I can
   retain the library's document handling in my required host.
2. As an embedder, I want RequestResponse discovery and execution, so normal
   suspension can progress without bidirectional streaming.
3. As a caller, I want initial ordinary issuance to survive worker replacement,
   so a visible earlier invoice is recovered rather than issued again.
4. As an operator, I want the required duplicate-order account setting stated,
   so I know which provider behavior the replay contract relies on.
5. As a caller, I want stable account, order and document intent across replay,
   so retries do not silently become a different business request.
6. As a caller, I want recorded write uncertainty retained, so application retries
   cannot bypass an unresolved earlier operation.
7. As an operator, I want pause/resume to reconcile recorded uncertainty read-only,
   so repairing a dependency does not automatically repeat a mutation.
8. As an operator, I want cancellation and kill to preserve unresolved markers,
   so releasing an invocation lock is not mistaken for provider rollback.
9. As a caller, I want later refusals kept distinct from settlement of earlier
   interrupted work, so an apparently rejected retry cannot hide prior effects.
10. As a caller, I want matching issuance distinguished from uniqueness, so I can
    understand the residual duplicate and delayed-execution risk I accept.
11. As an embedder, I want bounded calls and isolated sessions on Workers, so
    portability preserves the transport and credential-lifetime requirements.
12. As an operator, I want signed Restate requests and trusted scoped routing,
    so Workers hosting preserves account selection and request identity checks.
13. As an existing native consumer, I want explicit mutation support and migration
    behavior, so a host port does not silently remove or weaken my handlers.
14. As a recovery operator, I want old markers and invocations handled deliberately,
    so a deployment change cannot convert retained uncertainty into send permission.
15. As a maintainer, I want runtime failure cases and provider observations recorded
    separately, so neither mock passes nor one live pair become universal guarantees.

## Implementation Decisions

- **Approved scope:** initial ordinary invoice issuance and reads. The first slice
  excludes ordinary reissue and proforma conversion. It runs through the actual
  Order/Agent implementation in an isolated experimental endpoint, not another
  standalone imitation of Order. Unsupported mutations are inaccessible before
  provider I/O. Keep prototype-only routing/capability controls out of the stable
  public contract until the release matrix is decided.
- **Single selected direction:** no separate permission Virtual Object or external
  claim database. Do not automatically select strict/relaxed financial semantics
  from native/WASM compilation. Keep current production paths until the slice is
  explicitly enabled and verified; no blanket relaxation of shared mutation code.
- **Admission and replay:** retain existing body/key validation, account resolution,
  ownership/collision checks, ordinary-kind exclusivity and monetary preflight.
  Durably retain unresolved intent before possible send. A barrier may suspend and
  replay without depending on an execution-local permit. Re-executing an unfinished
  write must freshly verify the holder and the relevant current guards before any
  resend. A failed read or collision is never absence.
- **Outcome/clearance table before send-path implementation:** enumerate successful
  numbered and ambiguous acknowledgements, found/reversed holders, duplicates,
  vendor refusals and credentials, changed targets, failed initialization, query
  failure, cancellation and interrupted execution. For each, name its evidence,
  returned outcome/fault, marker state and next action. A later refusal alone must
  not prove that an earlier execution sent nothing. Review how evidence from an
  unrecorded earlier execution remains unknowable; do not rely on a volatile
  execution count as authoritative proof that a send is the first.
- **Recorded uncertainty:** journal as data, continue read-only reconciliation,
  pause on exhaustion and retain the original invocation/intent. Kill and a fresh
  Idempotency-Key do not clear the marker. Positive completion can clear the new
  slice's marker under accepted replay risk, but must not promise uniqueness or
  that an old execution cannot subsequently send. No automatic timed rearming.
- **Provider prerequisite:** explicitly document enabled duplicate-order checking
  and independent verification of the intended account. The existing account
  probe does not attest the toggle. Receipt call IDs and external ids do not
  substitute for ordinary duplicate checking. Preserve bounded retry/intervention
  policy without claiming a crash-proof send budget.
- **Transport/runtime:** prefer the existing Számla Agent wire seam for native and
  Workers exchange implementations; keep provider parsing and interpretation shared.
  Expose the smallest composition interface actually needed by the supplied services.
  Preserve no hidden HTTP resend, no credential-forwarding redirects, complete-body
  deadlines, repeated header handling, exact cookie matching and execution/account
  isolation. If no session persistence is selected, document and verify reauthentication
  explicitly. Audit credential retry sleep, resolver/store deadlines, marker wall
  clock, SDK clocks/entropy and task lifetimes. Confine JS future adaptation to the
  host adapter; do not weaken native thread-safety requirements incidentally.
- **Feature graph:** exclude native server/network features from the Workers bundle;
  this does not require eliminating every runtime-independent Tokio utility. Preserve
  a supported request-identity crypto backend and state exact interim SDK revision
  instructions until a suitable release is available.
- **Privacy and hosting:** account resolution remains journaled without credentials;
  completed operations need no credential fetch. Preserve query facts, decimal
  precision and safe diagnostics. Host-owned recovery authorization, operator
  attribution, ingress scope trust and SDK signature verification remain required.

### Release decisions still required

These are explicit approval gates, not permission for an implementing agent to
invent a broad public policy:

| Decision | Required disposition before production support |
|---|---|
| Mutation matrix | Define support for proforma/prepayment/final/corrective, Order storno/deletion, Agent storno/credit entries, reissue and conversion; include native migration |
| Correctives | Remain outside the approved relaxed contract unless explicit duplicate-risk acceptance or another mechanism is approved |
| Unsupported handlers | Decide omission versus explicit refusal and the released discovery/caller contract; fail before provider I/O |
| Settlement contract | Approve the complete outcome/marker-clearance table, including multiple potentially effective sends and failed pre-send checks on replay |
| Legacy state | Decide marker version/policy discrimination and old-marker recovery; unfamiliar state stays blocking |
| Deployment switch | Preserve old immutable endpoints for retained invocations, review exceptional replay, and define producer transition/drain/state inspection procedure |

First-slice development is ready; production rollout and the full mutation port
are blocked on this table. An implementation PR must state which gate it resolves
and which remain open. The overall issue closes only with an approved support matrix,
migration plan and the runtime checks below.

## Testing Decisions

The primary test seam is real Restate invoking actual registered Order/Agent
services through ingress, observing their outputs, journals, retained state and
an independently controlled provider. Use real workerd for final host acceptance;
use the existing transport/wire seam for focused HTTP-policy checks. Do not add a
generic mock of every Gateway method or treat source/binary searches as runtime proof.

- Failure-free RequestResponse discovery and initial issuance must complete. Verify
  actual SDK output consumption, suspension/resume, marker durability and signatures.
- Alternate endpoint instances; interrupt after provider receipt but before durable
  completion; keep an old execution alive while its replacement runs. Independently
  count physical sends and provider documents, rather than inferring sends from runs.
- Visible first issuance: fresh query settles without another send. Invisible first
  issuance: demonstrate the accepted extra-send exposure, with and without provider
  deduplication. Finding one document must not be asserted to prove uniqueness.
- Recorded uncertainty stays read-only over resume; reveal the document and complete.
  Produce no document at all and verify pause, retained marker, kill and blocked
  successor. Later refusal cannot falsely clear earlier uncertainty.
- Preserve ordinary ownership, exclusivity and invalid-input no-send cases, including
  changes between recorded prerequisites and a newly executing send run.
- Replay completed observations/results without refetching credentials or sending.
  Observe query response facts and exact decimals; scan journals for sensitive data.
- Exercise redirects, stalled body transfer, deadlines/cancellation, cookie capture
  or explicit no-persistence behavior, and cross-execution/account isolation in the
  actual Workers runtime. Test required signing and scope/account resolution.
- Run relevant native regression checks and add CI release-bundle build plus meaningful
  workerd execution. Any altered step sequence is reviewed for exceptional replay;
  allowed step patterns alone do not establish old-journal compatibility.
- Vendor evidence is a targeted manual acceptance activity with exact selection,
  no whole-test retry, retained evidence and known-document cleanup. Do not rerun
  corrective overlap as routine CI or treat the prior cleanup as successful.

### Acceptance checklist

- [x] Actual-service RequestResponse ordinary issuance slice passes the native buffered-host failure matrix (see `tests/request_response`); workerd acceptance remains below.
- [ ] Production outcome/clearance table and mutation capability matrix approved.
- [ ] Legacy marker/recovery and immutable-deployment migration procedure approved.
- [ ] Actual Order/Agent workers-rs release bundle builds with a documented feature graph.
- [ ] Real Restate + workerd verify signed/scoped execution, replay, pause and recovery.
- [ ] Native and Workers transport behavior and deadline/cancellation checks pass.
- [ ] Required native regressions pass; CI includes bundle and runtime checks.
- [ ] Embedding, provider-setting prerequisite, residual risk and unsupported operations documented.
- [ ] Prototype evidence is durably published with the change before it is used as an external reference.

## Out of Scope

- A strict physical-send cap, automatic timeout-based non-execution verdict, or
  an exactly-once provider-effect guarantee.
- A separate permission Virtual Object, D1 claim store or generic workflow framework.
- Automatic retries of recorded uncertainty, automatic resume loops, or a new key
  as outage recovery.
- Unapproved corrective risk acceptance, blanket relaxation of other mutations,
  or silent deletion of existing native capability.
- Changes to downstream D1 business storage, Dioxus, Cron or the Restate server.
- A production duplicate-frequency estimate from the small vendor probe.

## Further Notes

Decision: [ADR 0019](../adr/0019-request-response-ordinary-invoice-replay.md).
Runtime evidence: [prototype README and observations](../../crates/restate-szamlazz/tests/prototype_request_response/README.md).
Vendor evidence and remaining cleanup: [vendor findings](../../crates/restate-szamlazz/tests/prototype_request_response/VENDOR.md).
Predecessor implementation: [protected Order protocol](order-write-protocol.md).
SDK prerequisite: [upstream PR #128](https://github.com/restatedev/sdk-rust/pull/128),
inspected at `e882e9e04f9ad9942b4fe7f3788999a521dfa336`.

The prototype/evidence and ADR/brief were local uncommitted artifacts when initially
published to the tracker; the actual-service implementation publishes them together.
No new vendor mutation was needed for this slice.
