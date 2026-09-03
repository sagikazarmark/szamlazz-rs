# restate-szamlazz

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz)

**Restate services issuing and managing szamlazz.hu documents with durable, idempotent execution.**

The `Order` Virtual Object — keyed by the order number — owns every document issued for one order and serialises issuing per key, so that a caller can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes, concurrent callers and reversals. The stateless `SzamlaAgent` service exposes by-number operations (query, credit entries, storno of unmanaged documents) over the same low-level layer. Both are projections of the Számla Agent model: deployment constants live in config, line totals are computed, domain outcomes are returned as data.

The design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md), the decisions behind it in [`docs/adr/`](../../docs/adr), and the szamlazz.hu behaviour it relies on in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md). A ready-made binary and container image live in [`restate-szamlazz-endpoint`](../restate-szamlazz-endpoint).

## Quick Start

Bind both services to a Restate endpoint of your own:

```rust
use std::sync::Arc;

use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::{Config, Order, SzamlaAgentService};

async fn serve(config: Arc<Config>) -> Result<(), Box<dyn std::error::Error>> {
    let order = Order::new(Arc::clone(&config))?;
    let agent = SzamlaAgentService::from_parts(Arc::clone(order.agent()), config);
    let endpoint = Endpoint::builder().bind(order).bind(agent).build();
    HttpServer::new(endpoint)
        .listen_and_serve("0.0.0.0:9080".parse()?)
        .await;
    Ok(())
}
```

`Config` only implements `Deserialize`; the host chooses the file format and environment merging (the endpoint binary uses figment). Call `Config::validate()` after parsing.

## Scope Contract

- Issues proforma, invoice, prepayment, final and corrective documents through the Számla Agent, one handler per kind, exactly once per order, kind and generation.
- Reverses (`storno`) and deletes documents it issued, idempotently, and records reversals it did not perform.
- Keeps a `Ledger` per order as the source of truth for what the service issued: numbers, ids, totals, an HMAC fingerprint of the payload and journaled timestamps — never buyer data. szamlazz.hu is consulted to verify the ledger, not to rebuild it.
- Returns domain outcomes as data and reserves errors for faults, so a caller can always tell "the document exists" from "the outcome is unknown".
- Serves one szamlazz.hu account per deployment; the account slug namespaces the external ids.

Not in scope: PDF download, receipts, taxpayer query, IPN and Adatkapcsolat ingestion, the proforma → payment → invoice lifecycle workflow, multiple prepayments per order, and reissuing on the service's own initiative. A repeat create after an external reversal returns `reversed` and issues a replacement only with `reissue: true` and a new `request_id` ([ADR 0003](../../docs/adr/0003-explicit-reissue-after-external-reversal.md)).

Prerequisite: the szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) must be ON. It is the second guard against duplicates; the ledger and the external-id pre-query are the first.

## Feature Flags

Default features enable the Restate adapters. The SDK-free contract, configuration, identity and ledger modules remain available with default features disabled.

- `service`: enables `Order`, `SzamlaAgentService`, their generated clients and the `restate-sdk` dependency. Enabled by default.
- `schemars`: derives JSON Schema for the contract types, so Restate's discovery manifest and OpenAPI export document every handler's input and output. Forwards to `restate-sdk`'s `schemars` feature when `service` is enabled.

