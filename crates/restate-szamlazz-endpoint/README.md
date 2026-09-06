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

The image runs the binary as the non-root user `nonroot` (uid and gid 65532; numeric in the image, so Kubernetes' `runAsNonRoot` verifies it), with no writable filesystem needed. A mounted configuration file must be readable by that uid — world-readable is fine once the agent key comes from the environment, as above. It stops cleanly on `docker stop` (see [Stopping](#stopping)).

## Prerequisite

The szamlazz.hu account setting **"Rendelésszám ismétlődés tiltása"** (Disable order number repetition) **must be ON** on every account the deployment issues for. The service keys everything by order number and relies on szamlazz.hu rejecting a second document of the same kind under one order number (71/152) as its second guard against duplicates — the external-id query inside every execution of the create step is the first; without the toggle a retry that lands after the first request can issue a second legal document. The verified behavior and the go-live checklist are in [`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md).

One deployment serves one szamlazz.hu account unscoped (`[account]`) **or** any number of accounts selected per request by the Restate scope (`[accounts.<scope>]`); see [Multi-account mode](#multi-account-mode).

## Configuration

The binary reads a TOML, JSON or YAML file (by extension) and applies `RESTATE_SZAMLAZZ_` environment overrides on top, with `__` separating nesting levels. Everything constant for a deployment lives here and never travels in a request payload: the deployment-level settings at the top (`namespace`, `[issue]`, `[read]`, `[resolve]`) and the szamlazz.hu account under `[account]` — or, in [multi-account mode](#multi-account-mode), the accounts under `[accounts.<scope>]`.

```toml
identity_keys = ["publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f"]
namespace = "acct"            # 1–16 bytes of [a-z0-9-]; prefixes every external id ({namespace}:{order}:{kind}); permanent

[issue]                       # optional; the issue policy: the run retry policy of the create and storno steps
max_attempts = 5              # executions of the step, including the first
initial_delay = "2m"          # before the first re-execution; longer than a client timeout plus the longest observed server stall — at least 90s, or the endpoint refuses to start
factor = 2.0
max_delay = "10m"
max_duration = "1h"           # the hard bound on re-executing the step

[read]                        # optional; the read policy: the run retry policy of every read-only step (lookups, verifies, hints, get, query, query_taxpayer, check_account)
max_attempts = 3              # executions of the step, including the first
initial_delay = "5s"          # before the first re-execution
factor = 2.0
max_delay = "30s"
max_duration = "2m"           # the hard bound; sized to ride out szamlazz.hu's observed minute-long stalls

[resolve]                     # optional; the resolve policy: the run retry policy of the `account` step (no attempt cap)
initial_delay = "1s"
factor = 2.0
max_delay = "10s"
max_duration = "1m"

[account]
id = "acme"                   # the account's identifier as the worker knows it; journaled with every invocation, shown in the Restate UI
agent_key = "..."             # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional; the production URL by default
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

`namespace` and exactly one of `[account]` or `[accounts.<scope>]`, each account with `id` and `agent_key`, are required; everything else has a default. The agent key is never logged; the start-up log names the namespace, whether the deployment is scoped, and — per account — its scope (or `<unscoped>`), `id`, `mode`, `endpoint` and `supplier_id`.

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
INFO restate_szamlazz: bound Restate service service=Szamlazz.Agent kind=Service handlers=4
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
supplier_id = 972720          # REQUIRED in this shape
mode = "live"

[accounts.acme.seller]
bank_account = "..."

[accounts.beta_events]        # reachable as /restate/scope/beta_events/call/…
id = "beta"
agent_key = "beta-key"        # SECRET — prefer RESTATE_SZAMLAZZ_ACCOUNTS__BETA_EVENTS__AGENT_KEY
supplier_id = 972721
```

`[account]` and `[accounts.<scope>]` are mutually exclusive: both present is a load error, and there is no default account. In this shape an **unscoped** request is `unknown_account` (400); in the single-account shape a **scoped** one is. The configuration is validated at start-up against the checkable half of the safety contract — one szamlazz.hu account is reachable under exactly one scope — and the process exits on: a missing `supplier_id` (the only server-side account identity the worker can validate a found document against), two accounts sharing a `supplier_id`, two sharing an `(endpoint, agent_key)` pair, two sharing an `id` (the credential reference), or a scope key outside `[a-z0-9_]` / longer than 36 bytes.

**Scope format.** The static resolver's scope keys are `[a-z0-9_]`, 1–36 bytes — a strict subset of Restate's scope format (`[a-zA-Z0-9_.-]`, non-empty, at most 36 characters — ASCII, so bytes; a dashed UUID is exactly 36), chosen so that environment overrides can address them (`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY`; figment lowercases the segment). This is the constraint on the account identifiers your application uses as scopes with this binary; a deployment with its own `AccountResolver` may use Restate's full format.

**The scope is routing, not authorization.** Anyone who can reach the ingress under a scope issues on that account. Put the ingress behind a gateway that sets the scope from the authenticated identity, never forwards a caller-supplied scope path, and strips `x-restate-*` request headers (the ingress lets a caller's copy of one of its own headers win). Limit keys therefore travel as the `limit-key` query parameter, or are set by the gateway. The seven rules the worker relies on — one account under exactly one scope, an append-only mapping, a permanent namespace, order keys unique within an account, the caller recording key and scope as used, routing-not-authorization, ownership by external id alone — are in the [library README](../restate-szamlazz/README.md#scope-contract) and [ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md).

**Experimental Restate flags.** Scoped calls need `vqueues`, `protocol_v7` and `scoped_virtual_objects` on the server (`RESTATE_EXPERIMENTAL_ENABLE_*`; `compose.yaml` sets the three). They are experimental at Restate 1.7.8 — the server source calls `vqueues` "in heavy development" and scoped Virtual Objects "not officially supported in v1.7", while the docs present flow control as opt-in whose "configuration and APIs may change" — and the facts here were verified on server 1.7.8 with SDK 0.12.0; re-run the [deploy checklist](#deploy-checklist) after every server upgrade. ADR 0006 records the quotes, the Restate Cloud confirmation and the flagless contingency.

**Kafka ingress** is untested and unsupported in this mode. The server has a `kafka_scope` flag that scopes a record from an `x-restate-scope` record header, so "arrives unscoped" is not the reason — no scenario exercises it, and the record's scope would be set by the producer, outside the gateway above. Verify it yourself before wiring a subscription.

### Deploy checklist

After every deploy or configuration change, call `Szamlazz.Agent.check_account` **under each configured scope** (unscoped on a single-account deployment). It runs the prologue like every handler, sends one read-only query of a sentinel external id that nothing the service issues carries, and answers with what the SDK saw, the *configured* account, the namespace and whether szamlazz.hu accepted the credentials. It issues nothing.

```sh
curl -X POST localhost:8080/restate/scope/acme/call/Szamlazz.Agent/check_account
# {"scope":"acme","account":{"id":"acme","mode":"live","supplier_id":972720},"namespace":"acct","credentials":{"state":"ok"}}
```

| Answer | Meaning |
|---|---|
| `200` with `scope` equal to the scope you called under, the expected `account.id`, and `credentials: {"state": "ok"}` | The scope reaches the worker, resolves to the intended account, and its agent key works. |
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

`--config`, `--bind` and `--port` also read `CONFIG_FILE`, `BIND_ADDR` and `PORT`. Logging goes through `tracing` with `RUST_LOG` (default `info`). The endpoint binds `{bind}:{port}` — `0.0.0.0:9080` by default; `--bind 127.0.0.1` keeps it off the network, `--port 0` takes an ephemeral port; the start-up log names the bound address — and speaks HTTP/2 only, as every Restate SDK endpoint does. `--check-config` validates and exits instead (see [Configuration](#configuration)).

Register it with a Restate server:

```sh
restate deployments register http://host:9080
```

For local development the repository root has a `compose.yaml` with a Restate server; `docker compose up -d` starts it, `restate deployments register http://host.docker.internal:9080` registers an endpoint running on the host.

### Stopping

`SIGTERM` or `SIGINT` — `docker stop`, a Kubernetes rollout, `kill`, Ctrl-C — stops the process cleanly: it stops accepting connections, gives the open ones up to 10 s to finish (the Rust SDK's connection drain; the bound is the SDK's, not configurable here) and exits 0. The start-up log names the signals (`stop_on="SIGTERM, SIGINT"`) and a stop logs which one arrived (`signal="SIGTERM"`). With nothing in flight the process is gone within milliseconds. The container image runs the binary as PID 1 with `STOPSIGNAL SIGTERM`, so `docker stop` and the kubelet reach it directly.

**In-flight invocations.** An invocation whose connection the stop cuts — one still running when the connection drain ends — is neither lost nor duplicated. Restate keeps it and re-dispatches it after the handler's retry interval (`initial_interval`: 2 m on every handler that writes to szamlazz.hu — every `Szamlazz.Order` handler but `get`, `Szamlazz.Agent.set_payments` and `Szamlazz.Agent.storno` — longer than the 60 s client timeout, so neither an additive send nor a storno's leading query can run while the first send is still in flight; 10 s on `Szamlazz.Agent.query`, `query_taxpayer` and `check_account`; the server's ~500 ms default on `get`, which only reads); the re-execution replays the journal up to the open step and runs that step again. For a cut create step this is exactly what the query-first closure exists for: the re-execution queries the external id before sending and finds what the cut execution sent — `issued`, never a second document ([ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md); design §5). The cost is the delay and one of the invocation's attempts (five on the `Szamlazz.Order` 2 m handlers, two on `set_payments` and `Szamlazz.Agent.storno`; the last kills it). The create step is also the longest thing a stop can cut — a query, a send and a re-query at up to 60 s each, ~180 s in the worst case, well past the connection drain — so a rollout while orders are issuing will cut some of them; letting them finish would take a longer drain than the SDK offers.

**Grace period.** Give the process the whole connection drain plus a margin, so it is never `SIGKILL`ed mid-drain: at least 15 s. Kubernetes' default `terminationGracePeriodSeconds: 30` is fine; Docker's default `docker stop -t 10` is the bare bound, so pass `-t 15` (or `stop_grace_period: 15s` in Compose) where the endpoint carries traffic. A longer grace period buys nothing: the connection drain ends at 10 s regardless.

**Rolling updates.** To roll without cutting anything, empty the server side first — steps 1 and 2 of the [flag-day script](#single--multi-flag-day): make both services private and poll the `sys_invocation` query until nothing is in flight — then stop the old revision, register the new one and make the services public again. Without it a rollout is correct but stalls every in-flight order for the retry interval and spends one of its attempts. It is *correct* because every type the worker journals as a `ctx.run` result is additive-only and pinned by fixtures in CI ([ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md), *Journal compatibility*): an invocation the new revision resumes decodes the entries the old one wrote. The drain is what avoids the stall, not what keeps the invocations alive — a release that deliberately breaks a journaled shape says so in its notes, and for that one the drain is mandatory.

## Services

Every handler takes and returns JSON; the discovery manifest carries JSON Schemas for all of them, so Restate's OpenAPI export documents the full contract. Request bodies are **closed** (`additionalProperties: false`): a field the contract does not know — a misspelt `reissue`, `additive`, `force` or `buyer.tax_number` — is refused as a 400 `invalid_input` naming the field, never silently dropped and read as its default. Response bodies stay open; tolerate fields added later. Domain outcomes are data (HTTP 200): `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or `conflict` with a `conflict_reason`.

`Szamlazz.Order` is a Virtual Object keyed by the order number (`rendelésszám`, trimmed). It keeps no state: every handler answers from szamlazz.hu through the order's deterministic external ids (`{namespace}:{order}:{kind}`, the namespace being the top-level `namespace` key), so any invocation finds what an earlier one issued. The retry identity of a request is Restate's ingress `Idempotency-Key`. Eight handlers on `Szamlazz.Order`, four on `Szamlazz.Agent`:

| Handler | Description |
|---|---|
| `Szamlazz.Order.create_proforma` | Issues the proforma (`díjbekérő`) of the order. Refused with `conflict{order_invoiced}` once the order's invoice or prepayment invoice is live. |
| `Szamlazz.Order.create_invoice` | Issues the invoice (`számla`), converting the order's live proforma unless told otherwise (`options.proforma`: `auto`, `none` or `{"number": …}`); `options.reissue` issues a new one after a reversal. Refused with `conflict{prepaid_chain}` while a live prepayment invoice exists. |
| `Szamlazz.Order.create_prepayment` | Issues the prepayment invoice (`előlegszámla`); one per order, exclusive with the plain invoice. Takes no `options.proforma`: szamlazz.hu converts the order's live proforma by shared order number on its own. |
| `Szamlazz.Order.create_final` | Issues the final invoice (`végszámla`) settling the order's live prepayment invoice; the server does not net the prepayment into the totals. |
| `Szamlazz.Order.correct_invoice` | Issues a corrective invoice (`helyesbítő számla`) for an invoice of this order; a new `correction_id` issues a new corrective. |
| `Szamlazz.Order.storno_invoice` | Reverses (`sztornó`) an invoice of this order; idempotent. The storno carries the original's fulfillment date (`teljesitesDatum` = the invoice's `telj`), as NAV requires; there is no way to set another ([ADR 0007](../../docs/adr/0007-storno-repeats-the-originals-fulfillment-date.md)). |
| `Szamlazz.Order.delete_proforma` | Deletes the order's proforma; refuses a paid one unless `force`. |
| `Szamlazz.Order.get` | What szamlazz.hu holds under the order's external ids right now (proforma, invoice, prepayment, final), each `live`, `reversed` or — a proforma — `consumed`. No input. Read-only, never blocks behind issuing. |
| `Szamlazz.Agent.check_account` | The read-only probe of the [deploy checklist](#deploy-checklist): the scope the SDK saw, the configured account (`id`, `mode`, `supplier_id`), the namespace and whether szamlazz.hu accepted the credentials (`ok` / `rejected`). No input. One sentinel query; issues nothing. |
| `Szamlazz.Agent.query` | Queries a document by invoice number, order number or external id. |
| `Szamlazz.Agent.query_taxpayer` | Looks a Hungarian taxpayer up through NAV (`xmltaxpayer`) on the scope's account — `{"tax_number": "12345678-2-42"}` or the bare stem `"12345678"`, nothing else — and answers `{valid, name?, tax_number?, vat_code?, addresses[]}`; `valid: false` is a normal 200. Read-only, one step under the `[read]` policy; any other NAV or szamlazz.hu code is a 422 with that code. Not cached here — cache in the caller with a TTL on the order of a day. |
| `Szamlazz.Agent.set_payments` | Registers credit entries (`jóváírás`) on an invoice; replaces unless `additive`. **`additive: true` is at-least-once**: a lost reply is `outcome_unknown`, and the handler's one retry after a crash re-sends — each send that reaches szamlazz.hu appends the entries again. Query the invoice before re-sending an additive call that ended in `outcome_unknown`; a replacing call is repeated as is. |
| `Szamlazz.Agent.storno` | Reverses an invoice that no `Szamlazz.Order` manages; a document carrying an order number is answered with `managed_by_order` instead — after the account check, like every document found by number. The storno carries the original's fulfillment date, like `storno_invoice`. |

A create request through the ingress (`/restate/call/{service}/{key}/{handler}`; on a multi-account deployment `/restate/scope/{scope}/call/{service}/{key}/{handler}`):

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

**Caller contract:**

1. Send an `Idempotency-Key` per logical request; Restate dedupes retries and attaches concurrent duplicates to the in-flight invocation.
2. **Any error** from an issuing or storno handler means "outcome unknown — retry with a **new** key, or read `Szamlazz.Order.get`" (the stored completion of a failed invocation is replayed under the same key for the retention period — verified); the handler reconciles by external id, so the retry is safe. Never interpret an error as "no document exists".
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document → `conflict{live}`; the flag can never cause a duplicate.

**Faults.** Errors are `TerminalError`s with a JSON body `{ "code", "message", "order"?, "kind"?, "external_id"? }`; the ingress reports them with the HTTP status below and `x-restate-error-source: invocation`. Every one of them means "outcome unknown" (rule 2), never "no document exists". A malformed body has the same shape — the worker decodes its own request bodies, so an unknown field, a wrong type, a missing required field or invalid JSON is `{ "code": "invalid_input", "message": "malformed request body: …" }` with serde's message, naming the field when there is one (for example ``malformed request body: unknown field `resissue`, expected `reissue` or `proforma` at line 1 column …``), refused before anything is journaled or sent; you will not see the Restate SDK's plain-text `Cannot decode input payload` from these services.

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `invalid_input` | 400 | The request is malformed — its body carries a field the contract does not know, a wrong type or a missing required field (the message names it; nothing was journaled or sent) — or it names a document szamlazz.hu does not know, or an option the handler does not take (`options.proforma` on anything but `create_invoice`). | Fix the request. |
| `unknown_account` | 400 | The request names no account of this deployment: it arrived unscoped on a multi-account deployment (`[accounts.<scope>]`, which serves accounts by scope only), or under a scope no account is reachable by — on a single-account deployment (`[account]`, served unscoped only), any scope. Nothing was issued. | Fix the address — `/restate/scope/{scope}/call/…` with a configured scope, or `/restate/call/…` on a single-account deployment; do not retry as is. |
| `account_mismatch` | 409 | A document found by number — by `Szamlazz.Order.storno_invoice` / `correct_invoice` on their verify, or by `Szamlazz.Agent.query` / `storno` — belongs to another szamlazz.hu account (`teszt` or `szallito/id` differ from the resolved account's); the message names the observed and expected pins. Nothing was sent. `Szamlazz.Agent.set_payments` and `query_taxpayer` are the two exemptions: `set_payments` registers the credit entry without a preceding query — a verify round trip per credit entry to catch a misconfiguration every other found document already catches is not worth it, and a credit entry is not a legal document — and `query_taxpayer` finds no document at all (a taxpayer record is NAV's and carries no pins). | Check `account.mode` / `account.supplier_id` — a test account configured as live fails on its first found document — or the scope the call was made under; do not retry blindly. |
| `outcome_unknown` | 500 | The create or storno step ran out of its `[issue]` policy while a document may or may not have been issued — or `Szamlazz.Agent.set_payments` lost the reply to its one send. | Retry with a new `Idempotency-Key` or read `get`. For `set_payments` with `additive: true`, query the invoice first: the lost send may have appended the entries. |
| `unavailable` | 503 | szamlazz.hu did not answer a read-only step through every execution of the `[read]` policy (the message names the step, the last failure and — where the step knows them — the order, kind and external id), or answered it with a code nothing can be concluded from, or returned a storno's original without a fulfillment date (`telj`) — the date the storno must repeat, so the storno is not sent — or the worker's own account resolver or credential store could not answer. Nothing was sent by the execution that raised it. | Retry with a new `Idempotency-Key` later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (codes 3 invalid credentials, 135 browser session active, 136 login blocked, 164 multiple accounts). The execution that raised it **issued nothing** (szamlazz.hu answers these codes before acting on a request); an earlier one may have landed with a lost reply. The worker logs a `warn` with the namespace and the code. | Page the operator: fix `account.agent_key` (or the account state on szamlazz.hu). Then retry with a new `Idempotency-Key` or read `get`. |

A 503 whose `x-restate-error-source` is `invocation` is **this worker's** answer — `unavailable` or `credentials_rejected` — not the Restate ingress being down. Restate's [HTTP invocation docs](https://docs.restate.dev/invoke/http#retrying-requests) say to treat `invocation` errors as non-retryable and to auto-retry a `5xx` only when its source is `ingress` (or absent); do that here as well: page on an `invocation` 503 instead of retrying into it — `credentials_rejected` in particular repeats identically until the deployment is fixed — and only then retry with a new `Idempotency-Key`.

Handlers that call szamlazz.hu kill the invocation after five attempts (2 m → 10 m back-off) rather than pausing, so a stuck order never blocks its own recovery. Issuing itself is a read-only lookup step and a create step whose every execution — Restate re-executes it under the `[issue]` policy while szamlazz.hu's answer is unknown — queries the external id before it sends; that query is what the next call reconciles against, and an exhausted create step is a structured `outcome_unknown` naming the order, kind and external id. Storno has the same two steps under the same policy. Every read-only step — the lookups, verifies, hints, `get`'s queries, `query`, the `check_account` probe — is re-executed under the `[read]` policy while szamlazz.hu does not answer it (a transport failure, `szlahu_down`), so a single network blip on a read no longer fails the invocation; an exhausted read is a structured `unavailable`. A killed invocation also reaches the caller as HTTP 500 with `x-restate-error-source: invocation`, carrying the last retryable error's message. See [ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md) and [ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md).

## Caller guidance: a Pretix integration

The worked example behind multi-account mode ([ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md)): an application that issues szamlazz.hu documents for [Pretix](https://pretix.eu/) ticket orders, serving many organizers, each with its own szamlazz.hu account, where an event may override the organizer's account. The same rules apply to any caller; Pretix supplies the concrete identifiers.

**The account is a first-class entity in your application.** Model it as its own record — an id, the szamlazz.hu account it stands for, its state — with an organizer-level default and an optional per-event override. Resolve *event → account* in your application **before** every call, and store the account id **per invoice, as used**: the account an order was invoiced on is a fact about that invoice, not about the event's current setting, and it is what you need to storno, correct, reissue or inspect the order months later. Together with the order key it is all you need (the worker keeps nothing).

**Scope = your account id.** It must fit Restate's scope format — `[a-zA-Z0-9_.-]`, non-empty, at most 36 characters (a dashed UUID is exactly 36) — and, with this binary's static resolver, the stricter `[a-z0-9_]`. Never send the organizer, the event or any tenant identifier as the scope: one szamlazz.hu account ⇔ one scope, and an event that overrides its organizer's account is invoiced under *that* account's scope. Two organizers sharing one szamlazz.hu account share one scope.

**Order key = Pretix event slug + order code**, for example `democon-2026-ABC12`. Pretix order codes are unique per event, not per organizer, so the event slug makes the key unique within the account across every event the account serves — and it stays unique if the event later moves to another account, because the key's uniqueness is required *within* an account only. Keep it within the worker's key rule (1–64 bytes after trimming, no whitespace runs, no control characters); it is also what szamlazz.hu shows as `rendelésszám`.

**`Idempotency-Key` = the webhook notification id.** Pretix retries a webhook until it is acknowledged; Restate deduplicates those retries per scope, so the same notification under two accounts is two invocations — which is right, since it addresses two szamlazz.hu accounts. After a terminal fault (any 5xx with `x-restate-error-source: invocation`), a retry needs a **new** key: Restate replays the stored failure under the old one for the retention period.

**Per-event throttling via limit keys, never via scopes.** To keep one event's burst from starving the others on the same account, send the call under the scope with a limit key — `?limit-key={event-slug}` (each level `[a-zA-Z0-9_.-]`, at most 36 characters; the gateway strips `x-restate-*` headers, so use the query parameter or let the gateway set it). A limit key shapes concurrency only; it is not part of the identity, so it never changes which `Szamlazz.Order` instance a call reaches.

| Pretix flow | Call under the account's scope | Notes |
|---|---|---|
| Order placed, bank transfer pending | `Szamlazz.Order/{key}/create_proforma` | The proforma is the payment request. |
| Bank transfer received (proforma exists) | `Szamlazz.Order/{key}/create_invoice` | `options.proforma: auto` (default) converts the live proforma; szamlazz.hu links by shared order number anyway. |
| Card payment (paid at once) | `Szamlazz.Order/{key}/create_invoice` | No proforma; `auto` finds none. |
| Order canceled before payment | `Szamlazz.Order/{key}/delete_proforma` | `{deleted: true, reason: absent}` when it was never created or already consumed. |
| Full refund | `Szamlazz.Order/{key}/storno_invoice {invoice_number}` | `reversed`; idempotent. Credit entries are wiped by the storno — re-register on a new invoice if needed. |
| Partial refund / order changed | `Szamlazz.Order/{key}/correct_invoice {invoice_number, correction_id, document}` | `correction_id` = the Pretix change or notification id; a new id issues a new corrective. |
| Wrong buyer data — redo | `storno_invoice`, then `create_invoice {options: {reissue: true}}` with a new key | Buyer data cannot be fixed by a corrective (szamlazz.hu); a create without `reissue` after the storno returns `reversed`. |
| Reconcile / after any fault | `Szamlazz.Order/{key}/get` | The live view of the order's four documents; never blocks behind issuing. |

Reading responses: `outcome` is data (HTTP 200) — branch on `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` and `conflict{conflict_reason}`, not on status codes. `external_id` (`{namespace}:{key}:{kind}`) is the only namespace marker in any response; no response names the account, and `order_key` in a storno response (`managed_by_order`) is meaningful only under the scope you called under. A 503 with `x-restate-error-source: invocation` is the worker's `unavailable` or `credentials_rejected`: page, do not auto-retry into it; once fixed, retry with a new `Idempotency-Key` or read `get`. Adding an organizer's account is a resolver change — a row for a database-backed resolver; an `[accounts.<scope>]` entry and a new revision for the static one, following the [mapping-change procedure](#single--multi-flag-day) — followed by `check_account` under the new scope.

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
