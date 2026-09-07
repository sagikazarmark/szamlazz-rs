# Pretix side of the integration — primary-source findings

Sources: docs.pretix.eu (2026.8 dev build) and `pretix/pretix` master (paths below). No secondary write-ups.

## 1. Webhooks

- **Payload** (order events): `{notification_id, organizer, event, code, action}` only — no order body. VERIFIED `src/pretix/api/webhooks.py` `ParametrizedOrderWebhookEvent.build_payload`; docs …/api/webhooks.html "Receiving webhooks".
- **`notification_id` = the `LogEntry` pk**, passed unchanged into every retry (`send_webhook(logentry_id, …, retry_count)`). Stable; the consumer cannot rotate it. VERIFIED `webhooks.py`.
- **Order action types** (VERIFIED `register_default_webhook_events`; docs …/resources/webhooks.html): `placed`, `placed.require_approval`, `paid`, `canceled`, `reactivated`, `expired`, `expirychanged`, `modified` (buyer-input change incl. invoice address — `control/views/orders.py:2260`), `contact.changed`, `changed.*` (material: `item|price|cancel|add|addfee|feevalue|split|subevent|tax_rule|…`, `base/services/orders.py`), `refund.created|created.externally|requested|done|canceled|failed`, `payment.confirmed`, `approved`, `denied`, `deleted` (test mode only). Wildcards resolve by dotted prefix (`LogEntry.webhook_type`, `base/models/log.py`).
- **Invoicing meaning**: `paid` is logged in `OrderPayment._mark_paid_inner` only when the order becomes status `p` (payments − refunds ≥ total); `payment.confirmed` fires per payment, including partial ones and on already paid/canceled orders. VERIFIED `base/models/orders.py:1873, 1984-2000`. `expired` cancels pretix's own invoice (`services/orders.py:374-377`).
- **No webhook for pretix's own invoice generation**: `pretix.event.order.invoice.generated` is a log action, not a registered webhook type. VERIFIED by absence.
- **Retry schedule** on non-2xx or transport error (30 s timeout): `5s, 30s, 1m, 5m, 20m, 60m, 4h, 6h, 12h, 12h, 24h` → 11 retries / 12 deliveries, cumulative ≈ 213 995 s ≈ **59.4 h (~2.5 days)**; code comment: "approximately 3 days, as documented". Intervals < 5 m via Celery countdown, ≥ 5 m via a `WebHookCallRetry` row picked up by `periodic_task`. VERIFIED `send_webhook`; docs "Responding to a webhook".
- **What stops retries**: any 200–299 (redirects and 304 are failures; no redirect following); `410 Gone` disables the whole webhook; exhausting the list (`retry-given-up`) silently drops that notification. VERIFIED `send_webhook`; docs.
- Self-hosted without a Celery broker: "failed webhooks will not be retried". VERIFIED docs note.
- Duplicates: "In very rare cases, you could receive the same webhook notification twice." VERIFIED docs.
- **Ordering**: none. Each `LogEntry` spawns its own `send_webhook` task on commit (`LoggedModel.log_action` → `TransactionAwareTask`), retried independently; `paid` can arrive before `placed`. VERIFIED by construction (`webhooks.py`, `base/models/base.py:164-175`, `services/tasks.py`); as a documented guarantee UNVERIFIED.
- **Manual re-send**: organizer UI webhook-logs page has `expedite` (re-queue *pending* DB-phase retries now) and `drop`; nothing re-sends a delivered or given-up notification; no API. VERIFIED `control/views/organizer.py:1604-1643`.
- **Scope**: organizer-level `WebHook` with `all_events` or `limit_events`; one URL for any subset of events. VERIFIED `api/models.py:114-120`.

## 2. REST API

- **Fetch**: `GET /api/v1/organizers/{org}/events/{event}/orders/{code}/`; 404 unknown order, 403 unknown org/event. VERIFIED `doc/api/resources/orders.rst` "Fetching individual orders".
- **Fields**: `status` (`n|p|e|c`), `total`, `locale`, `last_modified`, `testmode`, `invoice_address` {`company, is_business, name, name_parts, street, zipcode, city, country, state, vat_id, vat_id_validated, internal_reference, last_modified`}, `positions[]` (`price, tax_rate, tax_value, tax_code, canceled`), `fees[]`, `payments[]` (`state ∈ created|pending|confirmed|canceled|failed|refunded, amount, provider, payment_date`), `refunds[]` (`state ∈ created|transit|external|canceled|failed|done, amount, source`), `cancellation_date`. Currency lives on the **event** resource. Order-level `payment_provider`/`payment_date` are "DEPRECATED AND INACCURATE". VERIFIED orders.rst 15-122, 274-322.
- **Reconciler query**: per-event and organizer-wide (`/organizers/{org}/orders/`) lists accept `modified_since`, `created_since`, `status`, `testmode`, `ordering=last_modified`; feed back the `X-Page-Generated` header as `modified_since`; docs recommend `testmode=false`. VERIFIED orders.rst 496-538; fundamentals.rst "Object-level conditional fetching".
- **Rate limit**: Hosted only — 360 req/min per organizer for token auth, `429` + `Retry-After`; self-hosted none by default. VERIFIED `doc/api/ratelimit.rst`.
- **Tokens**: team-level; team has `all_events`/`limit_events` and `limit_event_permissions` (`event.orders:read`, `event.orders:write`, `event.settings.invoicing:write`, …). One org-wide token with `event.orders:read` suffices for reading. VERIFIED `doc/api/tokenauth.rst`, `resources/teams.rst`.

## 3. Pretix's own invoicing

