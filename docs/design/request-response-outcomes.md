# Replay-enabled Order execution: outcomes and marker clearance

**Current selection:** `WorkerConfig.order_execution` chooses `protected` (default)
or `replay_enabled` on either host, without `test-util`. Agent has no selector;
only corrective issuance is unavailable under replay-enabled Order execution.
The per-operation outcome/clearance rules remain unchanged. See the
[execution transition procedure](../operations/order-execution-transition.md).

Current operation and settlement rules, accepted 2026-09-14 under
[ADR 0019](../adr/0019-request-response-ordinary-invoice-replay.md).
The account must have duplicate-order checking enabled, independently confirmed
by its operator. `check_account` verifies neither that setting nor seller mapping.

## Reissue, pinned proforma selection and recovery

Ordinary `create_invoice` now also supports reissue and proforma conversion through
the same native/Workers handlers. Before admission, reissue must name the exact
reversed holder; changed/absent targets remain `target_changed`. Inside an open
write, a different matching holder is positive replacement evidence under the
accepted newest-holder non-regression premise. Resubmission requires the exact old
holder still reversed, never absence. An old-number acknowledgement is uncertainty.

Proforma selection is pinned by completed prerequisite reads and retained in the
marker: `auto` selects its then-live owned proforma or no link, `none` selects no
link, named selection requires a live proforma of this Order. Every executing open
write first seeks positive invoice evidence, then rechecks the selected proforma
by number and refuses a different live namespace holder/order hint. Disappearance,
reversal or replacement before a send cannot substitute another number. After
admission such no-send outcomes retain uncertainty. A newly appearing proforma
cannot change a pinned no-link decision. These are sequential observations, not
provider-atomic compare-and-set or protection from changes after the final read.

Success continues to mean invoice issuance, not verified proforma linkage. The
request submits the pinned reference; queries expose the provider's reported link.
An invoice found after conversion settles issuance even if the proforma is gone.

The existing `observe_unresolved`/`recover` interface accepts protected and all
known replay-enabled markers, preserving exact marker equality, pinned account selection,
record-before-clear ordering and existing evidence requirements. Unknown versions
or execution contracts block. Recovery never grants send permission. The host
authorizes operator access on either host. Replay-risk positive evidence
still establishes neither uniqueness nor exclusion of delayed effects.

## Admission and support

### Agent credit entries

`Szamlazz.Agent.set_credit_entries` uses its existing shared implementation on
native and Workers. Its run journals one exchange outcome without deliberate send
retry or preceding query; an interrupted unrecorded run may still execute again.
Additive replay can append twice, replacement replay can overwrite a newer external
entry. These are accepted existing Agent semantics, not Order send protection.
Completed successes/faults replay; lost answers, vendor refusals after interruption,
contradictory reported identity and cancellation retain the existing uncertainty
faults. Credential failures do not establish non-execution of an earlier run.
There is no Order marker or worker-provided credit-entry recovery. The caller must
settle the exact request and exclude delayed execution before deliberate renewal,
then query and submit only remaining entries/current intended replacement.

All Agent handlers operate identically without an execution selector. Order's
configuration selects its replay-risk contract and blocks corrective issuance.
The same credit-entry scenarios run on native buffered RequestResponse
and signed/scoped workerd, including counted additive duplicates, replacement of a
newer external entry, exact money, lost answers, cancellation, credential/refusal
answers and completed replay.

### Storno

Order storno uses the same portable retained-write boundary. Unfinished replay
first seeks an existing reversal, requiring both a matching storno and the freshly
queried reversed original. Only the same live original (number, provider id,
Order, stornoable type, fulfillment date and appearance) permits another send.
The original's verified fulfillment date and derived e-invoice flag stay pinned,
as do the caller's comment/notification recipient in the invocation input. A changed
guard, missing original, later refusal, credentials failure or inconclusive answer
after admission retains uncertainty. An original-number echo or unnumbered success
does not establish reversal. Recorded uncertainty stays read-only through resume;
kill/cancellation retain state. Known reversal evidence or a numbered reversal
acknowledgement settles after recording, without proving notification delivery or
excluding delayed old execution. Candidate-associated notification warnings retain
their existing evidence rules. Operator recovery preserves exact marker echoes and
the pinned original identity. This relies on observed repeat-storno behavior, not
an exactly-once guarantee.

Agent storno is enabled with its existing unmanaged query-first issue policy and
no-Order guard, unchanged. It has no Order marker; its documented uncertainty and
no-op acknowledgement policy remain distinct. Native and Workers run these same
services. Credit entries use the existing Agent contract above; corrective issuance
remains outside the approved Order mutation subset.

