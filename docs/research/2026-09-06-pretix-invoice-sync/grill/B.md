# Grill: B, intent ledger + reconciler in Postgres

Adversary's read of `brainstorm/B-intent-ledger.md`; A and C as evidence. **V** = verified (source), **U** = unverified.

## 1. Re-implementing Restate in Postgres

B's drain loop carries five things Restate already sells the owner: a timer (`next_attempt_at` + cron ≈ `send_after`, V: raw/02 §2), an attempt counter (≈ K/V written before the call, C §2), dedup (`current_key` ≈ `Idempotency-Key`, V), a lock (`SKIP LOCKED` + one `in_flight` per `order_key` ≈ the VO key), a state machine (≈ `ctx.set`). What the table gets wrong:

- **Crash after the `in_flight` commit, before `/send`**: a row with a key and no invocation. "Re-`/send` is `PreviouslyAccepted`" (V) works only with a *sweeper* re-driving stale rows; B names none. `send_after` *is* the sweeper.
- **Crash mid-poll**: no transaction spans a 2 h HTTP poll, so the row is unlocked while polled; two replicas race on the state write (`done` vs `retrying` at a 470/result boundary) unless every write is `WHERE state='in_flight' AND current_key=$1`. Harmless to szamlazz.hu (query-first, V: brief), but code the VO lock deletes.
- **Breaker**: in memory it diverges across replicas; in the DB, another table.

Fair credit: `webhook_receipt` acks Pretix with the ingress down, a gain over A/C. The rest duplicates the engine. ADR 0005 removed ~4,400 lines of ledger from the worker for this reason (V); B rebuilds one outside it. **Unacceptable as the default.**

## 2. The planner

Shared: A, B and C all map `(pretix_order, prior intents) → ops`; A's is smaller only because it refuses correctives/reissue. B's own inconsistency: a storno "asks a human", yet `c + live invoice → storno_invoice` is automatic, while the brief's trigger is refunds `done = total`, a cancelled, unrefunded order keeps its invoice. Not B-specific.

## 3. Double bookkeeping vs ADR 0005

| Ledger | `get` | B's rule | Verdict |
|---|---|---|---|
| `done` | absent | reopen | **Wrong for invoices**: live accounts cannot delete invoices (V: ADR 0005); absent means wrong scope/namespace/account, reopening issues a second real-world invoice. → `needs_attention{vanished}`. Right for proformas, though a UI-deleted one returns nightly (§6). |
| `needs_attention` | live | backfill `done` | Fine. |
| `pending` | live, foreign | unlisted | `get` shows only our slots (V: `contract/storno.rs`); the create meets it → `conflict{foreign}`. Acceptable. |
| `done{X}` | live `Y` | unlisted | update `result`, flag. |
| `done` | reversed | attention | Correct: *Reissue* is explicit (V: CONTEXT). |

Must the ledger override szamlazz.hu? Only on `proposed`, `dismissed`, `superseded`, `approved_by`, `pretix_snapshot`: *intent* facts szamlazz.hu cannot hold. Not ADR 0005's document ledger; C keeps the same in VO state. **Acceptable** once row one is fixed.

## 4. Fresh key per attempt; the 2 h poll

Bound (V: `handlers.rs:45-55`, policy defaults): resolve ≤ 1 m + three reads ≤ 5 m + issue ≤ 1 h + five attempts each stalled ≤ `inactivity 4 m + abort 3 m` + 24 m back-off ≈ **2 h 15 m** worst case, and **unbounded** while *inboxed* behind another invocation on the key; `/output` is 470 for inboxed and running alike (V: raw/02 §1). So yes, it can outlive B's 2 h. Then `stuck` parks; *retry now* mints `{id}:{n+1}`, which inboxes behind the first; both complete; the second answers `already_issued` (V: brief). No duplicate, the worker's doing. B's constant is coupled to the worker's deployment values (widen `[issue]` as A proposes and it is silently wrong); C's `sleep(2h)` shares this. **Acceptable**: derive it from discovery.

## 5. 4 h horizon = "give up" ∧ "tell a human"

Outage 02:00–07:00: rows paid 01:00–03:00 park at 05:00–07:00; 300 rows wait for a 09:00 human, Monday, on a weekend. C has the identical rule (C §2); A's 12 h `[issue]` clears at 07:00 unattended. B conflates a *retry budget* (why stop a harmless query-first retry at 4 h?) with an *alert*. Repair: `horizon_at` raises the flag; `give_up_at` (≈ 72 h, Pretix's span) ends the transient loop at hourly cadence; `needs_attention` becomes an overlay, settled faults still stop at once. Equally C's fix. **Unacceptable as written; repaired, acceptable.**

## 6. Ledger lost

`dismissed` is ledger-only truth; a rebuild cannot tell it from never-attempted and auto-issues. Legitimate "don't invoice a paid order": refunded (Pretix shows it), test (`testmode=false`), invoiced elsewhere, same account → `conflict{foreign}`; **another account** → a duplicate legal document. C shares it. Repair: rebuild in `proposed` mode. **Acceptable**; the ledger owns one fact.

## 7. Two control planes

Operator kills an `Order` invocation mid-poll: `/output` returns **409** `killed`, `error-source: invocation`, plain text (V: raw/02 §2, §5). B expects a *500*; a status-keyed classifier falls through, classify by header + "body parses as `{code,…}`". Deeper: kill is no *stop* to B; the row re-sends in 15–30 m; only `needs_attention` stops it, which the admin port cannot set. Shared with C. **Acceptable** if the admin port is read-only for `Order`.

## 8. PII

`input_json` holds buyer name, address, tax number for the row's life: indefinitely, one DB across organizers (B is each one's processor; DPA, row-level scope; U as legal conclusion). A/C hold it 3 d (V: `journal_retention`), C's state none. B does not need it: replan re-fetches Pretix, and a changed name on a retry is absorbed by the worker (71/152 → `reconciled`, V: CONTEXT). Store `pretix_snapshot` stamps only. **Unacceptable as written; trivially fixed.**

## 9. Effort and ops

A second stateful system beside Restate: migrations, backups (HA unnecessary, §3's rebuild covers loss), a pool, Postgres in CI. The estimate omits the sweeper, optimistic-concurrency writes and crash tests of the drain loop, the bug class Restate exists to remove. The owner knows the SDK, not Postgres queue idioms.

## 10. Steel-man

B is right when: (a) Pretix is **self-hosted**; a plugin already has Postgres, Celery `periodic_task`, `order_info` (V: raw/01 §4); B is then the plugin, nearly free; (b) the attempt log must be reported per organizer (§220(3)), SQL beats scanning `value_utf8` on an unauthenticated admin port (C §6); (c) more downstreams than szamlazz.hu; (d) organizer-facing authz and audit past 3 d.

## Verdict

For a one-person team on hosted Pretix, **B as written is the wrong call**: it rebuilds timers, counting, dedup and locking on a table in a second stateful system, with a poller and sweeper whose crash-correctness is the owner's problem, the work ADR 0005 deleted. Its defects are fixable: `done+absent → attention` for invoices; split `horizon_at`/`give_up_at`; header-based classification; no `input_json`; rebuild in `proposed` mode; poll bound from discovery. Every fix applies to C unchanged. What remains is *where the intent lives* (table vs VO state) and *who fires the timer* (drain loop vs `send_after`), B's cost, not its benefit. **Modified B is C with a Postgres bill**, distinguishable only under (10a) or (10b). Recommend C, with B's schema as its state shape and B's classification table; if reporting is needed later, project VO state into Postgres read-only, a cache, never a ledger.
