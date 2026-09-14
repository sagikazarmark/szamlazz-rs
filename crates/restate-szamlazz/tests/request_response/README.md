# Actual Order RequestResponse acceptance (#247)

Agent credit-entry scenarios are shared with the workerd suite in
`tests/common/credit_scenarios.rs`: exact money and normal additive/replacement
requests, recorded success/fault replay, lost answers, refused/credential/mismatched
identity answers, cancellation, and interrupted execution with independently counted
effects. Additive replay appends twice; replacement replay can overwrite a newer
external entry. These existing risks require caller-owned settlement.

Storno coverage includes shared Order/Agent normal execution, visible reversal
recovery, unfinished-run resubmission, changed original date/appearance/id/Order,
later refusal, recorded uncertainty, same-number and unnumbered acknowledgements,
retained notification warning and operator recovery against the pinned original.

The prepayment/final extension tests pinned proforma/prepayment references, explicit
final deduction lines, missing/reversed/replaced prerequisites after interruption,
positive target evidence despite changed prerequisites, recorded uncertainty,
old-number reissue echoes and exact-marker recovery.

The proforma lifecycle extension covers create/delete through the same Order,
including interrupted visible/invisible creates, invoice-family guard changes,
paid/force and named deletion, interrupted deletion with an absent or still-present
target, post-interruption paid/refusal answers and recorded uncertainty. Resume
stays read-only; only acknowledged deletion or audited recovery clears deletion
uncertainty. No document query is negative or positive deletion settlement.

Run the fake-provider suite against real Restate (tested with 1.7.8 / SDK 0.12.0):

```sh
RESTATE_SERVER_BIN=/absolute/path/to/restate-server \
  cargo test --locked -p restate-szamlazz --test request_response \
  e2e_request_response_actual_order -- --exact --ignored --nocapture
```

The target is also selected by the nextest `e2e` profile used in CI. It contains
no live provider test. Historical prototype and vendor evidence remain in
[`../prototype_request_response`](../prototype_request_response/README.md).

The actual `Order` selects `WorkerConfig.order_execution = OrderExecution::ReplayEnabled`;
Agent needs no selector. Existing configurations default to `Protected`. `test-util`
is used by this native test solely for interruption observers. The separate Workers
example and acceptance suite build without that library feature. This host fully buffers both SDK
input and output and alternates two independent endpoint instances; ordinary SDK
suspension cannot rely on streaming acknowledgements.

The ordinary extension also exercises exact-target reissue, `auto` and named
proforma conversion, and existing recovery for both legacy/ordinary markers. It
checks visible/invisible interrupted reissue, a disappeared expected holder,
consumed/replaced proformas and newly appearing proformas after pinned no-link.
Recorded uncertainty stays read-only. Conversion success establishes issuance,
not verified linkage; recovery uses exact marker echoes and pinned accounts.

## Observations from the actual-service implementation

| Scenario | Sends | Fake documents | Result |
|---|---:|---:|---|
| Failure-free ordinary invoice | 1 | 1 | `issued`, marker cleared |
| Interrupted first write, visible | 1 | 1 | Found by replacement, completes |
| Interrupted first write, invisible | 2 | 2 | Accepted duplicate exposure |
| Invisible first write, fake deduplication | 2 | 1 | Provider returns existing number |
| Recorded uncertain answer | 1 | 1 | Pause/resume read-only; completes when visible |
| Recorded uncertainty, no effect | 1 | 0 | Pause; kill/cancel retains marker; successor blocked |
| Later refusal / credentials / duplicate code | 2 | 1 | Retains earlier uncertainty; later evidence settles |
| Duplicate answer with matching positive evidence | 1 | 1 | Records `reconciled` even if subsequent queries would fail |
| Changed prepayment/final/proforma/collision/foreign/query guard | 1 | 1 | No second send; read-only settlement |
| Credential store unavailable on replacement | 1 | 1 | Recorded uncertainty; repair resumes read-only |
| Surviving old execution held after guards | 2 | 2 | Old send can follow replacement completion |

Tests independently count sends and documents, assert retained state through
`observe_unresolved`, check recorded uncertainty in actual journals, and replay
completed ingress identities while credentials are unavailable. Unsupported
handlers/options and invalid money cause no provider I/O; initial collision,
exclusivity, proforma and foreign conflicts remain before marker admission.
Namespace-owned and order-hint proformas are both blocked before admission;
legacy and unfamiliar marker values remain blocking and inspectable.

Interruption cancels/joins the SDK output task after the provider reply but before
the run result is recorded. The overlap case deliberately keeps that task alive
after returning HTTP 503, then releases and joins it after replacement completion.
These are controlled in-process endpoint interruptions, not workerd, multi-process
crashes or leader-election tests. A matching document does not prove uniqueness
or exclude delayed effects. Production depends on the operator-confirmed provider
duplicate-order setting, not the fake provider's toggle.

The [outcome/clearance table](../../../../docs/design/request-response-outcomes.md)
defines conservative refusal and changed-guard behavior, operation restrictions,
discriminator and transition boundary. See the [execution transition procedure](../../../../docs/operations/order-execution-transition.md).
