# Exhausted retries kill the invocation instead of pausing it

Restate's server-wide default retry policy is `initial-interval 500ms`, factor 2, `max-interval 1m`,
`max-attempts 70`, `on-max-attempts pause` — about an hour of back-off followed by an indefinite
pause awaiting a human (the reference page; the guides disagree with each other on the initial
interval, so the policy is pinned in code and never left to the default). `on_max_attempts` is a
per-handler setting in Rust SDK 0.12 and the server honors it (verified: `GET /services/{name}`
shows the effective `retry_policy`; the `POST /deployments` response shows `null`s and is not
authoritative).

Every handler that calls szamlazz.hu sets `on_max_attempts = "kill"`. On `Order` the issuing,
correcting, storno and delete handlers carry `invocation_retry_policy(initial_interval = "2m",
factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")` with
`inactivity_timeout = "4m"` and `abort_timeout = "3m"` (the closure may take up to 180 s: external-id
query, hint, create, 60 s each). `SzamlaAgent.set_payments` and `storno` use `max_attempts = 2,
kill`; read-only handlers (`Order.get` with `verify`, `SzamlaAgent.query`) may retry more freely
because queries are safe to repeat, but they kill too. The `pending` slot written before
any issuing call (ADR 0002) is what makes kill safe; kill is what makes the slot reachable.

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
  is already held ≈ 39 minutes in a full outage. The split between attempts and episodes is the
  service's; the total is the server's.

## Consequences

- Kill is safe because state is the last committed `ctx.set`: the slot is `pending` with its
  generation, external id, `request_id`, `attempts` and `last_attempt_at`; there is nothing to
  compensate. The next call on the order pre-sleeps for the remainder of the first back-off and
  reconciles by external id before it considers sending.
- Caller contract, documented in the crate README: **any error from an issuing or storno handler
  means "outcome unknown — call again with the same `request_id`, or read `Order.get`"**, never "no
  document exists". A call that timed out on the client side may still run once the key frees;
  `get` is the way to learn its outcome. Callers should not long-poll an exclusive handler; `get`
  is the non-blocking status check. Callers should not rely on the ingress `Idempotency-Key`: it
  would replay `TerminalError{outcome_unknown}` for the configured `idempotency_retention` (7 days).
- Operations: alert on `sys_invocation` failed completions and on invocations in `backing-off` for
  more than 5 minutes; `idempotency_retention = 7d` keeps failed completions visible; `get` surfaces
  `pending` slots with `attempts`, `external_id` and `last_attempt_at`. Verify the effective policy
  with `GET /services/{name}`. The SDK endpoint speaks HTTP/2 only.
- Runbook: `Order.get` → re-call the same handler with the same `request_id`. It reconciles by
  external id first and resumes issuing while the attempt budget lasts; a new `request_id` takes
  over an exhausted `pending` slot with a fresh budget at the same generation. `record_reversal` and
  `forget` (`ingress_private`) exist for the account-mismatch and persistent-transport cases and are
  expected to be used almost never.
- In a pathological crash loop an episode executes at most loop attempts + (invocation attempts − 1)
  = 9 closures, each query-first — finite only because of kill.
- Because there is no child Restate service (ADR 0001), the "callee pauses and strands the parent"
  branch does not exist; the rule of thumb "no handler that `Order` awaits may pause" is trivially
  true.
