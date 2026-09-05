# Exhausted retries kill the invocation instead of pausing it

Status: partially superseded by [ADR 0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md);
amended by #22 (the create step under a run retry policy, below).
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
the leading external-id query, the create, a re-query, 60 s each). `Szamlazz.Agent.set_payments` and `storno` use `max_attempts = 2,
kill`; read-only handlers (`Szamlazz.Order.get` with `verify`, `Szamlazz.Agent.query`) may retry more freely
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
`Szamlazz.Agent.storno` alike. The issue policy is now the only retry envelope in the crate; no attempt
counter or sleep remains.

## Consequences

- Kill is safe because there is nothing to compensate: the external-id query inside the create
  step is what the next call reconciles against — it finds whatever landed, live or reversed,
  before it considers sending.
- Caller contract, documented in the crate README: **any error from an issuing or storno handler
  means "outcome unknown — retry with a **new** `Idempotency-Key`, or read `Szamlazz.Order.get`"**,
  never "no document exists". A call that timed out on the client side may still run once the key
  frees; `get` is the way to learn its outcome. Callers should not long-poll an exclusive handler;
  `get` is the non-blocking status check. The same key would replay the stored
  `TerminalError{outcome_unknown}` for `idempotency_retention` (30 days).
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
