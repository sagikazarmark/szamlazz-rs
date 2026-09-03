# restate-szamlazz-endpoint

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz-endpoint?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz-endpoint)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz-endpoint?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz-endpoint)

**Standalone endpoint hosting the szamlazz.hu services for [Restate](https://restate.dev/).**

The `restate-szamlazz` binary serves the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service of [`restate-szamlazz`](../restate-szamlazz) over HTTP/2 for a Restate server to register. It issues, reverses and reconciles szamlazz.hu documents exactly once per order; the design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md).

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
  -e RESTATE_SZAMLAZZ_ACCOUNT__FP_SECRET \
  ghcr.io/sagikazarmark/restate-szamlazz:latest
```

## Prerequisite

The szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) **must be ON**. The service keys everything by order number and relies on szamlazz.hu rejecting a second document of the same kind under one order number as its second guard against duplicates; without the toggle a retry that lands after the first request can issue a second legal document. The verified behaviour and the go-live checklist are in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md).

One deployment serves one szamlazz.hu account. A second account is a second deployment with its own `Szamlazz.Order` service.

## Configuration

The binary reads a TOML, JSON or YAML file (by extension) and applies `RESTATE_SZAMLAZZ_` environment overrides on top, with `__` separating nesting levels. Everything constant for a deployment lives here and never travels in a request payload.

```toml
identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f"]

[account]
slug = "acct"                 # 1–16 chars [a-z0-9-]; namespaces external ids
agent_key = "..."             # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional; the production URL by default
mode = "live"                 # live | test — validated against <teszt> on every adopted document
supplier_id = 972720          # optional pin; otherwise learned from the first query and stored in the ledger
fp_secret = "..."             # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNT__FP_SECRET; rotation invalidates stored fingerprints

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

[issue]
max_attempts = 5
first_backoff = "2m"
max_backoff = "10m"
detect_foreign = true         # the hint is mandatory when options.proforma == ledger regardless
```

`account.agent_key` (the Számla Agent key) and `account.fp_secret` (the HMAC key of the payload fingerprint) are secrets. Keep them out of the file and supply them through the environment instead:

```sh
RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY="..." \
RESTATE_SZAMLAZZ_ACCOUNT__FP_SECRET="..." \
restate-szamlazz --config restate-szamlazz.toml
```

Any key can be overridden the same way (`RESTATE_SZAMLAZZ_ACCOUNT__MODE=test`, `RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS=3`, `RESTATE_SZAMLAZZ_DEFAULTS__CURRENCY=EUR`). Only `[account]` is required; `slug`, `agent_key` and `fp_secret` have no defaults. The configuration is validated at start-up and the process exits with the first violated invariant. Neither secret is ever logged.

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

`Szamlazz.Order` is a Virtual Object keyed by the order number (`rendelésszám`, trimmed). Every issuing handler takes a `request_id`: the retry identity. The same id returns the entry's current state forever; a different id is a new logical request; a known id with a different payload is `conflict{payload_mismatch}`.

| Handler | Description |
|---|---|
| `Szamlazz.Order.create_proforma` | Issues the proforma (`díjbekérő`) of the order. |
| `Szamlazz.Order.create_invoice` | Issues the invoice (`számla`), optionally converting the order's proforma (`options.proforma`). |
| `Szamlazz.Order.create_prepayment` | Issues the prepayment invoice (`előlegszámla`); one per order. |
| `Szamlazz.Order.create_final` | Issues the final invoice (`végszámla`) settling the committed prepayment. |
| `Szamlazz.Order.correct_invoice` | Issues a corrective invoice (`helyesbítő számla`) for an invoice managed by this order; a new `request_id` issues a new corrective. |
| `Szamlazz.Order.storno_invoice` | Reverses (`sztornó`) an invoice managed by this order; idempotent. |
| `Szamlazz.Order.delete_proforma` | Deletes the order's proforma; refuses a paid one unless `force`. |
| `Szamlazz.Order.get` | The order's ledger as recorded, or with `verify` after checking every committed document against szamlazz.hu. Read-only, never blocks behind issuing. |
| `Szamlazz.Order.record_reversal` | Operator assertion that a recorded document is reversed or live (private: not reachable from the ingress). |
| `Szamlazz.Order.forget` | Operator drop of a slot whose document szamlazz.hu no longer knows (private). |
| `Szamlazz.Agent.query` | Queries a document by invoice number, order number or external id. |
| `Szamlazz.Agent.set_payments` | Registers credit entries (`jóváírás`) on an invoice; replaces unless `additive`. |
| `Szamlazz.Agent.storno` | Reverses an invoice that no `Szamlazz.Order` manages; a document carrying an order number is answered with `managed_by_order` instead. |

A create request through the ingress:

```sh
curl localhost:8080/Szamlazz.Order/ORD-1001/create_invoice \
  -H 'content-type: application/json' \
  -d '{
    "request_id": "8b2f6c4e-0001",
    "document": {
      "buyer": { "name": "Kovács Bt.", "zip": "2030", "city": "Érd", "address": "Tárnoki út 23." },
      "items": [{ "name": "Consulting", "quantity": "1", "unit": "db", "unit_price": "1000", "vat_rate": "27" }],
      "fulfillment_date": "2026-09-03",
      "due_date": "2026-09-11",
      "payment_method": "transfer"
    }
  }'
```

**Caller contract:** any error from an issuing or storno handler means "outcome unknown — re-call with the same `request_id` or read `Szamlazz.Order.get`", never "no document exists". Handlers that call szamlazz.hu kill the invocation after five attempts (2 m → 10 m back-off) rather than pausing, so a stuck order never blocks its own recovery; the `pending` slot written before the first call is what the next call reconciles against. Do not rely on Restate's ingress `Idempotency-Key`: it would replay the failure for its retention period. `request_id` is the retry identity. See [ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md).

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