### Prepayment and final issuance

Replay-enabled handlers permit prepayment and final issuance, including
exact-target reissue. The account's per-type duplicate-order checking is required;
ordinary-invoice overlap observations are not evidence of atomic deduplication for
these types. Unfinished executions may resubmit only after fresh target and chain
checks. Recorded uncertainty remains read-only; elapsed time, kill and cancellation
never authorize another send.

Prepayment keeps its selected proforma (or no link) and refreshes ordinary/final
exclusivity plus proforma guards. Final keeps the exact selected prepayment number;
after target discovery, every executing send requires that same live owned
prepayment under the chain's external id and by number, and refuses a live ordinary
invoice or unexpected proforma. The order hint may name the pinned prepayment,
never another live invoice-family document. Missing, reversed, colliding or replaced
prerequisites after admission retain uncertainty without sending. A changed
prerequisite before admission keeps existing prerequisite conflicts.

Positive matching target evidence takes precedence: a visible prepayment/final
invoice settles issuance even if its proforma was consumed or its prepayment later
reversed. Success establishes issuance, not independently verified linkage or
monetary allocation. Caller-supplied negative prepayment deduction lines remain
necessary on a final invoice; the provider does not net the reference into totals.
Expected reissue numbers and pinned references never change on replay. Markers
retain the prepayment reference separately from the proforma reference and use
distinct execution-contract tokens. Known markers remain recoverable through the
same exact-echo, pinned-account interface.

### Proforma creation and deletion

Unfinished-run replay is accepted for proforma create/delete, on both hosts
under the same explicit execution setting. Creation refreshes its own holder,
then ordinary/prepayment/final exclusivity and the order hint. Provider per-type
duplicate-order checking is a prerequisite, not a proven atomic guarantee. A
visible matching proforma settles creation; an invisible prior create may be
resubmitted after fresh checks. A consumed proforma cannot be recovered as creation
evidence from invoice absence or linkage alone. Recorded uncertainty stays read-only.

Deletion retains exact expected number, provider id, mode and force in a distinct
execution-contract marker. Each executing open run verifies the same proforma by
number, Order association, type, provider id and current credit entries; force
bypasses only the credit-entry guard. Namespace-owned mode also refreshes the
namespace slot, refusing a changed holder. Named mode never selects or deletes a
coexisting namespace holder. A positive deletion acknowledgement clears after
recording. Post-admission absence (query miss or code335), refusals, failed guards,
credentials/init failures and lost answers retain uncertainty, even on an apparent
first execution. They cannot settle a potentially effective earlier send. Repeated
open-run deletion is allowed only while the exact target still passes fresh checks;
positive completion does not fence delayed old execution or external changes.

Before admission the existing paid/ownership/expected-target conflicts and absence
remain settled no-send outcomes. After recorded uncertainty, deletion cannot be
settled by document queries: pause/resume is read-only and operator recovery needs
audited completion or non-execution evidence for the exact marker. Kill/cancellation
retain the marker. Known new contracts are recoverable; old or unknown prepare
results never grant permission for the operation.

### Shared admission rules

Replay-enabled Order execution permits all four create handlers (proforma, ordinary,
prepayment, final), exact-target `delete_proforma`, Order `storno_invoice` and unmanaged
Agent `storno`/`set_credit_entries`, including supported reissue
and pinned conversion, evidence-carrying `recover`, and reads (`get`,
`observe_unresolved`, Agent `query`, `query_taxpayer`, `check_account`). Corrective
issuance returns the existing `invalid_input` fault before account/provider I/O.
Omitted configuration selects protected execution. Compilation target never selects
financial semantics; Agent operations retain their own contracts without a selector.

Validation, pinned account/namespace, ownership, exclusivity, money and the full
lookup remain ahead of admission. The open write refreshes its target first, then
prepayment/final exclusivity, pinned proforma/no-link checks and the order-number hint. A target
already found before admission needs no marker. An observed non-namespace proforma
in the order hint blocks unless it is the explicitly selected proforma.
These reads are sequential observations, not an atomic provider transaction.

## Create outcome table

“Retain” below means journal uncertainty as data, perform read-only reconciliation,
and pause on invocation-policy exhaustion. Resume replays that uncertainty and
only repeats reconciliation. No new send is authorized by a later negative answer.

