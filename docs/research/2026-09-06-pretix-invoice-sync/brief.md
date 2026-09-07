# Brief: syncing Pretix invoices to szamlazz.hu — how failures are handled, end to end

Status of this document: the shared input to a research and design session (2026-09-06). Everything under
"Verified" has a source; everything under "Open" is what the session must settle or mark as needing an
experiment. Repository: `/home/laborant/szamlazz-rs`, read-only for the session.

## The use case, whole

An application (call it the **sync app**; it does not exist yet — that is the point of the session) keeps
szamlazz.hu invoicing in step with [Pretix](https://pretix.eu/) ticket orders, for many organizers, each with
its own szamlazz.hu account (ADR 0006; endpoint README "Caller guidance: a Pretix integration").

Documents follow the order's life: proforma when a bank-transfer order is placed; invoice when paid (card at
once, or transfer received — converting the proforma); proforma deletion when cancelled unpaid; storno on
full refund; corrective invoice on partial refund or order change; storno + reissue when buyer data was wrong.

**Two triggers**:

1. **Automatic** — a Pretix webhook (`pretix.event.order.paid`, `…placed`, `…canceled`, `…refunded`,
   `…changed`, …) arrives at the sync app; the sync app calls the worker.
2. **Manual** — a person (organizer staff or the operator) presses a button: "issue invoice for order
   ABC12", "reissue", "storno", "retry". Possibly from a Pretix plugin's order-detail panel, possibly from
   a standalone UI of the sync app.

Volumes: an event sells from tens to low thousands of tickets; a payment burst at ticket release is the
peak (hundreds of `paid` webhooks in minutes). Invoices are legal documents (NAV-reported); a duplicate is a
real problem, a missing one is a real problem, a *late* one (hours) is usually fine, a late one (days) is a
compliance issue in Hungary (invoice deadline: generally within 8 days of fulfilment, immediately for cash /
card on payment — a fact to verify in the research).

## The worker as it stands (after #61, #79, #87, #89)

`restate-szamlazz` is a Restate worker. `Szamlazz.Order` is a Virtual Object keyed by `{event-slug}-{order-code}`;
its write handlers are exclusive per key; it keeps **no state** — szamlazz.hu is the source of truth via
deterministic external ids (`{namespace}:{order}:{kind}`). Every create is two durable steps: a read-only lookup,
then a create step that is query-first *inside* its `ctx.run`, so any re-execution finds what an earlier one
sent and never issues twice. Domain results are **data** (HTTP 200: `issued`, `already_issued`, `reconciled`,
`reversed`, `rejected`, `conflict{reason}`); faults are `TerminalError`s with a `{code, message, …}` body and
`x-restate-error-source: invocation` — eight codes: `invalid_input` 400, `unknown_account` 400, `not_found` 404,
`account_mismatch` 409, `szamlazz_error` 422, `outcome_unknown` 500, `unavailable` 503, `credentials_rejected` 503.
`Szamlazz.Order.get` is a shared (non-blocking) read of the order's four documents.

