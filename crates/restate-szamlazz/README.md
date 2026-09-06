# restate-szamlazz

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz)

**Restate services issuing and managing szamlazz.hu documents with durable, idempotent execution.**

The `Szamlazz.Order` Virtual Object — keyed by the order number — serializes issuing per key, so that a caller
can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes,
concurrent callers and reversals. It keeps **no state**: szamlazz.hu is the source of truth, reached through
deterministic external ids (`{namespace}:{order}:{kind}`) so that any invocation can find what an earlier one
issued. The stateless `Szamlazz.Agent` service exposes by-number operations (query, credit entries, storno of
unmanaged documents), the NAV taxpayer lookup by tax number (`query_taxpayer`) and the read-only `check_account`
probe over the same gateway module. Both are projections of the Számla Agent model: deployment
constants live in config, line totals are computed, domain outcomes are returned as data.

The design is in [`docs/design/restate-szamlazz.md`](../../docs/design/restate-szamlazz.md), the decisions
behind it in [`docs/adr/`](../../docs/adr), and the szamlazz.hu behavior it relies on in
[`docs/szamlazz-hu-behaviour.md`](../../docs/szamlazz-hu-behaviour.md). A ready-made binary and container
image live in [`restate-szamlazz-endpoint`](../restate-szamlazz-endpoint).

## Quick Start

Bind both services to a Restate endpoint of your own:

```rust
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::account::{StaticConfig, StaticResolver};
use restate_szamlazz::{Accounts, Agent, Order, WorkerConfig};

async fn serve(accounts: StaticConfig, worker: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    worker.validate()?;
    let accounts = Accounts::from(StaticResolver::try_from(accounts)?);
    let order = Order::from_parts(accounts.clone(), worker.clone());
    let agent = Agent::from_parts(accounts, worker);
    let endpoint = Endpoint::builder().bind(order).bind(agent).build();
    HttpServer::new(endpoint)
        .listen_and_serve("0.0.0.0:9080".parse()?)
        .await;
    Ok(())
}
```