| Boundary / evidence | Caller result / next action | Marker |
|---|---|---|
| Invalid body/key, unsupported mutation/options, invalid document or money | `invalid_input`, no provider write | Not created |
| Before admission: resolver/init/unanswered prerequisite | Retain invocation retry/pause; completed observations replay | Not created |
| Before admission: credential or other answered prerequisite failure | Existing `credentials_rejected` / `unavailable` fault | Not created |
| Before admission: matching live / reversed target | `already_issued` / `reversed` | Not created |
| Before admission with reissue: matching live / changed or absent expected target | `conflict{live}` / `conflict{target_changed}` | Not created |
| Before admission: collision, exclusive live kind, live proforma or foreign invoice | Existing conflict; no send | Not created |
| Marker prepared, state written, barrier not yet acknowledged | Await barrier, replay safely; no provider write before barrier | Retain if state landed |
| Open write: fresh matching live / reversed holder | `issued` / `reversed`, positive issuance under accepted replay risk | Clear after recorded result |
| Open write: absent holder and all fresh guards pass | Submit same pinned create intent | Retain until result recorded |
| Open reissue: exact old holder still reversed, all fresh guards pass | Submit same replacement intent | Retain until result recorded |
| Open reissue: expected holder absent/live, or old-number acknowledgement | No replacement established; retain | Retain |
| Numbered successful acknowledgement (including notification warning) | `issued`, retain optional reported metadata/warning | Clear after recorded result |
| Unnumbered, malformed, contradictory or open-code acknowledgement; unavailable/transport failure | Retain; exact matching positive evidence may settle | Retain |
| Duplicate-order answer with matching holder | `reconciled` (or `reversed` for a reversed holder) | Clear after recorded evidence |
| Duplicate-order answer without matching holder, even if it names another document | Retain; duplicate refusal does not settle earlier execution | Retain |
| Vendor refusal, credential refusal or local request refusal inside open write | Retain, including on an apparently first execution | Retain |
| Fresh holder collision, changed exclusivity/proforma/foreign guard, or failed guard query | No send in this execution; retain earlier-write uncertainty | Retain |
| Initialization or leading query failure inside open write | No send in this execution; retain earlier-write uncertainty | Retain |
| Open execution interrupted before result is recorded | Re-execute open run with fresh target and guards; absence may lead to another submission | Retain |
| Recorded uncertainty, including an actually never-sent/no-effect write | Read-only reconcile and pause; absence/time are not settlement | Retain |
| Reconciliation finds matching issuance, live or since reversed | `reconciled` / `reversed` | Clear after recorded evidence |
| Reconciliation absent, collision, failed/credential-blocked query | Retry/pause read-only | Retain |
| Cancellation at/after barrier | `outcome_unknown`, `cause: cancelled`; no automatic renewal | Retain |
| Kill at/after barrier | Native kill completion; successor mutation blocked | Retain |
| Completed ingress-key replay | Recorded response, no credential fetch or provider operation | Replay recorded clearance |
| Surviving old execution after replacement's positive completion | May still submit; matching completion proves neither uniqueness nor exclusion of delayed effects | Replacement may already have cleared |

There is no authoritative “first send” counter. Even an initial-looking refusal
after admission is conservative uncertainty because an earlier open execution may
have sent without recording its result. This intentionally strands some never-sent
work until evidence-carrying recovery, rather than fabricating settlement.
Positive completion accepts residual duplicate/delayed-effect risk; it is not proof
that every possibly effective submission has finished.

## State, replay and transition boundary

Replay-enabled markers use the same `unresolved-write` blocking state key, with an
operation-specific `execution_contract`. `ReplayExecutionContract` carries the stable
`request_response_ordinary_v1`, `request_response_proforma_v1`,
`request_response_prepayment_v1`, `request_response_final_v1`,
`request_response_delete_v1` and `request_response_storno_v1` tokens. Creates retain
their pinned `proforma_number` or `prepayment_number` where applicable; deletion and
storno carry their operation-specific pinned guards. The shared decoder accepts
known protected and replay-enabled markers; older deployments may reject extended
shapes. Recovery compares the original JSON
marker values, including omitted versus null members, before evidence verification.
Observation preserves noncanonical known marker JSON as opaque content rather than
rewriting the echo. Unknown contracts remain blocking.
Protected and unknown state block new admission too. Replay-enabled execution never
consumes a protected marker as resend permission. The prepare run records a typed result
requiring the discriminator: an old prepare result cannot decode into the new
contract. Distinct prepare/barrier/write step names make the changed sequence
visible for review; names alone do not fence exceptional replay.

Known marker recovery is independent of the selected mode; decoding compatibility
does not authorize journal replay. Every release requires an immutable endpoint,
with old code/config retained and invocation/state settlement before switching
producers. Follow the [execution transition procedure](../operations/order-execution-transition.md),
including same-mode Workers releases and invocation-specific exceptional-replay review.
