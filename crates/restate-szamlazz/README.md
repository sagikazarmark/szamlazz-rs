# restate-szamlazz

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz)

**Restate services issuing and managing szamlazz.hu documents with durable, idempotent execution.**

The `Szamlazz.Order` Virtual Object — keyed by the order number — serializes issuing per key, so that a caller
can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes,
concurrent callers and reversals. It keeps **no state**: szamlazz.hu is the source of truth, reached through
deterministic external ids (`{slug}:{order}:{kind}`) so that any invocation can find what an earlier one
issued. The stateless `Szamlazz.Agent` service exposes by-number operations (query, credit entries, storno of
unmanaged documents) over the same steps. Both are projections of the Számla Agent model: deployment constants
live in config, line totals are computed, domain outcomes are returned as data.

The design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md), the decisions
behind it in [`docs/adr/`](../../docs/adr), and the szamlazz.hu behavior it relies on in
[`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md). A ready-made binary and container
image live in [`restate-szamlazz-endpoint`](../restate-szamlazz-endpoint).

## Quick Start

Bind both services to a Restate endpoint of your own:

```rust
use std::sync::Arc;

use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::{Agent, Config, Order};

async fn serve(config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let order = Order::new(Arc::clone(&config))?;
    let agent = Agent::from_parts(Arc::clone(order.steps()), config);
    let endpoint = Endpoint::builder().bind(order).bind(agent).build();
    HttpServer::new(endpoint)
        .listen_and_serve("0.0.0.0:9080".parse()?)
        .await;
    Ok(())
}
```

`Config` only implements `Deserialize`; the host chooses the file format and environment merging (the endpoint
binary uses figment). Call `Config::validate()` after parsing.

## Scope Contract

What `Szamlazz.Order` guarantees:

- **Exactly one live document per kind per order** — proforma, invoice, prepayment, final — under caller
  retries, process crashes and concurrent callers. Same-key handlers run one at a time; every issuing attempt
  queries szamlazz.hu by the document's external id *inside the same `ctx.run` closure* before it creates, so a
  request that landed before a crash or timeout is found, not re-issued.
- **Correctives** are issued under a caller-supplied `correction_id`; the same id finds the corrective it
  issued, a new id issues a new one.
- **Storno** (`storno_invoice`) and **proforma deletion** are idempotent; a document reversed by anyone — the
  UI, support, this service — is reported as `reversed` from `<sztornozott>`.
- **Domain outcomes are data** (HTTP 200) and errors are reserved for faults, so a caller can always tell "the
  document exists" from "the outcome is unknown".
- **`get`** is a live view: what szamlazz.hu holds under the order's four external ids right now, never a
  cached snapshot.

What it relies on:

- The szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition)
  **ON**. It is the server-side guard against a second live document of the same kind under one order number
  (71/152) and the reason a byte-identical resend is answered with the same number while the first document is
  live. Running without it is unsupported.
- **Deterministic external ids**, derived from the order key alone, so that a re-executed closure or a new
  invocation can ask "is there already one?" without state.
- **Validation of every found document** — order number, `tipus`, `teszt`, and the `supplier_id` pin when
  configured — because external ids are not unique server-side and a query returns the newest holder.
- One szamlazz.hu account per deployment; the account slug namespaces the external ids.

What it does not do: PDF download, receipts, taxpayer query, IPN and Adatkapcsolat ingestion, the proforma →
payment → invoice lifecycle workflow, multiple prepayments per order, tracking *who* reversed a document, and
reissuing on its own initiative — a create after any reversal returns `reversed` and issues a replacement only
with `reissue: true` and a new `Idempotency-Key` ([ADR 0003](../../docs/adr/0003-explicit-reissue-after-external-reversal.md),
[ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md)).

## Feature Flags

The crate has no default features; `restate-sdk` is always a dependency.

- `schemars`: derives JSON Schema for the contract types, so Restate's discovery manifest and OpenAPI export
  document every handler's input and output. Forwards to `restate-sdk`'s `schemars` feature.

See the [crate documentation](https://docs.rs/restate-szamlazz/latest/restate_szamlazz/) for API and feature
semantics and the [generated feature graph](https://docs.rs/crate/restate-szamlazz/latest/features) for
activation details.

## Key Types

- `contract::CreateRequest` / `CreateResponse`: input and output of the four `create_*` handlers. The request
  carries the `DocumentInput` (buyer, line items, dates, payment method, per-call overrides) and `CreateOptions`
  (`reissue`, `proforma: auto | none | {number}`); the response carries the `Outcome`, the identity (`kind`,
  `external_id`), the numbers and totals, and `warnings`. `CorrectRequest` (`invoice_number`, `correction_id`,
  `document`) is the input of `correct_invoice` and shares the response.
- `contract::Outcome` / `ConflictReason`: `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or
  `conflict` with a reason — `prepaid_chain`, `live`, `foreign`, `duplicate_order_number`,
  `external_id_collision`, `proforma_live`, `proforma_missing`, `prepayment_missing`, `prepayment_reversed`,
  `base_reversed`, `not_managed`.
- `contract::TerminalCode`: the four fault codes a `TerminalError` carries — `outcome_unknown`, `unavailable`,
  `account_mismatch`, `invalid_input`.
- `contract::StornoRequest` / `StornoResponse` (`StornoOutcome`: `reversed`, `rejected`, `conflict`,
  `managed_by_order`), `DeleteProformaRequest` / `DeleteProformaResponse`, `QueryRequest` (`Selector`) /
  `QueryResponse`, `SetPaymentsRequest` / `SetPaymentsResponse`: the remaining handler contracts.
- `contract::OrderStatus` / `DocumentStatus`: the live view `get` returns — one optional `DocumentStatus` per
  kind (`number`, `state`, `gross`, `net`, `payments`, `referenced_proforma`, `e_invoice`) with
  `DocumentState` flattened as `{state: live}`, `{state: reversed, storno_number?}` or, for a consumed
  proforma, `{state: consumed, by}`.
- `CorrectionId`: the caller-supplied identity of one corrective invoice,
  `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`; embedded in the corrective's external id.
- `OrderKey`: the `Order` key — the order number trimmed of leading and trailing whitespace, case preserved,
  validated (1–64 bytes, no control characters, no internal whitespace runs).
- `ExternalId`: the deterministic `szamlaKulsoAzon` of a document — `for_kind`, `for_corrective`,
  `for_storno`, `for_unmanaged_storno`.
- `Config`: the deployment configuration — `[account]` (`slug`, `agent_key`, `endpoint`, `mode`,
  `supplier_id`), `[defaults]`, `[seller]`, `[issue]` (`max_attempts`, `first_backoff`, `max_backoff`,
  `detect_foreign`). Secrets are `config::Secret`, whose `Debug` output is redacted.
- `steps::Steps`: the bodies of the durable steps over `szamlazz_agent::Client` — one plain async fn per
  `ctx.run` (`issue`, `verify`, `query`, `hint`, `storno`, `delete_proforma`, `set_payments`), each returning
  every expected szamlazz.hu outcome as data. `Szamlazz.Order` calls them inside `ctx.run`; the
  `Szamlazz.Agent` Restate service is a thin facade over the same instance. No Restate service calls another.
- `Order` / `Agent`: the Restate Virtual Object registered as `Szamlazz.Order` and the stateless service
  registered as `Szamlazz.Agent`, with generated `OrderClient` and `AgentClient` for typed calls from other
  handlers.

## Identity Model

Three identities work together ([ADR 0002](../../docs/adr/0002-order-keyed-idempotency-via-external-ids.md),
[ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md)):

- The **order key** decides which `Order` instance runs; same-key handlers run one at a time, which is what
  serializes issuing per order. The object holds no state.
- The **external id** identifies a document to szamlazz.hu and is derived from the key alone:

  | Document | External id |
  |---|---|
  | proforma, invoice, prepayment, final | `{slug}:{order}:{kind}` |
  | corrective | `{slug}:{order}:corrective:{correction_id}` |
  | storno of an order's invoice | `{slug}:{order}:storno:{original_number}` |
  | storno via `Szamlazz.Agent` (no order) | `{slug}:by-number:{number}:storno` |

  It is queried before every create inside the same `ctx.run` closure, so a request that landed before a
  crash or timeout is found, not re-issued. There is **no generation counter**: external ids are not unique
  server-side and a query returns the newest holder, which is exactly the question asked — "what is the newest
  document of this kind we issued for this order?" A reissued invoice becomes the newest holder of the same id;
  the stornoed original stays reachable by number and through the storno's `hivszamlaszam`. Because the id is
  not unique, every found document is **validated** before it is trusted: `rendelesszam == order`, `tipus` of
  the expected kind, `teszt == account.mode`, and `szallito/id == supplier_id` when pinned; anything else is
  `conflict{external_id_collision}`.
- The **`Idempotency-Key`** of the ingress identifies a logical request; the service never relies on it for
  safety.

## Securing the Worker

Restate signs every request it makes to an SDK endpoint when the runtime is configured with a request identity
key. Register the matching `publickeyv1_...` public keys on the endpoint builder to reject unsigned requests:

```rust
let endpoint = Endpoint::builder()
    .bind(order)
    .bind(agent)
    .identity_key("publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f")?
    .identity_key("publickeyv1_ChjENKeMvCtRnqG2mrBK1HmPKufgFUc98K8B3ononQvp")?
    .build();
```

Multiple keys stay valid at once, so rotation is a deployment change: register the old and the new key, switch
the runtime to the new private key, then drop the old one. Identity keys authenticate the Restate runtime to the
worker; callers authenticate to Restate ingress separately.

## Caller Contract

1. Send an **`Idempotency-Key`** per logical request. Restate dedupes retries and attaches concurrent duplicates
   to the in-flight invocation.
2. **Any error** from an issuing or storno handler means "outcome unknown — retry with a **new** key, or read
   `Szamlazz.Order.get`", never "no document exists". Restate replays a failed invocation's stored completion
   under the same key for the retention period, so the same key would repeat the failure; the retry with a new
   key reconciles by external id and is safe. A call that timed out on the client side may still run once the
   key frees; `get` is the way to learn its outcome.
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send
   `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document is
   `conflict{live}`, so the flag can never cause a duplicate.

Retry policy ([ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md)): every handler that calls
szamlazz.hu pins its own. On `Szamlazz.Order`, `initial_interval = 2m`, factor 2, `max_interval = 10m`,
`max_attempts = 5`, `on_max_attempts = kill`, with `inactivity_timeout = 4m`, `abort_timeout = 3m`,
`journal_retention = 3d` and `idempotency_retention = 30d`; `get` uses `max_attempts = 3`.
`Szamlazz.Agent.set_payments` and `storno` use two attempts, `query` three. Kill, not pause: a paused invocation
holds the order's key and blocks the very handler that would reconcile it. Kill releases the key, and the
external-id pre-query inside every attempt is what makes that safe.

Inside a handler, the issuing loop retries transport failures and unknown outcomes up to `issue.max_attempts`
with `issue.first_backoff` doubling to `issue.max_backoff`; every attempt is query-first. When the budget is
exhausted the handler fails with `TerminalError{outcome_unknown}`; the next invocation's pre-query finds
whatever landed.

## Testing

- `cargo test -p restate-szamlazz` runs the contract, config and identity unit tests, the discovery and binding
  tests of the adapters, and the wiremock tests of the steps module against synthetic szamlazz.hu responses
  (`tests/steps.rs`: `Found` / `FoundReversed` with and without `reissue`, the 71/152 re-query, `Collision`,
  `Foreign`, storno validation including the proforma / delivery-note no-op, 335, 7).
- `cargo test -p restate-szamlazz -- --ignored e2e` runs `tests/service.rs`: the `Szamlazz.Order` Virtual Object
  end to end against a real Restate server in docker with wiremock standing in for szamlazz.hu — issued →
  already_issued, `Idempotency-Key` replay, 152 → reconciled, storno → reversed → stale create → `reissue`,
  `reissue` on live → `conflict{live}`, an external reversal, proforma auto-link and `consumed` in `get`. It
  skips with a message when the docker daemon is not reachable; set `RESTATE_ADMIN_URL` /
  `RESTATE_INGRESS_URL` to reuse a running server.
- The go-live checklist in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md)
  re-establishes the verified szamlazz.hu facts on a target account before the worker is enabled; every step
  issues real documents there.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
