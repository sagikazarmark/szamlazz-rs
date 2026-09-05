# restate-szamlazz-endpoint

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz-endpoint?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz-endpoint)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz-endpoint?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz-endpoint)

**Standalone endpoint hosting the szamlazz.hu services for [Restate](https://restate.dev/).**

The `restate-szamlazz` binary serves the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service of [`restate-szamlazz`](../restate-szamlazz) over HTTP/2 for a Restate server to register. It issues and reverses szamlazz.hu documents exactly once per order, keeping no state of its own — szamlazz.hu is the source of truth, reached through deterministic external ids; the design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md).

## Install

```sh
cargo install restate-szamlazz-endpoint
```

A container image is published as `ghcr.io/sagikazarmark/restate-szamlazz` on every `v*` tag:

```sh
docker run --rm -p 9080:9080 \
  -v "$PWD/restate-szamlazz.toml:/etc/restate-szamlazz.toml:ro" \
  -e CONFIG_FILE=/etc/restate-szamlazz.toml \
  -e RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY \
  ghcr.io/sagikazarmark/restate-szamlazz:latest
```

## Prerequisite

The szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) **must be ON**. The service keys everything by order number and relies on szamlazz.hu rejecting a second document of the same kind under one order number (71/152) as its second guard against duplicates — the external-id query inside every execution of the create step is the first; without the toggle a retry that lands after the first request can issue a second legal document. The verified behavior and the go-live checklist are in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md).

One deployment serves one szamlazz.hu account. A second account is a second deployment with its own `Szamlazz.Order` service.

## Configuration

The binary reads a TOML, JSON or YAML file (by extension) and applies `RESTATE_SZAMLAZZ_` environment overrides on top, with `__` separating nesting levels. Everything constant for a deployment lives here and never travels in a request payload.

```toml
identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f"]

[account]
slug = "acct"                 # the namespace: 1–16 bytes of [a-z0-9-]; prefixes every external id; permanent
agent_key = "..."             # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional; the production URL by default
mode = "live"                 # live | test — validated against <teszt> on every document found under our external ids
supplier_id = 972720          # optional pin; when set, validated against szallito/id on every document found under our external ids

[defaults]
e_invoice = false
language = "hu"
currency = "HUF"
exchange_rate_bank = "MNB"
template = "default"          # optional
send_email = false            # optional
number_prefix = "..."         # optional
extra_logo = "..."            # optional
aggregator = "..."            # optional, not overridable per call
guardian = false              # optional, not overridable per call

[seller]                      # all optional; account data used where absent
bank = "..."
bank_account = "..."
signer_name = "..."
[seller.email]
reply_to = "..."
subject = "..."
body = "..."

[issue]                       # the issue policy: the run retry policy of the create and storno steps
max_attempts = 5              # executions of the step, including the first
initial_delay = "2m"          # before the first re-execution; longer than a client timeout plus the longest observed server stall
factor = 2.0
max_delay = "10m"
max_duration = "1h"           # the hard bound on re-executing the step
```

`account.agent_key` (the Számla Agent key) is a secret. Keep it out of the file and supply it through the environment instead:

```sh
RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY="..." \
restate-szamlazz --config restate-szamlazz.toml
```

Any key can be overridden the same way (`RESTATE_SZAMLAZZ_ACCOUNT__MODE=test`, `RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS=3`, `RESTATE_SZAMLAZZ_DEFAULTS__CURRENCY=EUR`). Only `[account]` is required; `slug` and `agent_key` have no defaults. The configuration is validated at start-up and the process exits with the first violated invariant. The agent key is never logged.

## Running

```sh
restate-szamlazz --config restate-szamlazz.toml --port 9080
```

`--config` and `--port` also read `CONFIG_FILE` and `PORT`. Logging goes through `tracing` with `RUST_LOG` (default `info`). The endpoint binds `0.0.0.0:{port}` and speaks HTTP/2 only, as every Restate SDK endpoint does.

Register it with a Restate server:

```sh
restate deployments register http://host:9080
```

For local development the repository root has a `compose.yaml` with a Restate server; `docker compose up -d` starts it, `restate deployments register http://host.docker.internal:9080` registers an endpoint running on the host.

## Services

Every handler takes and returns JSON; the discovery manifest carries JSON Schemas for all of them, so Restate's OpenAPI export documents the full contract. Domain outcomes are data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or `conflict` with a `conflict_reason`.

`Szamlazz.Order` is a Virtual Object keyed by the order number (`rendelésszám`, trimmed). It keeps no state: every handler answers from szamlazz.hu through the order's deterministic external ids (`{namespace}:{order}:{kind}`, the namespace being `account.slug`), so any invocation finds what an earlier one issued. The retry identity of a request is Restate's ingress `Idempotency-Key`. Eight handlers on `Szamlazz.Order`, three on `Szamlazz.Agent`:

