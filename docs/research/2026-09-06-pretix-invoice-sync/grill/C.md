# Grill: C — intent in Restate (`PretixOrder` Virtual Object)

Adversary's read of `brainstorm/C-intent-in-restate.md`; A, B, both grills as evidence. **V** = verified, **U** = unverified. New sources: restate-sdk 0.12.0 and restate 1.7.8 source (`resolve_call_request`, `handle_duplicated_requests`, `vqueue_enqueue_state_mutation`); docs *Flow control*, *SQL introspection*, *Versioning*.

## 1. The 2 h await

`restate_sdk::select!` races durable futures (V). The loser is **not** cancelled: the child runs on, its completion lands on a finished parent and is dropped — harmless; cancelling would be worse (cancel mid-create = `outcome_unknown`, V: ADR 0004). "Same n attaches" **holds**: a service-side `Call` with an idempotency key derives the ingress's deterministic id (V: `InvocationId::generate` in `resolve_call_request`) and `handle_duplicated_requests` appends the sink to any in-flight status regardless of source (V); a completed one replays its result, fault included.

Costs: `observe`, `retry_now`, `resolve` inbox ≤ 2 h; the child's real bound is ≈ 2 h 15 m (grill/B §4) — derive it from discovery. **Unstated, worse**: a parent *killed* mid-await (sync-app bad deploy) leaves the intent `in_flight` with a stale `next_at`; only the cron (§6) revives it. **Repair**: never await. `attempt` = `send()` with the key, `send_after(check, 5m)`; `check` peeks the scoped `POST /restate/output` inside a `ctx.run` (470 → reschedule, V: raw/02). Key held for milliseconds; §3 and §6 shrink with it.

## 2. Scope

`Request::scope` exists (V). **No inheritance**: `Request::new` sets `scope: None` and the SDK forwards exactly that (V); the server resolves the child's target from `cmd.scope` alone (V). Every internal call must set it — child call, `PretixOrganizer → observe`, and the self `send_after(attempt)`. The last is the trap: a missing `.scope()` schedules `attempt` on the **unscoped** instance — empty state, "stale ticket → return" — and the intent stalls silently. Missing on the child call it is `unknown_account` on the multi shape (V) and *correct by coincidence* on the single shape, breaking at the flag day. Cross-scope calls are allowed (V). The sync app was already the scope authority (it builds the ingress path); C adds internal plumbing ADR 0006 §6 never covered. **Repair**: a `scoped_send` helper copying `ctx.scope()`, terminal on `None` in multi mode; an e2e assertion.

## 3. Two deployments

A 2 h `attempt` pins the sync-app deployment (V: docs *Versioning*); the drain now covers two services, during which the webhook `/send`s into a private service (Pretix retries 59 h — survivable; the manual UI is not). Every release is a versioning event; B's binary deploys any time. Same three flags — no new dependency, but the flagless contingency would touch two services. **Repaired by §1**.

## 4. State growth and the attention query

