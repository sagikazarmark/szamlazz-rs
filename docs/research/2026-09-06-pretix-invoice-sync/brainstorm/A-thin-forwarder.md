# A — Thin forwarder; Restate owns the retry

Brainstorm, 2026-09-06. Facts marked **V** (source) or **U**; the rest is design.

## 1. Shape

Components: (a) **webhook handler**; (b) **operator pages** rendered live from Restate's SQL API (stuck / failed / refused lists, an order page, an on-demand audit page); (c) a **notifier** cron. No database; configuration only: organizer → `{scope, pretix token}`, Restate URLs.

Webhook handler, per POST:
1. Parse `{organizer, event, code}`; no local dedupe.
2. `GET …/orders/{code}/` (V: raw/01 §2); decide from **order state**, never `action` (V: raw/01 consequences): `p` → `create_invoice`; `n` with a pending transfer → `create_proforma`; `c` unpaid → `delete_proforma`; refunds `done` = `total` → `storno_invoice`; `modified`, partial refunds, `changed.*` → **no action**, a person decides (a correction needs judgment).
3. `POST /restate/scope/{scope}/send/Szamlazz.Order/{EVENT-SLUG}-{CODE}/{handler}`, `Idempotency-Key: pretix:{notification_id}`.
4. 202 → 200 to Pretix. Anything failing before the 202 → 5xx; Pretix retries ~59 h under the same id (V: raw/01 §1) — the only retry needed before Restate holds the intent.

`/send`, never `/call`: a `/call` on a pausing invocation waits forever (V: raw/02 §1) and would starve a burst behind Pretix's 30 s timeout (V: raw/01 §1).

Manual trigger: a button on the sync app's order page (Hosted runs no third-party plugins — V: raw/01 §4). It queries `sys_invocation` for in-flight invocations on the key; if none, `/send` with `Idempotency-Key: manual:{uuid}` and polls `GET /restate/output/{id}` ~10 s (470 while running — V: raw/02 §1).

State: none. The intent is the invocation, kept 30 d (`idempotency_retention`; V: brief); the truth is szamlazz.hu via `get`.

## 2. Failure handling

Two Restate envelopes hold "try again", both deployment decisions (V: raw/02 §6):

- **Run policies widened to hours** — `[read]`/`[issue]`, e.g. `max_delay 10m`, `max_duration 12h`. A szamlazz.hu outage is ridden inside the step, spending no invocation attempts (V: brief). The invocation is `backing-off` in `sys_vqueues` but `ready` in `sys_invocation` (V by source, U end to end: raw/02 §7).
- **`on_max_attempts = pause`**, `max_attempts` 5 → ~12 (≈1.6 h at `2m ×2 → 10m`) for worker-side failures. Paused is Sidekiq's Dead set: frozen, resumable, holding the key (V: brief; raw/03 §6).

| Class | Automatic | Human sees | Human does |
|---|---|---|---|
| Transient — no answer, `unavailable` inside the horizon | run retries → attempts → pause | *Stuck*: `status='paused'` ∪ `sys_vqueues` backing-off; key, handler, `last_failure`, age | resume (`?deployment=latest` after a fix); cancel (resumes for compensation — V: raw/02 §5); kill |
| Ambiguous — `outcome_unknown`, `unavailable` after 12 h | **completes** as a fault; nothing retries it | *Failed*: `completion_result='failure'`; `{code,message}` via `GET /restate/output/{id}` (V: raw/02 §2) | `get` on the order page; *issue now* with a new key — safe, the create step is query-first (V: brief) |
| Settled — 4xx/422, `conflict`, `rejected` | completes; `conflict`/`rejected` are 200s | *Refused*: completed successes whose `/output` body is `conflict`/`rejected` — one `/output` per row, so a 24 h window | fix Pretix data, *issue now*; or *storno + reissue* |

The notifier runs the three queries and mails on non-zero — the only unprompted path to a person; Restate pushes nothing (U, a negative: raw/02 §7). The admin port is unauthenticated (V: raw/02 §7): operator surfaces, not organizer ones.

