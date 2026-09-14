# RequestResponse ordinary issuance with explicit replay risk

Accepted direction 2026-09-14, following #247's runtime and vendor prototypes;
**implemented only as an isolated `test-util` experiment, not a release-wide mutation contract**. RequestResponse is
required. For ordinary invoice issuance, replace execution-local acknowledged
arming with fresh-query-before-open-run-replay, relying on enabled provider
duplicate-order checking and accepting residual duplicate risk. Retain recorded
uncertainty and read-only recovery; do not introduce a separate permission Virtual
Object or choose financial semantics implicitly from the compilation target.

## Why change the decision

The current Order writes its unresolved marker, executes an arm closure granting
a volatile permit, awaits acknowledgement, then consumes that permit in the write
run. The [real-Restate prototype](../../crates/restate-szamlazz/tests/prototype_request_response/README.md)
showed the actual Order reaching pause with **zero sends**, without injected failure,
under buffered RequestResponse on server 1.7.8 / Rust SDK 0.12.0 / shared core 7.0.3.
Receiving the arm completion requires another execution; completed-arm replay does
not grant permission. This is a progress failure, not duplicate issuance.

The experimental narrow shell completed normal RequestResponse work across two
endpoint instances. It recovered a visible first document without resending, but
sent twice when the first document remained invisible. Recorded uncertainty stayed
read-only, including after resume. A surviving old execution could send after its
replacement completed. These are controlled native-host/fake-provider observations,
not workerd or multi-process/leader-election evidence.

The [vendor overlap probe](../../crates/restate-szamlazz/tests/prototype_request_response/VENDOR.md)
then supplied operation-specific evidence:

- Two overlapping identical ordinary requests through independent fresh sessions
  both returned `CTEST-2026-28`, id `930038478`.
- Two overlapping identical corrective requests produced `CTEST-2026-31` and
  `CTEST-2026-32`, ids `930038529` and `930038532`, both referencing `CTEST-2026-30`.
  Their shared external id returned only the latter.
- Ordinary cleanup was verified. Corrective cleanup returned code 13, classified
  as inconclusive; the three linked test documents remain for operator review.
  The overall vendor lifecycle did not pass cleanup.

The provider [documents duplicate-order protection after timeout resubmission](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number).
Matching recent ordinary requests can return the previous invoice; otherwise an
already-used order number is refused. The setting is per account and checking is
per document type, with corrective/storno exceptions and reuse after reversal.
One observed pair supports that documented behavior, not universal concurrent
atomicity, a processing deadline, or a measured accidental-duplicate rate.

## Approved ordinary-issuance direction

1. The first implementation slice is **initial ordinary invoice issuance and
   reads**, on an isolated experimental actual-service deployment. Reissue and
   proforma conversion are excluded from that first slice. They retain their
   expected-document and cross-kind questions for release-scope adjudication.
2. Duplicate-order checking enabled on the intended account is an explicit
   deployment prerequisite. `check_account` proves neither that setting nor the
   seller mapping; do not invent a runtime verification claim. Retain the go-live
   account verification and document how the operator confirms this setting.
3. Preserve stable scope/account, Order identity, external id and document intent.
   Retain validation, ownership, collision and exclusivity rules. Completed reads
   replay their recorded observations; any read used to authorize an open-run
   resend must execute afresh inside that run, with prerequisite freshness reviewed.
4. Commit unresolved intent before possible provider activity using a
   RequestResponse-compatible durability barrier. No replayed result contains a
   reusable send permit. An unfinished write run may query afresh and, on a
   qualifying absence, send again under the accepted provider-deduplication premise.
5. A **recorded** uncertain write result permits only read-only reconciliation,
   followed by pause if inconclusive. Cancellation and kill do not clear markers
   or settle effects. A new ingress Idempotency-Key cannot bypass retained intent.
   There is no timed automatic resend of recorded uncertainty.
6. Matching positive issuance evidence may complete the invocation under this
   risk contract. It establishes a matching document, **not uniqueness or exclusion
   of another delayed effect**. A later refusal, changed target, failed initialization
   or empty query cannot independently settle an earlier interrupted send. The
   production outcome/marker-clearance matrix must embody this distinction before
   enabling the new path; deleting the Boolean alone is not an implementation.
