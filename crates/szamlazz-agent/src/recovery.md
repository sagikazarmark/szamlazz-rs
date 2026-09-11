
## Recovery

`outcome_class()` classifies **this exchange**, not all earlier sends of a
logical operation. `Rejected` is a refusal, not proof that an earlier lost send
did nothing. The client has no application-level retry/recovery loop. A supplied
HTTP client's retry policy remains active: one `send` can produce multiple POSTs,
without running reconciliation between them. Configure transport retries with
the operation's uncertainty and vendor send limit in mind.

| Operation | Recovery after a lost or uncertain answer |
|---|---|
| Invoice creation | Persist an external id and order before sending. Query with `InvoiceSelector::ExternalId`; the id is not unique server-side and returns its newest holder. Check the returned order, document type, number and reversal state before adopting it. Allow earlier sends to finish: an immediate code 7 alone does **not** authorize another create. Unresolved identity or timing means unresolved recovery. |
| Invoice storno | Query the known original and, if supplied, the storno's external id. Verify that the found document is `SS` and references that original, as well as its order and reversal state. A repeat of an already reversed invoice was observed to return the existing storno number, but still reconcile uncertain sends; a proforma/delivery-note storno can be a success-shaped no-op. |
| Receipt creation | Persist a unique `CreateReceipt::call_id` **before the first send**, and retain it for that logical issuance. Code 338 prevents another issue but supplies neither the original number nor PDF. Query a known number or a deliberately managed order with `ReceiptSelector`; verify returned call id, order, number, `NY`/`SN` and reversal data. If recovery remains unresolved, do not generate a fresh call id. |
| Receipt storno | Keep the logical call identity. Query the known original to inspect its reversal state; this alone does not recover the `SN` number or PDF. A known storno number can be queried and its original reference checked. The [vendor documents refusals](https://docs.szamlazz.hu/agent/reversing_receipt/response) for already reversed receipts and storno-receipt targets, not invoice-style successful replay. A refusal does not identify who reversed the receipt. A storno-specific 338 guarantee remains unestablished. |
| Reads (invoice/receipt queries, taxpayer lookup) | A repeat obtains current data and creates no document. Interpret not-found for the selector and operation; code 7 on receipt **send** may instead mean a missing subject. |
| Credit-entry registration or clearing | Query the invoice's current credit entries and balance. Invoice existence does not show that the mutation landed. Repeating additive entries can double amounts; replacing entries (including `ClearCreditEntries`, explicit empty replacement) can overwrite intervening state. Reconcile the intended mutation with current data before a deliberate new send. |
| Proforma deletion | Number selection targets one proforma; **order selection deletes all matching proformas**, not just the latest one a query returns. Repeating an order-based deletion can reach newly created matches. Query known numbers; absence can also mean consumption by an invoice. Code 335 is a settled refusal, not replayed success. Reconcile the intended target set before deliberately repeating. |
| Receipt email | Receipt existence does not establish whether an email was sent. A lost acknowledgement leaves delivery unresolved; deliberately repeating may send another email. Empty-block resend requires previously supplied email details. |

### Receipt selectors

`QueryReceipt` takes a receipt number or order number. Its optional wire
`hivasAzonosito` field has **unspecified query behavior**: omit `call_id` for
normal lookups. There is no call-ID-only selector. The
[first-party PHP docs](https://docs.szamlazz.hu/php/nyugta-lekerdezes) describe
order lookup as returning the **last matching document**; the exact meaning of
“last” and selection of `SN` versus `NY` remain unresolved. Never adopt that
match without checking its identity and type.
Receipts have their [own order-number repetition toggle](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number),
independent of the invoice setting. Restriction prevents another receipt with a
previously used order number; allowing repetition permits multiple matches.
This does not establish invoice-style replay rules for receipts.

### Retry limit and evidence

The [vendor limit](https://docs.szamlazz.hu/agent/basics/error-handling#retry-limit)
is **five total sends of the same request, including the initial send**, then
stop and involve the operator. Never retry until success or in a tight loop.
The same-request wording does not define exact combined accounting for a write
and its reconciliation queries.

- **Documented:** 55 means signing failed, not proven issuance. Timestamp-server
  access may be transient; certificate expiry requires remediation. The code
  stays `Unknown` with a potentially transient retry hint.
- **First-party PHP source:** 2.12.4 corroborates code 56 as issuance with a
  notification warning **only with a document number**. Without one it remains
  `Unknown`. The account probes did not trigger 55 or 56.
- **Observed:** the detailed A4d-2/A4d-q account record reports a ≥57-second
  stalled create and no issuance found by its order query. The project's broader
  delayed-issuance assertion has no linked probe establishing that outcome.
  Neither history is treated as disproven. A client timeout does not cancel
  server work: retain the 60-second default and allow in-flight work plus a
  margin before deciding whether to send again. Elapsed time alone is no proof.

The receipt and simplified-image codes added in #195 are sourced from the
[receipt supplement](https://docs.szamlazz.hu/agent/generating_receipt/response),
[general errors](https://docs.szamlazz.hu/agent/basics/error-handling) and
[simplified-image rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency),
not live-account observations. Offline parser tests establish classification
and message preservation, not vendor execution semantics.
