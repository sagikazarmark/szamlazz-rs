# Research, Pretix → szamlazz.hu invoice sync: who owns "try again"? (2026-09-06)

A research and design session on one question the worker cannot answer alone: **after `restate-szamlazz`'s own
retry budgets are exhausted (or when szamlazz.hu refuses) who owns "try again", and how does a failure become
visible to a person?** The owner framed it as "show it on a UI (more application work) vs keep retrying nearly
indefinitely (a retry loop it can't exit)". The session's answer is that it is neither: a **durable intent** with a
**durable attempt count and a wall-clock horizon**, held outside the worker, retried on a schedule and *overlaid* by
a human-facing flag, [`judge.md`](judge.md) §1.

## How it was produced

1. **A brief** ([`brief.md`](brief.md)) fixed the use case (automatic webhook-driven issuing *and* a manual trigger,
   many organizers, card-paid orders, Hungarian compliance clock) and separated verified facts from open questions.
2. **Three research agents** read primary sources only and wrote [`raw/`](raw/): the Pretix side (webhooks, REST API,
   plugin surface, order codes), the Restate invocation lifecycle at v1.7.8 (kill/pause/resume/restart-as-new/attach,
   `/send` + `/output`, the SQL tables, runtime overrides, Kafka), and the resilience patterns plus the Hungarian
   invoice-deadline law.
3. **Three brainstorm agents** each designed one architecture as well as it could be designed
   ([`brainstorm/`](brainstorm/)): **A** thin forwarder, Restate owns the retry via `pause`; **B** intent ledger in
   a Postgres the sync app owns, worker keeps kill; **C** intent as a `PretixOrder` Virtual Object in the sync app's
   own Restate deployment.
4. **Three grill agents** each attacked one design with the other two as evidence ([`grill/`](grill/)), verifying
   claims against Restate source where the brainstorms had marked them unverified.
5. **A judge** synthesised the verdict ([`judge.md`](judge.md)), re-fetching source where documents disagreed.

Every claim in `raw/` and the grills is marked VERIFIED (with source) or UNVERIFIED. The brainstorms are designs,
not findings; read them for the reasoning the judge compresses.

## What to read

| File | Use it for |
|---|---|
| [`judge.md`](judge.md) | **Start here.** The reframing, the ten deciding facts, the three designs in one table with a verdict each, the recommendation (C, modified: "intent in Restate, send-and-peek"), the full failure-handling contract (every worker response → automatic action / intent state / who sees it), the retry schedule and the exit proof, kill-vs-pause settled, what the worker must and must not change, the experiments to run before building, the roadmap, and five open questions for the owner. |
| [`brief.md`](brief.md) | The shared input: the use case in full, the worker as it stands after #61/#79/#87/#89, and the open questions the session had to settle. |
| [`raw/01-pretix.md`](raw/01-pretix.md) | Pretix facts. Corrects the brief: retries are ≈ 59 h / 12 deliveries (not "three days"); order codes are unique **per organizer**; hosted pretix.eu runs no third-party plugins; `orders/?modified_since` is organizer-wide. |
| [`raw/02-restate.md`](raw/02-restate.md) | Restate 1.7.8 facts. Corrects the brief: the **retry policy is not runtime-overridable** (only public / retentions / timeouts); `restart-as-new` **drops the Idempotency-Key**; under vqueues a backing-off invocation shows `ready` in `sys_invocation` (truth is in `sys_vqueues`); pause answers nobody (`/call` waits, `/output` 470). |
| [`raw/03-patterns-and-compliance.md`](raw/03-patterns-and-compliance.md) | What Stripe / GitHub / Pretix / Shopify tell webhook consumers; transactional outbox; DLQs and Temporal's unbounded-by-count-bounded-by-time default; the gRPC / AIP-194 retry classification mapped onto the worker's fault codes; **Áfa tv. §163** (card-paid = `haladéktalan`, otherwise 8 days) and the Art. fine regime. |
| [`brainstorm/`](brainstorm/), [`grill/`](grill/) | The three designs and their cross-examinations. `grill/C.md` ends with the C-vs-B comparison table. |

## The verdict in three lines

- **A falls** for this owner: intent lives only as an invocation, a paused write holds the order key until an
  operator acts on an unauthenticated port, and the organizer (who is liable), has neither signal nor lever.
- **B is the wrong default**: it rebuilds timers, counting, dedup and locking on a Postgres table (the machinery the
  worker was built on Restate to avoid), and one of its rules (`done` + absent → reopen) would issue a second legal
  invoice. It stays the right call if Pretix is self-hosted (a plugin) or per-organizer audit reporting is demanded.
- **C survives with four repairs**: send-and-peek instead of a 2 h parent await; an explicit `scoped_send` helper
  (scope is **not** inherited on service-to-service calls; verified); `Value`-first K/V reads (a decode failure is a
  retryable error, so a broken state type would burn every key's attempts); a per-organizer attention map.

**The worker keeps `kill`.** Under C a kill is one failed attempt of twelve; the caller holds the intent, reads `get`,
rotates the key. The #87 follow-up ("pause on the `Order` writes") closes as *not needed*.

## What this changes in this repository

Little, and nothing structural (`judge.md` §6): an ADR 0004 amendment stating that the caller owns the intent and
pinning the worst-case in-flight bound of an `Order` write (≈ 2 h 15 m, derived from the policy constants) in a test;
optionally an additive `issued_date` on `get`'s document slots. ADR 0005 (stateless `Order`), kill, and the absence of
`list`/`scan` all stand.

The sync app is a **new repository**; its tracer bullet and the experiments it must run first are in `judge.md` §7–8.