**Retry envelopes** (both Restate's):
- Run retry policies on the steps: `[issue]` 5 executions `2m → 10m` ≤ 1 h (create/storno; the szamlazz.hu-etiquette
  send bound); `[read]` 5 executions `5s → 60s` ≤ 5 m (every read); `[resolve]` `1s → 10s` ≤ 1 m.
  A szamlazz.hu outage is tolerated for as long as these allow; then the invocation **completes with a terminal
  fault** (`unavailable` / `outcome_unknown`).
- Invocation retry policy on the handlers: `Szamlazz.Order` writes and `Szamlazz.Agent.storno`: `2m ×2 → 10m`,
  `max_attempts 5`, `on_max_attempts kill` (~24 min of back-off); `set_payments`: 2 attempts (at-least-once
  send). **Attempts are spent only on worker-side failures** — worker unreachable, rollout cutting the stream,
  abort timeout, undecodable journal, non-deterministic replay — never on run retries (verified in 1.7.8 source
  and end to end, #89).

## Verified Restate facts (1.7.8; sources in ADR 0004 and its #87 amendment)

- **Kill** on exhausted attempts: the invocation completes as a failure; the VO key is released; a synchronous
  caller gets HTTP 500 with the *last retryable error's text* (not the worker's `{code,message}`) and
  `x-restate-error-source: invocation`.
- **Pause** on exhausted attempts: the invocation stays in flight, frozen; it **holds the VO key** (other
  exclusive calls on the key sit `inboxed`; the shared `get` answers); `resume` (per invocation or bulk per
  service) continues it from the journal — on the pinned deployment unless `--deployment latest`; `resume`
  re-runs the full attempt budget.
- **Idempotency-Key**: a request with the same key while the invocation is in flight — `Invoked`, `Suspended`,
  **`Paused`**, `Inboxed`, `Scheduled` — **attaches** (appends a response sink) and receives the eventual result
  (`crates/worker/src/partition/state_machine/mod.rs`, `handle_duplicated_requests`); against a `Completed`
  invocation it **replays the stored result — success or failure — for the idempotency retention** (30 d on the
  worker's write handlers). Keys are per scope.
- `restart-as-new`: for completed invocations; starts a new invocation id with the original input and headers;
  whether the new invocation is reachable under the *original* Idempotency-Key is **UNVERIFIED**.
- The ingress offers `/call` (synchronous, waits), `/send` (fire and forget, returns the invocation id, optional
  `delay`), `/attach` and `/output` on an invocation id or an idempotency key.
- Server-wide default retry policy: `initial 500ms`, factor 2, `max-interval 60s`, `max-attempts 70`, `pause`;
  `unlimited` is allowed. Per-handler policy can be overridden at runtime (`restate services config edit`)
  until the next deployment registers the code's values (whether an override reaches invocations already
  `backing-off`: UNVERIFIED).

## Verified caller-side facts

- Pretix webhooks: POST JSON `{notification_id, organizer, event, code, action}` — no order body; the
  consumer fetches the order via the REST API. Pretix retries a non-2xx delivery with back-off for **up to three
  days**, stops on 2xx (or 410); a 30 s timeout (docs.pretix.eu, webhooks). The `notification_id` is fixed
  across retries — the consumer **cannot make Pretix rotate it**.
- Pretix has a plugin system (Django apps) that can add order-detail panels, order actions, background tasks
  (Celery), and its own models; hosted pretix.eu does not run third-party plugins, self-hosted does.
- The worker's contract for the caller (README rule 2 after #89): a **fault** → rotate the key (or read `get`);
  **no answer** (client timeout, ingress-sourced 5xx) → keep the key, the retry attaches.

## Open — what this session must settle

The system-level question: **after the worker's own budgets are exhausted, who owns "try again", and how does
a failure become visible?** Two families the owner named:

- **A. Surface it.** The sync app records the failure and shows it (UI badge / list of orders needing
  attention / notification); a human retries (manual trigger) or fixes the cause. More application work.
- **B. Keep retrying (nearly) indefinitely.** Somewhere — the worker's invocation policy (`pause` / huge
  `max_attempts`), or the sync app's own queue — the attempt is repeated until it succeeds. Downside named by
  the owner: error handling must be perfect or a retry loop never exits.

Sub-questions:
1. Which failures are *transient* (worth retrying unattended) vs *settled* (need a human or a code fix)? Map
   the worker's eight fault codes + `conflict` reasons + "no answer" onto that split. Does the split need to be
   perfect, or is "retry the known-transient, surface the rest" enough?
2. Where should the retry loop live — Restate (invocation policy), the sync app (outbox/reconciler), or
   Pretix's own webhook retry (3 days, fixed notification id)? What does each require of the others?
3. Given attach-to-paused is verified: does `pause` on the `Order` writes make family B work with a *dumb*
   caller (forwarder)? What does a paused invocation cost (key held; other operations on that order wait)?
4. The manual trigger: how does it coexist with an in-flight or paused automatic attempt on the same order
   (the VO key serializes them; the manual call inboxes behind a paused one)?
5. Is a per-order **reconciler** (periodically: for orders that should have an invoice, `get`; if missing,
   issue with a fresh key) the thing that makes *every* other choice forgiving? What does it cost, and does it
   need worker support (e.g. a `list`/`scan` it does not have)?
6. What should a **UI** show, minimally, if family A? Which worker responses map to which states?
7. The compliance clock: how late may an invoice be, and does that bound the retry horizon?
8. What must the **worker** change, if anything (invocation policy, new handler, response fields)? What is
   purely the sync app's job?

Deliverable per agent: a Markdown file under `docs/research/2026-09-06-pretix-invoice-sync/raw/` (research) or
`…/brainstorm/`, `…/grill/`, `…/judge.md`, each claim marked VERIFIED (source) or UNVERIFIED, under the word
limit given in the agent's instructions.