See the [crate documentation](https://docs.rs/restate-szamlazz/latest/restate_szamlazz/) for API and feature semantics and the [generated feature graph](https://docs.rs/crate/restate-szamlazz/latest/features) for activation details.

## Key Types

- `contract::CreateRequest` / `CreateResponse`: input and output of the four `create_*` handlers (and `CorrectRequest` for correctives). The request carries a `request_id`, the `DocumentInput` (buyer, line items, dates, payment method, per-call overrides) and `CreateOptions` (`reissue`, `proforma`); the response carries the `Outcome`, the identity (`request_id`, `kind`, `generation`, `external_id`), the numbers and totals, and `warnings`.
- `contract::Outcome` / `ConflictReason`: `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or `conflict` with a reason such as `payload_mismatch`, `foreign`, `duplicate_order_number`, `proforma_live` or `prepaid_chain`.
- `contract::TerminalCode`: the four fault codes a `TerminalError` carries — `outcome_unknown`, `unavailable`, `account_mismatch`, `invalid_input`.
- `contract::StornoRequest` / `StornoResponse`, `DeleteProformaRequest`, `GetRequest` / `OrderSnapshot`, `QueryRequest` / `QueryResponse`, `SetPaymentsRequest`: the remaining handler contracts. `OrderSnapshot` is the ledger's public projection.
- `RequestId`: the caller-supplied retry identity, `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`. Ledger-only; never sent to szamlazz.hu.
- `OrderKey`: the `Order` key — the order number trimmed of leading and trailing whitespace, case preserved, validated (1–64 bytes, no control characters, no internal whitespace runs).
- `ExternalId`: the deterministic `szamlaKulsoAzon` of a document (`{slug}:{order}:{kind}:{gen}`, `{slug}:{order}:corrective:{cseq}`, `{original}:storno`).
- `Config`: the deployment configuration — `[account]` (slug, agent key, endpoint, live/test mode, supplier id pin, fingerprint secret), `[defaults]`, `[seller]`, `[issue]` (attempt budget and back-off). Secrets are `config::Secret`, whose `Debug` output is redacted.
- `Ledger`: the `Order` state — per-kind `Slot`s, corrective entries, the request-id map, a foreign hint and a bounded history. Every transition is a pure method that leaves the ledger unchanged on a precondition failure.
- `szamla_agent::SzamlaAgent`: the low-level layer over `szamlazz_agent::Client`. Plain async functions (`issue`, `verify`, `query`, `hint`, `storno`, `delete_proforma`, `set_payments`) that return every expected szamlazz.hu outcome as data. `Order` calls them inside `ctx.run`; the `SzamlaAgent` Restate service is a thin facade over the same instance. No Restate service calls another.
- `Order` / `SzamlaAgentService` (feature `service`): the Restate Virtual Object registered as `Order` and the stateless service registered as `SzamlaAgent`, with generated `OrderClient` and `SzamlaAgentServiceClient` for typed calls from other handlers.

## Identity Model

Three identities work together ([ADR 0002](../../docs/adr/0002-order-keyed-idempotency-via-external-ids.md)):

- The **order key** decides which `Order` instance runs; same-key handlers run one at a time, which is what serialises issuing per order.
- The **external id** identifies a document to szamlazz.hu. It is derived from the ledger, written to state as a `pending` slot *before* the first call, and queried before every create inside the same `ctx.run` closure, so a request that landed before a crash or timeout is found, not re-issued. External ids are not unique server-side, so every found document is validated (order number, kind, `teszt` flag, supplier id) before it is adopted.
- The **`request_id`** identifies a logical request to the service. The same id returns the entry's current state forever; a different id is a new logical request; a known id with a different payload is `conflict{payload_mismatch}`. The payload is compared through an HMAC fingerprint, never through stored buyer data.

The **generation** (`gen`) in a slot increments only on a verified reversal (invoice kinds) or a deletion or consumption (proforma). Each generation is one document identity; a reissued invoice never shares an external id with the stornoed one.

## Securing the Worker

Restate signs every request it makes to an SDK endpoint when the runtime is configured with a request identity key. Register the matching `publickeyv1_...` public keys on the endpoint builder to reject unsigned requests:

```rust
let endpoint = Endpoint::builder()
    .bind(order)
    .bind(agent)
    .identity_key("publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f")?
    .identity_key("publickeyv1_ChjENKeMvCtRnqG2mrBK1HmPKufgFUc98K8B3ononQvp")?
    .build();
```

Multiple keys stay valid at once, so rotation is a deployment change: register the old and the new key, switch the runtime to the new private key, then drop the old one. Identity keys authenticate the Restate runtime to the worker; callers authenticate to Restate ingress separately. `record_reversal` and `forget` are `ingress_private`: they are not reachable through the ingress, only from other Restate handlers.

## Retry Behavior

Every handler that calls szamlazz.hu pins its own retry policy ([ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md)): on `Order`, `initial_interval = 2m`, factor 2, `max_interval = 10m`, `max_attempts = 5`, `on_max_attempts = kill`, with `inactivity_timeout = 4m` and `abort_timeout = 3m`; `SzamlaAgent.set_payments` and `storno` use two attempts. Kill, not pause: a paused invocation holds the order's key and blocks the very handler that would reconcile it. Kill releases the key with the last committed state intact, and the `pending` slot written before the first call is what makes that safe.

Inside a handler, the issuing loop retries transport failures and unknown outcomes up to `issue.max_attempts` with `issue.first_backoff` doubling to `issue.max_backoff`; every attempt is query-first. When the budget is exhausted the slot stays `pending` and the handler fails with `TerminalError{outcome_unknown}`.

**Caller contract:** any error from an issuing or storno handler means "outcome unknown — call again with the same `request_id`, or read `Order.get`", never "no document exists". The next call pre-sleeps for the remainder of the first back-off, reconciles by external id and resumes issuing with a fresh budget. Do not rely on Restate's ingress `Idempotency-Key`: it would replay the stored failure for its retention period without re-executing. `request_id` is the retry identity, and `Order.get` is the non-blocking status check.

## Testing

- `cargo test -p restate-szamlazz --all-features` runs the pure ledger transitions, the discovery and binding tests of the adapters, and the wiremock tests of the low-level layer against synthetic szamlazz.hu responses (`tests/szamla_agent.rs`).
- `cargo test -p restate-szamlazz --all-features -- --ignored e2e` runs `tests/service.rs`: the `Order` Virtual Object end to end against a real Restate server in docker with wiremock standing in for szamlazz.hu. It skips with a message when the docker daemon is not reachable; set `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` to reuse a running server.
- The go-live checklist in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md) re-establishes the verified szamlazz.hu facts on a target account before the worker is enabled; every step issues real documents there.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