7. Keep provider-call deadlines, explicit no-retry/no-redirect HTTP policy,
   execution/account-local sessions, response interpretation, exact money/query
   facts and credential privacy. Invocation/run thresholds are not physical send
   caps. ADR 0018's intervention requirement and prohibition on automatic resume
   loops still apply.

The new direction intentionally gives up strict at-most-once sending for this
slice. It does not give up retained uncertainty once observed. An unanswered send
that actually did nothing can still leave an Order blocked, so the operational
recovery cost is reduced in some interruption windows, not eliminated.

## Operation and release boundaries still to decide

Corrective issuance is **not authorized under the new replay-risk contract** by
this ADR. Its lack of deduplication is observed, and a crash before recording
uncertainty cannot be fixed merely by pausing recorded failures. Supporting it
requires explicit risk acceptance or a separately approved exclusion mechanism.

Before production rollout, decide the behavior of every existing mutation:
proforma, prepayment, final, corrective, Order storno/deletion, Agent storno and
credit-entry writes, plus ordinary reissue and proforma conversion. For each,
record its supported host/mode, evidence rule and migration behavior. Do not
silently remove existing native handlers, route them through relaxed replay, or
present their mere discovery registration as tested RequestResponse support.
An experimental endpoint must prevent unsupported mutation paths before any
provider operation; the interface for a released capability restriction remains
an explicit decision, not an invented public fault token in this ADR.

The initial direction is one deliberate execution contract, not native-strict and
WASM-relaxed defaults. Maintaining a second permanent contract needs a concrete
consumer and a separate decision. Existing native production behavior remains
the implemented contract until the replacement's release scope is approved.

## Migration obligations

This prospectively revises the arm-dependent parts of ADR 0004, ADR 0018 and the
[implemented protected protocol](../design/order-write-protocol.md) for the approved
slice only. Those documents still describe deployed code. ADR 0012's exact target
intent, ADR 0014's direct Gateway permission obligations, host-owned recovery
authorization, and ADR 0009's exceptional-replay rules are not weakened implicitly.

A release must use a new immutable deployment and review the actual old journal
prefix before any exceptional replay; never replace `arm-write` semantics in place
or resume an old armed invocation into resend permission. Object state survives
deployment changes: old markers remain inspectable and block admission, with
unknown versions refused conservatively. A marker-schema/policy discriminator,
legacy recovery handling, mutation capability matrix and producer transition
procedure must be decided before a production switch. Draining invocation journals
alone does not establish that marker state is empty or provider effects settled.

## Alternatives considered

- **Keep current streaming arming:** defensible conservative send protection but
  cannot satisfy required RequestResponse progress; worker handoff can also block
  never-sent work.
- **Separate durable send gate:** potentially portable, but adds claim-response
  uncertainty, delayed-claim closure, durable tombstones, ingress/capacity wiring
  and blocked-work recovery. Not selected for this work.
- **Automatically resend after a waiting period:** broadens risk beyond open-run
  interruption and treats elapsed time as if it settled provider processing. Not
  selected.
- **Assume one retry policy works for all document types:** contradicted by the
  corrective experiment. Not selected.

Implementation and release gates are specified in the [#247 brief](../design/request-response-issuance-brief.md).

## Actual-service slice (2026-09-14)

The [outcome/marker-clearance table](../design/request-response-outcomes.md) now
defines the initial ordinary experiment. Explicit opt-in selects actual Order/Agent;
unsupported mutations are refused. All non-positive open-write results retain
uncertainty, including later refusals, credentials and failed fresh guards. A distinct
serialized prepare result and marker discriminator prevent a legacy prepare result
from being decoded as ordinary resend intent; new step names alone are not that fence.

The [actual-service suite](../../crates/restate-szamlazz/tests/request_response/README.md)
reproduces the prototype's visible/invisible writes, recorded uncertainty,
replacement, surviving execution, pause/resume and kill cases, plus changed guards
and credential initialization. These native buffered-host results do not approve
the production settlement/migration gates or establish Workers runtime support.
