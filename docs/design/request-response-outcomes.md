# Experimental ordinary issuance: outcomes and marker clearance

#247's first actual-service slice, 2026-09-14. This is the implemented experiment's
table, pending production approval under [ADR 0019](../adr/0019-request-response-ordinary-invoice-replay.md).
The account must have duplicate-order checking enabled, independently confirmed
by its operator. `check_account` verifies neither that setting nor seller mapping.

## Extension agreed with the user

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

The existing `observe_unresolved`/`recover` interface accepts legacy strict and
known ordinary markers, preserving exact marker equality, pinned account selection,
record-before-clear ordering and existing evidence requirements. Unknown versions
or execution contracts block. Recovery never grants send permission. The host
authorizes operator access on either host. Ordinary replay-risk positive evidence
still establishes neither uniqueness nor exclusion of delayed effects. Explicit
experimental selection remains; other mutation sends and production migration
remain outside this extension. This section supersedes the initial-slice exclusions
below.

## Admission and support

The explicit `test-util` opt-in on **both** Order and Agent is for an isolated
experimental endpoint. It permits ordinary `Order.create_invoice` including reissue
and pinned conversion, evidence-carrying `recover`, and reads (`get`,
`observe_unresolved`, Agent `query`, `query_taxpayer`, `check_account`). All other
mutations return the existing `invalid_input` fault before provider I/O.
Native constructors keep their existing behavior, including when the feature is
compiled. Compilation target never selects financial semantics.

Validation, pinned account/namespace, ownership, exclusivity, money and the full
lookup remain ahead of admission. The open write refreshes its target first, then
prepayment/final exclusivity, pinned proforma/no-link checks and the order-number hint. A target
already found before admission needs no marker. An observed non-namespace proforma
in the order hint blocks unless it is the explicitly selected proforma.
These reads are sequential observations, not an atomic provider transaction.

## Outcome table

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
| Open write: absent holder and all fresh guards pass | Submit same ordinary intent | Retain until result recorded |
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
| Reconciliation finds matching ordinary issuance, live or since reversed | `reconciled` / `reversed` | Clear after recorded evidence |
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

The experimental marker uses the same blocking state key, with a distinct
`execution_contract: request_response_ordinary_v1` field and optional pinned
`proforma_number`. The shared decoder accepts known legacy and ordinary markers;
older deployments reject the extended shape. Recovery compares the original JSON
marker values, including omitted versus null members, before evidence verification.
Observation preserves noncanonical known marker JSON as opaque content rather than
rewriting the echo. Unknown contracts remain blocking.
Legacy and unknown state block new admission too. The experiment never consumes a
legacy marker as resend permission. The prepare run also records a typed result
requiring the discriminator: an old prepare result cannot decode into the new
contract. Distinct prepare/barrier/write step names make the changed sequence
visible for review; names alone do not fence exceptional replay.

Use a fresh isolated deployment and billing units. Preserve old immutable endpoints
for their invocations. Do not switch existing producers or resume old journals onto
this experiment. Production capability and transition approvals remain #247 gates;
the experimental Workers build and signed/scoped runtime acceptance are implemented.