- Per-event `invoice_generate`: `False` (default), `admin`, `user`, `user_paid`, `True`, `paid`. VERIFIED `src/pretix/base/settings.py:1216-1245`.
- Generation decided in `_mark_order_paid` (`invoice_qualified` && setting); failure swallowed, logged `pretix.event.order.invoice.failed`. VERIFIED `base/models/orders.py:2010-2049`.
- API: `POST …/orders/{code}/create_invoice/` (400 when disabled/exists), `…/invoices/{number}/reissue/`, `…/regenerate/`; invoices immutable, changes = cancellation + new invoice. VERIFIED orders.rst 1532-1571; invoices.rst 485-545; `doc/api/guides/order_lifecycle.rst`.
- **Signals**: `order_placed(order, bulk)`, `order_paid(order)` (not sent for API orders created already paid, nor for splits), `order_canceled`, `order_expired`, `order_modified`, `order_changed`, `order_approved`, `order_denied`, `order_reactivated`, `build_invoice_data(invoice)`, `invoice_line_text(position)`, `register_invoice_renderers`. VERIFIED `src/pretix/base/signals.py:576-835, 1131`; orders.rst 1007-1011.
- **Renderer is the wrong seam**: `BaseInvoiceRenderer.generate(invoice) -> (filename, type, bytes)` renders a file for an `Invoice` pretix has already numbered and stored; `TransmissionProvider.transmit(invoice)` moves an existing invoice ("New transmission types can not be added by plugins"). Neither can suppress or replace the pretix number. VERIFIED `base/invoicing/pdf.py:153-185`, `base/invoicing/transmission.py:186-203`, `doc/development/api/invoicetransmission.rst`.

## 4. Plugin surface (self-hosted)

- Plugin = Django app with `PretixPluginMeta`; event/organizer/hybrid activation. VERIFIED `doc/development/api/plugins.rst`.
- Signals: the `order_*` set above plus `periodic_task` (global, "between a minute and a day", must be idempotent). VERIFIED `base/signals.py:944-951`.
- Control panel: `order_info(order, request)` → HTML on the order page; `order_position_buttons`; `nav_event`; custom `/control/event/{organizer}/{event}/…` views with `EventPermissionRequiredMixin` — enough for a badge and a retry button. VERIFIED `src/pretix/control/signals.py:59, 268, 290`; `doc/development/api/customview.rst`.
- Background jobs: `@pretix.celery_app.app.task` (synchronous without a broker). Own models: assumed by the quality checklist ("If the plugin adds any database models…"). VERIFIED `doc/development/implementation/background.rst`; `doc/development/api/quality.rst` B.
- Plugins may register extra webhook types (`register_webhook_events`). VERIFIED `src/pretix/api/signals.py:32`.
- **Hosted pretix.eu**: marketplace text — plugins are for "hosting pretix yourself … If you use pretix through our pretix Hosted offering, you do not need this page, most useful plugins are already installed for you." No self-service install. VERIFIED marketplace.pretix.eu. An explicit "third-party plugins refused" statement: UNVERIFIED (docs, pricing FAQ, marketplace checked).

## 5. Order codes

- Charset `ABCDEFGHJKLMNPQRSTUVWXYZ379`, default length 5 (`ENTROPY['order_code']`), lengthens after 20 collisions; test-mode codes carry `0` at position 2; API-created orders may supply `code` of `A-Z0-9` minus `O`,`1`; column `max_length=16`. VERIFIED `base/models/orders.py:208-212, 889-918`; `src/pretix/settings.py:430`; orders.rst 1007.
- **Unique per organizer** (`UniqueConstraint(organizer, code)`), not merely per event; `full_code` = `{EVENT-SLUG-UPPER}-{code}`. VERIFIED `orders.py:341-343, 577-582`.
- `code` and `event` are API read-only; no move/rename operation; a split makes a *new* order (`changed.split`). VERIFIED `api/serializers/order.py:889-892`. Absence of any UI rename path: UNVERIFIED.

## 6. Failing targets

- No "disable after N failures": only `410` disables; exhausted retries stop for that notification, webhook stays enabled. VERIFIED `send_webhook`.
- Each attempt logged as `WebHookCall` (`return_code, success, is_retry, payload, response_body ≤ 1 MiB, execution_time`), shown in the organizer UI ("Debugging webhooks", 30 days), purged after 30 days by `cleanup_webhook_logs`. VERIFIED `api/models.py:140-153`; `api/signals.py:69-73`. No API for the call log: UNVERIFIED (none documented).

## Consequences for the sync app

- The webhook is a hint: re-read the order and decide from `status`, `payments[].state`, `refunds[]`, `invoice_address` — never from `action`.
- Expect duplicates and out-of-order arrival (`paid` before `placed`); handlers must be idempotent on order state.
- Pretix's retry is ~2.5 days / 12 deliveries with a fixed `notification_id`, no re-send afterwards, no per-order re-trigger — it cannot be the durable retry loop or the Idempotency-Key source.
- Return 2xx once the event is *recorded*, not once the invoice is issued; the 30 s timeout and back-off would starve a payment burst.
- A reconciler is cheap: org-wide `orders/?modified_since=<X-Page-Generated>&testmode=false` with one `event.orders:read` token (360 req/min on Hosted).
- Codes are unique per organizer, so `{event-slug}-{code}` is safe; the slug is needed for the API path, not for identity.
- Set `invoice_generate=False` per event so szamlazz.hu is the only numbering authority; do not use the renderer/transmission seams.
- Manual trigger/badge: feasible as a self-hosted plugin (`order_info` + control view); on Hosted the sync app must own the UI.
