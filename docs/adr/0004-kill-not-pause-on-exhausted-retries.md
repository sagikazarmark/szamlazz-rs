# Exhausted retries kill the invocation instead of pausing it

Status: partially superseded by [ADR 0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md);
amended by #22 (the create step under a run retry policy), #30 (the storno step), #37 (the read policy), #41
(the `Szamlazz.Agent` writes' timeouts and retry interval), #61 (the ≥ 90 s re-check rule in code), #87 (what
an invocation attempt is spent on; `Szamlazz.Agent.storno` on `Order`'s policy; the read policy widened) and #114
(every wait has a bound), below.
Current implementation: `on_max_attempts = "kill"` on every handler that calls szamlazz.hu. The #205 decision
below selects retention plus a marker for a follow-up; it is not implemented by this amendment. Still holds: the verified Restate facts and
the operational alerts; the retry policy and timeout values hold as amended (#41, #61, #87, #114, the current values are
in the paragraph below and in the code). Withdrawn by #87: the third "considered option"'s etiquette rationale
(etiquette bounds sends, which the issue policy governs, not invocation attempts). Kill itself is under review for
the `Order` writes (#87's follow-up). Superseded: the `pending` slot as what makes
kill safe (there is currently no state; query-first alone does not establish safety after an unanswered send), the runbook and caller
contract phrased in terms of `request_id` (→ retry with a **new** `Idempotency-Key`, since a stored failure is
replayed under the same key), the operator handlers, and `idempotency_retention = 7d` (the code sets `30d`).

Restate's server-wide default retry policy is `initial-interval 500ms`, factor 2, `max-interval 1m`,
`max-attempts 70`, `on-max-attempts pause`, about an hour of back-off followed by an indefinite
pause awaiting a human (the reference page; the guides disagree with each other on the initial
interval, so the policy is pinned in code and never left to the default). `on_max_attempts` is a
per-handler setting in Rust SDK 0.12 and the server honors it (verified: `GET /services/{name}`
shows the effective `retry_policy`; the `POST /deployments` response shows `null`s and is not
authoritative).

Every handler that calls szamlazz.hu sets `on_max_attempts = "kill"`. On `Szamlazz.Order` the issuing,
correcting, storno and delete handlers carry `invocation_retry_policy(initial_interval = "2m",
factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")` with
`inactivity_timeout = "4m"` and `abort_timeout = "3m"` (the create closure may take up to 180 s:
the leading external-id query, the create, a re-query, 60 s each), and so does `Szamlazz.Agent.storno` (#87).
`Szamlazz.Agent.set_credit_entries` uses `initial_interval = "2m", max_attempts = 2, kill` (its timeouts and retry
interval: #41 and #61, below; why two: #87); read-only handlers (`Szamlazz.Order.get`, `Szamlazz.Agent.query`, `query_taxpayer`, `check_account`) may retry more freely
because queries are safe to repeat (`initial_interval = "10s", factor = 2.0, max_interval = "1m", max_attempts = 3`, pinned on
every one of them since the Restate-conventions review, so no server default leaks through), but they kill too. The external-id query inside the create step
is the next invocation's reconciliation mechanism; kill keeps the key reachable but does not establish that
an external send has stopped processing (#205).

## Verified Restate facts

- A paused invocation **holds the Virtual Object key**: `sys_keyed_service_status` names it, an
  exclusive call on the same key sits `inboxed` indefinitely (the client timed out), while a shared
  handler answers in milliseconds and sees the state committed by the `ctx.set` before the failure.
- `kill` releases the key with committed state intact: after the kill `sys_keyed_service_status` is
  empty and the state written before the failure is still present. An inboxed call then runs even
  though its ingress client has long disconnected (8 ms after the kill in the probe).
- `on_max_attempts = "kill"` completes the invocation as a failure after exactly `max_attempts`
  attempts; a synchronous ingress caller receives **HTTP 500** with a JSON body carrying the *last
  retryable error's message* and `x-restate-error-source: invocation`. By status alone this is
  indistinguishable from a handler-thrown `TerminalError`.
- `PATCH /invocations/{id}/resume` on a paused invocation re-runs the full `max_attempts` budget and
  pauses again.
- An ingress `Idempotency-Key` replays a stored terminal failure for the retention period without
  re-executing (same invocation id, identical body).

## Considered options

- **The server default (`pause` after 70 attempts).** Rejected: a stuck `Order` invocation holds the
  order's key for ~63 minutes of back-off and then until a human resumes or kills it. Every
  exclusive handler for that order (including the caller's own retries, which are the intended
  recovery path) queues behind it; only the shared `get` answers. A paused invocation blocks the
  very handler that would reconcile it.
- **`pause` with a small `max_attempts`.** Rejected: still holds the key, still needs a human per
  stuck order, and `resume` only re-runs the same budget on the same deployment. Fix-and-resume is
  not lost by choosing kill: the next call runs on the latest deployment against the same `pending`
  slot, without the non-determinism risk of resuming an old journal on new code.
- **`kill` with an unlimited or large attempt budget.** Rejected: szamlazz.hu etiquette bounds
  sends ("max 5 attempts", "no loops", banning), and at 5 attempts with `2m → 10m` back-off the key
  is already held ≈ 39 minutes in a full outage. The split between the create step's re-executions
  (the issue policy, a *run* retry policy) and invocation attempts (this handler policy) is the
  service's; the total is the server's.

## Amended (#22): the create step under a run retry policy

The create was first a hand-rolled loop, one `ctx.run` per attempt with `max_attempts(1)`, a
durable `ctx.sleep` between attempts, an attempt counter in the handler. It is now **one** `ctx.run`
(`create-{kind}`) under the issue policy, `RunRetryPolicy::new().initial_delay(2m)
.exponentiation_factor(2.0).max_delay(10m).max_attempts(5).max_duration(1h)`, and Restate does the
retrying: the closure returns `Err(Unconfirmed)` (a plain `std::error::Error`, retryable to the SDK)
only when szamlazz.hu's answer is *not* known, and every known answer is `Ok` data. Exhaustion
fails the run as a `TerminalError` (500, the last `Unconfirmed`'s message), which the handler maps
to `outcome_unknown{order, kind, external_id}`; so does a cancel mid-create (409).

The policy must be set explicitly, with `new()` and every field: the SDK's default for a run is
`RetryPolicy::Infinite`, which sends no `next_retry_delay`, and the server then consumes *this*
handler's `invocation_retry_policy` budget (5 attempts, kill) instead, 2 minutes between
re-executions and a kill that looks like a transport failure. `RunRetryPolicy::default()` caps at
2 s / 50 s; `new()` has factor 1.0 and no caps. Verified end to end: with a 1 s test policy the
re-execution follows the run's delay, not the handler's 2 m, and exhaustion is the structured fault.

**Rule: the query is inside the closure.** Every execution of the create step begins with the
external-id query, in the same closure as the send. A separate journaled pre-query would replay its
stale "nothing" on the retry and the re-executed closure would send again. The same rule made the
old loop safe ("every attempt is query-first"); under the run policy it is the *only* place the
query can live.

**Accepted limitation, corrected by #204:** both execution-count and duration limits can overshoot.
The SDK/shared core restores retry information only for the first entry processed after replay; otherwise it
starts from default retry information. Limits are evaluated after closure failure, and a crash before recording
the result can re-execute it. The configured `1h` is an exhaustion threshold, not a hard limit or a hung-closure
deadline. No propagation-lag retry is added inside the closure: after the one immediate re-query, absence leaves
the send unconfirmed. The observed near-zero query visibility lag (behaviour notes) does not prove that an
unanswered send is no longer processing.

## Amended (#30): the storno step under the same policy

Storno had kept a hand-rolled loop (three `run_once` attempts, a durable sleep, a doubling backoff read
off `issue.initial_delay` / `max_delay`). It now has the shape of the create: a read-only lookup step
(`lookup-storno-{number}`) and one `ctx.run` (`storno-{number}`) under the same issue policy, query-first
inside the closure, `Err(Unconfirmed)` only for an unknown answer, exhaustion mapped to
`outcome_unknown{order, kind, external_id}`, on `Szamlazz.Order.storno_invoice` and on
`Szamlazz.Agent.storno` alike. No durable attempt counter or `ctx.sleep` remains: the issue policy is
the retry envelope of every szamlazz.hu write, and the resolve policy (#25) that of the `account` step.
The one in-process retry left is credential fetch (three fetches 200 ms apart, inside an executing
operation run since #200, without persisting credentials, then terminal `unavailable` on that Run command),
which is deliberately not a Restate retry (design §4;
[ADR 0006](0006-account-selection-via-restate-scopes.md) records why terminal was chosen over retryable).

## Amended (#37): the read policy is the retry envelope of every szamlazz.hu read

Until #37 every read-only step (the lookup step, the exclusivity, proforma-link and `get` lookups, the
verifies, the order-number hint, the storno lookup, `Szamlazz.Agent.query`, the `check_account` probe)
ran once (`max_attempts(1)`), journaled a transport failure as a `Transport` outcome, and every handler
mapped that data to the **terminal** `unavailable`. One network blip on any of `create_invoice`'s up to
four reads was a 503 the caller is told to page on, and nothing retried it. There is no safety reason
a read must not be retried: it writes nothing, and a re-executed closure's answer is exactly as fresh
as a first answer (the create step re-queries inside its own closure regardless).

Every read now runs under a third deployment-level run retry policy, the **read policy** (`[read]`,
`RunRetryPolicy::new()` field for field like the issue policy; defaults `max_attempts = 3`,
`5s → 30s`, factor 2, `max_duration = 2m`, widened to `5`, `5s → 60s`, `5m` by #87, below). The gateway's read fns return `Err(Unanswered)` (a plain
`std::error::Error`, retryable to the SDK, the read-side twin of `Unconfirmed`) when szamlazz.hu did
not answer (a transport or parse failure, `szlahu_down`), and every *answer* (a document, code 7,
3/135/136/164, another API code) as `Ok` data; the `Transport` variants of the journaled outcome
types are gone, and another API code on a read is the new data variant `Api{code, message}` (still
`unavailable` in the handlers that could not conclude from it, except `Szamlazz.Agent.query`'s 422
pass-through and the probe's `Accepted`, which were already data). Exhaustion of the read policy, or
a cancel mid-read (409), is the `unavailable` fault, naming the step and the last `Unanswered`,
about the document when the step knows one (`{order, kind, external_id}` on the lookup step). The
storno-number hint after a verify that found the document already reversed stays best effort: a hint
the read policy could not get answered reports the reversal without the storno number rather than
failing a handler whose answer is already known.

The three policies are now the complete retry envelope of the worker: the issue policy for every
szamlazz.hu write, the read policy for every szamlazz.hu read, the resolve policy for the `account`
step; the credential fetch keeps its in-process retry. The handler timeouts do not change: a retryable
run failure is returned to the server with `next_retry_delay` and the invocation yields (the SDK does
not sleep in-process; `restate-sdk-shared-core` 7.0.3, `vm/transitions/journal.rs`), so a read's
retry delays are spent between executions and a failed read ends the execution it fails in; the
per-step sizing estimate (for example, the lookup's two 60 s calls or the create's leading query, send and re-query)
is what `inactivity_timeout = 4m` / `abort_timeout = 3m` were sized for. Successful steps continue in the same
execution; this is not a per-execution duration bound, and progress affects inactivity timeout (#204).
Verified end to end: a lookup that
loses its reply once is re-executed under a 1 s test read policy and the create completes `issued` in
one invocation with one create on the wire and `last_failure_related_command_name = lookup-invoice`
while in flight; a lookup that never answers is a 503 `unavailable{order, kind, external_id}` with
zero creates. The accepted risk of #22 applies unchanged: the attempt count is not durable across every
replay, and the duration threshold can overshoot too (#204).

## Amended (#41): the `Szamlazz.Agent` writes' timeouts and retry interval

`Szamlazz.Agent.storno` runs the same three-call storno closure as `Szamlazz.Order.storno_invoice` (query, send,
re-query at up to 60 s each, ~180 s in the worst case) but carried `inactivity_timeout = 2m` /
`abort_timeout = 2m`. The inactivity interval could request suspension during a slow storno; the abort interval
then allowed a further two minutes before forced abort, not a simultaneous two-minute deadline (#204).
An interrupted open run can re-execute with its leading query reconciling a landed storno. It now
carries `Szamlazz.Order`'s `4m` / `3m`; the discovery test asserts them.

`Szamlazz.Agent.set_credit_entries` had `max_attempts = 2, kill` with no `initial_interval`, so the one retry after a
crash ran on the server's ~500 ms default. Its send is **at-least-once** under `additive: true` (every send that
reaches szamlazz.hu appends the entries, and the handler cannot tell a lost reply from a lost request), so a retry
that fires while the first send is still in flight (the client waits up to 60 s) could append twice. It now
carries an explicit `initial_interval = "2m"`, longer than the client timeout; the `outcome_unknown` message is
conditional on `additive` ("query the invoice before re-sending" rather than "call set_credit_entries again"), and the
hazard is stated on the request field and in both READMEs. Its timeouts stay `2m` / `2m` (one send).

Amended #41 also settled the 71/152 contradiction (design §5 step 4): a duplicate-order-number refusal with nothing
under the order was `Err(Unconfirmed::Contradiction)` and re-sent the create for up to five executions; it is now
`conflict{duplicate_order_number}` without `existing_number` on the first occurrence, logged at `warn`; the
refusal is an answer szamlazz.hu already gave.

## Amended (#61): the ≥ 90 s re-check rule lives in code

Every value above rests on one timing rule: nothing re-checks or re-sends within ~90 s of a send, because the
Számla Agent client gives up on a reply at 60 s and szamlazz.hu has been seen to stall for about a minute while
still issuing (behaviour notes; [ADR 0002](0002-order-keyed-idempotency-via-external-ids.md) states the "never below
~90 s" bound). Until #61 the rule held by convention, with two gaps.

`Szamlazz.Agent.storno` had `max_attempts = 2, kill` with no `initial_interval` (the same omission #41 closed on
`set_credit_entries`), so the one retry after a crash (a rollout cutting the connection mid-storno) was re-dispatched at the
server's ~500 ms default while the first `xmlszamlast` could still be in flight; its re-execution's leading query
would then find nothing and send a second storno. Not lossy (szamlazz.hu answers a repeated storno with the existing
storno number), but a second send the rule forbids, and the endpoint README documented the 500 ms as the behaviour. It
now carries `initial_interval = "2m"` like every other write handler of both services, and the discovery test pins the
interval on both `Szamlazz.Agent` writes.

`WorkerConfig::validate` checked `max_attempts ≠ 0`, `initial ≤ max` and `factor ≥ 1`, so an operator could write
`issue.initial_delay = "5s"` and the endpoint started; the create step's lost-reply re-execution would then have raced
the original send. The issue policy's `initial_delay` now has a floor, `IssueConfig::MIN_INITIAL_DELAY` = the agent
crate's newly exported `client::REQUEST_TIMEOUT` (60 s) + `IssueConfig::RE_CHECK_MARGIN` (30 s) = 90 s, derived from
the client's constant rather than a second copy of "60 s"; a shorter delay is `WorkerConfigError::IssueDelayBelowFloor`,
whose message names the rule, and a deployment's configuration load fails with it. The floor is on the issue policy only (a read
writes nothing and the resolve policy never reaches szamlazz.hu), and bites where a deployment validates its configuration:
the e2e suite's 1 s policies are built in Rust, handed to `from_parts` and never pass through `validate`.

## Amended (#87): an invocation attempt is spent only on a worker-side failure

Two retry envelopes wrap every szamlazz.hu call, both Restate's: the **run retry policies** (`[issue]`, `[read]`,
`[resolve]`) on the `ctx.run` steps, and the handlers' **invocation retry policy**. This ADR sized the second as if it
were the first: its third considered option rejected a larger attempt budget because "szamlazz.hu etiquette bounds
sends". The issue policy governs deliberate send retries (not a hard send cap), and **a run retry does not spend an invocation
attempt**. Verified in Restate 1.7.8 (`crates/invoker-impl/src/invocation_state_machine.rs`, `handle_task_error`):
an SDK error carrying `next_retry_delay` (what the shared core sends for every run retry) becomes
`RequestedErrorBehavior::RetryWithIntervalOverride`, and the server takes
`next_retry_interval_override.or_else(|| retry_iter.next())`: the handler's attempt iterator is not advanced. It is
advanced by everything else: the worker unreachable (connection refused), the stream cut by a rollout or a crash,
the abort timeout, a journal entry the deployment cannot decode, a non-deterministic replay. The iterator is
cumulative over the invocation's life (`retry_iter.attempts()`; the re-dispatch after a scheduler retry
`fast_forward`s it, so the arithmetic holds under the vqueues flag); only the SDK-facing
`retry_count_since_last_stored_command` resets on progress. Asserted end to end (`(xi-e)` in
`tests/e2e/get.rs`): `get`'s handler allows three attempts, all four of its reads lose their reply once under a 1 s
test read policy, `sys_invocation.retry_count` (the invoker's count of starts, `start_count` in
`crates/worker-api/src/invoker/status_handle.rs`) is observed past the handler's budget while in flight (five
starts in the run that landed this amendment), and the invocation completes instead of being killed.

Consequences for the numbers of this ADR:

- **Which policy tolerates which outage.** szamlazz.hu unreachable, worker up: the run policies decide, and every
  handler's first szamlazz.hu call is a read, so `[read]` decides, the old default (3 executions, `5s → 30s`, `2m`;
  two delays of 5 and 10 s) gave up after ~15 s of connection refusals with a terminal 503 `unavailable` stored under the caller's `Idempotency-Key`
  for 30 days. The defaults are now `max_attempts = 5`, `5s → 60s`, factor 2, `max_duration = 5m`: 75 s of back-off
  rides out a blip of about a minute (four delays: 5 + 10 + 20 + 40 s, so the 60 s cap is inert at the defaults), and a
  stalling szamlazz.hu is evaluated against the 5 m `max_duration` after each failed closure, which can overshoot.
  The issue policy stays at a 5-execution threshold: it governs deliberate retries, not a hard send bound.
  Worker unreachable: the invocation policy
  decides; 4 re-dispatches at 2 → 4 → 8 → 10 min on the `Order` writes, ~24 min of back-off; every invocation, in
  flight or newly arriving, older than that when the worker returns is killed.
- **`Szamlazz.Agent.storno`** ran on `max_attempts = 2` (#41): any worker outage over 2 min killed it while
  `Szamlazz.Order.storno_invoice` (the same closure) survived ~24 min. Nothing about an unmanaged storno justifies
  the asymmetry: the step is query-first and szamlazz.hu's storno is idempotent server-side. It now carries
  `Szamlazz.Order`'s policy (`2m`, factor 2, `10m`, 5, kill); the discovery test pins it.
- **`Szamlazz.Agent.set_credit_entries` stays at 2.** Its send is at-least-once under `additive: true` and has no query
  in front of it, so every invocation attempt is a potential second copy of the entries; the one handler where the
  attempt count is a duplicate hazard rather than an availability knob.
- **The 500 of a killed invocation is the last retryable error's text**, not the worker's `{code, message}` fault
  body: a caller parsing faults must tolerate it (README, caller contract).

Two further facts, verified in the same source and recorded here for the follow-up that reconsiders kill on the
`Order` writes (a Pretix-style caller cannot rotate its `Idempotency-Key`, so a kill after a long worker outage is a
stored 500 under a key the caller will repeat for days): a same-key request against a **paused** invocation
**attaches** to it (`crates/worker/src/partition/state_machine/mod.rs`, `handle_duplicated_requests`: `Paused` is in
the arm with `Invoked | Suspended | Inboxed | Scheduled`, `do_append_response_sink`; only `Completed` replays), and
`resume` keeps the pinned deployment unless `--deployment latest` is passed (`lifecycle/manual_resume.rs`,
`resolve_pinned_deployment`). Neither is exercised end to end yet; this amendment changes no `on_max_attempts`.

## Amended (#114): every wait has a bound, the reads' timeouts, a deadline on the prologue's calls

Two waits the worker could sit in had no bound of their own (review 2026-09-06, J10 and J6).

**The read handlers ran on the server's defaults.** `Szamlazz.Order.get`, `Szamlazz.Agent.query`, `query_taxpayer`
and `check_account` set neither `inactivity_timeout` nor `abort_timeout`, so they took Restate's 1 m / 1 m, while
every write handler sized its own from one rule, a step's szamlazz.hu round trips at the Számla Agent client's
`REQUEST_TIMEOUT` (60 s) each, plus margin: `4m` / `3m` for the create and storno steps' three trips, `2m` / `2m` for
`set_credit_entries`' one send. A read step is one trip bounded by the same 60 s, and szamlazz.hu has been observed to stall
for a minute at a time and still answer (behaviour notes), so a stalled read landed exactly on the default inactivity
boundary: suspension requested, with forced abort only if the SDK does not suspend within the subsequent abort
interval; `get` is
four such reads back to back. The four read handlers now carry `inactivity_timeout = 2m`, `abort_timeout = 2m`, the
one-trip value of the same rule; the discovery test pins them beside the writes'. #50 proposes concurrent `get`
reads; it is not implemented, and must respect Rust SDK 0.12.0's immediate-await rule.

**The prologue's `resolve` and `fetch` had no deadline.** The resolve policy bounds re-executions of the `account`
step, not one hung call inside it (the policy evaluates only on closure *failure*), and the fetch loop pauses between
attempts, but each attempt was unbounded: a hung `AccountResolver::resolve` or `CredentialStore::fetch` (a
database-backed embedder's pool that never answers) held the execution until the handler's inactivity and abort
timeouts, spending an invocation attempt on a wait the policies exist to retry. Both calls now run under
`tokio::time::timeout` with a worker-owned constant, `prologue::CALL_DEADLINE` = 10 s: a constant, not a setting: the
static resolver is in memory and never reaches it, and a resolver that has not answered in ten seconds is not going
to. At the deadline the future is dropped and the timeout is the existing retryable answer: in the `account` step the
closure's error (`ResolverUnavailable::TimedOut`, so the resolve policy re-executes it, and the exhausted step's fault
text, the last error's display, as for every `run_retrying` step, names the deadline); in the fetch loop one
attempt's failure (`FetchFailure::TimedOut`, retried through the same branch as a reported `Unavailable`, then the
terminal `unavailable` fault, whose text names the deadline and neither the account nor the credential reference,
#65). The loop therefore ends within `3 × 10 s` plus the two 200 ms pauses. The timeout is a variant of its own
beside the reported `Unavailable` rather than folded into `ResolveError::Unavailable` / `FetchError::Unavailable` as
#114 first sketched it: those displays are fixed and never echo their source, so a fold could not name the deadline in
the fault text, and a resolver that reports itself unavailable and one the worker gave up on are two things an
operator reads differently. Both are unit-tested under a paused tokio clock (a resolver that never answers is the
error at exactly the deadline; a store that never answers is the fault after three attempts within that bound); the
trait rustdoc checklists tell an embedder that the worker bounds the call and that a resolver or store over a pool
sets its own, shorter timeouts. No journaled type changed shape.

## Run thresholds and execution deadlines (#204, 2026-09-10)

The [Rust SDK 0.12.0 `RunRetryPolicy`](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/struct.RunRetryPolicy.html)
explicitly permits actual execution count **and duration** to exceed their configured values. Shared core
[7.0.3 `retries.rs`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/retries.rs) evaluates thresholds
after failure, and [`ProposeRunCompletion`](https://docs.rs/crate/restate-sdk-shared-core/7.0.3/source/src/vm/transitions/journal.rs)
adds the closure duration before that evaluation. `max_duration` never interrupts a hung closure. Neither
threshold counts external sends durably; `max_attempts(1)` disables policy-driven retries but a crash between
the external effect and recorded completion can re-execute the open closure. This also qualifies `run_once` and
one-shot writes. There is no justified arithmetic upper bound of nine external executions in a crash loop.

Keep real per-call deadlines: Számla Agent's 60 s request timeout and the worker's ten-second resolver/store
deadlines. Handler timeouts serve another purpose: **inactivity timeout** waits for progress before asking the
SDK to suspend; **abort timeout** starts after that request and limits the further wait before forcibly aborting
the execution ([server 1.7.8 timeout model](https://github.com/restatedev/restate/blob/v1.7.8/crates/admin-rest-model/src/services.rs)).
They are not two timers starting with an HTTP call and do not imply terminal invocation kill or external rollback.
No custom deadline engine is needed for these corrections.

`get` is a non-atomic external observation with separately journaled reads and fresh invocation/key per new poll
(ADR 0005); native attach/output answers whether a particular invocation completed. Kill and exhausted writes
may leave a send processing: [#205](https://github.com/sagikazarmark/szamlazz-rs/issues/205) owns the unresolved-write
policy, and [#45](https://github.com/sagikazarmark/szamlazz-rs/issues/45) the broader operational runbook. The
configuration above remains the current behavior, not proof that absence authorizes an immediate reissue.

## Unresolved Order writes (#205, 2026-09-10)

Implementation amendment (#216): Order mutations now use a pre-send unresolved marker, acknowledged
arming with execution-local one-use permission, and retained read-only reconciliation. Invocation exhaustion
pauses; cancellation/kill cannot clear the marker. The exact command/recovery/state contract and migration
boundary are [specified here](../design/order-write-protocol.md). Unkeyed Agent writes retain their own policy.

**Decision: retain the original invocation for read-only reconciliation, with pause on unresolved recovery or
invocation-policy exhaustion, plus a minimal durable unresolved-write marker armed before sending.** The owner
explicitly selected “Retain plus marker” after reviewing the reproduction and alternatives. This is the approved
direction and bounded [implementation brief](../design/unresolved-order-writes.md), not a production recovery
change in #205. The current terminal run exhaustion and kill attributes remain until that work lands.

The required protection is order-wide: no subsequent mutation may send while an earlier send might still act,
including correctives and cross-kind creates. Retention protects queued invocations while the owner reconciles;
the marker protects after cooperative cancellation or manual/automatic kill releases its lock. Marker state
answers only “may a write still act?”, not document status. This is a narrow exception to ADR 0005's no-state
decision. It requires guarding replay of the owner's **open send closure** too: a marker check replayed as
absent does not protect that closure. The brief specifies the candidate one-use execution-local send permit and
the crash-window tests required before shipping it.

Alternatives considered:

| Option | Benefit | Cost / decision |
|---|---|---|
| Keep terminal completion/kill and query-first renewal | Reachable order, simple state-free worker | Reject as sufficient protection: an immediate empty query permits another send while the first may still act |
| Retain and pause, no marker | Keeps original identity and lock; no cross-invocation state | Operational quarantine required after kill/cancel and ambiguous replay; insufficient for the selected cooperative cross-invocation protection |
| Marker and terminal completion | Caller gets a bounded failure; later writes can fail closed | Recovery becomes a separate operator/caller protocol; original invocation cannot finish reconciling for its attached callers |
| **Retain plus marker** | Original invocation owns normal recovery; marker survives release | **Selected**: paused orders block exclusive work, false-positive markers can need manual evidence, state migration and operator recovery are required |

A real-server script with a valid one-execution issue policy observed a queued second send before the first
became visible, for invoice→invoice, corrective→same corrective and invoice→prepayment. Its initially failing
one-send assertion was 2 versus 1. A test-only protection probe verified pause/queue/attach/shared read/resume,
and that a marker survives manual kill and refuses a queued mutation. These are **scripted vendor assumptions**,
not observations of duplicate invoices at szamlazz.hu. The named 57 s stall probe says no issuance; older
“stalled and still issued” prose has conflicting provenance. Post-success visibility at 771 ms is not a bound
on unanswered processing. No delay or immediate empty query proves an external effect cannot still land.

The pinned Rust SDK exposes invocation-policy pause, but explicit `RunRetryPolicy` exhaustion always maps to
`FailAsTerminal`. Changing `kill` to `pause` alone misses that path. The brief records source links, the event-by-event
lock/recovery matrix, the available read-only pause sequence, marker lifecycle, permissible positive/negative
evidence, cancellation, operator recovery, Idempotency-Key rules and deployment/journal implications.

**Superseding recovery guidance:** keep the original key while the invocation is unfinished, paused included.
After a completed uncertain fault, cancellation or kill, reconcile before deliberately renewing with a new key.
Negative settlement requires evidence that the earlier send did not act **and cannot act later**; if unavailable,
remain blocked. Before manual kill/recovery, quiesce producers and account for queued mutations. Neither `get`
absence nor kill is permission to issue again. This corrects #45's “kill is always safe — query-first” premise
and the older consequences below; #45 supplies the broader operational runbook.

## Consequences

- Kill releases the key without compensating external effects. The next create queries by external id before
  considering a send; absence alone does not establish that the earlier send cannot still land (#205).
- Historical caller contract, superseded for renewal permission by #205 above: **any error from an issuing or storno handler
  means "outcome unknown, retry with a **new** `Idempotency-Key`, or read `Szamlazz.Order.get`"**,
  never "no document exists". A call that timed out on the client side may still run once the key
  frees; attach/output is the way to learn that invocation's completion. Fresh `get` polls observe external
  state concurrently, without a completion barrier. The same key would replay the stored
  `TerminalError{outcome_unknown}` for `idempotency_retention` (30 days). (#67 later scoped the
  rule to `outcome_unknown`, `unavailable` and `credentials_rejected`; the settled 4xx/422 faults
  are not "outcome unknown", design §7.)
- Operations: alert on `sys_invocation` failed completions, on invocations in `backing-off` for
  more than 5 minutes and on any invocation `paused` (none is expected under kill; one is a policy
  override or a server default leaking through); `idempotency_retention = 30d` keeps failed completions
  visible. Verify the effective policy with `GET /services/{name}`. After a worker outage,
  `restate invocations resume Szamlazz.Order` pulls the backing-off invocations forward instead of
  waiting out their intervals (#87). The SDK endpoint speaks HTTP/2 only.
- Recovery: establish invocation completion with attach/output, reconcile through fresh `get` observations,
  then deliberately renew an operation only if still intended. #205 owns unresolved-write exhaustion/kill.
- In a pathological crash loop the run execution count and elapsed duration can exceed the configured
  thresholds; no hard external-send bound follows from multiplying or adding the two policies' counts.
- Because there is no child Restate service (ADR 0001), the "callee pauses and strands the parent"
  branch does not exist; the rule of thumb "no handler that `Order` awaits may pause" is trivially
  true.