`state` has `scope, service_key, key, value_utf8` (V); push-down on the scan U. 5×10⁵ rows per UI load at 10⁵ orders: unacceptable as written. **Repair**: an `attention` map on a per-organizer VO (not the reconciler's, so a Pretix fetch never blocks a flag), fed by one-way sends — a burst is hundreds of ms-level `ctx.set`s. Compaction: `observe` clears old `done`; a nightly sweep clears `done`-only keys — safe because `done` is re-derivable from `get`; **`dismissed` is not** (grill/B §6), keep it. Organizer = scope puts parent + child on one partition (docs warn on low cardinality, V) — fine at hundreds/min.

## 5. State type evolution

Heavier than admitted. A journal break hits in-flight invocations; a K/V break hits **every key**: `ctx.get::<T>()` decode failure is `ErrorInner::Deserialization` → 500, **retryable** (V), so every handler on 10⁵ keys burns its attempts and is killed. B's `ALTER TABLE` is one transaction. Same discipline the owner already runs; blast radius orders of magnitude larger. **Repair**: read `Json<Value>` first, convert with fallback to `needs_attention{undecodable}`.

## 6. The cron sweep

Not a net: after §1 it is **load-bearing**, and it is B's drain loop over an unauthenticated port. Loop risk: an intent that dies before its `n` write is re-kicked every sweep forever — C's exit proof covers `attempt`, not the kicker. **Repair**: §1's `check` cadence removes the need; any remaining sweep counts `kicks` in state, stops at 3. `PretixOrganizer`'s self-loop needs a boot kick under a fixed key (attaches if alive, V), else a panic before its `send_after` ends it silently.

## 7. Poison in the parent

`PATCH /services/{svc}/state` takes `object_key`, `scope`, full `new_state` (V); bulk = N PATCHes scripted over `state`, each queued on the VO's vqueue as an exclusive entry (V), waiting behind a held key. §5 makes it moot.

## 8. PII

`observe` journals the **whole Pretix order** (email, attendees) as a run result, and the `Call` entry repeats the buyer block — ADR 0001's rejected "journaled three times". Exposure > B-repaired (stamps only). Expiry is automatic; erasure is `purge` per invocation found by `target_service_key` (V) vs B's `DELETE`. **Repair**: project inside the run; build the request inside `attempt`'s run; `journal_retention` 1 d.

## 9. Classification

Parent sees `TerminalError{status, message}`; worker faults are `new_with_code(status, json)` (V: `support.rs:196`). Holes: JSON with an unknown `code` (a ninth `TerminalCode`) — must be `needs_attention`, C is silent; non-JSON — kill text (500), `killed`/`canceled` (409, V) → `get`; a child target the server cannot resolve (worker deployment deprecated) fails the parent's *command* — a parent kill, not a fault. Settled faults never spend attempts. Rule: unknown → `needs_attention`, never `retrying`.

## 10. Testing

`plan`, `classify`, `next(intent, outcome, now)` are pure if the handler is a thin shell — C should say so. No in-memory `ObjectContext` in the Rust SDK (U): durable paths need Restate in Docker, but the owner's harness (`tests/service.rs`) already binds the real `Szamlazz.Order` and can add `PretixOrder` with a Pretix wiremock. B mocks the worker over HTTP but needs Postgres in Docker and must test its own loop's crash points.

## 11. Steel-man

Restate already runs with the flags; no second stateful system; timers, dedup, lock and count are the engine's (grill/B §1); one workspace shares `contract` types; `get` stays the truth; ADR 0001's stuck-parent case inverts because `Order` kills.

## Verdict

**C survives, modified.** One structural fault — the 2 h await (holds the key, pins the deployment, orphans on parent kill, makes the cron load-bearing) — and three sharp edges: no scope inheritance (V), K/V poison as a retry storm (V), a full-table attention scan. All repair inside the design: send-and-peek, a scope helper, `Value`-first reads, a per-organizer attention map. Modified C is B's schema and table on VO state, driven by `send_after`, without Postgres.

| | **C (modified)** | **B (modified)** |
|---|---|---|
| Moving parts | 2 VOs + planner + webhook/UI | planner + drain + sweeper + reconciler + Postgres + cron |
| Ops burden | one more service on the same Restate; journal + K/V additive rule; admin SQL for lists | migrations, backups, pool, crash-correct loop |
| Testability | pure planner/classifier; e2e on the existing harness | pure planner; loop under HTTP mocks; Postgres in Docker |
| Failure visibility | attention map + `status`; attempts in state | SQL; attempt log; per-organizer reporting trivial |
| Compliance clock | `horizon_at` flag + `give_up_at` (grill/B §5), durable timers | same rule, cron-driven |
| PII | journals ≤ 1 d, none in state; purge per invocation | stamps only; `DELETE` |
| Effort | ~3 wk + 1 wk UI + ~1 wk repairs | ~3–4 wk + 1–2 wk UI; sweeper/OCC unbudgeted |
| Lock-in | experimental flags, SDK K/V shape, admin SQL | Postgres schema; Restate as executor only |

Recommend **C with the four repairs**; B only under grill/B §10 (self-hosted plugin, or per-organizer §220(3) reporting).
