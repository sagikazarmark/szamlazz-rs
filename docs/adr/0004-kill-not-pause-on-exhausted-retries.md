# Exhausted retries kill the invocation instead of pausing it

Status: partially superseded by [ADR 0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md);
amended by #22 (the create step under a run retry policy), #30 (the storno step), #37 (the read policy), #41
(the `Szamlazz.Agent` writes' timeouts and retry interval) and #61 (the ≥ 90 s re-check rule in code), below.
Still holds: `on_max_attempts = "kill"` on every handler that calls szamlazz.hu, the retry policy and timeout
values, the verified Restate facts, and the operational alerts. Superseded: the `pending` slot as what makes
kill safe (there is no state; the external-id query inside the create step is), the runbook and caller
contract phrased in terms of `request_id` (→ retry with a **new** `Idempotency-Key`, since a stored failure is
replayed under the same key), the operator handlers, and `idempotency_retention = 7d` (the code sets `30d`).

Restate's server-wide default retry policy is `initial-interval 500ms`, factor 2, `max-interval 1m`,
`max-attempts 70`, `on-max-attempts pause` — about an hour of back-off followed by an indefinite
pause awaiting a human (the reference page; the guides disagree with each other on the initial
interval, so the policy is pinned in code and never left to the default). `on_max_attempts` is a
per-handler setting in Rust SDK 0.12 and the server honors it (verified: `GET /services/{name}`
shows the effective `retry_policy`; the `POST /deployments` response shows `null`s and is not
authoritative).

Every handler that calls szamlazz.hu sets `on_max_attempts = "kill"`. On `Szamlazz.Order` the issuing,
correcting, storno and delete handlers carry `invocation_retry_policy(initial_interval = "2m",
factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")` with
`inactivity_timeout = "4m"` and `abort_timeout = "3m"` (the create closure may take up to 180 s:
the leading external-id query, the create, a re-query, 60 s each). `Szamlazz.Agent.set_payments` and `storno` use `initial_interval = "2m",
max_attempts = 2, kill` (their timeouts and retry interval: #41 and #61, below); read-only handlers (`Szamlazz.Order.get` with `verify`, `Szamlazz.Agent.query`) may retry more freely
because queries are safe to repeat, but they kill too. The external-id query inside the create step
is what makes kill safe; kill is what keeps the key reachable.

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
  exclusive handler for that order — including the caller's own retries, which are the intended
  recovery path — queues behind it; only the shared `get` answers. A paused invocation blocks the
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

The create was first a hand-rolled loop — one `ctx.run` per attempt with `max_attempts(1)`, a
durable `ctx.sleep` between attempts, an attempt counter in the handler. It is now **one** `ctx.run`
(`create-{kind}`) under the issue policy, `RunRetryPolicy::new().initial_delay(2m)
.exponentiation_factor(2.0).max_delay(10m).max_attempts(5).max_duration(1h)`, and Restate does the
retrying: the closure returns `Err(Unconfirmed)` — a plain `std::error::Error`, retryable to the SDK
— only when szamlazz.hu's answer is *not* known, and every known answer is `Ok` data. Exhaustion
fails the run as a `TerminalError` (500, the last `Unconfirmed`'s message), which the handler maps
to `outcome_unknown{order, kind, external_id}`; so does a cancel mid-create (409).

The policy must be set explicitly, with `new()` and every field: the SDK's default for a run is
`RetryPolicy::Infinite`, which sends no `next_retry_delay`, and the server then consumes *this*
handler's `invocation_retry_policy` budget (5 attempts, kill) instead — 2 minutes between
re-executions and a kill that looks like a transport failure. `RunRetryPolicy::default()` caps at
2 s / 50 s; `new()` has factor 1.0 and no caps. Verified end to end: with a 1 s test policy the
re-execution follows the run's delay, not the handler's 2 m, and exhaustion is the structured fault.

**Rule: the query is inside the closure.** Every execution of the create step begins with the
external-id query, in the same closure as the send. A separate journaled pre-query would replay its
stale "nothing" on the retry and the re-executed closure would send again. The same rule made the
old loop safe ("every attempt is query-first"); under the run policy it is the *only* place the
query can live.

**Accepted risk: the attempt count is not durable.** The SDK restores a run's retry count and
elapsed duration from the server's `retry_count_since_last_stored_command` only when the failing
run is the first journal entry after replay — true for this handler, whose code is deterministic
and whose open entry on re-dispatch is always the create step. Otherwise the count restarts, and
`max_attempts` is not a hard bound. `max_duration` is the hard limit and is set accordingly (1 h,
above the ≈ 39 minutes five attempts at `2m → 10m` take). No propagation-lag retry is added inside
the closure: read-your-writes lag by external id is measured at ≈ 0 (behaviour notes), so the one
immediate re-query after an open outcome is enough, and "nothing" means nothing.

## Amended (#30): the storno step under the same policy

Storno had kept a hand-rolled loop (three `run_once` attempts, a durable sleep, a doubling backoff read
off `issue.initial_delay` / `max_delay`). It now has the shape of the create: a read-only lookup step
(`lookup-storno-{number}`) and one `ctx.run` (`storno-{number}`) under the same issue policy, query-first
inside the closure, `Err(Unconfirmed)` only for an unknown answer, exhaustion mapped to
`outcome_unknown{order, kind, external_id}` — on `Szamlazz.Order.storno_invoice` and on
`Szamlazz.Agent.storno` alike. No durable attempt counter or `ctx.sleep` remains: the issue policy is
the retry envelope of every szamlazz.hu write, and the resolve policy (#25) that of the `account` step.
The one in-process retry left is the prologue's credential fetch — three fetches 200 ms apart, outside
the journal, then a terminal `unavailable` — which is deliberately not a Restate retry (design §4;
[ADR 0006](0006-account-selection-via-restate-scopes.md) records why terminal was chosen over retryable).

## Amended (#37): the read policy is the retry envelope of every szamlazz.hu read

Until #37 every read-only step — the lookup step, the exclusivity, proforma-link and `get` lookups, the
verifies, the order-number hint, the storno lookup, `Szamlazz.Agent.query`, the `check_account` probe —
ran once (`max_attempts(1)`), journaled a transport failure as a `Transport` outcome, and every handler
mapped that data to the **terminal** `unavailable`. One network blip on any of `create_invoice`'s up to
four reads was a 503 the caller is told to page on, and nothing retried it. There is no safety reason
a read must not be retried: it writes nothing, and a re-executed closure's answer is exactly as fresh
as a first answer (the create step re-queries inside its own closure regardless).

Every read now runs under a third deployment-level run retry policy, the **read policy** (`[read]`,
`RunRetryPolicy::new()` field for field like the issue policy; defaults `max_attempts = 3`,
`5s → 30s`, factor 2, `max_duration = 2m`). The gateway's read fns return `Err(Unanswered)` — a plain
`std::error::Error`, retryable to the SDK, the read-side twin of `Unconfirmed` — when szamlazz.hu did
not answer (a transport or parse failure, `szlahu_down`), and every *answer* — a document, code 7,
3/135/136/164, another API code — as `Ok` data; the `Transport` variants of the journaled outcome
types are gone, and another API code on a read is the new data variant `Api{code, message}` (still
`unavailable` in the handlers that could not conclude from it, except `Szamlazz.Agent.query`'s 422
pass-through and the probe's `Accepted`, which were already data). Exhaustion of the read policy — or
a cancel mid-read (409) — is the `unavailable` fault, naming the step and the last `Unanswered`,
about the document when the step knows one (`{order, kind, external_id}` on the lookup step). The
storno-number hint after a verify that found the document already reversed stays best effort: a hint
the read policy could not get answered reports the reversal without the storno number rather than
failing a handler whose answer is already known.

The three policies are now the complete retry envelope of the worker: the issue policy for every
szamlazz.hu write, the read policy for every szamlazz.hu read, the resolve policy for the `account`
step; the credential fetch keeps its in-process retry. The handler timeouts do not change: a retryable
run failure is returned to the server with `next_retry_delay` and the invocation yields (the SDK does
not sleep in-process — `restate-sdk-shared-core` 7.0.3, `vm/transitions/journal.rs`), so a read's
retry delays are spent between executions and a failed read ends the execution it fails in; the
per-execution worst case — one read of at most two 60 s calls, or the create closure's three — is the
one `inactivity_timeout = 4m` / `abort_timeout = 3m` were sized for. Verified end to end: a lookup that
loses its reply once is re-executed under a 1 s test read policy and the create completes `issued` in
one invocation with one create on the wire and `last_failure_related_command_name = lookup-invoice`
while in flight; a lookup that never answers is a 503 `unavailable{order, kind, external_id}` with
zero creates. The accepted risk of #22 applies unchanged: the attempt count is not durable across every
replay, `max_duration` is the bound.

## Amended (#41): the `Szamlazz.Agent` writes' timeouts and retry interval

`Szamlazz.Agent.storno` runs the same three-call storno closure as `Szamlazz.Order.storno_invoice` (query, send,
re-query at up to 60 s each, ~180 s in the worst case) but carried `inactivity_timeout = 2m` /
`abort_timeout = 2m`, below that worst case: a slow storno was suspended and resumed mid-step — not lossy, the
re-execution's leading query finds a landed storno, but a full prologue replay and a needless round. It now
carries `Szamlazz.Order`'s `4m` / `3m`; the discovery test asserts them.

`Szamlazz.Agent.set_payments` had `max_attempts = 2, kill` with no `initial_interval`, so the one retry after a
crash ran on the server's ~500 ms default. Its send is **at-least-once** under `additive: true` — every send that
reaches szamlazz.hu appends the entries, and the handler cannot tell a lost reply from a lost request — so a retry
that fires while the first send is still in flight (the client waits up to 60 s) could append twice. It now
carries an explicit `initial_interval = "2m"`, longer than the client timeout; the `outcome_unknown` message is
conditional on `additive` ("query the invoice before re-sending" rather than "call set_payments again"), and the
hazard is stated on the request field and in both READMEs. Its timeouts stay `2m` / `2m` (one send).

Amended #41 also settled the 71/152 contradiction (design §5 step 4): a duplicate-order-number refusal with nothing
under the order was `Err(Unconfirmed::Contradiction)` and re-sent the create for up to five executions; it is now
`conflict{duplicate_order_number}` without `existing_number` on the first occurrence, logged at `warn` — the
refusal is an answer szamlazz.hu already gave.

## Amended (#61): the ≥ 90 s re-check rule lives in code

Every value above rests on one timing rule: nothing re-checks or re-sends within ~90 s of a send, because the
Számla Agent client gives up on a reply at 60 s and szamlazz.hu has been seen to stall for about a minute while
still issuing (behaviour notes; [ADR 0002](0002-order-keyed-idempotency-via-external-ids.md) states the "never below
~90 s" bound). Until #61 the rule held by convention, with two gaps.

`Szamlazz.Agent.storno` had `max_attempts = 2, kill` with no `initial_interval` — the same omission #41 closed on
`set_payments` — so the one retry after a crash (a rollout cutting the connection mid-storno) was re-dispatched at the
server's ~500 ms default while the first `xmlszamlast` could still be in flight; its re-execution's leading query
would then find nothing and send a second storno. Not lossy — szamlazz.hu answers a repeated storno with the existing
storno number — but a second send the rule forbids, and the endpoint README documented the 500 ms as the behaviour. It
now carries `initial_interval = "2m"` like every other write handler of both services, and the discovery test pins the
interval on both `Szamlazz.Agent` writes.

`WorkerConfig::validate` checked `max_attempts ≠ 0`, `initial ≤ max` and `factor ≥ 1`, so an operator could write
`issue.initial_delay = "5s"` and the endpoint started; the create step's lost-reply re-execution would then have raced
the original send. The issue policy's `initial_delay` now has a floor, `IssueConfig::MIN_INITIAL_DELAY` = the agent
crate's newly exported `client::REQUEST_TIMEOUT` (60 s) + `IssueConfig::RE_CHECK_MARGIN` (30 s) = 90 s, derived from
the client's constant rather than a second copy of "60 s"; a shorter delay is `WorkerConfigError::IssueDelayBelowFloor`,
whose message names the rule, and `--check-config` fails with it. The floor is on the issue policy only — a read
writes nothing and the resolve policy never reaches szamlazz.hu — and bites where the endpoint loads configuration:
the e2e suite's 1 s policies are built in Rust, handed to `from_parts` and never pass through `validate`.

## Consequences

- Kill is safe because there is nothing to compensate: the external-id query inside the create
  step is what the next call reconciles against — it finds whatever landed, live or reversed,
  before it considers sending.
- Caller contract, documented in the crate README: **any error from an issuing or storno handler
  means "outcome unknown — retry with a **new** `Idempotency-Key`, or read `Szamlazz.Order.get`"**,
  never "no document exists". A call that timed out on the client side may still run once the key
  frees; `get` is the way to learn its outcome. Callers should not long-poll an exclusive handler;
  `get` is the non-blocking status check. The same key would replay the stored
  `TerminalError{outcome_unknown}` for `idempotency_retention` (30 days). (#67 later scoped the
  rule to `outcome_unknown`, `unavailable` and `credentials_rejected`; the settled 4xx/422 faults
  are not "outcome unknown" — design §7.)
- Operations: alert on `sys_invocation` failed completions and on invocations in `backing-off` for
  more than 5 minutes; `idempotency_retention = 30d` keeps failed completions visible. Verify the
  effective policy with `GET /services/{name}`. The SDK endpoint speaks HTTP/2 only.
- Runbook: `Szamlazz.Order.get`, then re-call the same handler with a new key. Its lookup step
  reconciles by external id and only then does its create step send.
- In a pathological crash loop an episode executes the create closure at most (issue policy
  executions) + (invocation attempts − 1) = 9 times, each query-first — finite only because of kill
  and of the issue policy's `max_duration`.
- Because there is no child Restate service (ADR 0001), the "callee pauses and strands the parent"
  branch does not exist; the rule of thumb "no handler that `Order` awaits may pause" is trivially
  true.