## 3. Scenarios

**Worker down 1 h, burst.** Hundreds of `/send`s are accepted; attempts fail on connection refused; 12 attempts outlast the hour and the burst drains unattended once the worker is back. A 3 h outage pauses all; one `restate invocations resume Szamlazz.Order` clears it (V: raw/02 §4; bulk `deployment=`: U).

**szamlazz.hu down 1 h.** `Unanswered`/`Unconfirmed` → run retries every ≤10 min for up to 12 h; nothing pauses, nobody acts; each re-execution is query-first (V: brief). A UI `get` during the outage hangs under the same `[read]` — see §4.

**Poison invocation after a bad deploy.** Undecodable journal spends attempts (V: brief) → pause, `last_failure` naming it. Fix, deploy, `resume ?deployment=latest` (V: raw/02 §4). Unfixable → kill (releases the key; V: raw/02 §5), then *issue now*.

**Wrong buyer data.** `modified` does nothing automatically. *Storno + reissue* on the order page: `/call storno_invoice` (30 s client timeout), then `/call create_invoice {reissue: true}` with the address re-read from Pretix. On a live invoice the flag is `conflict{live}`, so a mis-ordered pair cannot duplicate (V: CONTEXT.md *Reissue*).

**"Issue now" while paused.** The manual `/send` would inbox behind the paused invocation until resume/cancel/kill (V: brief). The button refuses: "blocked by X, paused since T: `<last_failure>`", offering those three actions; after resume the manual call answers `already_issued`.

**Duplicate / out-of-order.** Same `notification_id` → same key → `PreviouslyAccepted`, same id (V: raw/02 §2). `paid` before `placed`: both fetches see `p`, both map to `create_invoice` under different keys; the VO serializes them, the second answers `already_issued`.

## 4. Worker changes

**Must:** `on_max_attempts = "pause"`, `max_attempts ≈ 12` on the `Szamlazz.Order` writes and `Szamlazz.Agent.storno` (`crates/restate-szamlazz/src/service/handlers.rs:45`; discovery test pins `kill`, `service/tests.rs:74`); widen `[read]`/`[issue]`; **split the read policy** so interactive reads (`get`, `Szamlazz.Agent.query`, `check_account`) keep 5 m while reads inside write handlers get the widened one — else every UI refresh in an outage spawns a 12 h invocation. Amend ADR 0004.

**Must not:** make `unavailable`/`outcome_unknown` retryable (they would spend attempts and blur the contract); add state, list/scan, callbacks or per-request retry knobs.

## 5. Honest weaknesses

- **No intent row.** An order whose webhook never reached `/send` (Pretix gave up at 59 h, no Celery broker, forwarder down) exists nowhere; only the audit page (Pretix `status=p` × `get`) finds it.
- **Compliance clock.** Card-paid orders are `haladéktalan` (V: raw/03 §5). Unattended horizon ≈ 12 h + 1.6 h; past that a fault sits in a table for 30 d, seen only if the mail is read. Paused invocations are invisible to organizers.
- **The unexitable loop** is bounded by construction (≤ 12 h, ≤ 12 attempts, then pause). The real risk is a *forgotten* pause — or an auto-resume cron re-running poison invocations every ~1.6 h forever.
- **A paused write blocks the order**: later storno, correction, manual button all inbox until a person acts — days if unnoticed.
- `sys_invocation` misreports backing-off as `ready` (U end to end); pages must join `sys_vqueues`.
- The Pretix-state → operation decision survives only as invocation input (3 d journal).

## 6. Effort

Sync app (Rust, axum, no DB): webhook + Pretix → `CreateRequest` mapping ~350 lines; Pretix client ~150; Restate client (`/send`, `/call`, `/output`, `/query`, resume/cancel/kill) ~200; three lists + order page with four actions ~500; notifier ~50; config ~50; audit page ~150 — ≈1.4 k lines. Worker: attributes on nine handlers + discovery test, read-policy split ~100 lines + tests, ADR amendment — one small PR.
