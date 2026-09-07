# restate-szamlazz-endpoint

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz-endpoint?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz-endpoint)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz-endpoint?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz-endpoint)

**Standalone endpoint hosting the szamlazz.hu services for [Restate](https://restate.dev/).**

The `restate-szamlazz` binary serves the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service of [`restate-szamlazz`](../restate-szamlazz) over HTTP/2 for a Restate server to register. It issues and reverses szamlazz.hu documents exactly once per order, keeping no state of its own — szamlazz.hu is the source of truth, reached through deterministic external ids; the design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md).

**What Restate gives you.** You call an HTTP endpoint — `POST /restate/call/Szamlazz.Order/{order}/create_invoice` with a JSON body — from any language, and Restate runs the worker durably: it locks the order number so two calls for the same order never issue at once, journals every step so a crash or a redeploy resumes where the worker left off, retries szamlazz.hu on your behalf while it is down, and deduplicates your retries by an `Idempotency-Key` header. What you get back is either a domain outcome as data (`issued`, `already_issued`, `reversed`, `conflict`, … with the invoice number and totals — HTTP 200) or a fault with a `code` you branch on. You need no Restate SDK and no Rust: this README is the whole contract.

**Start here.** Calling the services from your application → [Quick start](#quick-start), then [Services](#services) (the request and response reference, faults, the caller contract). Deploying and operating → [Configuration](#configuration), [Multi-account mode](#multi-account-mode), [Running](#running). Embedding the services in a Rust endpoint of your own → the [library README](../restate-szamlazz/README.md). Why it is built this way → the [design](../../docs/design/restate-szamlazz.md) and the [ADRs](../../docs/adr).

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

The image runs the binary as the non-root user `nonroot` (uid and gid 65532; numeric in the image, so Kubernetes' `runAsNonRoot` verifies it), with no writable filesystem needed. A mounted configuration file must be readable by that uid — world-readable is fine once the agent key comes from the environment, as above. It stops cleanly on `docker stop` (see [Stopping](#stopping)).

## Quick start

From nothing to a first invoice on a szamlazz.hu test account, on one machine.

**1. A szamlazz.hu account with an agent key.** Register at [szamlazz.hu](https://www.szamlazz.hu) and switch the account to test mode (*Tesztfiók*: the account owner turns it on and off — "a tesztfiókot a felhasználó be- és kikapcsolhatja", [szamlazz.hu docs](https://docs.szamlazz.hu/hu/agent/basics/details); a test account issues documents that are not legal invoices, and szamlazz.hu asks for at most 100 documents an hour on one). Then generate a Számla Agent key: log in as the owner or an administrator, scroll to the bottom of the dashboard (*vezérlőpult*) to the **Számla Agent kulcsok** section and click the key icon — the new key appears at once ([szamlazz.hu docs](https://docs.szamlazz.hu/hu/agent/basics/agent-key)). The key is API-only; it cannot log into the website.

**2. The account setting the worker relies on.** Turn **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) **on**: *Beállítások → Fiók beállításai → Számlázás beállítások*, section *Számlaszerkesztő beállítások* ([szamlazz.hu docs](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number)). Why it must be on is under [Prerequisite](#prerequisite). To confirm it is, issue two documents under one order number with different content and expect the second to fail with code 152 — one line with the [`szamlazz` CLI](../szamlazz-cli) and its example request (`SZAMLAZZ_AGENT_KEY` set; the two documents are test-account documents you then storno):

```sh
for name in "Kovács Bt." "Kovács Bt. (2)"; do jq --arg n "$name" '.buyer.name = $n' examples/invoice.json | szamlazz invoice create -f - --json; done   # the second answers error 152
```

**3. A minimal configuration.** `namespace` and the account's `id` and `agent_key` are all the binary needs; everything else has a default (the full reference is under [Configuration](#configuration)). `identity_keys` is production-only — it makes the endpoint refuse requests a Restate server did not sign — and is left out here:

```toml
namespace = "acct"            # prefixes every external id the worker writes to szamlazz.hu; pick once, never change

[account]
id = "acme"
agent_key = "..."             # or RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY in the environment
mode = "test"                 # the account is a test account; "live" (the default) for a real one
```

**4. Start Restate and the endpoint, register the endpoint.** The repository root has a `compose.yaml` with a Restate server (ingress on `:8080`, admin API and UI on `:9070`, the three experimental flags multi-account mode needs); `docker compose up -d` starts it. Start the endpoint on the host and register it with the server — with the [`restate` CLI](https://docs.restate.dev/installation) or with a plain call to the admin API:

```sh
RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY="..." restate-szamlazz --config restate-szamlazz.toml   # listens on 0.0.0.0:9080

restate deployments register http://host.docker.internal:9080
# or, without the CLI:
curl localhost:9070/deployments -H 'content-type: application/json' \
  -d '{"uri": "http://host.docker.internal:9080"}'
```

The Restate UI at <http://localhost:9070> lists the two services and every invocation with its journal — the debugging tool for everything below.

**5. Check the account.** `Szamlazz.Agent.check_account` runs the same prologue every handler runs and one read-only query; it issues nothing. On this single-account configuration it is called unscoped; a multi-account deployment calls it under each scope ([deploy checklist](#deploy-checklist)):

```sh
curl -X POST localhost:8080/restate/call/Szamlazz.Agent/check_account
```

```json CheckAccountResponse
{ "scope": null, "account": { "id": "acme", "mode": "test", "supplier_id": null }, "namespace": "acct", "credentials": { "state": "ok" } }
```

`credentials: {"state": "ok"}` means szamlazz.hu accepted the key. `{"state": "rejected", "code": "3", …}` means it did not — fix the key; that answer is data, not an error.

**6. Issue the first invoice.** A create request through the ingress (`/restate/call/{service}/{key}/{handler}`; the key is the order number, trimmed):

```sh
curl localhost:8080/restate/call/Szamlazz.Order/ORD-1001/create_invoice \
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

```json CreateResponse
{
  "outcome": "issued",
  "conflict_reason": null,
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": "E-TST-2026-123",
  "storno_number": null,
  "net_total": "1000",
  "gross_total": "1270",
  "outstanding": "1270",
  "customer_account_url": "https://www.szamlazz.hu/szamla/fiok?p=...",
  "existing_number": null,
  "code": null,
  "message": null,
  "warnings": []
}
```

Run the same `curl` again — same `idempotency-key` — and Restate answers the stored response without touching szamlazz.hu; run it with a new key and the worker finds the invoice under its external id and answers `already_issued` with the same number. Everything a caller needs from here — every request body, every response, every fault — is under [Services](#services).

## Prerequisite

The szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) **must be ON** on every account the deployment issues for — *Beállítások → Fiók beállításai → Számlázás beállítások*, section *Számlaszerkesztő beállítások*; the receipt editor has a toggle of the same name that does not matter here. The service keys everything by order number and relies on szamlazz.hu rejecting a second document of the same kind under one order number (71/152) as its second guard against duplicates — the external-id query inside every execution of the create step is the first; without the toggle a retry that lands after the first request can issue a second legal document. The verified behavior and the go-live checklist are in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md).

One deployment serves one szamlazz.hu account unscoped (`[account]`) **or** any number of accounts selected per request by the Restate scope (`[accounts.<scope>]`); see [Multi-account mode](#multi-account-mode).

## Configuration

The binary reads a TOML, JSON or YAML file (by extension) and applies `RESTATE_SZAMLAZZ_` environment overrides on top, with `__` separating nesting levels. Everything constant for a deployment lives here and never travels in a request payload: the deployment-level settings at the top (`namespace`, `[issue]`, `[read]`, `[resolve]`) and the szamlazz.hu account under `[account]` — or, in [multi-account mode](#multi-account-mode), the accounts under `[accounts.<scope>]`. The full reference, every key with its default:

```toml
identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f"]   # production: the Restate server's request identity keys (see Request Identity)
namespace = "acct"            # 1–16 bytes of [a-z0-9-]; prefixes every external id ({namespace}:{order}:{kind}); permanent

[issue]                       # optional; the issue policy: the run retry policy of the create and storno steps
max_attempts = 5              # executions of the step, including the first
initial_delay = "2m"          # before the first re-execution; longer than a client timeout plus the longest observed server stall — at least 90s, or the endpoint refuses to start
factor = 2.0
max_delay = "10m"
max_duration = "1h"           # the hard bound on re-executing the step

[read]                        # optional; the read policy: the run retry policy of every read-only step (lookups, verifies, hints, get, query, query_taxpayer, check_account)
max_attempts = 5              # executions of the step, including the first
initial_delay = "5s"          # before the first re-execution
factor = 2.0
max_delay = "60s"
max_duration = "5m"           # the hard bound; a szamlazz.hu outage is tolerated for this long, not for the handlers' attempts

[resolve]                     # optional; the resolve policy: the run retry policy of the `account` step (no attempt cap)
initial_delay = "1s"
factor = 2.0
max_delay = "10s"
max_duration = "1m"

[account]
id = "acme"                   # the account's identifier as the worker knows it; journaled with every invocation, shown in the Restate UI
agent_key = "..."             # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional; the production URL by default. http or https, no user:password@ (a load error); plain http off loopback is a start-up warn
mode = "live"                 # live | test — validated against <teszt> on every document found under our external ids
supplier_id = 972720          # optional pin; when set, validated against szallito/id on every document found under our external ids

[account.defaults]            # all optional
e_invoice = false
language = "hu"
currency = "HUF"
exchange_rate_bank = "MNB"
template = "default"
send_email = false
number_prefix = "..."
extra_logo = "..."
aggregator = "..."            # not overridable per call
guardian = false              # not overridable per call

[account.seller]              # all optional; account data used where absent
bank = "..."
bank_account = "..."
signer_name = "..."
[account.seller.email]
reply_to = "..."
subject = "..."
body = "..."
```

`account.agent_key` (the Számla Agent key) is a secret. Keep it out of the file and supply it through the environment instead:

```sh
RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY="..." \
restate-szamlazz --config restate-szamlazz.toml
```

Any key can be overridden the same way (`RESTATE_SZAMLAZZ_ACCOUNT__MODE=test`, `RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS=3`, `RESTATE_SZAMLAZZ_READ__MAX_ATTEMPTS=5`, `RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY=EUR`). An environment value is read as the **string** it was set to, and the key's type decides what it means: `3` is a count on `max_attempts`, `1.5` a factor, `true` a flag, `90` ninety seconds on a duration — and an agent key is taken exactly as written, so an all-digit key keeps its leading zeros (`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY=0071234` reaches szamlazz.hu as `0071234`; no quoting needed). Durations are `"90s"`, `"2m"`, `"1h"` or a bare non-negative integer of seconds, in the file and in the environment alike.

`namespace` and exactly one of `[account]` or `[accounts.<scope>]`, each account with `id` and `agent_key`, are required; everything else has a default. The agent key is never logged; the start-up log names the namespace, whether the deployment is scoped, and — per account — its scope (or `<unscoped>`), `id`, `mode`, `endpoint` and `supplier_id`. An `endpoint` is an `http` or `https` URL with a host and **no userinfo**: `https://user:password@host/` is a load error, because the endpoint is journaled with the account (shown in the Restate UI for the retention period) and printed in that start-up log. Plain `http` is allowed — a local mock, a proxy — but the agent key travels in the request body, so an `http` endpoint on a host other than loopback is logged at `warn` at start-up as sending it in cleartext.

**The configuration is strict.** A key the binary does not know — at any level: the top level, a policy table, an account table, its `defaults`, `seller` or `seller.email` — is refused at start-up with an error naming the key, its path, where it came from and what is accepted there, instead of being ignored and leaving the setting at its default:

```
Error: invalid configuration

Caused by:
    2 unknown keys:
      unknown key `account.mod` in restate-szamlazz.toml TOML file; expected one of `id`, `agent_key`, `endpoint`, `mode`, `supplier_id`, `defaults`, `seller`
      unknown key `isue` (RESTATE_SZAMLAZZ_ISUE__MAX_ATTEMPTS) in environment variables; expected one of `namespace`, `issue`, `read`, `resolve`, `account`, `accounts`, `identity_keys`
```

Every unknown key is reported at once. A value of the wrong type is refused the same way, naming the key and the source (`invalid type: found string "three", expected u32 for key "RESTATE_SZAMLAZZ_ISSUE__MAX_ATTEMPTS" in environment variables`); `[account]` together with a non-empty `[accounts]` names both tables and where each came from — the case of a stray `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` left over after the [flag day](#single--multi-flag-day), which would otherwise materialise a partial `[account]`. The pre-release layout — `account.slug` for the namespace, top-level `[defaults]` and `[seller]` tables — is refused with the same error, each moved key named with where it went. Then the invariants are checked (`WorkerConfig::validate`, the static resolver's account rules) and the process exits with the first violated one. One of them is a floor rather than a shape: `issue.initial_delay` must be at least 90 s — the Számla Agent client's 60 s request timeout plus a 30 s margin — because a create or storno step that is re-executed sooner would query for the cut execution's send while that send may still be in flight (szamlazz.hu has been seen to stall for a minute and still issue). `initial_delay = "5s"` is refused with `issue.initial_delay (5s) must be at least 90s — …`, naming the rule; `[read]` has no floor because a read writes nothing, `[resolve]` none because it never reaches szamlazz.hu.

**Check a configuration without starting.** `--check-config` loads and validates the configuration, builds the endpoint — so an account or identity-key error surfaces too — logs the same summary the start-up log prints and exits 0 without listening; an invalid configuration exits non-zero with the error. Run it in CI and as an init container before the real process:

```sh
$ restate-szamlazz --check-config --config restate-szamlazz.toml
INFO restate_szamlazz: loaded szamlazz.hu account configuration namespace=acct scoped=false accounts=1
INFO restate_szamlazz: szamlazz.hu account scope="<unscoped>" account=acme mode=Live endpoint=https://www.szamlazz.hu/szamla/ supplier_id=Some(972720)
INFO restate_szamlazz: bound Restate service service=Szamlazz.Order kind=VirtualObject handlers=8
INFO restate_szamlazz: bound Restate service service=Szamlazz.Agent kind=Service handlers=5
INFO restate_szamlazz: configuration is valid; not listening (--check-config)
```

The two example files under [`fixtures/`](fixtures) — the single-account configuration above and the multi-account one below — are what the test suite runs `--check-config` against.

## Multi-account mode

Several szamlazz.hu accounts in one deployment, selected per request by the **Restate scope**: the caller addresses `/restate/scope/{scope}/call/Szamlazz.Order/{order}/{handler}` (and `/restate/scope/{scope}/call/Szamlazz.Agent/{handler}`), and the worker resolves the scope to the account configured under `[accounts.<scope>]`. Restate namespaces the Virtual Object key and the `Idempotency-Key` per scope, so two accounts' orders never share a lock or a stored response — the same order number under two scopes is two `Szamlazz.Order` instances. The scope is the only channel for the account: no header, body field or key prefix selects it.

```toml
namespace = "acct"            # one namespace for the deployment; every account's external ids share it

[accounts.acme]               # reachable as /restate/scope/acme/call/…
id = "acme"
agent_key = "acme-key"        # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNTS__ACME__AGENT_KEY
supplier_id = 972720          # optional pin: the seller record's id (szallito/id) on the account's documents
mode = "live"

[accounts.acme.seller]
bank_account = "..."

[accounts.beta_events]        # reachable as /restate/scope/beta_events/call/…
id = "beta"
agent_key = "beta-key"        # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNTS__BETA_EVENTS__AGENT_KEY
```

`[account]` and `[accounts.<scope>]` are mutually exclusive: both present is a load error, and there is no default account. In this shape an **unscoped** request is `unknown_account` (400); in the single-account shape a **scoped** one is. The configuration is validated at start-up against the checkable half of the safety contract — one szamlazz.hu account is reachable under exactly one scope — and the process exits on: two accounts sharing an `(endpoint, agent_key)` pair, two sharing an `id` (the credential reference), two pinning the same `supplier_id`, or a scope key outside `[a-z0-9_]` / longer than 36 bytes.

**`supplier_id` is an optional pin, in this shape too.** It is `szallito/id` — szamlazz.hu's id for the seller record printed on every document the account issues, 972720 on the szamlazz.hu test account; read it off any of the account's documents with `Szamlazz.Agent.query` or `szamlazz invoice get --json` (`.supplier.id`). When set, every document a handler finds is checked against it and a mismatch is `account_mismatch` (409) or `conflict{external_id_collision}` — which catches an agent key configured under the wrong scope on the first found document, something `mode` alone cannot. The worker cannot verify the value itself (szamlazz.hu has no "which account am I?" operation, and `check_account` finds no document), so it is a fact you record, not one the worker establishes; leave it unset until you have read it off a real document rather than guess it.

**Scope format.** The static resolver's scope keys are `[a-z0-9_]`, 1–36 bytes — a strict subset of Restate's scope format (`[a-zA-Z0-9_.-]`, non-empty, at most 36 characters — ASCII, so bytes; a dashed UUID is exactly 36), chosen so that environment overrides can address them (`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY`; figment lowercases the segment). This is the constraint on the account identifiers your application uses as scopes with this binary; a deployment with its own `AccountResolver` may use Restate's full format.

**The scope is routing, not authorization.** Anyone who can reach the ingress under a scope issues on that account. Put the ingress behind a gateway that sets the scope from the authenticated identity, never forwards a caller-supplied scope path, and strips `x-restate-*` request headers (the ingress lets a caller's copy of one of its own headers win). Limit keys therefore travel as the `limit-key` query parameter, or are set by the gateway. The seven rules the worker relies on — one account under exactly one scope, an append-only mapping, a permanent namespace, order keys unique within an account, the caller recording key and scope as used, routing-not-authorization, ownership by external id alone — are in the [library README](../restate-szamlazz/README.md#scope-contract) and [ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md).

**Experimental Restate flags.** Scoped calls need `vqueues`, `protocol_v7` and `scoped_virtual_objects` on the server (`RESTATE_EXPERIMENTAL_ENABLE_*`; `compose.yaml` sets the three). They are experimental at Restate 1.7.8 — the server source calls `vqueues` "in heavy development" and scoped Virtual Objects "not officially supported in v1.7", while the docs present flow control as opt-in whose "configuration and APIs may change" — and the facts here were verified on server 1.7.8 with SDK 0.12.0; re-run the [deploy checklist](#deploy-checklist) after every server upgrade. ADR 0006 records the quotes, the Restate Cloud confirmation and the flagless contingency.

**Kafka ingress** is untested and unsupported in this mode. The server has a `kafka_scope` flag that scopes a record from an `x-restate-scope` record header, so "arrives unscoped" is not the reason — no scenario exercises it, and the record's scope would be set by the producer, outside the gateway above. Verify it yourself before wiring a subscription.

### Deploy checklist

After every deploy or configuration change, call `Szamlazz.Agent.check_account` **under each configured scope** (unscoped on a single-account deployment). It runs the prologue like every handler, sends one read-only query of a sentinel external id that nothing the service issues carries, and answers with what the SDK saw, the *configured* account, the namespace and whether szamlazz.hu accepted the credentials. It issues nothing.

```sh
# a multi-account deployment: once per scope
curl -X POST localhost:8080/restate/scope/acme/call/Szamlazz.Agent/check_account
# {"scope":"acme","account":{"id":"acme","mode":"live","supplier_id":972720},"namespace":"acct","credentials":{"state":"ok"}}

# a single-account deployment: unscoped
curl -X POST localhost:8080/restate/call/Szamlazz.Agent/check_account
# {"scope":null,"account":{"id":"acme","mode":"live","supplier_id":972720},"namespace":"acct","credentials":{"state":"ok"}}
```

| Answer | Meaning |
|---|---|
| `200` with `scope` equal to the scope you called under (`null` on an unscoped call to a single-account deployment), the expected `account.id`, and `credentials: {"state": "ok"}` | The scope reaches the worker, resolves to the intended account, and its agent key works. |
| `200` with `credentials: {"state": "rejected", "code", "message"}` | Resolution is right; szamlazz.hu refused the key (3 invalid credentials, 135 browser session active, 136 login blocked, 164 multiple accounts). Fix `agent_key` or the account's state on szamlazz.hu. Data, not a fault. |
| `200` with `"scope": null` under a **scoped** call | **Stop.** The server accepted the scoped path but did not forward the scope: `protocol_v7` is off (the ingress gates a scoped path on `vqueues` and `scoped_virtual_objects` only). On a single-account deployment every scoped request would issue on the one account. Enable `RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7` and probe again before opening the services. |
| `400 unknown_account` | The scope names no account (or the request is unscoped on a multi-account deployment). Fix the configuration or the address. |
| `503 unavailable` | szamlazz.hu did not answer the probe through the `[read]` policy, or the resolver or the credential store could not be reached; call again. |

Credential acceptance is the only szamlazz.hu-verified fact in the answer: the supplier id appears only in found-document bodies, so a not-found probe cannot cross-check `supplier_id` — it echoes the configuration. A wrong `mode` or `supplier_id` surfaces on the first document found under the account, on any handler that finds one (`account_mismatch` by number — `Szamlazz.Agent.query` is the likeliest first — `conflict{external_id_collision}` under an external id), not here. The probe is the only defence against the `protocol_v7`-off case: the worker has no per-request signal of "was this call scoped?" that it is willing to depend on (the ingress's `x-restate-ingress-path` header is undocumented and caller-overridable), so run the probe under every scope before you open the services, and after every server upgrade.

### Single → multi flag day

Going from `[account]` to `[accounts.<scope>]` is a configuration change plus a caller change, with **no data migration**: the namespace stays, so the first scoped create for an already-invoiced order finds its document under the unchanged external id. What must not happen is one szamlazz.hu account being reachable under two identities at once (unscoped *and* under its scope), which would split an order's lock across two Virtual Objects — so drain first, switch, then resume. Scripted against the admin API (`:9070`) and the `restate` CLI:

```sh
# 1. Make both services private: the ingress refuses new calls (400) without creating invocations.
curl -X PATCH localhost:9070/services/Szamlazz.Order -H 'content-type: application/json' -d '{"public": false}'
curl -X PATCH localhost:9070/services/Szamlazz.Agent -H 'content-type: application/json' -d '{"public": false}'

# 2. Drain: wait until nothing is in flight (the SQL introspection API; the same query the e2e harness polls).
until [ "$(curl -s localhost:9070/query -H 'accept: application/json' -H 'content-type: application/json' \
      -d '{"query": "SELECT count(*) AS n FROM sys_invocation WHERE status <> '"'"'completed'"'"'"}' | jq -r '.rows[0].n')" = "0" ]; do sleep 2; done

# 3. Switch the configuration — keep `namespace`; move the account under `[accounts.<scope>]` and add `supplier_id` —
#    and register the new revision (a new deployment URI; the old revision serves nothing once drained).
restate deployments register http://host:9081

# 4. Point callers at scoped paths: /restate/scope/{scope}/call/Szamlazz.Order/{order}/{handler}.

# 5. Make the services public again.
curl -X PATCH localhost:9070/services/Szamlazz.Order -H 'content-type: application/json' -d '{"public": true}'
curl -X PATCH localhost:9070/services/Szamlazz.Agent -H 'content-type: application/json' -d '{"public": true}'

# 6. Probe every scope (the deploy checklist above): each answers its account with credentials ok.
for scope in acme beta_events; do
  curl -X POST "localhost:8080/restate/scope/$scope/call/Szamlazz.Agent/check_account"
done
```

The mapping is append-only: moving traffic to another szamlazz.hu account means a new scope, never re-pointing an existing one (re-pointing is forbidden, not merely undrained). The same drain–switch–resume applies to any change that could put one szamlazz.hu account under two identities at once; appending an account cannot, so adding one is steps 3 and 6 alone — register the revision, probe the new scope. The end-to-end suite performs this flag day on a live Restate server (`tests/service.rs`, phase 2).

## Running

```sh
restate-szamlazz --config restate-szamlazz.toml --bind 0.0.0.0 --port 9080
```

`--config`, `--bind` and `--port` also read `CONFIG_FILE`, `BIND_ADDR` and `PORT`. Logging goes through `tracing` with `RUST_LOG` (default `info`). Every handler execution runs inside one span, `execution{scope, order, restate.invocation.id, account.id}` — the scope the request arrived under (`<unscoped>` when none), the order key (absent on `Szamlazz.Agent`), the invocation id the caller got as `x-restate-id`, and the id of the account the request resolved to — so every line the worker logs during an invocation, the `credentials_rejected` warning included, says which account and which invocation it is about; correlate a caller's fault by its `x-restate-id` with `restate.invocation.id` in the log and with `id` in Restate's `sys_invocation`. The agent key appears in no log line. The endpoint binds `{bind}:{port}` — `0.0.0.0:9080` by default; `--bind 127.0.0.1` keeps it off the network, `--port 0` takes an ephemeral port; the start-up log names the bound address — and speaks HTTP/2 only, as every Restate SDK endpoint does. `--check-config` validates and exits instead (see [Configuration](#configuration)).

Register it with a Restate server — the `restate` CLI ([install](https://docs.restate.dev/installation)) or the admin API's `POST /deployments` do the same thing:

```sh
restate deployments register http://host:9080
curl localhost:9070/deployments -H 'content-type: application/json' -d '{"uri": "http://host:9080"}'
```

For local development the repository root has a `compose.yaml` with a Restate server; `docker compose up -d` starts it, and `http://host.docker.internal:9080` is the URI of an endpoint running on the host (see [Quick start](#quick-start)).

### Stopping

`SIGTERM` or `SIGINT` — `docker stop`, a Kubernetes rollout, `kill`, Ctrl-C — stops the process cleanly: it stops accepting connections, gives the open ones up to 10 s to finish (the Rust SDK's connection drain; the bound is the SDK's, not configurable here) and exits 0. The start-up log names the signals (`stop_on="SIGTERM, SIGINT"`) and a stop logs which one arrived (`signal="SIGTERM"`). With nothing in flight the process is gone within milliseconds. The container image runs the binary as PID 1 with `STOPSIGNAL SIGTERM`, so `docker stop` and the kubelet reach it directly.

**In-flight invocations.** An invocation whose connection the stop cuts — one still running when the connection drain ends — is neither lost nor duplicated. Restate keeps it and re-dispatches it after the handler's retry interval (`initial_interval`: 2 m on every handler that writes to szamlazz.hu — every `Szamlazz.Order` handler but `get`, `Szamlazz.Agent.set_payments` and `Szamlazz.Agent.storno` — longer than the 60 s client timeout, so neither an additive send nor a storno's leading query can run while the first send is still in flight; 10 s on `Szamlazz.Agent.query`, `query_taxpayer` and `check_account`; the server's ~500 ms default on `get`, which only reads); the re-execution replays the journal up to the open step and runs that step again. For a cut create step this is exactly what the query-first closure exists for: the re-execution queries the external id before sending and finds what the cut execution sent — `issued`, never a second document ([ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md); design §5). The cost is the delay and one of the invocation's attempts (five on the `Szamlazz.Order` 2 m handlers and on `Szamlazz.Agent.storno`, two on `set_payments`; the last kills it). Only a worker-side failure like this cut spends an attempt: a step Restate re-executes under the `[issue]`, `[read]` or `[resolve]` policy is re-dispatched without advancing the count (verified end to end against 1.7.8; ADR 0004, #87), so the attempt budget is the worker-outage budget — the four re-dispatches of a `Szamlazz.Order` write are 2 → 4 → 8 → 10 min, about 24 min of back-off — and the run policies are the szamlazz.hu-outage budget. The create step is also the longest thing a stop can cut — a query, a send and a re-query at up to 60 s each, ~180 s in the worst case, well past the connection drain — so a rollout while orders are issuing will cut some of them; letting them finish would take a longer drain than the SDK offers.

**Grace period.** Give the process the whole connection drain plus a margin, so it is never `SIGKILL`ed mid-drain: at least 15 s. Kubernetes' default `terminationGracePeriodSeconds: 30` is fine; Docker's default `docker stop -t 10` is the bare bound, so pass `-t 15` (or `stop_grace_period: 15s` in Compose) where the endpoint carries traffic. A longer grace period buys nothing: the connection drain ends at 10 s regardless.

**After a worker outage.** Invocations that arrived or were cut while the process was down sit `backing-off` between re-dispatches; once the process is back, `restate invocations resume Szamlazz.Order` (or `PATCH /invocations/{id}/resume` per invocation) pulls them forward instead of waiting out their intervals. One that outlived its attempts was killed: its caller holds a stored 500 under its `Idempotency-Key` (rule 2 of the [caller contract](#caller-contract) — a new key, or `get`). Alert on invocations `backing-off` for more than 5 minutes, on failed completions in `sys_invocation`, and on any invocation `paused` — none is expected under `kill`; one means a policy override or a server default leaked through.

**Rolling updates.** To roll without cutting anything, empty the server side first — steps 1 and 2 of the [flag-day script](#single--multi-flag-day): make both services private and poll the `sys_invocation` query until nothing is in flight — then stop the old revision, register the new one and make the services public again. Without it a rollout is correct but stalls every in-flight order for the retry interval and spends one of its attempts. It is *correct* because every type the worker journals as a `ctx.run` result is additive-only and pinned by fixtures in CI ([ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md), *Journal compatibility*): an invocation the new revision resumes decodes the entries the old one wrote. The drain is what avoids the stall, not what keeps the invocations alive — a release that deliberately breaks a journaled shape says so in its notes, and for that one the drain is mandatory.

## Services

Every handler takes and returns JSON; the discovery manifest carries JSON Schemas for all of them, so Restate's OpenAPI export documents the full contract. Request bodies are **closed** (`additionalProperties: false`): a field the contract does not know — a misspelt `reissue`, `additive`, `force` or `buyer.tax_number` — is refused as a 400 `invalid_input` naming the field, never silently dropped and read as its default. Response bodies stay open; tolerate fields added later. Domain outcomes are data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or `conflict` with a `conflict_reason`.

`Szamlazz.Order` is a Virtual Object keyed by the order number (`rendelésszám`, trimmed). It keeps no state: every handler answers from szamlazz.hu through the order's deterministic external ids (`{namespace}:{order}:{kind}`, the namespace being the top-level `namespace` key), so any invocation finds what an earlier one issued. The retry identity of a request is Restate's ingress `Idempotency-Key`. Eight handlers on `Szamlazz.Order`, five on `Szamlazz.Agent`:

| Handler | Description |
|---|---|
| `Szamlazz.Order.create_proforma` | Issues the proforma (`díjbekérő`) of the order. Refused with `conflict{order_invoiced}` once the order's invoice or prepayment invoice is live. |
| `Szamlazz.Order.create_invoice` | Issues the invoice (`számla`), converting the order's live proforma unless told otherwise (`options.proforma`: `auto`, `none` or `{"number": …}`); `options.reissue` issues a new one after a reversal. Refused with `conflict{prepaid_chain}` while a live prepayment invoice exists. |
| `Szamlazz.Order.create_prepayment` | Issues the prepayment invoice (`előlegszámla`); one per order, exclusive with the plain invoice. Takes no `options.proforma`: szamlazz.hu converts the order's live proforma by shared order number on its own. |
| `Szamlazz.Order.create_final` | Issues the final invoice (`végszámla`) settling the order's live prepayment invoice; the server does not net the prepayment into the totals. |
| `Szamlazz.Order.correct_invoice` | Issues a corrective invoice (`helyesbítő számla`) for an invoice of this order; a new `correction_id` issues a new corrective, the same id finds the one it issued. Keep the ids: `get` does not list correctives — find one with `Szamlazz.Agent.query {"selector": {"external_id": "acct:ORD-1001:corrective:<id>"}}`. |
| `Szamlazz.Order.storno_invoice` | Reverses (`sztornó`) an invoice of this order; idempotent. The storno carries the original's fulfillment date (`teljesitesDatum` = the invoice's `telj`), as NAV requires; there is no way to set another ([ADR 0007](../../docs/adr/0007-storno-repeats-the-originals-fulfillment-date.md)). |
| `Szamlazz.Order.delete_proforma` | Deletes the order's proforma; answers `{deleted, reason}` — a paid one is `{deleted: false, reason: "proforma_paid"}` unless `force`. |
| `Szamlazz.Order.get` | What szamlazz.hu holds under the order's external ids right now (proforma, invoice, prepayment, final), each `live`, `reversed` or — a proforma — `consumed`. No input. Read-only, never blocks behind issuing. Does not list correctives and never fills `storno_number` (the create and storno handlers report it). |
| `Szamlazz.Agent.check_account` | The read-only probe of the [deploy checklist](#deploy-checklist): the scope the SDK saw, the configured account (`id`, `mode`, `supplier_id`), the namespace and whether szamlazz.hu accepted the credentials (`ok` / `rejected`). No input. One sentinel query; issues nothing. |
| `Szamlazz.Agent.query` | Queries a document by invoice number, order number or external id. Code 7 is 404 `not_found`; another szamlazz.hu code is passed through as 422 `szamlazz_error` with the code in `szamlazz_code`. |
| `Szamlazz.Agent.query_taxpayer` | Looks a Hungarian taxpayer up through NAV (`xmltaxpayer`) on the scope's account — `{"tax_number": "12345678-2-42"}` or the bare stem `"12345678"`, nothing else — and answers `{valid, name?, tax_number?, vat_code?, addresses[]}`; `valid: false` is a normal 200. Read-only, one step under the `[read]` policy; any other NAV or szamlazz.hu code is a 422 `szamlazz_error` with that code in `szamlazz_code`. Not cached here — cache in the caller with a TTL on the order of a day. |
| `Szamlazz.Agent.set_payments` | Registers credit entries (`jóváírás`) on an invoice; replaces unless `additive`. At most five entries — a sixth is 400 `invalid_input`, nothing sent; szamlazz.hu refusing the entries is 422 `szamlazz_error`. No query precedes the send, so it has no `not_found` path. **`additive: true` is at-least-once**: a lost reply is `outcome_unknown`, and the handler's one retry after a crash re-sends — each send that reaches szamlazz.hu appends the entries again. Query the invoice before re-sending an additive call that ended in `outcome_unknown`; a replacing call is repeated as is. |
| `Szamlazz.Agent.storno` | Reverses an invoice that no `Szamlazz.Order` manages; a document carrying an order number is answered with `managed_by_order` instead — after the account check, like every document found by number; an unknown invoice number is 404 `not_found`; szamlazz.hu refusing the storno is `outcome: rejected` (200), never a 422. The storno carries the original's fulfillment date, like `storno_invoice`. |

### Calling a handler

`POST /restate/call/{service}/{key}/{handler}` on the ingress (`:8080`) for `Szamlazz.Order` (the key is the order number), `POST /restate/call/Szamlazz.Agent/{handler}` for the stateless service; on a multi-account deployment `/restate/scope/{scope}/call/…`. Send the body with `content-type: application/json` and an `idempotency-key` header, one value per logical request (rules below). `get` and `check_account` take no body. The machine-readable contract is the OpenAPI export of the admin API:

```sh
curl localhost:9070/services/Szamlazz.Order/openapi
curl localhost:9070/services/Szamlazz.Agent/openapi
```

### Request reference

**`create_invoice`, exercising every optional field** — a B2B invoice in EUR for a card order paid at checkout, to a Hungarian company buyer, emailed by szamlazz.hu. `create_proforma`, `create_prepayment` and `create_final` take the same body (`options.proforma` only on `create_invoice`); the minimal body is the one in the [Quick start](#quick-start):

```json CreateRequest
{
  "document": {
    "buyer": {
      "name": "Kovács Bt.",
      "zip": "2030",
      "city": "Érd",
      "address": "Tárnoki út 23.",
      "country": "Magyarország",
      "email": "billing@kovacs.example",
      "tax_number": "12345678-2-42",
      "taxpayer_status": "has_tax_number"
    },
    "items": [
      {
        "name": "DemoCon 2026 conference ticket",
        "quantity": "2",
        "unit": "db",
        "unit_price": "199.00",
        "vat_rate": "27",
        "id": "TICKET-STD",
        "comment": "Order ABC12"
      }
    ],
    "fulfillment_date": "2026-09-07",
    "due_date": "2026-09-07",
    "payment_method": "card",
    "paid": true,
    "comment": "Paid by card at checkout.",
    "issue_date": "2026-09-07",
    "overrides": {
      "language": "en",
      "currency": "EUR",
      "exchange_rate": { "bank": "MNB", "rate": "395.50" },
      "send_email": true
    }
  },
  "options": {
    "reissue": false,
    "proforma": "auto"
  }
}
```

| Field | Notes |
|---|---|
| `buyer` | `name`, `zip`, `city`, `address` required. Optional: `country`, `email` (comma-separated for several recipients), `tax_number` (Hungarian, `NNNNNNNN-N-NN`), `eu_tax_number`, `group_id`, `taxpayer_status`, `phone`, `comment`, `postal_address` (`{name, country, zip, city, address}`, all optional), `id` (partner id in the account's partner database). For a Hungarian business buyer set `tax_number` and `taxpayer_status: has_tax_number`; for an EU business `eu_tax_number` and `eu_business`; for a private individual `no_tax_number` and no tax number — szamlazz.hu reports to NAV from these. `Szamlazz.Agent.query_taxpayer` checks a Hungarian number against NAV before you issue. |
| `taxpayer_status` | `has_tax_number`, `eu_business`, `non_eu_business`, `no_tax_number`, `unknown`. |
| `items[]` | `name`, `quantity`, `unit`, `unit_price` (net), `vat_rate` required; optional `id` (your item id), `comment`. Net, VAT and gross are computed by the worker, rounded to the currency's minor unit (whole forints for HUF, cents for EUR), half away from zero — never sent by you. |
| `vat_rate` | A percentage as a string (`"27"`, `"18"`, `"5"`, `"0"`) or a NAV code (`"AAM"`, `"TAM"`, `"EUT"`, `"EUKT"`, `"KBAET"`, `"F.AFA"`, …). The code set is NAV's and open — the worker passes it through. |
| `invoice_number` (where a request names one) | 1–40 bytes, no whitespace, no control character, no `:` — exactly as szamlazz.hu reported it (`E-2026-123`); a padded or longer number is a malformed body (400 `invalid_input` naming the rule), never sent. |
| Decimals | `quantity`, `unit_price`, `exchange_rate.rate` and every amount in a response are **decimal strings** (`"199.00"`); a JSON number is accepted on input, but a string is exact. |
| Dates | `YYYY-MM-DD`. `fulfillment_date` is the day of delivery or service, `due_date` the payment deadline (for a card order paid at once, the same day and `paid: true`). Leave `issue_date` unset to let szamlazz.hu date the document at issue time — shown above only to exercise it; a pinned date is sent unchanged on every retry. |
| `payment_method` | `transfer`, `cash`, `card`, `check`, `cash_on_delivery`, `pay_pal`, `szep_card`, or `{"other": "…"}` for a free-text method sent to szamlazz.hu verbatim. |
| `paid`, `comment` | `paid` marks the document paid (`fizetve`); `comment` is printed on it. Both optional. |
| `overrides` | Per-call overrides of the account's `[account.defaults]`, all optional: `language` (`hu`, `en`, `de`, …), `currency` (`HUF`, `EUR`, …), `exchange_rate` (`{bank, rate?}` — required for a non-HUF currency unless the configured bank is `MNB`, whose rate szamlazz.hu looks up when `rate` is omitted), `template`, `send_email`, `e_invoice`, `number_prefix` (pre-registered on the account). |
| `options` | `reissue` (default `false`): issue a new document after the existing one was reversed; on a live document it is `conflict{live}`. `proforma` (`create_invoice` only; default `"auto"`): `"auto"` converts the order's live proforma if there is one, `"none"` refuses to (`conflict{proforma_live}` while one is live), `{"number": "D-…"}` names one. |

**`correct_invoice` — refund one of the two tickets.** A corrective (`helyesbítő számla`) carries the *difference*: a negative line for what is refunded. `correction_id` is yours — the same id always finds the corrective it issued, a new id issues a new one; `^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$` and not one of the external-id tokens (`proforma`, `invoice`, `prepayment`, `final`, `corrective`, `storno`, `by-number`, `check-account`, in any letter case), because it is embedded in the corrective's external id:

```json CorrectRequest
{
  "invoice_number": "E-2026-123",
  "correction_id": "refund-ABC12-1",
  "document": {
    "buyer": {
      "name": "Kovács Bt.",
      "zip": "2030",
      "city": "Érd",
      "address": "Tárnoki út 23.",
      "tax_number": "12345678-2-42",
      "taxpayer_status": "has_tax_number"
    },
    "items": [
      {
        "name": "DemoCon 2026 conference ticket (refund, 1 of 2)",
        "quantity": "-1",
        "unit": "db",
        "unit_price": "199.00",
        "vat_rate": "27"
      }
    ],
    "fulfillment_date": "2026-09-14",
    "due_date": "2026-09-14",
    "payment_method": "card",
    "paid": true,
    "overrides": {
      "language": "en",
      "currency": "EUR",
      "exchange_rate": { "bank": "MNB", "rate": "395.50" }
    }
  }
}
```

**The other bodies.** `storno_invoice` (on the order) and `Szamlazz.Agent.storno` (by number) take the same request; `comment` is optional and there is no date — the storno repeats the original's fulfillment date:

```json StornoRequest
{ "invoice_number": "E-2026-123", "comment": "Order canceled by the buyer" }
```

`delete_proforma` (`{}` is fine; `force` deletes a proforma with registered credit entries too):

```json DeleteProformaRequest
{ "force": false }
```

`Szamlazz.Agent.query` — one of `invoice_number`, `order_number` (the newest document under it) or `external_id`:

```json QueryRequest
{ "selector": { "external_id": "acct:ORD-1001:corrective:refund-ABC12-1" } }
```

`Szamlazz.Agent.query_taxpayer` — the full number or the bare eight-digit stem:

```json QueryTaxpayerRequest
{ "tax_number": "12345678-2-42" }
```

`Szamlazz.Agent.set_payments` — at most five entries; `additive` appends instead of replacing:

```json SetPaymentsRequest
{
  "invoice_number": "E-2026-123",
  "entries": [
    { "date": "2026-09-07", "method": "card", "amount": "398.00", "description": "Stripe ch_3Nx…" }
  ],
  "additive": false
}
```

### Response reference

**`CreateResponse`** — every `create_*` and `correct_invoice` answer, always the full object (unset fields are `null`, amounts are decimal strings). `kind` is `proforma`, `invoice`, `prepayment`, `final` or `corrective`; `external_id` is the document's handle on szamlazz.hu. **`issued`, `already_issued` and `reconciled` are all success**: the document exists and `invoice_number` and the totals are its current values — the distinction is diagnostic (`issued`: this call created it; `already_issued`: a live one was found under the external id before anything was sent; `reconciled`: szamlazz.hu refused the order number as a duplicate and the re-query found ours). `customer_account_url` is present only on the execution that actually issued — never on `already_issued`, `reconciled` or `get` — so persist it on first sight.

```json CreateResponse
{
  "outcome": "already_issued",
  "conflict_reason": null,
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": "E-2026-123",
  "storno_number": null,
  "net_total": "398.00",
  "gross_total": "505.46",
  "outstanding": "0.00",
  "customer_account_url": null,
  "existing_number": null,
  "code": null,
  "message": null,
  "warnings": []
}
```

```json CreateResponse
{
  "outcome": "reconciled",
  "conflict_reason": null,
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": "E-2026-123",
  "storno_number": null,
  "net_total": "398.00",
  "gross_total": "505.46",
  "outstanding": "0.00",
  "customer_account_url": null,
  "existing_number": null,
  "code": null,
  "message": null,
  "warnings": []
}
```

`reversed` — the document of this kind was reversed (by this service, the szamlazz.hu UI or anyone) and nothing new was issued; `storno_number` is set when szamlazz.hu's newest document under the order is the storno, `null` otherwise. Send `options.reissue: true` with a new `Idempotency-Key` when a new document is actually wanted:

```json CreateResponse
{
  "outcome": "reversed",
  "conflict_reason": null,
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": "E-2026-123",
  "storno_number": "E-2026-124",
  "net_total": null,
  "gross_total": null,
  "outstanding": null,
  "customer_account_url": null,
  "existing_number": null,
  "code": null,
  "message": null,
  "warnings": []
}
```

`rejected` — szamlazz.hu refused the document and nothing was issued; `code` is szamlazz.hu's numeric code as a string and `message` its text (Hungarian). Fix the request before re-sending:

```json CreateResponse
{
  "outcome": "rejected",
  "conflict_reason": null,
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": null,
  "storno_number": null,
  "net_total": null,
  "gross_total": null,
  "outstanding": null,
  "customer_account_url": null,
  "existing_number": null,
  "code": "202",
  "message": "Nem regisztrált számlaszám előtag: ACME",
  "warnings": []
}
```

`conflict` — the request contradicts what szamlazz.hu holds for the order; nothing was issued. `conflict_reason` says why and `existing_number` names the document it is about when there is one (`code` and `message` are set on `duplicate_order_number` only, with szamlazz.hu's 71/152):

```json CreateResponse
{
  "outcome": "conflict",
  "conflict_reason": "foreign",
  "kind": "invoice",
  "external_id": "acct:ORD-1001:invoice",
  "invoice_number": null,
  "storno_number": null,
  "net_total": null,
  "gross_total": null,
  "outstanding": null,
  "customer_account_url": null,
  "existing_number": "E-2026-098",
  "code": null,
  "message": null,
  "warnings": []
}
```

| `conflict_reason` | Meaning | Who acts | Fields set |
|---|---|---|---|
| `live` | `options.reissue: true`, but the document of this kind is live. | The caller: nothing to reissue — drop the flag (the plain create answers `already_issued`), or `storno_invoice` first if a new one is really wanted. | `existing_number` |
| `prepaid_chain` | A plain invoice while the order's own prepayment or final invoice is live, or a prepayment invoice while the order's own invoice or final invoice is: the two chains are exclusive. | The caller's flow is wrong for this order; never retryable as is. | `existing_number` |
| `order_invoiced` | `create_proforma` after the order's own invoice, prepayment or final invoice is live. | The caller: skip the proforma; the order is invoiced. | `existing_number` |
| `proforma_live` | `options.proforma: "none"` while a live proforma of the order exists — szamlazz.hu would link it by order number anyway. | The caller: use `"auto"`, or `delete_proforma` first. | `existing_number` |
| `proforma_missing` | `options.proforma: {"number"}` names a proforma szamlazz.hu does not know. | The caller: fix the number, or use `"auto"`. | `existing_number` (the number sent) |
| `not_managed` | The document named by number — the invoice to correct, or the proforma of `options.proforma: {"number"}` — does not carry this order's number (`storno_invoice` answers the same in a `StornoResponse`). | The caller: call under the order that owns it, or `Szamlazz.Agent.storno` for an invoice without an order number. | `existing_number` (the number sent) |
| `prepayment_missing` | `create_final` while the order has no prepayment invoice. | The caller: `create_prepayment` first. | — |
| `prepayment_reversed` | `create_final` while the order's prepayment invoice is reversed. | The caller: `create_prepayment` with `reissue: true` first, then the final. | `existing_number` |
| `base_reversed` | `correct_invoice` on a reversed invoice. | The caller: a reversed invoice cannot be corrected; correct the reissued one. | `existing_number` |
| `foreign` | A live invoice under this order number that is under none of the order's external ids — issued outside this worker (another channel, another namespace) on the same szamlazz.hu account. | **State of the world — page.** Someone issued outside the worker; open `existing_number` on szamlazz.hu and decide (storno and let the worker issue, or keep it and stop calling for this order). Never retry blindly. | `existing_number` |
| `duplicate_order_number` | szamlazz.hu refused the order number as a duplicate (71/152) and no live document of ours could be found under the external id. | **State of the world — page**, as `foreign`; `existing_number` names the document when the order-number query finds one. | `code`, `message`, `existing_number?` |
| `external_id_collision` | The newest document under one of the order's external ids belongs to another order, kind, account mode or supplier. | **Page**: a namespace shared by two deployments, or a `mode` / `supplier_id` misconfiguration. Query the `external_id` with `Szamlazz.Agent.query` to see whose it is. Never retryable as is. | `existing_number` |

**`rejected` pseudo-codes.** `code` on a `rejected` outcome is szamlazz.hu's numeric code — except for three tokens the worker sets itself, none of which szamlazz.hu answered: `not_stornoable` (a `StornoResponse`: szamlazz.hu echoed the document unchanged — a proforma or delivery note, which cannot be stornoed), `proforma_paid` (`delete_proforma`'s `reason`: the proforma has registered credit entries and `force` was not sent) and `request` (the request violates the Számla Agent wire contract and was never sent — a document without line items, a character XML cannot carry; a create or storno answers it as `rejected{code: "request"}` with the reason in `message`, and `set_payments` turns the same case — a sixth credit entry — into the `invalid_input` fault). Fix the request; re-sending as is repeats the answer.

**Enums are non-exhaustive.** `outcome`, `conflict_reason`, `kind`, `warnings[]` and `state` may gain values in a later release; branch with a default arm.

**`StornoResponse`** — `storno_invoice` and `Szamlazz.Agent.storno`. `reversed` means the invoice is reversed now or was already (idempotent), with the storno's number when known; `rejected` carries szamlazz.hu's `code` and `message` (or `not_stornoable`); `conflict{not_managed}` is an invoice that does not carry this order's number; `managed_by_order` is `Szamlazz.Agent.storno`'s answer for an invoice an order owns, naming the order key (meaningful under the scope you called under):

```json StornoResponse
{
  "outcome": "reversed",
  "conflict_reason": null,
  "invoice_number": "E-2026-123",
  "storno_number": "E-2026-124",
  "order_key": null,
  "code": null,
  "message": null
}
```

```json StornoResponse
{
  "outcome": "managed_by_order",
  "conflict_reason": null,
  "invoice_number": "E-2026-123",
  "storno_number": null,
  "order_key": "ORD-1001",
  "code": null,
  "message": null
}
```

**`DeleteProformaResponse`** — `deleted: true` with `reason: null` (deleted now) or `"absent"` (there was nothing to delete: deleted earlier or consumed by the invoice — `get` tells which); `deleted: false` with `reason` `"proforma_paid"`, `"external_id_collision"` or a szamlazz.hu code:

```json DeleteProformaResponse
{ "deleted": false, "reason": "proforma_paid" }
```

**`SetPaymentsResponse`** — the invoice's totals after the credit entries were registered:

```json SetPaymentsResponse
{ "invoice_number": "E-2026-123", "outstanding": "0.00", "gross_total": "505.46" }
```

**`QueryResponse`** — `Szamlazz.Agent.query`'s projection of the document as szamlazz.hu holds it. `document_type` is szamlazz.hu's `tipus` code — `SZ` invoice, `D` proforma, `ES` prepayment, `VS` final, `SS` storno, `HS` corrective — and `referenced_invoice_number` is the invoice a storno reversed or a corrective corrected; `supplier_id` and `test` are the pins the account check reads (`account_mismatch` when they are not the resolved account's); `payments` are the credit entries as recorded:

```json QueryResponse
{
  "invoice_number": "E-2026-125",
  "document_type": "HS",
  "reversed": false,
  "referenced_invoice_number": "E-2026-123",
  "referenced_proforma_number": null,
  "order_number": "ORD-1001",
  "issue_date": "2026-09-14",
  "fulfillment_date": "2026-09-14",
  "due_date": "2026-09-14",
  "currency": "EUR",
  "net_total": "-199.00",
  "vat_total": "-53.73",
  "gross_total": "-252.73",
  "payments": [
    { "date": "2026-09-14", "title": "bankkártya", "amount": "-252.73", "comment": null, "bank_account": null }
  ],
  "outstanding": "0.00",
  "supplier_id": 972720,
  "test": true
}
```

**`QueryTaxpayerResponse`** — NAV's record; `valid: false` with everything else empty is the answer for a number NAV does not know, not a fault:

```json QueryTaxpayerResponse
{
  "valid": true,
  "name": "KOVÁCS BT.",
  "tax_number": "12345678-2-42",
  "vat_code": "2",
  "addresses": [
    {
      "kind": "HQ",
      "country_code": "HU",
      "region": null,
      "postal_code": "2030",
      "city": "ÉRD",
      "street_name": "TÁRNOKI",
      "public_place_category": "ÚT",
      "number": "23.",
      "building": null,
      "staircase": null,
      "floor": null,
      "door": null,
      "lot_number": null,
      "additional_address_detail": null
    }
  ]
}
```

**`get`** — the live view of the order's four slots, straight from szamlazz.hu. Each slot is the document's number, its `state` and what szamlazz.hu reports about it: `payments` are the registered credit entry amounts as decimal strings, `referenced_proforma` the proforma the invoice converted, `e_invoice` whether it is an e-invoice (`null` for a proforma). A `reversed` slot carries `storno_number: null` always — `get` does not look the storno up; the create and storno handlers report it. A `consumed` proforma is one szamlazz.hu no longer returns because the invoice or prepayment in `by` converted it. A `null` slot means *nothing of ours* under that external id — either nothing at all, or a document that fails validation (another order's, another account's), which a read must not fail on; a create meeting the latter answers `conflict{external_id_collision}`. Correctives are not in the view (their external ids carry your `correction_id`s).

```json OrderStatus
{
  "proforma": {
    "number": "D-2026-045",
    "state": "consumed",
    "by": "E-2026-123",
    "gross": null,
    "net": null,
    "payments": [],
    "referenced_proforma": null,
    "e_invoice": null
  },
  "invoice": {
    "number": "E-2026-123",
    "state": "reversed",
    "storno_number": null,
    "gross": "505.46",
    "net": "398.00",
    "payments": ["505.46"],
    "referenced_proforma": "D-2026-045",
    "e_invoice": false
  },
  "prepayment": null,
  "final": null
}
```

### Faults

A fault is the invocation ending in a `TerminalError`. The ingress reports it with the HTTP status the fault's `code` pins, the header `x-restate-error-source: invocation`, and **Restate's own JSON envelope as the body** — `code` is the HTTP status as a number, `source` is `"invocation"`, and the worker's fault is the JSON **string** in `message`. Parse `message` a second time; its `code` is one of the eight tokens in the table below, never a number. A caller matching `body.code == "unavailable"` on the envelope never matches — the envelope's `code` is `503`.

```http
HTTP/1.1 503 Service Unavailable
content-type: application/json
x-restate-error-source: invocation
x-restate-id: inv_1aBcD…
```

```json Fault
{
  "code": 503,
  "message": "{\"code\":\"credentials_rejected\",\"message\":\"szamlazz.hu rejected the agent credentials (code 3: Sikertelen bejelentkezés); this attempt issued nothing — fix the account's agent key, then retry with a new Idempotency-Key or read get\",\"szamlazz_code\":\"3\",\"order\":\"ORD-1001\",\"kind\":\"invoice\",\"external_id\":\"acct:ORD-1001:invoice\"}",
  "source": "invocation"
}
```

The inner object is `{ "code", "message", "szamlazz_code"?, "order"?, "kind"?, "external_id"? }`: `szamlazz_code` is present on every fault a szamlazz.hu answer caused (`szamlazz_error`, `credentials_rejected`, and `unavailable` on a code a read cannot conclude from); `order`, `kind` and `external_id` name the document a `Szamlazz.Order` fault is about, when the step knows one. An exhausted create step:

```json Fault
{
  "code": 500,
  "message": "{\"code\":\"outcome_unknown\",\"message\":\"the create step ended without a confirmed outcome (500): szamlazz.hu did not confirm the invoice; retry with a new Idempotency-Key\",\"order\":\"ORD-1001\",\"kind\":\"invoice\",\"external_id\":\"acct:ORD-1001:invoice\"}",
  "source": "invocation"
}
```

A malformed body has the same shape: the worker decodes its own request bodies, so an unknown field, a wrong type, a missing required field or invalid JSON is `invalid_input` with serde's message naming the field, refused before anything is journaled or sent — you will not see the Restate SDK's plain-text `Cannot decode input payload` from these services:

```json Fault
{
  "code": 400,
  "message": "{\"code\":\"invalid_input\",\"message\":\"malformed request body: unknown field `resissue`, expected `reissue` or `proforma` at line 1 column 215\"}",
  "source": "invocation"
}
```

Two more bodies wear the same envelope and are **not** the worker's fault. A **killed invocation** — the handler's invocation attempts exhausted by a worker outage, ADR 0004 — is a 500 with `source: invocation` whose `message` is Restate's description of the last retryable error, **text, not JSON** (the wording is Restate's and varies); treat it as `outcome_unknown`:

```json KilledInvocation
{
  "code": 500,
  "message": "[500] Error: failed to reach the service endpoint: connection refused",
  "source": "invocation"
}
```

An **ingress error** — Restate refusing the request itself: a private service, a bad path, a body without `content-type: application/json`, an overloaded ingress — carries `source: "ingress"` (and `x-restate-error-source: ingress`), and Restate's message. Nothing reached the worker; a 5xx of this kind is the one that is safe to auto-retry with the same key:

```json IngressError
{
  "code": 400,
  "message": "the invoked service is not public",
  "source": "ingress"
}
```

Decoding, in pseudo-code:

```text
if status == 200:                 outcome = body                               # branch on body.outcome
elif header x-restate-error-source == "invocation":
    fault = try parse_json(body.message)
    if fault: branch on fault.code                                             # the table below
    else:     treat as outcome_unknown                                         # a killed invocation
else:                             restate_error = body.message                 # retry with the same key
```

The e2e harness asserts this envelope on every fault it receives from a live Restate 1.7.8 (`crates/restate-szamlazz/tests/service.rs`, `Reply::fault`), and the examples above are held to the contract types by `tests/readme.rs`.

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `invalid_input` | 400 | The request is malformed — its body carries a field the contract does not know, a wrong type, a missing required field, an `invoice_number` or `correction_id` outside its bound, or an order key outside the key alphabet (the message names the rule; nothing was journaled or sent) — or it carries a value the operation cannot take: an option the handler does not take (`options.proforma` on anything but `create_invoice`), a `{number}` proforma link that is not a proforma, an empty `buyer.name`, a `query_taxpayer` tax number in neither accepted form, a sixth credit entry on `set_payments` (the wire contract takes five; nothing is sent), a line item whose arithmetic overflows a decimal (nothing is sent). | Fix the request. |
| `unknown_account` | 400 | The request names no account of this deployment: it arrived unscoped on a multi-account deployment (`[accounts.<scope>]`, which serves accounts by scope only), or under a scope no account is reachable by — on a single-account deployment (`[account]`, served unscoped only), any scope. Nothing was issued. | Fix the address — `/restate/scope/{scope}/call/…` with a configured scope, or `/restate/call/…` on a single-account deployment; do not retry as is. |
| `not_found` | 404 | The document the request names by number is not known to szamlazz.hu (code 7): `Szamlazz.Agent.query`'s selector, the invoice of `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of `Szamlazz.Order.correct_invoice`. Nothing was sent. (A missing proforma named by `options.proforma: {number}` is `conflict{proforma_missing}`, an outcome, not this fault.) | Fix the number — the invoice was never issued, or the number you stored is wrong; do not retry as is. |
| `account_mismatch` | 409 | A document found by number — by `Szamlazz.Order.storno_invoice` / `correct_invoice` on their verify, the proforma of `options.proforma: {number}`, or by `Szamlazz.Agent.query` / `storno` — belongs to another szamlazz.hu account (`teszt` or `szallito/id` differ from the resolved account's); the message names the observed and expected pins. Nothing was sent. `Szamlazz.Agent.set_payments` and `query_taxpayer` are the two exemptions: `set_payments` registers the credit entry without a preceding query — a verify round trip per credit entry to catch a misconfiguration every other found document already catches is not worth it, and a credit entry is not a legal document — and `query_taxpayer` finds no document at all (a taxpayer record is NAV's and carries no pins). | Check `account.mode` / `account.supplier_id` — a test account configured as live fails on its first found document — or the scope the call was made under; do not retry blindly. |
| `szamlazz_error` | 422 | szamlazz.hu answered with an error code of its own that the handler passes through rather than concludes from: `Szamlazz.Agent.query` on a code that is neither 7 nor a credential code, `query_taxpayer` on any `funcCode ≠ OK` (szamlazz.hu's own or NAV's relayed one — `valid: false` is a 200, not this), `set_payments` on szamlazz.hu refusing the credit entries. `szamlazz_code` carries the code, `message` szamlazz.hu's text. | Branch on `szamlazz_code`. A NAV outage on `query_taxpayer` is retried with a new `Idempotency-Key`; a refused credit entry is fixed before it is re-sent. |
| `outcome_unknown` | 500 | The create or storno step ran out of its `[issue]` policy while a document may or may not have been issued — or `Szamlazz.Agent.set_payments` lost the reply to its one send. | Retry with a new `Idempotency-Key` or read `get`. For `set_payments` with `additive: true`, query the invoice first: the lost send may have appended the entries. |
| `unavailable` | 503 | szamlazz.hu did not answer a read-only step through every execution of the `[read]` policy (the message names the step, the last failure and — where the step knows them — the order, kind and external id), or answered it with a code nothing can be concluded from (`szamlazz_code` carries it), or returned a storno's original without a fulfillment date (`telj`) — the date the storno must repeat, so the storno is not sent — or the worker's own account resolver or credential store could not answer. Nothing was sent by the execution that raised it. | Retry with a new `Idempotency-Key` later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (codes 3 invalid credentials, 135 browser session active, 136 login blocked, 164 multiple accounts; `szamlazz_code` carries it). The execution that raised it **issued nothing** (szamlazz.hu answers these codes before acting on a request); an earlier one may have landed with a lost reply. The worker logs a `warn` with the namespace and the code, inside the execution span that names the scope, the account id and the invocation id (see [Running](#running)). | Page the operator: fix `account.agent_key` (or the account state on szamlazz.hu) of the account the log line names. Then retry with a new `Idempotency-Key` or read `get`. |

A 503 whose `x-restate-error-source` is `invocation` is **this worker's** answer — `unavailable` or `credentials_rejected` — not the Restate ingress being down. Restate's [HTTP invocation docs](https://docs.restate.dev/invoke/http#retrying-requests) say to treat `invocation` errors as non-retryable and to auto-retry a `5xx` only when its source is `ingress` (or absent); do that here as well: page on an `invocation` 503 instead of retrying into it — `credentials_rejected` in particular repeats identically until the deployment is fixed — and only then retry with a new `Idempotency-Key`.

Handlers that call szamlazz.hu kill the invocation after five attempts (2 m → 10 m back-off) rather than pausing, so a stuck order never blocks its own recovery. Issuing itself is a read-only lookup step and a create step whose every execution — Restate re-executes it under the `[issue]` policy while szamlazz.hu's answer is unknown — queries the external id before it sends; that query is what the next call reconciles against, and an exhausted create step is a structured `outcome_unknown` naming the order, kind and external id. Storno has the same two steps under the same policy. Every read-only step — the lookups, verifies, hints, `get`'s queries, `query`, the `check_account` probe — is re-executed under the `[read]` policy while szamlazz.hu does not answer it (a transport failure, `szlahu_down`), so a single network blip on a read no longer fails the invocation; an exhausted read is a structured `unavailable`. A killed invocation also reaches the caller as HTTP 500 with `x-restate-error-source: invocation`, carrying the last retryable error's message. See [ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md) and [ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md).

### Caller contract

1. Send an `Idempotency-Key` per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation. It is deduplicated per scope.
2. Tell a **fault** from **no answer** before deciding what to do with the key. A fault — a 4xx/5xx with `x-restate-error-source: invocation` and a body the worker wrote — is a completed invocation whose stored completion Restate replays under the same key for the retention period (30 days on every handler that writes; verified): an **`outcome_unknown`, `unavailable` or `credentials_rejected`** fault from an issuing or storno handler means "outcome unknown — retry with a **new** key, or read `Szamlazz.Order.get`", never "no document exists"; the handler reconciles by external id, so the retry is safe. No answer — a client timeout, an ingress 5xx whose source is *not* `invocation` — is an invocation still in flight, re-dispatched by Restate for up to ~24 min under a worker outage: **keep the key** and retry with it (the retry attaches to the in-flight invocation and receives its outcome) or read `get`; a new key would start a second invocation behind the first. A killed invocation (attempts exhausted) is a fault whose envelope `message` is the last retryable error's text, not the worker's `{code, message}`: treat it as `outcome_unknown`. The other faults are settled — nothing landed: `invalid_input`, `unknown_account`, `not_found` and `account_mismatch` are raised before anything is sent, and `szamlazz_error` is szamlazz.hu answering with an error (to a read, or refusing the credit entries it was sent). Retrying as is repeats the answer: fix the request, the number, the scope or the account — or, for a `szamlazz_error` relaying a NAV outage, retry later with a new key.
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document → `conflict{live}`; the flag can never cause a duplicate.

**Calling from a webhook handler.** A create normally answers within a few seconds, but it can legitimately take **minutes**: while szamlazz.hu is flaky the worker waits out its `[read]` policy (up to 5 m per read step by default) and re-executes the create step under `[issue]` (2 m → 10 m between executions); the call stays open the whole time. So:

- Set your client timeout to about **90 s** — longer than szamlazz.hu's own 60 s request timeout, so a healthy call always completes inside it. On timeout, either re-send with the **same** `Idempotency-Key` (the retry attaches to the invocation still in flight and returns its outcome when it completes) or poll `Szamlazz.Order.get`.
- Or do not wait at all: `POST /restate/send/Szamlazz.Order/{order}/create_invoice` with the same body and key answers `{"invocationId": "inv_…", "status": "Accepted"}` at once, and the webhook handler reads the result later with `get` — or attaches to the invocation with `GET /restate/attach/{invocationId}`, which waits for and returns its outcome ([Restate docs](https://docs.restate.dev/services/invocation/http)) — the shape for a webhook framework with a short deadline.
- **Always acknowledge the webhook and own the retry queue yourself.** A webhook provider retries with the *same* notification id; if that id is your `Idempotency-Key`, a retry after a fault replays the stored fault for the retention period (30 d) instead of trying again. Record the fault, acknowledge the webhook, and schedule your own retry with a new key.
- **Any changed request needs a new key**, always: Restate answers a repeated key from the stored response without reading the body, so a corrected buyer under the old key gets the old answer. A retry of an *unchanged* request that got no answer keeps its key (rule 2).

### What your database stores per order

The worker keeps nothing, so your application is the only record of what it asked for. Per order — sized so that you can storno, correct, reissue or inspect the order months later, with nothing but szamlazz.hu and this table:

| Column | Why |
|---|---|
| order key, **exactly as sent** (trimmed) | The `Szamlazz.Order` key and the `rendelésszám` szamlazz.hu shows; every external id derives from it. |
| scope, as used (multi-account mode) | The account the order was invoiced on is a fact about the order, not about your customer's current setting; `order_key` in a `managed_by_order` answer is meaningful under this scope only. |
| `invoice_number` per kind issued (proforma, invoice, prepayment, final) | Human-readable handle for support and for the by-number handlers (`storno_invoice`, `correct_invoice`, `set_payments`, `query`). `get` re-derives them at any time, so this is a cache, not the truth. |
| `storno_number` when a storno reported one | `get` never fills it; the storno and create handlers do, once. |
| every `correction_id` you issued, with the corrective's number | `get` does not list correctives; the id is the only way back to one (`Szamlazz.Agent.query` by `acct:{order}:corrective:{id}`). |
| the last `Idempotency-Key` per operation, and whether it ended in a **fault** | Decides the next key: a fault (rule 2) needs a new one; no answer keeps it. |
| `customer_account_url` on first sight | Present on a fresh `issued` only — never on `already_issued`, `reconciled` or `get`. |

`external_id`s need no column: `{namespace}:{order}:{kind}` is deterministic. The account (its `id`, `mode`, `supplier_id`) is never in a response — record the scope, not the account.

## Caller guidance: a Pretix integration

The worked example behind multi-account mode ([ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md)): an application that issues szamlazz.hu documents for [Pretix](https://pretix.eu/) ticket orders, serving many organizers, each with its own szamlazz.hu account, where an event may override the organizer's account. The same rules apply to any caller; Pretix supplies the concrete identifiers.

**The account is a first-class entity in your application.** Model it as its own record — an id, the szamlazz.hu account it stands for, its state — with an organizer-level default and an optional per-event override. Resolve *event → account* in your application **before** every call, and store the account id **per invoice, as used**: the account an order was invoiced on is a fact about that invoice, not about the event's current setting, and it is what you need to storno, correct, reissue or inspect the order months later. Together with the order key it is all you need (the worker keeps nothing).

**Scope = your account id.** It must fit Restate's scope format — `[a-zA-Z0-9_.-]`, non-empty, at most 36 characters (a dashed UUID is exactly 36) — and, with this binary's static resolver, the stricter `[a-z0-9_]`. Never send the organizer, the event or any tenant identifier as the scope: one szamlazz.hu account ⇔ one scope, and an event that overrides its organizer's account is invoiced under *that* account's scope. Two organizers sharing one szamlazz.hu account share one scope.

**Order key = Pretix event slug + order code**, for example `democon-2026-ABC12`. Pretix order codes are unique per event, not per organizer, so the event slug makes the key unique within the account across every event the account serves — and it stays unique if the event later moves to another account, because the key's uniqueness is required *within* an account only. Keep it within the worker's key rule (1–40 bytes after trimming, no internal whitespace, no `:`, no control characters, Unicode NFC); it is also what szamlazz.hu shows as `rendelésszám`.

**`Idempotency-Key` = the webhook notification id — and own the retry.** Pretix retries a webhook until it is acknowledged; Restate deduplicates those retries per scope, so the same notification under two accounts is two invocations — which is right, since it addresses two szamlazz.hu accounts. After a fault (a 4xx/5xx with `x-restate-error-source: invocation`), a retry needs a **new** key — Restate replays the stored failure under the old one for the retention period — and Pretix cannot rotate its notification id, so acknowledge the webhook, record the fault and retry from a queue of your own with a new key ([Calling from a webhook handler](#caller-contract)).

**Per-event throttling via limit keys, never via scopes.** To keep one event's burst from starving the others on the same account, send the call under the scope with a limit key — `?limit-key={event-slug}` (each level `[a-zA-Z0-9_.-]`, at most 36 characters; the gateway strips `x-restate-*` headers, so use the query parameter or let the gateway set it). A limit key shapes concurrency only; it is not part of the identity, so it never changes which `Szamlazz.Order` instance a call reaches.

| Pretix flow | Call under the account's scope | Notes |
|---|---|---|
| Order placed, bank transfer pending | `Szamlazz.Order/{key}/create_proforma` | The proforma is the payment request. |
| Bank transfer received (proforma exists) | `Szamlazz.Order/{key}/create_invoice` | `options.proforma: auto` (default) converts the live proforma; szamlazz.hu links by shared order number anyway. |
| Card payment (paid at once) | `Szamlazz.Order/{key}/create_invoice` | No proforma; `auto` finds none. `paid: true`, `due_date` = `fulfillment_date`. |
| Order canceled before payment | `Szamlazz.Order/{key}/delete_proforma` | `{deleted: true, reason: "absent"}` when it was never created or already consumed. |
| Full refund | `Szamlazz.Order/{key}/storno_invoice {invoice_number}` | `reversed`; idempotent. Credit entries are wiped by the storno — re-register on a new invoice if needed. |
| Partial refund / order changed | `Szamlazz.Order/{key}/correct_invoice {invoice_number, correction_id, document}` | `correction_id` = the Pretix change or notification id; a new id issues a new corrective. Persist every id you send. |
| Wrong buyer data — redo | `storno_invoice`, then `create_invoice {options: {reissue: true}}` with a new key | Buyer data cannot be fixed by a corrective (szamlazz.hu); a create without `reissue` after the storno returns `reversed`. |
| Reconcile / after any fault | `Szamlazz.Order/{key}/get` | The live view of the order's four documents; never blocks behind issuing. |

Reading responses: `outcome` is data (HTTP 200) — branch on `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` and `conflict{conflict_reason}`, not on status codes. `external_id` (`{namespace}:{key}:{kind}`) is the only namespace marker in any response; no response names the account, and `order_key` in a storno response (`managed_by_order`) is meaningful only under the scope you called under. A 503 with `x-restate-error-source: invocation` is the worker's `unavailable` or `credentials_rejected`: page, do not auto-retry into it; once fixed, retry with a new `Idempotency-Key` or read `get`. Adding an organizer's account is a resolver change — a row for a database-backed resolver; an `[accounts.<scope>]` entry and a new revision for the static one, following the [mapping-change procedure](#single--multi-flag-day) — followed by `check_account` under the new scope.

## Request Identity

Restate signs every request it makes to a service endpoint when the runtime is configured with a request identity key. `identity_keys` lists the matching `publickeyv1_...` public keys; with at least one key configured the endpoint rejects unsigned requests. Multiple keys stay valid at once, so rotation is a config change: add the new key, switch the runtime to the new private key, then drop the old one. The environment override accepts a comma-separated list:

```sh
RESTATE_SZAMLAZZ_IDENTITY_KEYS="publickeyv1_old,publickeyv1_new" restate-szamlazz --config restate-szamlazz.toml
```

Without `identity_keys` the endpoint accepts unsigned requests — fine on a laptop, not in production. Identity keys authenticate the Restate runtime to this endpoint; callers authenticate to Restate ingress separately.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
