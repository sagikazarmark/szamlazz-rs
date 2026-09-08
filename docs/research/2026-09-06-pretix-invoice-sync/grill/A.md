# Grill: A, thin forwarder, Restate owns the retry

Adversary's read of `brainstorm/A-thin-forwarder.md`, B and C as evidence. **V** = verified (source), **U** = unverified.

## 1. Forgotten pause; invisible to the organizer

Card-paid order, `create_invoice` `/send`t, bad worker deploy (undecodable journal). Attempts spend at 2→4→8→10 min; 12 attempts ≈ 1.6 h → `paused`. The notifier mails the operator; nobody reads it.

- **Hour 12**: paused ~10 h. Card payment is the `haladéktalan` bucket (V: raw/03 §5), already late. Pretix got 200; its retry is spent (V: raw/01 §1).
- **Day 2**: key held; the organizer's Pretix shows a paid order and no invoice, no badge, because A's only data source is the admin port, "operator surfaces, not organizer ones" (A §2).
- **Day 5**: §220(2) lateness is a breach; the §220(3) defence needs an attempt log; A's journal lasts 3 d (V: `handlers.rs`).

Liable: the taxpayer issuing the invoice; the organizer (fines fall on the taxpayer, V: raw/03 §5; that the organizer is that taxpayer: interpretation); the operator's exposure is contractual (U). Structural fault: the liable party has neither signal nor lever. **Unacceptable** for many organizers. Repair (an organizer page proxying `sys_invocation` by `scope`) needs scope→organizer authz and shows nothing past 3 d / 30 d.

## 2. A paused write blocks the order

Every exclusive handler on the key inboxes behind the paused create (V: ADR 0004): `storno_invoice` (refund), `correct_invoice`, `create_invoice{reissue}`, `delete_proforma`, the manual button's `/send`. Only `get` answers. Duration: until resume/cancel/kill on the admin port, operator-only; the organizer's button says "blocked, call the operator" (A §3). Worse, a dependent op cannot be *formed*: `storno_invoice` needs `invoice_number` (V: `contract/storno.rs:21`), which A reads from `get`; while the create is paused `get` shows none, so the refund webhook is 5xx'd (Pretix carries it ≤ 59 h, V: raw/01) or 200'd and lost. A 12 h `[read]` holds the key the same way without pause (V: `has_lock`, docs sql-introspection). **Acceptable** during a szamlazz.hu outage (nothing else could run); **unacceptable** after a worker fault, where the hold is human latency.

## 3. 12 h run policies

`[read]` is one deployment-level policy for *every* read, `get` and `Szamlazz.Agent.query` included (V: `config.rs:68-70`, ADR 0004 #37). Widened, every UI refresh and every webhook-handler `get` during an outage spawns a 12 h invocation, the split is mandatory. It is a worker change: two `RunRetryPolicy`s threaded through the gateway's read fns by call site; additive, no journal shape, plausibly acceptable, but it exists only to serve A. Also inert as written: `max_duration 12h` with `max_attempts 5`, `max_delay 10m` ends in ≤ 40 min; A needs `max_attempts ≈ 70`; ~70 probes per stuck invocation × hundreds during a burst-time outage, no account breaker (B has one). `abort_timeout`: **A survives**; a run retry returns `next_retry_delay`, the SDK does not sleep in-process, timeouts are per execution (V: ADR 0004 #37). The non-durable attempt count makes `max_duration` the real bound: 12 h (V: ADR 0004 #22).

## 4. `/send` + no state: what remains

After kill or fault: a `completed/failure` row for 30 d, journal 3 d (V: raw/02 §1). It says "this *invocation* failed", not "this *order* needs an invoice": one order may hold three failed invocations (Pretix duplicate + manual); the answer is `get`, four szamlazz.hu queries per order (V: `handlers.rs:244`). The *Refused* list is worse: `rejected`/`conflict` are 200s, so A calls `/output` per completed success and caps at 24 h (A §2), a business buyer with a bad tax number vanishes from the UI after a day while the clock runs. The audit page (Pretix `status=p` × `get`) is B's nightly reconciler minus persistence and write-back, re-paying 4N szamlazz.hu queries per view. Add "issue now" per row and it *is* B's reconciler. A is thinner in what it remembers, not what it computes.

## 5. Coupling to `sys_vqueues`

`sys_vqueues` / `sys_vqueue_entry_status` are in the documented reference, `stage`, `status`, `run_at`, `has_lock`, `num_pauses` (V: docs.restate.dev/references/sql-introspection), a public surface, so weaker than posed. But `status` is documented as "Examples are …" (open set), and `sys_invocation` showing backing-off as `ready` is itself a view in flux (V by source: raw/02 §7). **Acceptable** with an integration test pinned to the Restate version, owned for good.

## 6. Unauthenticated admin port

Needs a network policy so only the sync app reaches 9070 (V: raw/02 §7) and a proxy that authenticates staff and checks the invocation's `scope` column against the caller's organizer before forwarding. Feasible; kill is destructive and cross-tenant if the check slips, and A keeps no audit of who pressed what. Organizer-facing resume/kill exists only behind an authz layer A meant to avoid.

## 7. Auto-resume cron

`resume` re-runs the full budget and pauses again (V: ADR 0004; under vqueues U). A cron resuming everything paused is pause→1.6 h→pause forever on a poison journal, the owner's unexitable loop. A holds no durable count; Restate exposes `num_pauses` (V: documented column), and "resume while `num_pauses < 3`" repairs it, but that is C's `n ≤ 8` over an SQL view, and the poison fix is `--deployment latest`, not resume.

## 8. Bulk resume

Per-service bulk `resume` exists (V: docs managing-invocations). After a plain worker outage the pinned deployment is still right, 300 paused clear with one command; **A survives**. `--deployment` is documented per invocation only (bulk: U); after a bad deploy the operator scripts `POST /query` → 300 `PATCH …/resume?deployment=latest`, and the docs warn a changed flow fails with non-determinism → pause → kill → re-send with fresh keys from a list: a hand-run reconciler.

## 9. Where A is right

One organizer who is the operator; bank-transfer-dominant sales (8-day clock, V: raw/03); volumes where 4N `get`s per audit view are cheap; self-hosted Restate whose UI is the console; an on-call engineer who reads mail. Also as phase 0: A's webhook mapping is the planner every design needs; B and C keep it.

## Verdict

A does **not** survive the owner's case. Fatal, not cosmetic: (1) the liable party has neither signal nor lever; (2) settled refusals (the commonest human-needed class) are visible for 24 h; (3) intent evaporates on kill, cancel or day 30; (4) auto-resume has no exit. Every repair of 1–3 persists `(scope, order, op, state, last_fault)` from a poller over `sys_invocation`/`/output`: B's ledger minus attempts, with Restate as executor instead of a drain loop; the repair of 4 is C's bounded `n`. Modified A is B-lite or C-lite, distinguishable only by how much it forgets. Keep `kill`; take A's forwarder as the first 500 lines of B or C.