Both configuration types only implement `Deserialize`; the host chooses the file format and environment merging
(the endpoint binary uses figment). `WorkerConfig` is the deployment-level part — the `namespace` of the external
ids and the three run retry policies, `[issue]`, `[read]` and `[resolve]`; call `validate()` after parsing. `StaticConfig` is the static
resolver's configuration in one of two mutually exclusive shapes — a single `[account]`, served unscoped, or a table
of `[accounts.<scope>]`, each served under its scope only (`/restate/scope/{scope}/call/…`) — and
`StaticResolver::try_from` validates it and implements both the account resolver and the credential store;
`Accounts::from` bundles the two. A deployment with its own resolver and store builds `Accounts::new` over them
instead. Neither service holds a gateway or a client: every handler resolves its account and opens a `Gateway` for
its own execution. Going from `[account]` to `[accounts.<scope>]` is a flag day with no data migration, scripted in
the [endpoint README](../restate-szamlazz-endpoint/README.md#single--multi-flag-day).

## Scope Contract

What `Szamlazz.Order` guarantees:

- **Exactly one live document per kind per order** — proforma, invoice, prepayment, final — under caller
  retries, process crashes and concurrent callers. Same-key handlers run one at a time; issuing is a read-only
  **lookup** step that settles every case needing no create, then a **create** step under a run retry policy whose
  every execution queries szamlazz.hu by the document's external id *inside the same `ctx.run` closure* before it
  creates, so a request that landed before a crash, a timeout or a lost reply is found, not re-issued.
- **Correctives** are issued under a caller-supplied `correction_id`; the same id finds the corrective it
  issued, a new id issues a new one.
- **Storno** (`storno_invoice`) and **proforma deletion** are idempotent; a document reversed by anyone — the
  UI, support, this service — is reported as `reversed` from `<sztornozott>`. The storno carries the original's
  fulfillment date (`teljesitesDatum` = the verified document's `telj`), which NAV requires it to repeat; the
  caller cannot set it ([ADR 0007](../../docs/adr/0007-storno-repeats-the-originals-fulfillment-date.md)).
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
- The deployment's **namespace** prefixes the external ids and is permanent — changing it would hide every
  document issued so far. It is pinned per invocation, so a redeploy cannot move a running invocation.
- **The account is resolved once per invocation and journaled.** Every handler resolves the request's scope to
  its `Account` in a durable step named `account` under the resolve policy, so an invocation finishes on the
  account it started on; the journaled `Account` (visible in the Restate UI for the retention period) carries
  everything but the agent key. Unscoped and unknown scopes are `unknown_account` (400) before anything is
  issued. The **scope** is the only channel for the account — never a header, a body field or the key. The scope
  reaches the worker only under **protocol v7**, and Restate's ingress does not refuse a scoped path when v7 is
  off — the SDK would see no scope and a single-account deployment would issue on its one account.
  `Szamlazz.Agent.check_account` under each scope after every deploy is the defence: it answers the `scope` the
  SDK saw (`null` under a scoped call is that misconfiguration), the configured account, and whether szamlazz.hu
  accepted its key. Multi-account mode depends on three experimental Restate flags (`vqueues`, `protocol_v7`,
  `scoped_virtual_objects`), verified on server 1.7.8 with SDK 0.12.0 — see
  [ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md) for what the server source says about
  them and for the flagless contingency. Kafka ingress is untested and unsupported in multi-account mode.
- **Credentials are fetched on every handler execution, outside the journal**, and held only for that
  execution — a rotation is picked up on the next execution of every in-flight invocation, and no agent key is
  ever written into Restate (the `Credentials` type has no serde implementation; the e2e suite scans every
  journal entry for the run's keys). A failed fetch is a **terminal** `unavailable` after a short in-process
  retry, by decision: a retryable error would route a prolonged store outage into the handler's kill-on-five and
  an unstructured 500, whereas the terminal fault is structured and immediate. The cost: a store outage during a
  **replay** of an invocation whose create already landed surfaces as `unavailable` even though the document
  exists — `get` or a retry with a new `Idempotency-Key` reconciles it (`already_issued`).
- **The safety contract** ([ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md)) — the
  static resolver enforces what can be checked at load time; a resolver of your own, and the operator, guarantee
  the rest:
  1. One szamlazz.hu account is reachable under exactly one scope value; unscoped counts as a value; no fan-in
     (two scopes reaching one account would split an order's per-key lock across two Virtual Objects). The
     static resolver's single `[account]` is served unscoped and knows no scope; its `[accounts.<scope>]` shape is
     served by scope only and is checked at load time (a `supplier_id` on every account, unique supplier ids,
     unique `(endpoint, agent_key)` pairs, unique ids).
  2. The scope → account mapping is append-only: moving traffic to another account means a new scope, never
     re-pointing an existing one. Appending a scope cannot create fan-in; any change that could put one account
     under two identities at once (the single → multi flag day above all) is a drain–switch–resume.
  3. The namespace is permanent for the deployment.
  4. Order keys are unique within an account for its lifetime, across all writers.
  5. The caller records the order key and the scope as used; nothing else is needed to operate on the order
     later.
  6. The scope is routing, not authorization: the ingress sits behind a gateway that sets the scope from the
     authenticated identity, never forwards a caller-supplied scope path, and strips `x-restate-*` request
     headers (the ingress lets a caller's copy of one of its own headers win).
  7. Ownership of a document is decided by the external-id query alone; the order-number query can name a
     document but never prove ownership.

What it does not do: PDF download, receipts, IPN and Adatkapcsolat ingestion, the proforma →
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
  `document`) is the input of `correct_invoice` and shares the response. Every request type — and every object it
  nests — is closed (`#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the schema): a field the
  contract does not know is refused as `invalid_input` naming the field, never silently dropped. Response types stay
  open.
- `service::Body<T>`: how every handler takes its input — a `Json<T>` whose decode runs in the handler, so a
  malformed body is the structured `invalid_input` fault instead of the SDK's plain-text 400. Same discovery
  schema as `Json<T>`; built with `Body::new` / `From<T>` for calls through the generated clients.
- `contract::Outcome` / `ConflictReason`: `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or
  `conflict` with a reason — `prepaid_chain` (a plain invoice while the order's own prepayment invoice or final
  invoice is live, or a prepayment invoice while the order's own invoice or final invoice is: the final invoice
  keeps the chain closed after its prepayment is reversed), `order_invoiced` (a proforma after the order's own live
  invoice, prepayment invoice or final invoice), `live`, `foreign` (a live invoice under the order number that is
  under none of the order's external ids — another channel's), `duplicate_order_number`,
  `external_id_collision`, `proforma_live`, `proforma_missing`, `prepayment_missing`, `prepayment_reversed`,
  `base_reversed`, `not_managed`.
- `contract::TerminalCode`: the eight fault codes a `TerminalError` carries, each with its HTTP status
  (`TerminalCode::status`; `TerminalCode::ALL` lists them in the order of the fault table below) — `invalid_input`
  (400), `unknown_account` (400: the request names no account of this deployment), `not_found` (404: the document the
  request names by number is not known to szamlazz.hu — code 7), `account_mismatch` (409: a document found by number —
  on `Szamlazz.Order`'s verifies, including the proforma of `options.proforma: {number}`, or on `Szamlazz.Agent.query`
  / `storno` — belongs to another szamlazz.hu account than the resolved one; `set_payments` and `query_taxpayer` find
  none and are exempt), `szamlazz_error` (422: szamlazz.hu answered with an error code of its own that the handler
  passes through, the code in the fault's `szamlazz_code` field), `outcome_unknown` (500), `unavailable` (503; also
  the prologue's own faults: the resolve policy exhausted, the credential store gone or unavailable) and
  `credentials_rejected` (503: szamlazz.hu answered 3, 135, 136 or 164 — the worker's agent key is wrong, not the
  request; the execution that raised it issued nothing).
- `contract::CheckAccountResponse` (`CheckedAccount`, `CredentialsCheck`): the output of
  `Szamlazz.Agent.check_account` — `scope`, `account: {id, mode, supplier_id}`, `namespace` and
  `credentials: {state: ok} | {state: rejected, code, message}`; credential acceptance is its only szamlazz.hu-verified
  fact, the rest echoes the configured account.
- `contract::QueryTaxpayerRequest` / `QueryTaxpayerResponse` (`TaxpayerAddress`): the contract of
  `Szamlazz.Agent.query_taxpayer` — `tax_number` in, the bare eight-digit stem (`12345678`) or the full
  `NNNNNNNN-N-NN` form (`12345678-2-42`) and nothing else (`QueryTaxpayerRequest::prefix` derives the prefix or
  names what was wrong); `{valid, name?, tax_number?, vat_code?, addresses[]}` out, a crate-owned additive-only
  projection of the agent crate's `TaxpayerInfo` — what the read step journals — with `valid: false` a normal
  answer. Not cached by the worker; cache it in the caller with a TTL on the order of a day.
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
  validated (1–64 bytes, no control characters, no internal whitespace runs). The type trims; the `Order` handlers
  do not: a Virtual Object key with leading or trailing whitespace is refused as `invalid_input` naming the rule
  (see "Identity Model").
- `ExternalId`: the deterministic `szamlaKulsoAzon` of a document — `for_kind`, `for_corrective`,
  `for_storno`, `for_unmanaged_storno` — and `for_probe`, the two-segment `{namespace}:check-account` sentinel
  that nothing the service issues carries.
- `WorkerConfig`: the deployment-level configuration the services hold — `namespace` (the `config::Namespace`, the
  external-id prefix of the deployment, 1–16 bytes of `[a-z0-9-]`, permanent), `[issue]` (the issue policy:
  `max_attempts`, `initial_delay`, `factor`, `max_delay`, `max_duration`; `5` executions, `2m` → `10m`, bounded by
  `1h` by default), `[read]` (the read policy: the same fields; `5` executions, `5s` → `60s`, bounded by `5m` by
  default) and `[resolve]` (the resolve policy: the same fields without `max_attempts`; `1s` → `10s`, bounded by `1m`
  by default). `IssueConfig::run_retry_policy` is the `RunRetryPolicy` of the create and storno steps,
  `ReadConfig::run_retry_policy` that of every read-only step, `ResolveConfig::run_retry_policy` that of the `account`
  step. `validate()` checks the cross-field invariants — `max_attempts ≥ 1`, `initial_delay ≤ max_delay`, `factor ≥ 1`
  — and one floor: `issue.initial_delay ≥ IssueConfig::MIN_INITIAL_DELAY`, the Számla Agent client's exported
  `REQUEST_TIMEOUT` (60 s) plus a 30 s margin (90 s), because a create or storno step re-executed sooner would re-check while
  its send may still be in flight; the error names the rule. Nothing account-shaped is in it: document defaults and the
  seller block belong to the `Account` — their value types (`config::Defaults`, `config::SellerConfig`,
  `config::AccountMode`, and `config::Secret`, whose `Debug` output is redacted) live in `config` so that any
  resolver's configuration can reuse them.
- `account::Account`, `Accounts`, `AccountResolver`, `CredentialStore`, `StaticResolver`, `StaticConfig`: one
  szamlazz.hu account as the worker knows it (never its key), the bundle of the two pluggable traits both services
  hold, and the configuration-backed implementation of both. `StaticConfig` is either `[account]` (`id`,
  `agent_key`, `endpoint`, `mode`, `supplier_id`, `defaults`, `seller`; reachable unscoped) or a table of
  `[accounts.<scope>]` (the same fields, `supplier_id` required; each reachable under its scope only, keys
  `[a-z0-9_]` of at most 36 bytes so environment overrides can address them) — never both. `StaticResolver::try_from`
  validates it, and `Accounts::from` bundles it as resolver and store.
- `gateway::Gateway`: the module that speaks to szamlazz.hu on behalf of one account, over
  `szamlazz_agent::Client` — one plain async fn per `ctx.run` (`lookup`, `create`, `verify`, `query`, `hint`,
  `lookup_storno`, `storno`, `delete_proforma`, `set_payments`, `probe`), each returning every expected szamlazz.hu
  outcome as data. Two `Err`s say what a run retry policy may re-execute: the read fns (`lookup`, `verify`, `query`,
  `hint`, `lookup_storno`, `probe`) return `Err(Unanswered)` when szamlazz.hu did not answer — a transport or parse
  failure, `szlahu_down` — and `create` and `storno` return `Err(Unconfirmed)` for an outcome that is *not* known.
  It is not a second client: the Számla Agent `Client` is the transport it wraps. Every read of account configuration by the services goes through `Gateway::account()`; a gateway is opened
  per handler execution by the prologue (`Gateway::open`) and never outlives it. `Szamlazz.Order` calls it inside
  `ctx.run`; the `Szamlazz.Agent` Restate service is a thin facade over the same module. No Restate service calls
  another.
- `Order` / `Agent`: the Restate Virtual Object registered as `Szamlazz.Order` and the stateless service
  registered as `Szamlazz.Agent`, with generated `OrderClient` and `AgentClient` for typed calls from other
  handlers. Both are built `from_parts(Accounts, WorkerConfig)`; every handler decodes its body (`Body<T>` — a
  malformed one is `invalid_input` before anything is journaled) and runs the prologue — pin the
  namespace, resolve the account (`account` step), fetch the credentials, open the gateway — before its operation.

## Identity Model

Three identities work together ([ADR 0002](../../docs/adr/0002-order-keyed-idempotency-via-external-ids.md),
[ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md)):

- The **order key** decides which `Order` instance runs; same-key handlers run one at a time, which is what
  serializes issuing per order. The object holds no state. The key is the order number **trimmed by the caller**:
  Restate's per-key lock is on the raw key, so `ORD-1` and ` ORD-1` would be two instances with two locks mapping
  to one szamlazz.hu order and identical external ids, and two concurrent creates under them would both pass their
  lookup and both send. A key with leading or trailing whitespace is therefore refused as `invalid_input` before
  anything is journaled or sent.
- The **external id** identifies a document to szamlazz.hu and is derived from the key alone under the
  deployment's namespace:

  | Document | External id |
  |---|---|
  | proforma, invoice, prepayment, final | `{namespace}:{order}:{kind}` |
  | corrective | `{namespace}:{order}:corrective:{correction_id}` |
  | storno of an order's invoice | `{namespace}:{order}:storno:{original_number}` |
  | storno via `Szamlazz.Agent` (no order) | `{namespace}:by-number:{number}:storno` |

  It is queried by the lookup step and again by every execution of the create step, inside the create's own
  `ctx.run` closure, so a request that landed before a crash, a timeout or a lost reply is found, not re-issued.
  There is **no generation counter**: external ids are not unique
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
2. Tell a **fault** from **no answer** before deciding what to do with the key.
   - **A fault** is a 5xx **with a body the worker wrote** and `x-restate-error-source: invocation`: the invocation
     completed, and Restate stores that completion under the key for the retention period, so the same key would
     replay the failure. An **`outcome_unknown`, `unavailable` or `credentials_rejected`** fault from an issuing or
     storno handler means "outcome unknown — retry with a **new** key, or read `Szamlazz.Order.get`", never "no
     document exists"; the retry with a new key reconciles by external id and is safe.
   - **No answer** — your client timed out, or the ingress answered with a 5xx whose source is *not* `invocation` —
     means the invocation is still in flight: it runs on once the key frees, and under a worker outage Restate
     re-dispatches it for up to ~24 min on `Szamlazz.Order` (five attempts, 2 m → 10 m). **Keep the key** and retry
     with it — the retry attaches to the in-flight invocation and receives its outcome — or read `get`; a new key
     here would start a second invocation that queues behind the first.
   - **A killed invocation** (attempts exhausted) is a fault whose body is the last retryable error's **text**, not
     the worker's `{code, message}` JSON: treat an unparsable 5xx `invocation` body as `outcome_unknown`.
   - **The other faults are settled** — nothing landed: `invalid_input`, `unknown_account`, `not_found` and
     `account_mismatch` are raised before anything is sent, and `szamlazz_error` is szamlazz.hu answering with an
     error (to a read, or refusing the credit entries it was sent). Retrying as is repeats the answer: fix the
     request, the number, the scope or the account — or, for a `szamlazz_error` relaying a NAV outage, retry later
     with a new key.
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send
   `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document is
   `conflict{live}`, so the flag can never cause a duplicate.
4. A `credentials_rejected` fault (503) means szamlazz.hu refused the worker's agent key (codes 3, 135, 136, 164)
   on some step: the deployment is misconfigured, not the request. The execution that raised it issued nothing, but
   an earlier execution may have landed with a lost reply, so rule 2 applies — once the key is fixed, retry with a
   new key or read `get`. The worker logs every occurrence at `warn` with the namespace and the code; the key itself
   appears in neither the log nor the fault.
5. An `unknown_account` fault (400) means the request named no account of this deployment — unscoped where
   accounts are scoped, or a scope no account is reachable by. Nothing was issued and the same request never
   succeeds: fix the scope, do not retry.
6. On a multi-account deployment, set the scope on **every** call — it is a path segment of the request, not a
   session — and record the order key *and the scope as used* per order. `Idempotency-Key`s are deduplicated per scope. `order_key` in a
   storno response is meaningful only under the same scope; `external_id` is the only namespace marker in any
   response, and no response names the account.

Faults are `TerminalError`s with a JSON body `{ "code", "message", "szamlazz_code"?, "order"?, "kind"?,
"external_id"? }`; the ingress reports them with the HTTP status below and `x-restate-error-source: invocation`. `code`
is always one of the eight tokens of `contract::TerminalCode` — a szamlazz.hu code never travels in it, but in
`szamlazz_code` beside it, present on every fault a szamlazz.hu answer caused: the `szamlazz_error` pass-through,
`credentials_rejected`, and `unavailable` on a code a read cannot conclude from. A malformed body is the same shape:
every handler decodes its own body (`service::Body<T>`, not the SDK's `Json<T>`), so an unknown field, a wrong type, a
missing required field or invalid JSON is `{ "code": "invalid_input", "message": "malformed request body: …" }` with
serde's message, naming the field when there is one — never the SDK's plain-text `Cannot decode input payload`:

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `invalid_input` | 400 | The request is malformed — its body carries a field the contract does not know (every request type is closed: ``unknown field `resissue`, expected `reissue` or `proforma` ``), a wrong type or a missing required field, or its `Order` key has leading or trailing whitespace; refused before anything is journaled or sent — or it carries a value the operation cannot take: an option the handler does not take, a `{number}` proforma link that is not a proforma, a sixth credit entry on `set_payments` (the wire contract takes five; nothing is sent). | Fix the request. |
| `unknown_account` | 400 | The request names no account of this deployment (rule 5). | Fix the scope; do not retry as is. |
| `not_found` | 404 | The document the request names by number is not known to szamlazz.hu (code 7): `Szamlazz.Agent.query`'s selector, the invoice of `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of `correct_invoice`. Nothing was sent. (A missing proforma named by `options.proforma: {number}` is `conflict{proforma_missing}`, an outcome.) | Fix the number; do not retry as is. |
| `account_mismatch` | 409 | A document found by number — by `Szamlazz.Order`'s verifies (`storno_invoice`, a corrective's base, the proforma of `create_invoice`'s `options.proforma: {number}`) or by `Szamlazz.Agent.query` / `storno` — belongs to another szamlazz.hu account (`teszt` or `szallito/id` differ from the resolved account's); the message names the observed and expected pins. Nothing was sent. `set_payments` sends without a query and `query_taxpayer` finds no document (a taxpayer record carries no pins): the two handlers that cannot raise it. | Check the account's `mode` / `supplier_id`, or the scope; do not retry blindly. |
| `szamlazz_error` | 422 | szamlazz.hu answered with an error code of its own that the handler passes through rather than concludes from — `Szamlazz.Agent.query` on a code that is neither 7 nor a credential code, `query_taxpayer` on any `funcCode ≠ OK` (szamlazz.hu's own or NAV's relayed one; `valid: false` is a 200), `set_payments` on szamlazz.hu refusing the credit entries. `szamlazz_code` carries the code, `message` szamlazz.hu's text. | Read `szamlazz_code`; a NAV outage on `query_taxpayer` is retried with a new `Idempotency-Key`, a refused credit entry is fixed. |
| `outcome_unknown` | 500 | The create or storno step ran out of the issue policy while a document may or may not have been issued — or `set_payments` lost the reply to its one send. | Rule 2. For `set_payments` with `additive: true` — **at-least-once**: every send that reached szamlazz.hu appended the entries — query the invoice before re-sending; a replacing call is repeated as is. |
| `unavailable` | 503 | szamlazz.hu did not answer a read-only step through every execution of the read policy (the message names the step and the last failure; the order, kind and external id when the step knows them), or answered it with a code nothing can be concluded from (`szamlazz_code` carries it), or returned a storno's original without a fulfillment date (`telj`) — the date the storno must repeat, so it is not sent ([ADR 0007](../../docs/adr/0007-storno-repeats-the-originals-fulfillment-date.md)) — or the account resolver or credential store could not answer. Nothing was sent by the execution that raised it. | Rule 2, later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (rule 4; `szamlazz_code` carries the code). | Page the operator; then rule 2. |

A 5xx whose `x-restate-error-source` is `invocation` is **this worker's** answer, not the Restate ingress being
down. Restate's HTTP invocation docs say to treat `invocation` errors as non-retryable and to auto-retry a 5xx only
when its source is `ingress` (or absent); do that here: page on an `invocation` 503 instead of retrying into it —
`credentials_rejected` in particular repeats identically until the deployment is fixed — and only then retry with
a new `Idempotency-Key` or read `get`.

Retry policy ([ADR 0004](../../docs/adr/0004-kill-not-pause-on-exhausted-retries.md)): every handler that calls
szamlazz.hu pins its own. On `Szamlazz.Order`, `initial_interval = 2m`, factor 2, `max_interval = 10m`,
`max_attempts = 5`, `on_max_attempts = kill`, with `inactivity_timeout = 4m`, `abort_timeout = 3m`,
`journal_retention = 3d` and `idempotency_retention = 30d`; `get` uses `max_attempts = 3` and
`journal_retention = 1d` (inspectable, nothing to replay). `Szamlazz.Agent.storno` carries `Szamlazz.Order`'s policy
and timeouts throughout (its storno step is the same closure); `set_payments` uses two attempts with the same
`initial_interval = 2m` and `2m` / `2m` timeouts (one send) — two, because an additive send is at-least-once and
every attempt is a potential second copy of the entries; `query`, `query_taxpayer` and `check_account` three with the
same one-day journal retention. The 2 m interval is longer than the 60 s client timeout, so the retry after a crash
cannot run while the first send is still in flight. **An invocation attempt is spent only on a worker-side failure**
— the worker unreachable, a rollout cutting the connection, the abort timeout, an undecodable journal — never on a
run retry: a step re-executed under `[issue]`, `[read]` or `[resolve]` is re-dispatched by the server without
advancing the handler's attempt count (verified end to end against 1.7.8). So the run policies decide how long a
szamlazz.hu outage is tolerated, the invocation policy how long a worker outage is (~24 min of back-off on the
`Szamlazz.Order` writes and `Szamlazz.Agent.storno`, 2 min on `set_payments`), and szamlazz.hu's "max 5 attempts"
etiquette is the issue policy's business — every re-dispatch is query-first and multiplies no sends. Kill, not
pause: a paused invocation holds the order's key and blocks the very handler that would reconcile it. Kill releases
the key, and the external-id query inside the create step is what makes that safe.

Inside a handler, issuing is two durable steps. The **lookup** (`lookup-{kind}`) is read-only and settles every
case that needs no create: a live document of ours is `already_issued` (or `conflict{live}` with `reissue`), a
reversed one is `reversed` (or proceeds with `reissue`), an invalid holder is `conflict{external_id_collision}`, a
live invoice under the order that is not ours is `conflict{foreign}`. Like every read-only step of both services —
the exclusivity and proforma-link lookups before it, the verifies, the order-number hint, the storno lookup, `get`'s
four queries, `Szamlazz.Agent.query`, `query_taxpayer`'s one step (`taxpayer-{prefix}`), the `check_account` probe —
it runs under the **read policy** (`[read]`: `5`
executions `5s` → `60s`, bounded by `5m` by default): every szamlazz.hu *answer* is journaled data, and a query
szamlazz.hu did not answer — a transport or parse failure, `szlahu_down` — is the step's retryable error
(`Unanswered`), re-executed after the policy's delay; a read writes nothing, so re-executing it is safe and its
answer is as fresh as a first one. When the read policy is exhausted the handler fails with
`TerminalError{unavailable}` naming the step, the last failure and — where the step knows it — the order, kind and
external id. The **create** (`create-{kind}`) runs under
the issue policy — `[issue]`: `max_attempts` executions, `initial_delay` growing by `factor` to `max_delay`,
bounded by `max_duration` — and every execution is query-first: it sends only when the external id holds
**nothing**, or **exactly the document the lookup step saw reversed**; a live document an earlier execution issued
is answered `issued` without sending, a document reversed since the lookup is answered `reversed` without sending
(a new document needs an explicit `reissue`, [ADR 0003](../../docs/adr/0003-explicit-reissue-after-external-reversal.md)),
and the lookup's reversed document reported live is `conflict{live}`. A lost reply is re-queried once, immediately;
when nothing landed the step is *unconfirmed* and
Restate re-executes it after the delay. When the policy is exhausted (or the invocation is cancelled mid-create)
the handler fails with `TerminalError{outcome_unknown}` naming the order, kind and external id; the next
invocation's lookup finds whatever landed. Correctives take no order-number hint, and a duplicate-order-number
answer their re-query cannot resolve is `rejected`. Storno has the same shape: a read-only lookup of the storno
external id (`lookup-storno-{number}`) and a storno step (`storno-{number}`) under the same issue policy, query-first
on every execution — on both `Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`. The storno request is a
pure function of the verified original — its `telj` as `teljesitesDatum`, its `eszamla` (or the account default) as
the e-invoice flag — so every execution of the step sends byte-identical bytes; a verified original without a `telj`
is `unavailable` with nothing sent, raised after the answers that need no send.

## Testing

- `cargo test -p restate-szamlazz` runs the contract, config and identity unit tests (every request type refusing an
  unknown top-level and nested field by name while the documented bodies still deserialize; with `--features
  schemars`, every request schema closed with `additionalProperties: false` and the response schemas open), the
  discovery and binding tests of the adapters (with `Body<T>` turning a malformed body — an unknown field, a wrong
  type, a missing field, invalid JSON — into the 400 `invalid_input` fault while discovering exactly as `Json<T>`,
  the account pins of a document found by number, the storno intent built from a verified document — its `telj` as
  the fulfillment date, `eszamla` lifted with the account default as fallback, an empty `telj` as the 503
  `unavailable` fault naming the invoice — the `Order` handlers' key parsing refusing an untrimmed key as
  `invalid_input` while `OrderKey::parse` still trims, and the sentinels that the agent key reaches neither the
  `credentials_rejected` warning nor the body of a `credentials_rejected` or `account_mismatch` fault), and the
  wiremock tests of the gateway against synthetic szamlazz.hu responses
  (`tests/gateway.rs`: the lookup matrix — `Absent`, `Live`, `Reversed`, `Collision`, `Foreign`, the corrective's
  exemption from the hint, `Unanswered` on a lost reply and `Api` on another code — and the create step — `Issued`,
  `Found` on a re-executed step, the open codes and `Unconfirmed`, the 71/152 matrix, the corrective's 71/152 →
  `Rejected` — the storno lookup and step — `AlreadyReversed` on a re-executed step, a lost reply re-queried once,
  `Unconfirmed` when nothing landed, the body carrying `<teljesitesDatum>` and no `<keltDatum>`, two executions
  after a lost reply sending byte-identical bodies — plus storno validation including the proforma / delivery-note
  no-op, a verified document's `telj` (and its absence) surfaced, 335, 7,
  the credential codes 3/135/136/164 on every operation, every read fn answering a 500, an empty body or
  `szlahu_down` as `Err(Unanswered)` rather than data, the `check_account` probe as exactly one query of the
  sentinel id with a wrong key as data, and the taxpayer query answering a known prefix as `Found` with NAV's
  registered data, an unknown one as `Found{valid: false}`, a wrong key as `CredentialsRejected`, NAV's relayed
  `funcCode ERROR` or a szamlazz.hu code as `Api`, and a 500, an unparseable body or `szlahu_down` as
  `Err(Unanswered)`).
- The same run checks **journal compatibility**: every type the services journal as a `ctx.run` result is
  additive-only (the rule is in the `gateway` module docs and
  [ADR 0005](../../docs/adr/0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md)), and
  `tests/journal/<type>/<variant>.json` pins one fixture per variant. The generator fails when the current code
  writes a different shape; the compatibility test replays every fixture ever committed through the current types.
  After an additive change, regenerate with `UPDATE_JOURNAL_FIXTURES=1 cargo test -p restate-szamlazz journal` — it
  writes missing fixtures and archives a differing one beside the new shape — and review the diff as a contract
  change. Never regenerate away a rename: an in-flight invocation of the previous deployment would be killed on
  upgrade.
- `cargo test -p restate-szamlazz -- --ignored e2e` runs `tests/service.rs`: the `Szamlazz.Order` Virtual Object
  and `Szamlazz.Agent` end to end against a real Restate server in docker (1.7.8, with the experimental `vqueues`,
  `protocol_v7` and `scoped_virtual_objects` flags — `compose.yaml` sets the same three) with wiremock standing in
  for szamlazz.hu, in two phases on one server. The single-account phase: issued → already_issued, `Idempotency-Key`
  replay, 152 → reconciled, storno → reversed (the storno mock matched on `<teljesitesDatum>` equal to the original's
  `telj`, no `<keltDatum>`) → stale create → `reissue`, a `telj`-less original answered 503 `unavailable` naming the
  order, kind and storno external id with only the verify journaled and the storno mock `expect(0)` — after a
  `telj`-less document of another order → `conflict{not_managed}`, a `telj`-less proforma → `rejected{not_stornoable}`
  and a `telj`-less reversed invoice → `reversed` with its storno number — a storno whose first reply is lost
  re-executed with a byte-identical body under one `storno-{number}` entry, `reissue` on live → `conflict{live}`, an
  external reversal, proforma auto-link and `consumed` in `get`, `options.proforma: {number}` checked like every
  found document (a proforma of this order with the wrong `teszt` → `account_mismatch` after the verify alone with
  the create mock `expect(0)`, another order's or an order-less proforma → `conflict{not_managed}` naming it, this
  order's → `issued` with `dijbekeroSzamlaszam` on the wire), a create with a misspelt `options.reissue` answered
  400 `invalid_input` naming the field with nothing journaled and zero szamlazz.hu requests, a create under an
  untrimmed key (a `%20` before or after the order number) answered 400 `invalid_input` naming the rule with nothing
  journaled and zero szamlazz.hu requests, an exhausted create step answering a structured
  `outcome_unknown` within the run policy's delays with the run's retries visible on `sys_invocation` while it is in
  flight, a lookup whose reply is lost once retried under the read policy and completing `issued` in one invocation
  with exactly one create on the wire, a lookup that never answers as a structured `unavailable` naming the order,
  kind and external id with zero creates, `get` completing after one of its reads is retried, a scoped call answered
  `unknown_account` with zero szamlazz.hu requests, `check_account` unscoped answering
  the account with `credentials: ok` after one sentinel query (and `rejected` as data on code 3), a purged invocation querying
  szamlazz.hu again, a flaky resolver retried under the resolve policy, a failing credential store as a terminal
  `unavailable`, and a positive control for the journal-leak check (a sentinel in a szamlazz.hu rejection is found in
  the hex-decoded `raw` of the create run's result). Then the documented single → multi **flag day** (private,
  drain, register the multi-account revision, public) and the multi-account phase: the first scoped create for an
  order invoiced unscoped finds it under the unchanged external id; unscoped → `unknown_account`; the same order key
  under two scopes concurrently → two `issued` with each account's key on the create wire exactly once; the same
  `Idempotency-Key` under two scopes → two invocation ids and two documents, each replaying its own completion;
  `check_account` under each scope → its own account with its key on the probe, unscoped → `unknown_account`; an
  order whose invocations were purged stornoed and reissued; `Szamlazz.Agent.storno` refusing a document whose
  `teszt` or `szallito/id` is not the resolved account's as `account_mismatch` after the verify alone (storno mock
  `expect(0)`), not checking the supplier id when the account pins none, reversing a document of the account's own
  pins, and checking an order-bearing document's pins before answering it — mismatched pins `account_mismatch`
  without echoing the other account's order number, the account's own pins `managed_by_order`; `Szamlazz.Agent.storno`
  sending `<teljesitesDatum>` equal to the original's `telj` and answering a `telj`-less original 503 `unavailable`
  without an order identity, only the verify journaled and nothing sent — after `account_mismatch`,
  `managed_by_order` and `reversed` on `telj`-less documents; `Szamlazz.Agent.query`
  answering a mismatched document `account_mismatch`, a matching one as the projection and code 7 as `not_found`;
  `Szamlazz.Agent.query_taxpayer` under each scope asking NAV with that scope's key and nothing else on the wire, the
  full tax number under one scope and the bare stem under the other both journaling one `taxpayer-12345678` step,
  `valid: false` as a 200, and a malformed tax number answered 400 `invalid_input` naming it with nothing journaled
  and zero szamlazz.hu requests; an account
  change between two executions not reaching the running invocation (the journaled `Account` wins); a credential
  rotation between two executions picked up by the second with the `account` entry byte-identical; and, last, that
  no agent key of the run appears in the hex-decoded `raw` of any journal entry of any invocation, nor in any
  `completion_failure`, while the same scan finds the positive control's sentinel. The harness calls through
  `/restate/call/…` and `/restate/scope/{scope}/call/…`, reports `x-restate-id`, parses fault bodies and reads
  `sys_journal` / `sys_invocation` through the SQL introspection API. It skips with a message when the docker
  daemon is not reachable; set `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` to reuse a running server (which must
  run with the three flags).
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
