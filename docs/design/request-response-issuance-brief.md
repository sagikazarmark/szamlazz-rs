# #247 — RequestResponse and Workers implementation record

Completed implementation scope, 2026-09-14, PR #249. This replaces the initial
experimental brief and its superseded readiness checklist. Current rules live in
[ADR 0019](../adr/0019-request-response-ordinary-invoice-replay.md), the
[outcome/clearance table](request-response-outcomes.md) and the
[execution transition procedure](../operations/order-execution-transition.md).

## Delivered

- Actual Order and Agent services embedded in workers-rs, using RequestResponse
  hosting, signed runtime requests and scoped account resolution.
- Explicit, host-independent `WorkerConfig.order_execution`: `protected` by default,
  or `replay_enabled`, without library `test-util`. Agent needs no selector.
- Replay-enabled proforma, ordinary, prepayment and final creation, exact-target
  reissue, pinned conversion/chain references, proforma deletion, Order storno,
  shared observations and exact-marker operator recovery. Corrective issuance is
  refused before provider I/O in this mode; protected execution still supports it.
- Shared Számla Agent/reqwest transport on native and Workers, complete-response
  deadlines, dropped-exchange cancellation and execution/account isolation. The
  selected WASM redirect/header behavior is documented in ADR 0019.
- Existing Agent storno and credit-entry semantics on both hosts, including their
  operation-specific uncertainty and caller-owned settlement obligations.
- Known-marker recovery across modes, conservative unknown-state handling and an
  immutable-release transition procedure covering invocations and durable state.

## Verification and maintained seams

The [native actual-service suite](../../crates/restate-szamlazz/tests/request_response/README.md)
runs real Restate with buffered SDK input/output and independently controlled
provider visibility, counted sends and counted documents. It covers normal progress,
visible/invisible interrupted writes, changed guards, surviving old execution,
recorded uncertainty, pause/resume, cancellation, kill, exact recovery and completed
replay without credential acquisition or provider activity.

The [Workers example and acceptance suite](../../examples/workers/README.md) build
the actual services against the pinned interim SDK fix and run signed/scoped workerd
checks. Credit-entry scenarios are shared between native and workerd. CI includes
the release-bundle build and runtime checks; local native regression, doctest,
typecheck and Clippy checks accompany implementation validation.

These controlled runtime tests do not prove provider atomicity, multi-process
failover or uniqueness after a matching completion. The
[archived prototype report](../research/2026-09-14-request-response/README.md) and
[vendor findings](../research/2026-09-14-request-response/VENDOR.md) retain the
original observations, including inconclusive corrective cleanup. Their executable
runners and illustrative walkthrough were retired after maintained suites replaced
them. No further live vendor overlap is part of routine verification.

## Remaining boundary

Replay-enabled corrective issuance is not supported. Supporting it requires a new
decision about its demonstrated duplicate exposure or another exclusion mechanism.
There is no strict physical-send cap, automatic timeout-based non-execution verdict,
or exactly-once provider-effect guarantee. Recorded uncertainty stays read-only;
the operator-confirmed duplicate-order setting and go-live account mapping checks
remain deployment prerequisites. The transition procedure applies to every release,
including same-mode Workers releases, rather than treating readable marker state as
permission to replay old journals on new code.
