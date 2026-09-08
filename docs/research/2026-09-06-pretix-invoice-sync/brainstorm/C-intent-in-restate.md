# C, Intent in Restate: a `PretixOrder` Virtual Object owns the retry; no database

Ground truth: `brief.md`, `raw/01–03`. New claims **V** (cite) or **U**.

## 0. Variant

**(i) a Virtual Object wins.** (ii) Workflow: state lives only for the workflow's retention (V: docs *State*), so
"needs attention" expires; `run` is once per id and, awaiting for hours, pins its deployment (V: docs *Durable
timers*). (iii) Kafka / `/send?delay=`: Kafka never redelivers, a kill drops the event, no idempotency key (V:
raw/02 §8); a delayed send records no *why*, count or exit. VO state is indefinite (V: docs *State*).

## 1. Shape

**Sync-app deployment**: one Rust binary: axum (webhook + UI) plus a restate-sdk endpoint on the worker's Restate:

- **`PretixOrder`** VO, key `{EVENT-SLUG}-{code}` = the `Szamlazz.Order` key, under the organizer's scope. State:
  `intents: {op → Intent{target, n, next_at, horizon_at, ticket, state, last_fault{code, szamlazz_code, message, raw},
  result{number}, issued_at, attempts[]}}`; `attention` (own key, set only when `needs_attention`);
  `seen: [notification_id]` (bounded). **No buyer data in state**: the order is re-fetched per attempt.
  Exclusive handlers: `observe{organizer, event, code}`, `attempt{op, ticket}` (self-scheduled), `retry_now{op}`,
  `resolve{op, approve|dismiss}`; shared: `status` (never blocks; V: brief).
- **`PretixOrganizer`** VO, key = organizer: `cursor` (`X-Page-Generated`) and a `send_after` loop (docs *Cron*
  pattern) over `orders/?modified_since&testmode=false` (V: raw/01 §2), `observe`-ing each.

**Webhook**: `/restate/scope/{scope}/send/PretixOrder/{key}/observe`, `Idempotency-Key: pretix:{notification_id}` → 202
→ **200 to Pretix**. Duplicate ids collapse (V: raw/02 §2); ingress down → 5xx, Pretix retries 59 h (V: raw/01).

**`observe`**: `ctx.run` fetch → planner (from `status`, `payments`, `refunds`, `invoice_address.last_modified`; never
`action`) → upsert intents; a new `pending` intent gets `n=0, horizon_at, ticket` and one `send_after(attempt, 0)`.
Address change on a live invoice → `proposed` (storno + reissue), no send.

**`attempt`**: stale ticket → return. `now ≥ horizon_at ∨ n ≥ 8` → `needs_attention`. Else `n += 1` (`ctx.set`
**before** the call), re-fetch Pretix, still warranted? (else `superseded`), then
`ctx.request(Szamlazz.Order/{key}/{op}, input).scope(ctx.scope()).idempotency_key("pretix:{key}:{op}:{intent}:{n}").call()`
raced with `ctx.sleep(2h)` (`Request::{scope, idempotency_key, call, send_after}` V: docs.rs restate-sdk 0.12;
`ctx.scope()` V: `crates/restate-szamlazz/src/service/support.rs:499`). Outcome → §2; `retrying` writes `next_at`, a
new `ticket`, exactly one `send_after(attempt, delay)`. Self-sends, not sleep: a sleeping exclusive handler queues
every other call to the object (V: docs *Durable timers*).

**Manual**: `retry_now` → `next_at = now`, new ticket, `send_after(0)`; `resolve{approve}` → two chained `pending`
intents; `dismiss` clears `attention`.

**UI**: per order `status` beside a live `Szamlazz.Order.get`; org-wide
`select scope, service_key, value_utf8 from state where service_name='PretixOrder' and key='attention'` (V: docs *SQL
introspection*). A sync-app cron sweeps `key='next_at' and value_utf8 < now` → `retry_now` (§3).

## 2. Failure handling

| Child answer | Class | Parent |
|---|---|---|
| 200 `issued`/`already_issued`/`reconciled`; storno `reversed` | ok | `done{number, issued_at}` |
| `unavailable` | transient | `retrying`, n+1 (a fault → rotate; V: brief) |
| 2 h timeout | no answer | `retrying`, **same n**, the next call attaches (V by source, raw/02 §1; service-to-service **U**) |
| `outcome_unknown`; kill = `TerminalError` whose message is not `{code,…}` JSON | ambiguous | `get` (shared): live → `done`; else `retrying`, n+1 |
| `credentials_rejected` | account | `needs_attention{account}` |
| `invalid_input`, `unknown_account`, `not_found`, `account_mismatch`, `szamlazz_error`, `rejected`, `conflict{…}`, create → `reversed` | settled | `needs_attention{reason}` |

