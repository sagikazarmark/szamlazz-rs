# 03 — Reliable side effects from webhooks, and the Hungarian compliance clock

Research note for the 2026-09-06 session. Primary sources only; each claim VERIFIED (fetched today) or
UNVERIFIED; interpretation is marked.

## 1. Ack-then-process: what the senders tell consumers

- **Stripe**: return 2xx *before* complex logic ("you must return a 200 response before updating a customer's
  invoice as paid in your accounting system"); process via an async queue; retries up to 3 days; no ordering
  guarantee — dedupe by event ID; "use the API to retrieve any missing objects". VERIFIED
  (docs.stripe.com/webhooks).
- **GitHub**: 2xx within 10 s or the delivery fails; queue and process in the background; dedupe on
  `X-GitHub-Delivery`. VERIFIED (docs.github.com, webhook best practices).
- **Pretix**: the body is "only … a trigger to fetch updated data"; "in very rare cases, you could receive the
  same webhook notification twice"; non-2xx or >30 s → retries up to three days with back-off, so make the
  endpoint safe to call "multiple times for the same event". VERIFIED (docs.pretix.eu, Webhooks).
- **Shopify**: "your app shouldn't rely on receiving data from webhooks … use reconciliation jobs to
  periodically fetch data"; suggests a UI button for manual reconciliation. VERIFIED (shopify.dev).

## 2. Transactional outbox + reconciler

- Richardson: store the message "in the database as part of the transaction that updates the business
  entities"; a separate relay publishes; the relay "might publish a message more than once", so "a message
  consumer must be idempotent". VERIFIED (microservices.io, Transactional outbox; Saga).
- Neither page places the "retry forever vs give up" decision: the pattern guarantees delivery *to the broker*
  and leaves consumer failure handling open. VERIFIED (absence).
- Helland, "Idempotence Is Not a Medical Condition" (ACM Queue 2012): UNVERIFIED — ACM answered 403.
- Interpretation: a Restate invocation is durable but *ends* — fault, kill, or stored `unavailable`. What
  survives is the caller's record that an invoice *should* exist for order X: the outbox row as intent, keyed by
  order, with a status. A durable downstream removes the need for at-least-once *delivery* (the worker is
  idempotent by external id), not for the row. Without it, "issue X" lives only in Pretix's 3-day retry.

## 3. Poison messages and DLQs

- **SQS**: `maxReceiveCount` moves a message to the DLQ after N receives; DLQs "isolate unconsumed messages to
  determine why processing did not succeed", with redrive back. VERIFIED (AWS SQS Developer Guide).
- **Pub/Sub**: dead-letter topic after *maximum delivery attempts* 5–100 (default 5), "for analysis and offline
  debugging". VERIFIED (cloud.google.com/pubsub/docs/handling-failures). **Kafka**: consumer-side only.
  UNVERIFIED.
- **Temporal**: default Activity retry = 1 s initial, ×2, max interval 100 s, **Maximum Attempts = ∞**,
  no non-retryable errors; rationale: "permanent failures … require you to make some change to your logic or
  your input. Therefore, it is better to surface them than to retry them"; bound by Schedule-To-Close timeout,
  not attempt count. VERIFIED (docs.temporal.io, Retry Policies).
- **Restate**: "assumes by default that all errors are transient"; server default 70 attempts then `pause`;
  `unlimited` allowed; a DLQ is a try/catch forwarding the failed invocation "to a DLQ Kafka topic or a
  catch-all handler". VERIFIED (docs.restate.dev/guides/error-handling).
- No fetched source recommends unbounded retry for a side effect with a deadline; unbounded defaults target
  infrastructure failures, with permanent ones marked terminal and surfaced. VERIFIED.

## 4. Retry classification

- **gRPC**: `UNAVAILABLE` "most likely a transient condition … not always safe to retry non-idempotent
  operations"; `DEADLINE_EXCEEDED` "may be returned even if the operation has completed successfully"; "no
  fixed list of status codes on which it is appropriate to retry". VERIFIED (grpc.github.io statuscodes).
- **Google AIP-194**: auto-retry only where "repeated runs would not cause unintended state changes"; retry
  `UNAVAILABLE`; never `INVALID_ARGUMENT`, `DEADLINE_EXCEEDED`, `DATA_LOSS`; generally not `INTERNAL`,
  `UNKNOWN` ("may not be safe to retry … immediately be surfaced"), `ABORTED` (retry at a higher level); not
  until state changes: `NOT_FOUND`, `PERMISSION_DENIED`. VERIFIED (google.aip.dev/194).
- **AWS Builders' Library**: a lost `CreateResource` reply is retried with the *same* client token; answering
  `ResourceAlreadyExists` "leads to uncertainty from the perspective of the caller". VERIFIED (Making retries
  safe with idempotent APIs).
- Mapping (interpretation, semantics from the brief):

  | Class | gRPC analogue | Worker |
  |---|---|---|
  | Transient, retry unattended | `UNAVAILABLE` | `unavailable`; **no answer** (keep key, attach) |
  | Ambiguous, query first | `DEADLINE_EXCEEDED`, `UNKNOWN` | `outcome_unknown`, kill body → `get`, then new key |
  | Settled, fix something | `INVALID_ARGUMENT`, `NOT_FOUND`, `PERMISSION_DENIED` | `invalid_input`, `unknown_account`, `not_found`, `account_mismatch`, `szamlazz_error`, `credentials_rejected`, `conflict{…}`, `rejected` |

  "Auto-retry the known-transient, surface the rest" is the standard advice (AIP-194, Temporal). For the
  ambiguous class both agree: never blind-retry a state-changing call — query (AIP `ABORTED`) or retry with the
  same token (AWS). The worker's create step already queries first; the caller reads `get`, then decides.

## 5. Compliance clock (Hungary)

- **Law, Áfa tv. §163(1)**: issue at the latest (a) by performance, (b) for an advance by when tax becomes
  chargeable, "but at most within a reasonable time" thereafter. **§163(2)**: reasonable time = (a) 15th of the
  next month for §89 / reverse-charge cases; (b) **`haladéktalan`** (without delay) where the consideration is
  paid by performance, or for an advance by the time tax is chargeable; (c) otherwise **8 days** where the
  invoice carries VAT. VERIFIED (net.jogtar.hu consolidated text; njt.hu unreachable).
- **Law, §59(1)**: money received before performance is an advance; tax chargeable on receipt. **§55(1)**:
  performance = the fact realising the taxable transaction. VERIFIED.
- **Ticket = advance or performance?** Interpretation, UNVERIFIED as NAV position: a ticket paid before the
  event fits §59 → invoice `haladéktalan` on receipt; the service is performed at the event. Either way a
  card-paid order is in the *immediate* bucket, not the 8-day one. No number for "haladéktalan" found.
- **Online reporting**: 23/2014 NGM r. §13/A(1): the invoicing program transmits "at issuance, immediately"
  (`kiállításakor azonnal`); done when NAV confirms processing. VERIFIED (net.jogtar.hu). The clock runs from
  *issuance* — a late invoice is a §163 matter; szamlazz.hu owns reporting once the invoice exists.
- **Fines (Art., 2017. évi CL.)**: §228(1) up to **2 000 000 HUF** for failing the invoice obligation; §220(1)
  general cap 400 000 / 1 000 000 HUF (natural / other person), §220(2) lateness is a breach, §220(3) no fine
  for lateness if the duty is performed and the taxpayer acted "as generally expected in the situation"; §229
  reporting cap = affected invoices × general maximum. VERIFIED (net.jogtar.hu).
- szamlazz.hu docs on deadlines: not located. UNVERIFIED.

## 6. Human-in-the-loop UI for failed jobs

Sidekiq: 25 retries over ~20 days, then the **Dead set** ("you must manually retry them via the UI"); Web UI
tabs *Retries* and *Dead* to run, inspect or delete; capped at 10 000 jobs / 6 months, then discarded;
`death_handlers` notify; "retries are for unexpected errors" — expected ones belong in a state machine.
VERIFIED (sidekiq wiki, Error Handling). Stripe: *Event deliveries* tab, `Delivered`/`Pending`/`Failed`, status
and next retry per attempt; **Resend** up to 15 days. VERIFIED (docs.stripe.com/webhooks). Pretix: 30-day
delivery log. VERIFIED. Shopify: reconciliation as a button. VERIFIED.

## Consequences for the design

- The sync app must own an **intent row per order/kind** (outbox semantics): Pretix's retry is the only other
  holder of "issue X", expires in 3 days, and its key cannot be rotated (§1, §2).
- Ack Pretix 2xx once the intent is persisted; call the worker from a queue; dedupe on `notification_id`.
- Classify as in §4: auto-retry `unavailable` and no-answer (keep the key); `outcome_unknown`/kill → `get`
  first; every 4xx/422/`conflict` → surface. Misclassification is safe: the worker is query-first, so the cost
  is a human retry, never a duplicate.
- Bound retries by the clock, not attempt counts (Temporal's advice): card-paid orders are `haladéktalan`, so
  unattended retry targets **hours**; anything unresolved by the next business day must reach a person (§5).
- A **reconciler** (periodic `get` for orders that should have a document; issue with a fresh key if absent) is
  what every sender recommends and makes ordering/duplicates/misses harmless (§1).
- UI: Sidekiq's two lists — *retrying* (next attempt) and *dead* (needs a person) — with *retry now*, *view last
  error*, *discard*, and an age-out rule (§6).
- Restate `unlimited`/`pause` cannot be the sole owner of "try again": paused invocations hold the order key and
  are invisible to the organizer — a complement to the intent row, not a replacement (§3).
- Keep the attempt/failure log as the §220(3) defence that the taxpayer "acted as expected" if late.