| Handler | Description |
|---|---|
| `Szamlazz.Order.create_proforma` | Issues the proforma (`díjbekérő`) of the order. |
| `Szamlazz.Order.create_invoice` | Issues the invoice (`számla`), converting the order's live proforma unless told otherwise (`options.proforma`: `auto`, `none` or `{"number": …}`); `options.reissue` issues a new one after a reversal. Refused with `conflict{prepaid_chain}` while a live prepayment invoice exists. |
| `Szamlazz.Order.create_prepayment` | Issues the prepayment invoice (`előlegszámla`); one per order, exclusive with the plain invoice. Takes no `options.proforma`: szamlazz.hu converts the order's live proforma by shared order number on its own. |
| `Szamlazz.Order.create_final` | Issues the final invoice (`végszámla`) settling the order's live prepayment invoice; the server does not net the prepayment into the totals. |
| `Szamlazz.Order.correct_invoice` | Issues a corrective invoice (`helyesbítő számla`) for an invoice of this order; a new `correction_id` issues a new corrective. |
| `Szamlazz.Order.storno_invoice` | Reverses (`sztornó`) an invoice of this order; idempotent. |
| `Szamlazz.Order.delete_proforma` | Deletes the order's proforma; refuses a paid one unless `force`. |
| `Szamlazz.Order.get` | What szamlazz.hu holds under the order's external ids right now (proforma, invoice, prepayment, final), each `live`, `reversed` or — a proforma — `consumed`. No input. Read-only, never blocks behind issuing. |
| `Szamlazz.Agent.query` | Queries a document by invoice number, order number or external id. |
| `Szamlazz.Agent.set_payments` | Registers credit entries (`jóváírás`) on an invoice; replaces unless `additive`. |
| `Szamlazz.Agent.storno` | Reverses an invoice that no `Szamlazz.Order` manages; a document carrying an order number is answered with `managed_by_order` instead. |

A create request through the ingress:

```sh
curl localhost:8080/Szamlazz.Order/ORD-1001/create_invoice \
  -H 'content-type: application/json' \
  -H 'idempotency-key: 8b2f6c4e-0001' \
  -d '{
    "document": {
      "buyer": { "name": "Kovács Bt.", "zip": "2030", "city": "Érd", "address": "Tárnoki út 23." },
      "items": [{ "name": "Consulting", "quantity": "1", "unit": "db", "unit_price": "1000", "vat_rate": "27" }],
      "fulfillment_date": "2026-09-03",
      "due_date": "2026-09-11",
      "payment_method": "transfer"
    }
  }'
```

**Caller contract:**

1. Send an `Idempotency-Key` per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation.
2. **Any error** from an issuing or storno handler means "outcome unknown — retry with a **new** key, or read `Szamlazz.Order.get`" (the stored completion of a failed invocation is replayed under the same key for the retention period — verified); the handler reconciles by external id, so the retry is safe. Never interpret an error as "no document exists".
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document → `conflict{live}`; the flag can never cause a duplicate.

**Faults.** Errors are `TerminalError`s with a JSON body `{ "code", "message", "order"?, "kind"?, "external_id"? }`; the ingress reports them with the HTTP status below and `x-restate-error-source: invocation`. Every one of them means "outcome unknown" (rule 2), never "no document exists".

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `invalid_input` | 400 | The request is malformed or names a document szamlazz.hu does not know. | Fix the request. |
| `account_mismatch` | 409 | A document carrying this order's number belongs to another szamlazz.hu account (`teszt` or `szallito/id` differ). | Check `account.mode` / `account.supplier_id`; do not retry blindly. |
| `outcome_unknown` | 500 | The create or storno step ran out of its `[issue]` policy while a document may or may not have been issued. | Retry with a new `Idempotency-Key` or read `get`. |
| `unavailable` | 503 | szamlazz.hu could not be reached for a check that must succeed before anything is sent. | Retry with a new `Idempotency-Key` later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (codes 3 invalid credentials, 135 browser session active, 136 login blocked, 164 multiple accounts). The execution that raised it **issued nothing** (szamlazz.hu answers these codes before acting on a request); an earlier one may have landed with a lost reply. The worker logs a `warn` with the namespace and the code. | Page the operator: fix `account.agent_key` (or the account state on szamlazz.hu). Then retry with a new `Idempotency-Key` or read `get`. |

A 503 whose `x-restate-error-source` is `invocation` is **this worker's** answer — `unavailable` or `credentials_rejected` — not the Restate ingress being down. Restate's [HTTP invocation docs](https://docs.restate.dev/invoke/http#retrying-requests) say to treat `invocation` errors as non-retryable and to auto-retry a `5xx` only when its source is `ingress` (or absent); do that here as well: page on an `invocation` 503 instead of retrying into it — `credentials_rejected` in particular repeats identically until the deployment is fixed — and only then retry with a new `Idempotency-Key`.

Handlers that call szamlazz.hu kill the invocation after five attempts (2 m → 10 m back-off) rather than pausing, so a stuck order never blocks its own recovery. Issuing itself is a read-only lookup step and a create step whose every execution — Restate re-executes it under the `[issue]` policy while szamlazz.hu's answer is unknown — queries the external id before it sends; that query is what the next call reconciles against, and an exhausted create step is a structured `outcome_unknown` naming the order, kind and external id. Storno has the same two steps under the same policy. A killed invocation also reaches the caller as HTTP 500 with `x-restate-error-source: invocation`, carrying the last retryable error's message. See [ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md) and [ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md).

## Request Identity

Restate signs every request it makes to a service endpoint when the runtime is configured with a request identity key. `identity_keys` lists the matching `publickeyv1_...` public keys; with at least one key configured the endpoint rejects unsigned requests. Multiple keys stay valid at once, so rotation is a config change: add the new key, switch the runtime to the new private key, then drop the old one. The environment override accepts a comma-separated list:

```sh
RESTATE_SZAMLAZZ_IDENTITY_KEYS="publickeyv1_old,publickeyv1_new" restate-szamlazz --config restate-szamlazz.toml
```

Without `identity_keys` the endpoint accepts unsigned requests. Identity keys authenticate the Restate runtime to this endpoint; callers authenticate to Restate ingress separately.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
