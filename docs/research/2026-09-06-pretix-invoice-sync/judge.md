# Judge: Pretix → szamlazz.hu invoice sync — who owns "try again"

Inputs: the brief, `raw/01–03`, `brainstorm/A–C`, `grill/A–C`, ADRs 0001/0004/0005, `CONTEXT.md`, the endpoint README. ✓ = re-verified against Restate v1.7.8 source.

## 1. Answer to the two-way framing

It is not "UI vs retry forever"; both are needed. The retry belongs to a **durable intent** — "order X should have document Y" — that outlives any single call, is retried on a schedule with a **durable attempt count and a wall-clock horizon**, and raises a flag for a human at a compliance-driven hour while *still* retrying at low cadence. "Retry forever" is dangerous not because classification is imperfect but because nothing bounds it; a count written *before* each attempt plus a horizon bounds it whatever any answer meant. Misclassification is cheap: the worker is query-first, so a wrongly retried settled fault costs a few reads, never a duplicate; a wrongly parked transient costs one click. The UI is the overlay on the intent, not the alternative to retrying.

## 2. Facts that decide it

1. The invocation retry policy is **not** runtime-modifiable in 1.7.8: `ModifyServiceRequest` has only `public`, retentions, two timeouts (raw/02 §6; ✓ `admin-rest-model/src/services.rs`). Corrects the brief, as does 2.
2. `restart-as-new` sets `idempotency_key: None`: unreachable under the original key, which keeps replaying the old result (raw/02 §3; ✓ `restart_as_new.rs:308`).
3. Under vqueues a backing-off invocation shows `ready` in `sys_invocation`; truth is `sys_vqueues` (raw/02 §7; V by source, U end to end).
4. Pause answers nobody (`/call` waits, `/output` 470, same-key requests attach and block) and holds the key. Kill completes, frees the key, returns a raw-text 500 stored under the key 30 d (raw/02 §1; ADR 0004).
5. Invocation attempts are spent only on worker-side failures, never run retries (ADR 0004 #87). Worst-case in-flight `Order` write ≈ **2 h 15 m** (grill/B §4; derived, U).
6. Pretix retries a non-2xx webhook 12 times over ≈ 59.4 h with a **fixed** `notification_id`; nothing re-sends afterwards; no ordering; duplicates possible (raw/01 §1).
7. Org-wide `orders/?modified_since=<X-Page-Generated>`, 360 req/min on Hosted; codes unique **per organizer** (raw/01 §2, §5).
8. Hosted pretix.eu installs no third-party plugins (raw/01 §4).
9. Áfa tv. §163(2): `haladéktalan` when paid at performance or as an advance — card-paid tickets; 8 days otherwise. Art. §220(3): no lateness fine if the taxpayer acted "as generally expected" (raw/03 §5).
10. A service-side send with an idempotency key derives the same deterministic id and attaches to any in-flight status; `Request::scope` is **not** inherited (grill/C §1–2, V by source).

## 3. The three designs

| | A thin forwarder | B Postgres ledger | C intent in Restate (`PretixOrder` VO) |
|---|---|---|---|
| Durability of intent | the invocation (30 d); none if `/send` never ran | row per `(scope, order, op)` | VO state per order |
| Who retries | Restate: 12 h run policies, then `pause` | drain loop + cron | `attempt` via `send_after` |
| Bounded how | `max_duration` + 12 attempts → pause; auto-resume unbounded | `attempts` column + `horizon_at` | `n` in K/V written before the call + `horizon_at` |
| Failure visibility | operator SQL + mail; refusals 24 h; organizer: nothing | attention list, SQL | `status` + per-organizer attention map |
| Manual trigger | `/send` new key; refused while paused | `retry now` / forced row | `retry_now`, `resolve` |
| Blocks the order? | **yes** — a paused write holds the key until a human acts | no | no after send-and-peek |
| Worker changes | pause, wider policies, read-policy split | none | none |
| Moving parts | webhook, pages, notifier | Postgres, drain, sweeper, cron, planner, UI | 2 VOs, planner, webhook/UI |
| Effort | ≈ 1.4 k lines + worker PR | 3–4 wk + 1–2 wk UI (sweeper unbudgeted) | ≈ 3 wk + 1 wk UI + 1 wk repairs |
| Grill verdict | falls | wrong default — "C with a Postgres bill" | survives with four repairs |

**A.** The grill is right: A falls for *this* owner. Fatal: intent exists only as an invocation — never if `/send` was not reached, gone on kill, cancel or day 30; a paused write holds the order key until an operator acts on an unauthenticated port, so the liable organizer has neither signal nor lever; settled refusals (the commonest human case) are visible 24 h; auto-resume is the unexitable loop. Not fatal: the `sys_vqueues` coupling, the read-policy split, bulk resume; A's webhook → `/send` shape is C's phase 0.

**B.** Right: B rebuilds timers, counting, dedup and locking on a table — the machinery the owner chose Restate to avoid — with a sweeper nobody budgeted; `done + absent → reopen` would issue a second legal invoice; 4 h "give up ∧ tell a human" conflates two decisions. Overreached in "Postgres bill": B alone acks Pretix with the ingress down and keeps audit past 3 d — decisive if Pretix is self-hosted or per-organizer §220(3) reporting is demanded.

**C.** Right: the 2 h parent await is structural — holds the parent key, pins the deployment, orphans the intent on a parent kill, makes the cron load-bearing — and send-and-peek removes it inside the design; scope non-inheritance and K/V poison are verified and repairable. Overreached: state growth at 10⁵ orders is beyond the stated volumes.

## 4. Recommendation

**C, modified — "intent in Restate, send-and-peek."** A `PretixOrder` Virtual Object per order holds the intents and calls `Szamlazz.Order` with `send()` + idempotency key, peeking `/output` on a timer; B's classification and state shape; grill/B §5's split of *flag* from *stop*; a per-organizer attention map; a `scoped_send` helper; `Value`-first state reads. It fits the owner: no second stateful system; timers, dedup, lock and the durable count are Restate's, so the only hand-rolled part is a pure `next(intent, outcome, now)`; the worker changes nothing.

**Failure-handling contract** (O = organizer attention list, Op = operator alert):

| Worker response | Automatic action | Intent state | Human |
|---|---|---|---|
| `issued`, `already_issued`, `reconciled`; storno `reversed` | record number | `done` | — |
| create → `reversed` | none (reissue is explicit) | `needs_attention{reversed}` | O: approve reissue |
| `rejected{code}` | none | `needs_attention{rejected}` | O: fix Pretix data |
| `conflict{live}` | `get`; live → done | `done` | — |
| `conflict{order_invoiced}` | drop the proforma intent | `superseded` | — |
| `conflict{foreign, duplicate_order_number, not_managed}` | none | `needs_attention{reason, existing_number}` | O |
| `conflict{prepaid_chain, proforma_*, prepayment_*, base_reversed}` | replan once, then none | `needs_attention{planner}` | O + Op |
| `conflict{external_id_collision}`; `invalid_input` 400 | freeze the scope's sends | `needs_attention{bug}` | Op |
| `unknown_account` 400, `account_mismatch` 409, `credentials_rejected` 503 | freeze the scope's sends | `needs_attention{account}` | Op, then bulk retry |
| `not_found` 404; `szamlazz_error` 422 | `get` | `needs_attention{drift / szamlazz_code}` | O |
| `unavailable` 503 | new key, `n+1` | `retrying` | O past `horizon_at` |
| `outcome_unknown` 500; killed raw-text 500; 409 `killed`/`canceled`; `/output` 404 | `get`: live → done; else new key, `n+1` | `done` / `retrying` | Op; O past horizon |
| no answer (`/output` 470) | keep key; peek at 1, 2, 5 m, then every 10 m; flag past 2 h 30 m, then hourly | `in_flight` → `needs_attention{stuck}` | O + Op past 2 h 30 m |
| unknown future `code`, `outcome`, `conflict_reason` | none | `needs_attention{unknown}` | Op |

**Schedule and horizon.** Retry gaps `1, 5, 15, 30 m, 1, 2, 4, 6, 12, 12, 24 h` — Pretix's own ladder — so `n ≤ 12` and the last attempt is ≈ 63 h after the first; `give_up_at = created + 72 h` clamps every `send_after`. `horizon_at` raises the flag: **4 h** for `create_invoice` on a paid order (card-paid is `haladéktalan`, raw/03 §5), 24 h for proforma, delete, storno, corrective (8-day bucket or non-financial). Flag ≠ stop (grill/B §5): a szamlazz.hu weekend outage clears unattended while the organizer sees the row; a settled fault stops at once. `attempts[]` in state is the §220(3) record.

**Exit proof.** `n` is written to state before every child call, is monotone, and `attempt` refuses to run past `n = 12` or `give_up_at`; every `attempt` ends `done | superseded | needs_attention | retrying`, and `retrying` schedules exactly one `send_after` with `next_at ≤ give_up_at`. Nothing else increments `n` or schedules `attempt` except `retry_now`, a human act; `check` peeks carry their own count. Hence at most 12 child invocations and a bounded number of peeks per intent, and after `give_up_at` nothing fires without a human — regardless of classification.

## 5. Kill vs pause, settled

Keep `kill` on the `Order` writes; the #87 review closes. Under C the intent lives in `PretixOrder`, so a kill is one failed attempt of twelve: the child completes, the order key frees, the peek sees a raw-text 500, `get` says whether anything landed, and the next attempt uses a fresh key — safe because the create step queries by external id before it sends. Pause would freeze the child, hold the key, answer 470 forever and need an operator per order, moving the intent where the organizer cannot see it. Kill is a bounded, safe primitive: resilience to it is not in the worker but in the caller holding the intent and rotating the key.

## 6. What `restate-szamlazz` must change

- Amend ADR 0004: kill stays, the caller owns the intent; state the worst-case in-flight bound (≈ 2 h 15 m) and pin it in a test computed from the policy constants, so a widened `[issue]` fails loudly.
- Optional, additive: `issued_date` on `get`'s `DocumentStatus` — today only `Szamlazz.Agent.query` returns it.
- Nothing else; the raw-text kill body is already in the caller contract (#89).

Must **not**: add state to `Order` (ADR 0005); switch to `pause`; add `list`/`scan`; widen or split `[read]` for a forwarder.

## 7. Verify by experiment before building

1. **Service-side send + scope.** `PretixOrder` sends `create_invoice` with `.scope(ctx.scope()).idempotency_key(k)` on the multi-account shape: document on the right account; a repeated send is `PreviouslyAccepted`, same id; without `.scope()` → `unknown_account`.
2. **Peek.** `GET /restate/output/{id}` inside `ctx.run`: 470 in flight; body after; killed → raw-text 500, `error-source: invocation`; purged → 404. Fallback: POST-body `/output` by key with `scope`.
3. **`send_after` to self under scope.** With `.scope()` the timer lands on the scoped instance; without, on the unscoped one.
4. **K/V decode failure.** Corrupt state via `PATCH /services/{svc}/state`: `ctx.get::<T>()` burns attempts and kills; a `Json<Value>`-first read completes as `needs_attention{undecodable}`.
5. **In-flight bound.** Stall szamlazz.hu (wiremock > 60 s), cut the worker in windows: `created_at → completed_at` ≤ 2 h 15 m + margin; a manual kill mid-create → 409 `killed` on `/output`, `get` answers.
6. **Pretix.** 200 stops retries; duplicate and `paid`-before-`placed` deliveries; org-wide `modified_since`.

## 8. Roadmap

W = this repo, S = new sync-app repo.

1. **W** ADR 0004 amendment + in-flight-bound test.
2. **S — tracer bullet.** `PretixOrder` (`observe` / `attempt` / `check` / `status`) for `create_invoice` on `paid` only; webhook → `/send` → 200; `scoped_send`; classification; schedule and horizons; e2e on the existing harness with a Pretix wiremock; experiments 1–4 are its first tests.
3. **S** Planner as a pure function: proforma on `n` + transfer, delete on unpaid cancel, storno on full refund, `proposed` storno + reissue on address change.
4. **S** `PretixOrganizer`: attention map, reconciler cursor loop, scope freeze.
5. **S** UI: attention list (retrying / needs attention / proposed), order page beside live `get`, actions; operator mail.
6. **S** State fixtures (additive-only), compaction, PII rule (project inside the run, `journal_retention` 1 d).
7. **W, optional** `issued_date` on `get` slots.
8. **S, later** self-hosted `order_info` panel; read-only Postgres projection for reporting, if §9 requires.

## 9. Open questions for the owner

1. Hosted or self-hosted Pretix? Self-hosted reopens B as a plugin.
2. Who acts on the attention list — organizer staff or only the operator?
3. Is any storno automatic — full refund → storno, address change → storno + reissue — or always `proposed`?
4. A cancelled but unrefunded order with a live invoice: storno or keep? (grill/B §2)
5. Must the 4 h flag reach a person at 03:00 on a Sunday, or is "next business day" the real SLA?