Schedule `1, 5, 15, 30 m, then 60 m`; `horizon_at` = created + **4 h** for `create_invoice` (`haladéktalan`, V: raw/03
§5), 24 h otherwise. **Exit**: every `attempt` ends `done | superseded | needs_attention | retrying`, and `retrying`
requires `n+1 ≤ 8 ∧ next_at ≤ horizon_at`; `n` is K/V written before the call, so a crash-replay does not re-count,
a hard bound, unlike the SDK's run attempt count (V: brief, ADR 0004). Misclassification costs ≤ 8 calls, never a
duplicate: the create step is query-first (V: brief). **Human**: the attention list (scope, order, op, age,
`last_fault`, `attempts[]`); retry, approve, dismiss, fix Pretix data.

## 3. Scenarios

- **Worker down 1 h, burst.** `observe`s run; each `attempt` suspends on its child; children kill at ~24 min (V:
  brief) → raw 500 → `get` fails too → n+1; parents idle between timers; after recovery one call each, inside 4 h.
- **szamlazz.hu down 1 h.** The child's issue policy absorbs it; worst case `unavailable` → n+1 → success.
- **Poison child after bad deploy.** ~24 min per attempt → ≤ 8 reach attention with one `last_fault`; fix, redeploy,
  bulk `retry_now`. Kill releases the child key; `get` keeps answering (V: brief).
- **Intent service bug.** A panic spends *its* attempts → kill; state written before it stays; the cron sweep
  re-kicks overdue intents. Undecodable state fails every handler on the key until fixed (`restate kv edit`, V: docs
  *Introspection*), so state types are additive-only with fixtures (ADR 0005 §47). No unbounded loop: `n` moves
  only inside an `attempt`, bounded by §2.
- **Wrong buyer data.** `modified` → `proposed` → approval → storno; `reversed` releases `create_invoice{reissue:true}`;
  `conflict{live}` → already reissued → `get` → `done`.
- **Manual retry while waiting.** The VO is *idle* between attempts: `retry_now` runs at once, the stale ticket
  makes the old timer inert. During an in-flight `attempt` it inboxes ≤ 2 h; `status` still answers.
- **Webhook twice / out of order.** Same id → same invocation; different ids serialize on the key; the planner
  reads `status`, so a late `placed` on a `p` order plans no proforma.
- **Restate data lost.** `PretixOrganizer` lists `status=p, created_since −30 d`; every `observe` plans afresh; the
  worker answers `already_issued` for what exists (V: brief). Lost: `attempts[]`, the §220(3) defence.

## 4. ADR 0005 / ADR 0001

Not ADR 0005's ledger: that duplicated *document* truth inside `Szamlazz.Order`. `PretixOrder` records *Pretix's*
facts (an invoice should exist; buyer data changed) and its *own* (attempts, faults, horizon): nothing szamlazz.hu
can answer. `result{number}` is a cache; `get` wins, the UI shows both. ADR 0001 rejected `Order` calling a *child*
whose pause could hold `Order`'s key; here `Order` is the child, always completes (kill, ADR 0004), and nothing calls
upward, the dependency direction ADR 0001 wanted. Deadlock needs a cycle or cross call (V: docs *Service
communication*); there is none.

## 5. Worker changes / deployment

Worker: **none**. Keep `kill`: `pause` would freeze the parent's await forever (V: raw/02 §1). Sync-app deployment:
the binary, organizer config `{scope, Pretix token}`, the scoped-VO flag (already on), sole network path to port 9070.

## 6. Honest weaknesses

- **Restate as the attention store**: SQL on the unauthenticated admin port (V: raw/02 §7); no index on `value_utf8`,
  so the list scans all `PretixOrder` state, fine at 10⁴ orders, **U** beyond; indefinite state needs compaction.
- **Scope on the child call is explicit**; implicit inheritance **U**, forgetting it hits the unscoped account
  (`unknown_account`, settled, visible). Scoped VO calls need the experimental flag (V: docs *Flow control*).
- **Two deployments, two policies**; in-flight `attempt`s pin the sync-app deployment ≤ 2 h, drain before deploy.
- **PII** in the parent's `ctx.run` result and `Call` entry (short `journal_retention`), never in state.
- Parent-awaits-child holds the parent key ≤ 2 h; same-key attach and kill text as `TerminalError` are V by source
  only.

## 7. Effort

Two VOs + planner ≈ 1.2 k lines (planner shared with any design), Pretix client 150, webhook/UI/SQL ≈ 600, state
fixtures + e2e, ~3 weeks one engineer; UI +1 week; worker 0. Ongoing: additive-only state types; the planner
tracks Pretix.
