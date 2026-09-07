# B — Intent ledger + reconciler: the sync app owns the retry, the worker keeps kill

Ground truth: `brief.md`, `raw/01–03`. New claims: **V** (cite) or **U**; contract facts cite
`crates/restate-szamlazz/src/contract/*.rs`.

## 1. Shape

**Components** (one binary): webhook receiver; **planner** — a pure function `plan(pretix_order, ledger_rows) →
desired ops`, shared by webhook, reconciler and manual paths (the webhook is only a hint, raw/01);
**drain loop**; **reconciler** (cron); Postgres; a small UI (self-hosted Pretix `order_info` panel optional, raw/01 §4).

**Ledger** — `intent`, one *open* row per `(scope, order_key, op, target)` (partial unique index): `op ∈
create_proforma | create_invoice | delete_proforma | storno_invoice | correct_invoice`, `target` (invoice number /
correction_id), `input_json` (the exact `DocumentInput`), `pretix_snapshot {order.last_modified, status, total,
invoice_address.last_modified}`, `state, attempts, horizon_at, next_attempt_at, current_key, current_invocation_id,
last_fault {code, szamlazz_code, message, raw_500_text}, result {number, storno_number}, depends_on, source,
approved_by`. Child `attempt(n, key, sent_at, ended_at, status, body)` — the §220(3) "acted as expected" log
(raw/03 §5). `webhook_receipt(notification_id PK)`.

**States**: `proposed` → `pending` → `in_flight` → {`done`, `retrying`, `checking`, `needs_attention`};
`retrying` → `pending` at `next_attempt_at`; `checking` → `done | pending`; `needs_attention` → `pending` **only by a
human or reconciler evidence**; `superseded`.

**Webhook handler**: per-organizer secret URL (Pretix signs nothing — **U**); `INSERT webhook_receipt ON CONFLICT DO
NOTHING`; enqueue `replan(org, event, code)`; **200**. No Pretix fetch, no worker call (raw/03 §1).

**Replan** (a drain job): fetch the order (raw/01 §2), run the planner, upsert intents. Rules: `n` + transfer →
`create_proforma`; `p` → `create_invoice{paid, proforma:auto}` (README table); `c|e`, no invoice →
`delete_proforma`; `c`, live invoice → `storno_invoice`; partial refund `done` → `correct_invoice{correction_id =
refund id}`; `invoice_address.last_modified > snapshot` with a live invoice → `storno_invoice` + `create_invoice
{reissue:true}` chained, `proposed` unless the organizer opted in.

**Drain loop**: `SELECT … FOR UPDATE SKIP LOCKED WHERE state='pending' AND next_attempt_at<=now()`, one `in_flight`
row per `order_key`. Per attempt: `key = {intent_id}:{attempts+1}`; `POST /restate/scope/{scope}/send/Szamlazz.Order/
{key}/{op}?limit-key={event_slug}` (a crash-repeated `/send` is `PreviouslyAccepted`, same id, raw/02 §2 **V**); poll
the scoped `POST /restate/output` at 1, 2, 5 s, then 30 s: 470 → wait; result → §2; 404 → ambiguous. `/send`+`/output`
rather than `/call` so "no answer" is not a special case. Worst case ~1 h issue policy + ~24 min invocation budget, so
a row polls up to **2 h**, then `needs_attention{stuck, invocation_id}` — never a second attempt while one is open.

**Reconciler**: every 15 min per organizer `GET /organizers/{org}/orders/?modified_since={X-Page-Generated}&
testmode=false` → replan each (raw/01 §2 **V**). Nightly, orders paid/cancelled in the last 30 d: `get`; expected slot
absent → new intent; live with no `done` row → backfill `done`; reversed where `done` →
`needs_attention{reversed_externally}`. Cost: one szamlazz.hu read per order per night — etiquette bound **U**.

**Manual trigger**: *retry now* = `next_attempt_at=now` on a parked row (a flag); *issue / storno / reissue now* =
replan with `source=manual`, or a forced row with `approved_by` (an intent).

## 2. Failure handling

| Worker answer | Class | Ledger |
|---|---|---|
| 200 `issued`, `already_issued`, `reconciled`; storno `reversed` | ok | `done` |
| `unavailable` 503 (a completed fault → rotate key, brief) | transient | `retrying`, fresh key |
| no answer: 470 past 2 h, ingress unreachable | transient | keep key / poll; breaker |
| `outcome_unknown`; killed raw-text 500 (`error-source: invocation`, unparsable body); `/output` 404 | ambiguous | `checking`: `get` (shared, never blocks) → live → `done`; else `retrying`, fresh key |
| `credentials_rejected` | account | `needs_attention{account}`, trips the breaker |
| `invalid_input`, `unknown_account`, `not_found`, `account_mismatch`, `szamlazz_error`; `rejected`; any `conflict{…}`; create → `reversed` | settled | `needs_attention{reason}` |

`get` in the ambiguous branch is diagnostics, not safety: the create step is query-first (brief), so re-sending after
`outcome_unknown` can only answer `issued`/`already_issued`.

**Schedule / horizon**: `1, 5, 15, 30 m, then 60 m` until `horizon_at` = created + **4 h** for `create_invoice`
(card-paid is `haladéktalan`, raw/03 §5), 24 h for other ops; then `needs_attention`. **Exit guarantee**: every attempt
ends `done`, `needs_attention`, or `retrying` with `next_attempt_at ≤ horizon_at`; `attempts` is monotone; nothing
leaves `needs_attention` without a human or `get` evidence. A settled fault misclassified as transient costs ≤ 8
wasted calls — never a loop, never a duplicate. **Account breaker**: > 50 % of an account's attempts in 10 min
killed / `unavailable` / unreachable → pause its sends 15 m, one alert.

## 3. Scenarios

*Worker down 1 h, payment burst.* Webhooks ack in ms; rows queue. Ingress down: `/send` fails, no key consumed,
breaker trips. Endpoint down: kill at ~24 min (brief) → raw 500 → `checking` → `get` unanswered → breaker. After
recovery one attempt per row succeeds; `limit-key` per event paces the flood.

*szamlazz.hu down 1 h.* The issue policy absorbs up to 1 h (brief); rows poll 470. Those that exhaust get
`unavailable` → fresh key in 15–30 m → succeed. Inside the 4 h horizon.

*Poison invocation after a bad deploy.* Killed 500 → `checking` → absent → retry; ≤ 3× then
`needs_attention{killed}`; the breaker trips earlier. The attention list groups by fault text; fix, redeploy, bulk
*retry now*. Kill releases the key (brief), so `get` and manual ops keep working.

*Wrong buyer data.* Two chained rows, `proposed` by default: `storno_invoice{number}`, then `create_invoice
{reissue:true}` (README). Storno `reversed` releases the reissue; `conflict{live}` on it → someone already reissued →
`checking` → `done`. A storno is a legal act, so the default asks a human.

*Manual "issue now" during an in-flight attempt.* One open row per order: the button shows "attempt n in flight" and
queues a *retry after*. The VO key would serialize them anyway (brief).

*Webhook twice / `paid` before `placed`.* `webhook_receipt` dedupes; the planner reads `status`, never `action`
(raw/01). `paid` first → `create_invoice`; the late `placed` replans a `p` order → no proforma intent.

*Ledger lost.* Accounts from config; org-wide order list; per order `get` → live slots become `done` rows;
`Szamlazz.Agent.query{invoice_number}` gives `issue_date` (contract/agent.rs **V**) to re-seed drift detection;
expected-but-absent → new intents. Lost: attempt history.

## 4. UI

**Attention list**: tabs *retrying / needs attention / proposed* (Sidekiq's split, raw/03 §6); columns state, order
(Pretix link), op, age, last fault, attempts; actions *retry now*, *mark resolved*, *approve*, *open in Pretix*.
**Order page**: ledger rows + attempt log beside a live `get`; buttons issue / storno / reissue / correct. **Badge**
(`order_info` on self-hosted; hosted: the sync app's page): `invoiced SZ-…` / `pending` / `needs attention`.

## 5. Worker changes

Required: **none** — `get`, the eight faults, `conflict` reasons and `/send`+`/output` suffice; Pretix is the
enumerator, so no `list`. Nice-to-have: `issue_date` on `get` slots. Must **not**: switch `Order` writes to `pause`
(a paused invocation holds the key and 470s forever — "stuck" would become normal); add state to `Order`; cut
idempotency retention below the 2 h poll.

## 6. Honest weaknesses

- Three new moving parts (DB, queue-on-a-table, cron) and a planner holding the whole Pretix → document mapping —
  the riskiest code; a planner bug parks every order at once.
- Double bookkeeping vs ADR 0005: the ledger records *intent and attempts*, not document truth — `done` is always
  szamlazz.hu's answer and on disagreement `get` wins (done+absent → reopen; absent+live → backfill). But
  `pretix_snapshot` *is* state szamlazz.hu cannot return (no buyer data in `get`/`query`, contract **V**).
- Fresh key per attempt: ≤ ~10 stored completions per row for 30 d — negligible (**U**); no stale-failure replay.
- Card payments: seconds normally, up to 4 h unattended; "haladéktalan" has no number (raw/03 **U**).
- A document issued outside the ledger is unseen until the nightly `get`; a create then parks as `conflict{foreign}`
  or completes as `already_issued` — safe, a day late.

## 7. Effort

Planner + ledger + drain + reconciler ~3–4 weeks (one engineer); UI 1–2 weeks; Pretix plugin panel +1 week
(self-hosted only). Worker: 0. Ongoing: the planner tracks Pretix semantics.
