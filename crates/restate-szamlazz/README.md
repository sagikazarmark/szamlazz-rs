# restate-szamlazz

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz)

**Restate services issuing and managing szamlazz.hu documents with durable, idempotent execution.**

The `Szamlazz.Order` Virtual Object, keyed by the order number, serializes issuing per key: a caller says "issue
the invoice for order X" and gets exactly one legal document under retries, process crashes, concurrent callers
and reversals. It keeps **no state**. szamlazz.hu is the source of truth, reached through deterministic external
ids (`{namespace}:{order}:{kind}`), so any invocation can find what an earlier one issued.

The stateless `Szamlazz.Agent` service exposes by-number operations (query, credit entries, storno of unmanaged
documents), the NAV taxpayer lookup (`query_taxpayer`) and the read-only `check_account` probe over the same
gateway module. It is **unkeyed**: its invocations run concurrently, so two by-number writes on one invoice are
not serialised by the worker the way an order's handlers are. Two replacing `set_payments` (`additive: false`)
race and the last send to land wins, which under reordered webhook deliveries may be the older snapshot. The
caller serialises per invoice on its side, or sends `additive: true` and lets szamlazz.hu sum.

Both services are projections of the Számla Agent model: deployment constants live in configuration, line totals
are computed, domain outcomes are returned as data.

## Quick Start

Bind both services to a Restate endpoint of your own:

```rust
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::account::{StaticConfig, StaticResolver};
use restate_szamlazz::{Accounts, Agent, Order, WorkerConfig};

async fn serve(accounts: StaticConfig, worker: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    let worker = worker.validate()?;
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
(a TOML file layered with environment overrides through figment, for instance).

- `WorkerConfig` is the deployment-level part: the `namespace` of the external ids and the three run retry
  policies, `[issue]`, `[read]` and `[resolve]`. Call `validate()` after parsing.
- `StaticConfig` is the static resolver's configuration, in one of two mutually exclusive shapes: a single
  `[account]`, served unscoped, or a table of `[accounts.<scope>]`, each served under its scope only
  (`/restate/scope/{scope}/call/…`). `StaticResolver::try_from` validates it and implements both the account
  resolver and the credential store; `Accounts::from` bundles the two.
- A deployment with its own resolver and store builds `Accounts::new` over them instead. The
  [crate documentation](https://docs.rs/restate-szamlazz/latest/restate_szamlazz/#your-own-resolver-and-store)
  has a database-backed resolver, a credential store of the embedder's own and a service of the embedder's own on
  one endpoint, and names the two compiler errors that path hits: `account::Endpoint` vs
  `restate_sdk::prelude::Endpoint` (alias one), and, when one value is both resolver and store,
  `Arc::clone(&db)` inferring `Arc::<dyn AccountResolver>::clone` where `db.clone()` coerces. The checklist a
  resolver of your own must guarantee is on the `AccountResolver` and `CredentialStore` rustdoc.

Neither service holds a gateway or a client: every handler resolves its account and opens a `Gateway` for its own
execution. Going from `[account]` to `[accounts.<scope>]` is a flag day with no data migration, scripted in
[ADR 0006](../../docs/adr/0006-account-selection-via-restate-scopes.md) and the
[design document](../../docs/design/restate-szamlazz.md).

## Compatibility

The crate re-exports the two crates it is built on, `restate_szamlazz::restate_sdk` and
`restate_szamlazz::szamlazz_agent`, and the two Számla Agent types the `CredentialStore` trait is written in,
`restate_szamlazz::{Credentials, AgentKey}`, so an embedder pins one version of each and never meets two
`Credentials` types. The coupling is:

| restate-szamlazz | szamlazz-agent | restate-sdk | Restate server |
|---|---|---|---|
| 0.x | 0.x, same minor | 0.12 | 1.7.8 with protocol v7 (`vqueues`, `protocol_v7`, `scoped_virtual_objects` for multi-account mode) |

The SDK's `#[restate_sdk::service]` / `#[object]` macros expand to `::restate_sdk` paths, so a crate that defines
services of its own also names `restate-sdk` as a direct dependency at the same minor (Cargo unifies the two into
one build of the SDK); a crate that binds only `Order` and `Agent` needs the re-export alone. The workspace MSRV
is Rust 1.92.

## Scope Contract

### What `Szamlazz.Order` guarantees

- **Exactly one live document per kind per order** (proforma, invoice, prepayment, final) under caller retries,
  process crashes and concurrent callers. Same-key handlers run one at a time. Issuing is a read-only **lookup**
  step that settles every case needing no create, then a **create** step under a run retry policy whose every
  execution queries szamlazz.hu by the document's external id *inside the same `ctx.run` closure* before it
  creates, so a request that landed before a crash, a timeout or a lost reply is found, not re-issued.
- **Correctives** are issued under a caller-supplied `correction_id`: the same id finds the corrective it
  issued, a new id issues a new one.
- **Storno** (`storno_invoice`) and **proforma deletion** are idempotent. A document reversed by anyone (the UI,
  support, this service) is reported as `reversed` from `<sztornozott>`. The storno carries the original's
  fulfillment date (`teljesitesDatum` = the verified document's `telj`), which NAV requires it to repeat; the
  caller cannot set it.
- **Domain outcomes are data** (HTTP 200) and errors are reserved for faults, so a caller can always tell "the
  document exists" from "the outcome is unknown".
- **`get`** is a live view: what szamlazz.hu holds under the order's four external ids right now, never a
  cached snapshot.

### What it relies on

**The szamlazz.hu account setting "Rendelésszám ismétlődés tiltása" (Disable order number repetition) is
ON.** It is the server-side guard against a second live document of the same kind under one order number
(71/152) and the reason a byte-identical resend is answered with the same number while the first document is
live. Running without it is unsupported.

**Deterministic external ids**, derived from the order key alone, so that a re-executed closure or a new
invocation can ask "is there already one?" without state.

**Validation of every found document**: order number and `tipus`, because external ids are not unique
server-side and a query returns the newest holder. Nothing about the account: the worker holds **no account
pin**. A create's reply carries neither `teszt` nor the seller block (only a query body does), so any check on
them could fire only after the first document of a fresh order had been issued into whatever account the key
opens; 0.3's two tripwires (`mode` against `teszt`, `supplier_id` against the undocumented `szallito/id`) were
dropped for that reason. Which account a key opens, and whether it is a test account, is the operator's go-live
check: query a known document under each scope and read `test` and the seller block.

**A permanent namespace.** The deployment's namespace prefixes the external ids; changing it would hide every
document issued so far. It is pinned per invocation, so a redeploy cannot move a running invocation.

**The account is resolved once per invocation and journaled.** Every handler resolves the request's scope to its
`Account` in a durable step named `account` under the resolve policy, so an invocation finishes on the account it
started on; the journaled `Account` (visible in the Restate UI for the retention period) carries everything but
the agent key. Unscoped and unknown scopes are `unknown_account` (400) before anything is issued.

**The scope is the only channel for the account**, never a header, a body field or the key. The scope reaches
the worker only under **protocol v7**, and Restate's ingress does not refuse a scoped path when v7 is off: the
SDK would see no scope and a single-account deployment would issue on its one account.
`Szamlazz.Agent.check_account` under each scope after every deploy is the defence: it answers the `scope` the SDK
saw (`null` under a scoped call is that misconfiguration), the configured account, and whether szamlazz.hu
accepted its key. Multi-account mode depends on three experimental Restate flags (`vqueues`, `protocol_v7`,
`scoped_virtual_objects`), verified on server 1.7.8 with SDK 0.12.0. Kafka ingress is untested and unsupported in
multi-account mode.

**Credentials are fetched on every handler execution, outside the journal**, and held only for that execution.
A rotation is picked up on the next execution of every in-flight invocation, and no agent key is ever written
into Restate (the `Credentials` type has no serde implementation; the e2e suite scans every journal entry for the
run's keys). A failed fetch is a **terminal** `unavailable` after a short in-process retry, by decision: a
retryable error would route a prolonged store outage into the handler's kill-on-five and an unstructured 500,
whereas the terminal fault is structured and immediate. The cost: a store outage during a **replay** of an
invocation whose create already landed surfaces as `unavailable` even though the document exists; `get` or a
retry with a new `Idempotency-Key` reconciles it (`already_issued`).

### The safety contract

The static resolver enforces what can be checked at load time; a resolver of your own, and the operator,
guarantee the rest.

1. One szamlazz.hu account is reachable under exactly one scope value; unscoped counts as a value; no fan-in
   (two scopes reaching one account would split an order's per-key lock across two Virtual Objects). The static
   resolver's single `[account]` is served unscoped and knows no scope; its `[accounts.<scope>]` shape is served
   by scope only and is checked at load time: unique `(endpoint, agent_key)` pairs, the endpoint compared
   normalised (`Endpoint::normalized`, so two spellings of one server are one), and unique ids.
2. The scope → account mapping is append-only: moving traffic to another account means a new scope, never
   re-pointing an existing one. Appending a scope cannot create fan-in; any change that could put one account
   under two identities at once (the single → multi flag day above all) is a drain, switch, resume.
3. The namespace is permanent for the deployment.
4. Order keys are unique within an account for its lifetime, across all writers.
5. The caller records the order key and the scope as used; nothing else is needed to operate on the order
   later.
6. The scope is routing, not authorization: the ingress sits behind a gateway that sets the scope from the
   authenticated identity, never forwards a caller-supplied scope path, and strips `x-restate-*` request
   headers (the ingress lets a caller's copy of one of its own headers win).
7. Ownership of a document is decided by the external-id query alone; the order-number query can name a
   document but never prove ownership.

### What it does not do

PDF download, receipts, IPN and Adatkapcsolat ingestion, the proforma → payment → invoice lifecycle workflow,
multiple prepayments per order, tracking *who* reversed a document, serialising `Szamlazz.Agent`'s by-number
writes per invoice (the service is unkeyed, see above), and reissuing on its own initiative: a create after any
reversal returns `reversed` and issues a replacement only with `reissue: true` and a new `Idempotency-Key`.

## Feature Flags

The crate has no default features; `restate-sdk` is always a dependency.

- `schemars`: derives JSON Schema for the contract types, so Restate's discovery manifest and OpenAPI export
  document every handler's input and output. Enables `restate-sdk/schemars` as well, and Cargo unifies features
  per build, so with it on `restate_sdk::serde::Json<T>` implements `PayloadMetadata` only for `T: JsonSchema`:
  every `Json<T>` handler on the shared endpoint, the embedder's own included, must derive `schemars::JsonSchema`
  on its `T` (or implement `PayloadMetadata` by hand). The contract types' schema descriptions are plain prose;
  no rustdoc link syntax leaks into the OpenAPI export (a schema test pins it).

See the [crate documentation](https://docs.rs/restate-szamlazz/latest/restate_szamlazz/) for API and feature
semantics and the [generated feature graph](https://docs.rs/crate/restate-szamlazz/latest/features) for
activation details.

## Key Types

### Contract

`contract::CreateRequest` / `CreateResponse` are the input and output of the four `create_*` handlers. The
request carries the `DocumentInput` (buyer, line items, dates, payment method, per-call overrides) and
`CreateOptions` (`reissue`, `proforma: auto | none | {number}`); the response carries the `Outcome`, the identity
(`kind`, `external_id`), the numbers and totals, and `warnings`. `customer_account_url` is set only on the
execution that actually issued (a fresh `issued`), never on `already_issued`, `reconciled` or `get`.
`CorrectRequest` (`invoice_number`, `correction_id`, `document`) is the input of `correct_invoice` and shares the
response.

Every request type, and every object it nests, is closed (`#[serde(deny_unknown_fields)]`,
`additionalProperties: false` in the schema): a field the contract does not know is refused as `invalid_input`
naming the field, never silently dropped. Response types stay open. The `conflict_reason` table is below; the
`schemars` feature puts the full request and response schemas into the discovery manifest.

`service::Body<T>` is how every handler takes its input: a `Json<T>` whose decode runs in the handler, so a
malformed body is the structured `invalid_input` fault instead of the SDK's plain-text 400. Same discovery schema
as `Json<T>`; built with `Body::new` / `From<T>` for calls through the generated clients.

`contract::Outcome` / `ConflictReason`: `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or
`conflict` with a reason. Both carry `ALL` and `as_str` (the snake-case token, what a caller branches on), and
both are `#[non_exhaustive]`: a client branches with a default arm. The
reasons:

| Reason | When |
|---|---|
| `prepaid_chain` | A plain invoice while the order's own prepayment invoice or final invoice is live, or a prepayment invoice while the order's own invoice or final invoice is. The final invoice keeps the chain closed after its prepayment is reversed. |
| `order_invoiced` | A proforma after the order's own live invoice, prepayment invoice or final invoice. |
| `live` | `reissue: true` on a live document. |
| `foreign` | A live invoice under the order number that is under none of the order's external ids (another channel's). |
| `duplicate_order_number` | szamlazz.hu refused the order number (71/152) and the external-id re-query found nothing of ours live. |
| `external_id_collision` | The external id's holder does not carry this order's number and kind. |
| `proforma_live`, `proforma_missing` | `options.proforma` and the order's proforma disagree. |
| `prepayment_missing`, `prepayment_reversed` | `create_final` without a live prepayment invoice. |
| `base_reversed` | `correct_invoice` on a reversed base. |
| `not_managed` | A document named by number does not carry this order's number. |

`contract::TerminalCode` is the set of fault codes a `TerminalError` carries, each with its HTTP status
(`TerminalCode::status`; `TerminalCode::ALL` lists them in the order of the fault table below):

- `invalid_input` (400)
- `unknown_account` (400): the request names no account of this deployment
- `not_found` (404): the document the request names by number is not known to szamlazz.hu (code 7)
- `szamlazz_error` (422): szamlazz.hu answered with an error code of its own that the handler passes through,
  the code in the fault's `szamlazz_code` field
- `outcome_unknown` (500)
- `unavailable` (503); also the prologue's own faults: the resolve policy exhausted, the credential store gone
  or unavailable
- `credentials_rejected` (503): szamlazz.hu answered 3, 135, 136 or 164; the worker's agent key is wrong, not
  the request, and the outcome is not known

`contract::CheckAccountResponse` (`CheckedAccount`, `CredentialsCheck`) is the output of
`Szamlazz.Agent.check_account`: `scope`, `account: {id}`, `namespace` and
`credentials: {state: ok} | {state: rejected, code, message}`. Credential acceptance is its only
szamlazz.hu-verified fact; the rest echoes the configuration.

`contract::QueryTaxpayerRequest` / `QueryTaxpayerResponse` (`TaxpayerAddress`) is the contract of
`Szamlazz.Agent.query_taxpayer`. In: `tax_number`, the bare eight-digit stem (`12345678`) or the full
`NNNNNNNN-N-NN` form (`12345678-2-42`) and nothing else (`QueryTaxpayerRequest::prefix` derives the prefix or
names what was wrong). Out: `{valid, name?, tax_number?, vat_code?, addresses[]}`, a crate-owned additive-only
projection of the agent crate's `TaxpayerInfo` (what the read step journals), with `valid: false` a normal
answer. Not cached by the worker; cache it in the caller with a TTL on the order of a day.

`contract::StornoRequest` / `StornoResponse` (`StornoOutcome`: `reversed`, `rejected`, `conflict`,
`managed_by_order`), `DeleteProformaRequest` / `DeleteProformaResponse`, `QueryRequest` (`Selector`) /
`QueryResponse` and `SetPaymentsRequest` / `SetPaymentsResponse` are the remaining handler contracts.

`contract::OrderStatus` / `DocumentStatus` is the live view `get` returns: one optional `DocumentStatus` per kind
(`number`, `state`, `gross`, `net`, `payments`, `referenced_proforma`, `e_invoice`) with `DocumentState`
flattened as `{state: live}`, `{state: reversed, storno_number}` or, for a consumed proforma,
`{state: consumed, by}`. `get` never fills `storno_number` (finding the storno would take the order-number hint,
which shows only the newest document); the create and storno handlers report it. A `null` slot is *nothing of
ours* under that external id, which may still be a foreign holder (a create there answers
`conflict{external_id_collision}`). Correctives are not in the view.

### Identity

`CorrectionId` is the caller-supplied identity of one corrective invoice: `^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$`
and not one of the external-id tokens (`ExternalId::TOKENS`, in any letter case); embedded in the corrective's
external id.

`contract::InvoiceNumber` is an invoice number as the by-number requests take it (`StornoRequest`,
`CorrectRequest`, `SetPaymentsRequest`, `Selector::InvoiceNumber`, `ProformaLink::Number`): 1–40 bytes, no
whitespace, no control character, no `:`. Refused by its `Deserialize`, so a bad number is a malformed body
(`invalid_input` before the prologue). It flows into step names and the storno external ids, hence the bound.
Distinct from `szamlazz_agent::InvoiceNumber`, the unvalidated wire type; responses echo numbers as plain strings.

`OrderKey` is the `Order` key: the order number trimmed of leading and trailing whitespace, case preserved,
validated (1–40 bytes, no control characters, no internal whitespace of any kind, no `:`, Unicode NFC; nothing
is collapsed or normalised). The type trims; the `Order` handlers do not: a Virtual Object key with leading or
trailing whitespace is refused as `invalid_input` naming the rule (see "Identity Model").

`ExternalId` is the deterministic `szamlaKulsoAzon` of a document: `for_kind`, `for_corrective`, `for_storno`,
`for_unmanaged_storno`, and `for_probe`, the two-segment `{namespace}:check-account` sentinel that nothing the
service issues carries. Every composition is at most `ExternalId::MAX_LEN` = 110 bytes (the length verified
accepted and queryable) because its parts are bounded (namespace 16, order key, correction id and invoice number
40 each), which `const` assertions prove for the longest shape of each (109 for the corrective).

### Configuration

`WorkerConfig` is the deployment-level configuration the services hold, closed to unknown keys at every level:

- `namespace` (the `identity::Namespace`): the external-id prefix of the deployment, 1–16 bytes of `[a-z0-9-]`,
  permanent.
- `[issue]`, `[read]`, `[resolve]`: the three run retry policies, one `RetryPolicyConfig` each with the table's
  defaults (`IssueConfig`, `ReadConfig`, `ResolveConfig` are the three instantiations): `max_attempts` (optional; the
  duration is the sole bound when unset), `initial_delay`, `factor`, `max_delay`, `max_duration`. The issue policy
  runs the create and storno steps, by default `5` executions, `2m` → `10m`, bounded by `1h`; the read policy runs
  every read-only step, by default `5` executions, `5s` → `60s`, bounded by `5m`; the resolve policy runs the
  `account` step, by default with no attempt cap, `1s` → `10s`, bounded by `1m`.

Each policy's `run_retry_policy()` is the `RunRetryPolicy` its steps run under. `validate()` checks the
cross-field invariants (`max_attempts ≥ 1` where set, `initial_delay ≤ max_delay`, a finite `factor ≥ 1`) and one floor:
`issue.initial_delay ≥ IssueConfig::MIN_INITIAL_DELAY`, the Számla Agent client's exported `REQUEST_TIMEOUT`
(60 s) plus a 30 s margin (90 s), because a create or storno step re-executed sooner would re-check while its send
may still be in flight; the error names the rule. It yields the `ValidatedWorkerConfig` that `Order::from_parts`
and `Agent::from_parts` take, so a deployment cannot run on a policy below the floor (the `test-util` feature's
`ValidatedWorkerConfig::unchecked` is for test harnesses whose szamlazz.hu is a mock).

Nothing account-shaped is in `WorkerConfig`: document defaults and the seller block belong to the `Account`, and
their value types (`account::Defaults`, `account::SellerConfig`, `account::SellerEmailConfig`) are journaled with it,
so they stay permissive and additive-only. The static resolver reads them through closed input types of its own
(`StaticDefaults`, `StaticSeller`, `StaticSellerEmail`, beside `StaticAccount`'s `Secret` agent key, whose `Debug`
output is redacted), which mirror them field for field.

### Accounts

`account::Account` is one szamlazz.hu account as the worker knows it (never its key). `Accounts` bundles the two
pluggable traits both services hold, `AccountResolver` and `CredentialStore`; `StaticResolver` / `StaticConfig`
is the configuration-backed implementation of both.

The traits are object-safe (`BoxFuture`), require no `Debug`, and carry the checklist a resolver of your own
guarantees: no fan-in, append-only, unique `(endpoint, credentials)` pairs, the right key under the right scope
(the worker holds no account pin: verified at go-live by reading `test` and the seller block of a known document
under each scope, never inferred), a stable `credential_ref` across rotations, never caching `Unscoped` /
`Unknown`. `Accounts`' `Debug` (and so `Order`'s and `Agent`'s) names the trait objects without descending into
them, so a store that derives `Debug` over a key map cannot print its keys through the services.

`StaticConfig` is either `[account]` (`id`, `agent_key`, `endpoint`, `defaults`, `seller`; reachable unscoped) or
a table of `[accounts.<scope>]` (the same fields; each reachable under its scope only, keys `[a-z0-9_]` of at
most 36 bytes so environment overrides can address them), never both. `StaticResolver::try_from` validates it,
and `Accounts::from` bundles it as resolver and store.

### Gateway and services

`gateway::Gateway` is the module that speaks to szamlazz.hu on behalf of one account, over
`szamlazz_agent::Client`: one plain async fn per `ctx.run` (`lookup`, `create`, `verify`, `query`, `hint`,
`lookup_storno`, `storno`, `delete_proforma`, `set_payments`, `query_taxpayer`, `probe`), each returning every
expected szamlazz.hu outcome as data. Two `Err`s say what a run retry policy may re-execute:

- the read fns (`lookup`, `verify`, `query`, `hint`, `lookup_storno`, `query_taxpayer`, `probe`) return
  `Err(Unanswered)` when szamlazz.hu did not answer (a transport or parse failure, `szlahu_down`);
- `create` and `storno` return `Err(Unconfirmed)` for an outcome that is *not* known. An answer to their leading
  query (another code, `szlahu_down`) is data: nothing was sent.

It is not a second client: the Számla Agent `Client` is the transport it wraps. Every read of account
configuration by the services goes through `Gateway::account()`; a gateway is opened per handler execution by the
prologue (`Gateway::open`) and never outlives it. `Gateway::open_with_http` opens one over a caller-built
`reqwest::Client` (re-exported as `szamlazz_agent::reqwest`): the embedder's hook for a proxy or a custom TLS
setup, and what this crate's unit and wiremock tests open their gateways with, over a client that loads no root
certificates, so that none of them parses the system CA store for a plain-`http://` mock; a fresh client per
gateway is then the caller's to keep, since two gateways over one client share its cookie jar. `Szamlazz.Order`
calls it inside `ctx.run`; the `Szamlazz.Agent` Restate service is a thin facade over the same module. No Restate
service calls another.

`Order` / `Agent` are the Restate Virtual Object registered as `Szamlazz.Order` and the stateless service
registered as `Szamlazz.Agent`, with generated `OrderClient` and `AgentClient` for typed calls from other
handlers. Both are built `from_parts(Accounts, WorkerConfig)`. Every handler decodes its body (`Body<T>`; a
malformed one is `invalid_input` before anything is journaled) and runs the prologue (pin the namespace, resolve
the account in the `account` step, fetch the credentials, open the gateway) before its operation.

## Identity Model

Three identities work together.

**The order key** decides which `Order` instance runs; same-key handlers run one at a time, which is what
serializes issuing per order. The object holds no state. The key is the order number **trimmed by the caller**:
Restate's per-key lock is on the raw key, so `ORD-1` and ` ORD-1` would be two instances with two locks mapping
to one szamlazz.hu order and identical external ids, and two concurrent creates under them would both pass their
lookup and both send. A key with leading or trailing whitespace is therefore refused as `invalid_input` before
anything is journaled or sent.

**The external id** identifies a document to szamlazz.hu and is derived from the key alone under the
deployment's namespace:

| Document | External id |
|---|---|
| proforma, invoice, prepayment, final | `{namespace}:{order}:{kind}` |
| corrective | `{namespace}:{order}:corrective:{correction_id}` |
| storno of an order's invoice | `{namespace}:{order}:storno:{original_number}` |
| storno via `Szamlazz.Agent` (no order) | `{namespace}:by-number:{number}:storno` |

Every id is at most **110 bytes**, the length verified accepted and queryable on szamlazz.hu (which documents no
limit), because its parts are bounded: the namespace at 16, the order key, the `correction_id` and the caller's
invoice number at 40 each (a dashed UUID fits every one). `:` is the separator and is excluded from every part; a
`correction_id` equal to one of the tokens (`invoice`, `storno`, `check-account`, …) is refused.

The id is queried by the lookup step and again by every execution of the create step, inside the create's own
`ctx.run` closure, so a request that landed before a crash, a timeout or a lost reply is found, not re-issued.
There is **no generation counter**: external ids are not unique server-side and a query returns the newest
holder, which is exactly the question asked ("what is the newest document of this kind we issued for this
order?"). A reissued invoice becomes the newest holder of the same id; the stornoed original stays reachable by
number and through the storno's `hivszamlaszam`. Because the id is not unique, every found document is
**validated** before it is trusted: `rendelesszam == order` and `tipus` of the expected kind; anything else is
`conflict{external_id_collision}`.

**The `Idempotency-Key`** of the ingress identifies a logical request; the service never relies on it for safety.

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

The request and response bodies are the `contract` types above (their `serde` shape is the wire shape; the
`schemars` feature publishes it in the discovery manifest); the fault envelope is under *Faults* below. The rules,
for a caller:

1. Send an **`Idempotency-Key`** per logical request. Restate dedupes retries and attaches concurrent duplicates
   to the in-flight invocation.
2. Tell a **fault** from **no answer** before deciding what to do with the key.
   - **A fault** is a 4xx/5xx **with a body the worker wrote** and `x-restate-error-source: invocation`: the
     invocation completed, and Restate stores that completion under the key for the retention period (30 days on
     the write handlers), so the same key would replay the failure. An **`outcome_unknown`, `unavailable` or
     `credentials_rejected`** fault from an issuing or storno handler means "outcome unknown: retry with a
     **new** key, or read `Szamlazz.Order.get`", never "no document exists"; the retry with a new key reconciles
     by external id and is safe.
   - **No answer** (your client timed out, or the ingress answered with a 5xx whose source is *not*
     `invocation`) means the invocation is still in flight: it runs on once the key frees, and under a worker
     outage Restate re-dispatches it for up to ~24 min on `Szamlazz.Order` (five attempts, 2 m → 10 m). **Keep
     the key** and retry with it (the retry attaches to the in-flight invocation and receives its outcome) or
     read `get`; a new key here would start a second invocation that queues behind the first.
   - **A killed invocation** (attempts exhausted) is a fault whose envelope `message` is the last retryable
     error's **text**, not the worker's `{code, message}` JSON: treat an unparsable 5xx `invocation` body as
     `outcome_unknown`.
   - **The other faults are settled**, nothing landed: `invalid_input`, `unknown_account` and `not_found` are
     raised before anything is sent, and `szamlazz_error` is szamlazz.hu answering with an error (to a read, or
     refusing the credit entries it was sent). Retrying as is repeats the answer: fix the request, the number,
     the scope or the account, or, for a `szamlazz_error` relaying a NAV outage, retry later with a new key.
3. After a storno (by this service, the UI or anyone) a create returns `outcome: reversed`. Send
   `reissue: true` (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document is
   `conflict{live}`, so the flag can never cause a duplicate.
4. A `credentials_rejected` fault (503) means szamlazz.hu refused the worker's agent key (codes 3, 135, 136,
   164) on some step: the deployment is misconfigured, not the request. The request that drew the code was not
   acted on, but the code may have come to a re-query after a send, and an earlier execution may have landed
   with a lost reply, so rule 2 applies: once the key is fixed, retry with a new key or read `get`. The worker
   logs every occurrence at `warn` with the namespace and the code, inside the execution's span
   (`execution{scope, order, restate.invocation.id, account.id}`, which every handler execution runs in), so the
   line says whose key broke and under which invocation (`restate.invocation.id` is the `x-restate-id` the caller
   got); the key itself appears in neither the log nor the fault.
5. An `unknown_account` fault (400) means the request named no account of this deployment: unscoped where
   accounts are scoped, or a scope no account is reachable by. Nothing was issued and the same request never
   succeeds: fix the scope, do not retry.
6. On a multi-account deployment, set the scope on **every** call (it is a path segment of the request, not a
   session) and record the order key *and the scope as used* per order. `Idempotency-Key`s are deduplicated per
   scope. `order_key` in a storno response is meaningful only under the same scope; `external_id` is the only
   namespace marker in any response, and no response names the account.

### Faults

Faults are `TerminalError`s whose message is the JSON `{ "code", "message", "szamlazz_code"?, "order"?, "kind"?,
"external_id"? }`. **On the wire that JSON is a string inside Restate's ingress envelope**: the body is
`{"code": <HTTP status>, "message": "<the fault JSON>", "source": "invocation"}` under
`x-restate-error-source: invocation`, so a caller parses `message` a second time; the envelope's own `code` is
the status below, never the token (the Rust SDK carries a terminal error as code plus message and offers no other
channel).

The fault's `code` is always one of the tokens of `contract::TerminalCode`. A szamlazz.hu code never travels in
it, but in `szamlazz_code` beside it, present on every fault a szamlazz.hu answer caused: the `szamlazz_error`
pass-through, `credentials_rejected`, and `unavailable` on a code a read cannot conclude from.

A malformed body is the same shape: every handler decodes its own body (`service::Body<T>`, not the SDK's
`Json<T>`), so an unknown field, a wrong type, a missing required field or invalid JSON is
`{ "code": "invalid_input", "message": "malformed request body: …" }` with serde's message, naming the field
when there is one, never the SDK's plain-text `Cannot decode input payload`.

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `invalid_input` | 400 | The request is malformed: its body carries a field the contract does not know (every request type is closed: ``unknown field `resissue`, expected `reissue` or `proforma` ``), a wrong type, a missing required field, an `invoice_number` or `correction_id` outside its bound (40 bytes; no whitespace or `:`; not an external-id token), or its `Order` key has leading or trailing whitespace or is outside the key alphabet (1–40 bytes, no internal whitespace, no `:`, NFC); refused before anything is journaled or sent. Or it carries a value the operation cannot take: an option the handler does not take, a `{number}` proforma link that is not a proforma, a sixth credit entry on `set_payments`, a replacing `set_payments` (`additive: false`) with no entries (the wire contract takes five, and an empty replace would clear the invoice's payments; nothing is sent), or a line item whose arithmetic overflows a decimal (after the prologue's two journal entries, before any read; nothing is sent). | Fix the request. |
| `unknown_account` | 400 | The request names no account of this deployment (rule 5). | Fix the scope; do not retry as is. |
| `not_found` | 404 | The document the request names by number is not known to szamlazz.hu (code 7): `Szamlazz.Agent.query`'s selector, the invoice of `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of `correct_invoice`. Nothing was sent. (A missing proforma named by `options.proforma: {number}` is `conflict{proforma_missing}`, an outcome.) | Fix the number; do not retry as is. |
| `szamlazz_error` | 422 | szamlazz.hu answered with an error code of its own that the handler passes through rather than concludes from: `Szamlazz.Agent.query` on a code that is neither 7 nor a credential code, `query_taxpayer` on any `funcCode ≠ OK` (szamlazz.hu's own or NAV's relayed one; `valid: false` is a 200), `set_payments` on szamlazz.hu refusing the credit entries. `szamlazz_code` carries the code, `message` szamlazz.hu's text. | Read `szamlazz_code`; a NAV outage on `query_taxpayer` is retried with a new `Idempotency-Key`, a refused credit entry is fixed. |
| `outcome_unknown` | 500 | The create or storno step ran out of the issue policy while a document may or may not have been issued, or `set_payments` lost the reply to its one send. | Rule 2. For `set_payments` with `additive: true` (**at-least-once**: every send that reached szamlazz.hu appended the entries) query the invoice before re-sending; a replacing call is repeated as is. |
| `unavailable` | 503 | szamlazz.hu did not answer a read-only step through every execution of the read policy (the message names the step and the last failure; the order, kind and external id when the step knows them), or answered it with a code nothing can be concluded from (`szamlazz_code` carries it), or returned a storno's original without a fulfillment date (`telj`), the date the storno must repeat, so it is not sent; or the account resolver or credential store could not answer (reporting so, or silent past the worker's ten-second bound on the call). Nothing was sent by the execution that raised it. | Rule 2, later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (rule 4; `szamlazz_code` carries the code). | Page the operator; then rule 2. |

A 5xx whose `x-restate-error-source` is `invocation` is **this worker's** answer, not the Restate ingress being
down. Restate's HTTP invocation docs say to treat `invocation` errors as non-retryable and to auto-retry a 5xx
only when its source is `ingress` (or absent); do that here: page on an `invocation` 503 instead of retrying into
it (`credentials_rejected` in particular repeats identically until the deployment is fixed) and only then retry
with a new `Idempotency-Key` or read `get`.

### Retry policy

Every handler that calls szamlazz.hu pins its own invocation retry policy.

| Handler | Attempts | Interval | Timeouts (inactivity / abort) | Journal retention |
|---|---|---|---|---|
| `Szamlazz.Order` writes (`create_*`, `correct_invoice`, `storno_invoice`, `delete_proforma`) | 5, kill | 2m → 10m, factor 2 | 4m / 3m | 3d (idempotency 30d) |
| `Szamlazz.Order.get` | 3 | server default | 2m / 2m | 1d |
| `Szamlazz.Agent.storno` | 5, kill | 2m → 10m | 4m / 3m | 3d |
| `Szamlazz.Agent.set_payments` | 2 | 2m | 2m / 2m | 3d |
| `Szamlazz.Agent.query`, `query_taxpayer`, `check_account` | 3 | 10s | 2m / 2m | 1d |

`set_payments` gets two attempts because an additive send is at-least-once and every attempt is a potential
second copy of the entries. The timeouts follow one rule: a step's szamlazz.hu round trips at the client's 60 s
`REQUEST_TIMEOUT` each, plus the margin a stalling szamlazz.hu needs. `4m` / `3m` where the step is three trips
(the create and storno steps' leading query, send and re-query); `2m` / `2m` where it is one, which is
`set_payments`' send and every read step alike. The reads never run on the server's 1 m defaults, on which a read
stalled for the minute szamlazz.hu has been seen to stall would be suspended and then aborted, an invocation
attempt spent on a read that would have completed. The 2 m interval is longer than the 60 s client timeout, so
the retry after a crash cannot run while the first send is still in flight.

**An invocation attempt is spent only on a worker-side failure** (the worker unreachable, a rollout cutting the
connection, the abort timeout, an undecodable journal), never on a run retry: a step re-executed under `[issue]`,
`[read]` or `[resolve]` is re-dispatched by the server without advancing the handler's attempt count (verified end
to end against 1.7.8). So the run policies decide how long a szamlazz.hu outage is tolerated, the invocation
policy how long a worker outage is (~24 min of back-off on the `Szamlazz.Order` writes and
`Szamlazz.Agent.storno`, 2 min on `set_payments`), and szamlazz.hu's "max 5 attempts" etiquette is the issue
policy's business: every re-dispatch is query-first and multiplies no sends.

**Kill, not pause.** A paused invocation holds the order's key and blocks the very handler that would reconcile
it. Kill releases the key, and the external-id query inside the create step is what makes that safe.

**The prologue's own waits are bounded too.** One `AccountResolver::resolve` or `CredentialStore::fetch` call
gets ten seconds (a worker constant, not a setting: a resolver that has not answered by then is not going to),
after which the call is dropped and answered as unavailable, retried under `[resolve]`, or by the fetch loop's
three in-process attempts, then the terminal `unavailable`, whose text names the deadline and neither the account
nor the credential reference. A hung database pool behind an embedder's resolver therefore never holds an
execution until the handler's inactivity timeout.

### Issuing, step by step

Inside a handler, issuing is two durable steps.

**The lookup** (`lookup-{kind}`) is read-only and settles every case that needs no create: a live document of
ours is `already_issued` (or `conflict{live}` with `reissue`), a reversed one is `reversed` (or proceeds with
`reissue`), an invalid holder is `conflict{external_id_collision}`, a live invoice under the order that is not
ours is `conflict{foreign}`.

Like every read-only step of both services (the exclusivity and proforma-link lookups before it, the verifies,
the order-number hint, the storno lookup, `get`'s four queries, `Szamlazz.Agent.query`, `query_taxpayer`'s one
step `taxpayer-{prefix}`, the `check_account` probe) it runs under the **read policy** (`[read]`: `5` executions
`5s` → `60s`, bounded by `5m` by default). Every szamlazz.hu *answer* is journaled data, and a query szamlazz.hu
did not answer (a transport or parse failure, `szlahu_down`) is the step's retryable error (`Unanswered`),
re-executed after the policy's delay; a read writes nothing, so re-executing it is safe and its answer is as
fresh as a first one. When the read policy is exhausted the handler fails with `TerminalError{unavailable}`
naming the step, the last failure and, where the step knows it, the order, kind and external id.

**The create** (`create-{kind}`) runs under the issue policy (`[issue]`: `max_attempts` executions,
`initial_delay` growing by `factor` to `max_delay`, bounded by `max_duration`) and every execution is
query-first. It sends only when the external id holds **nothing**, or **exactly the document the lookup step saw
reversed**:

- a live document an earlier execution issued is answered `issued` without sending;
- a document reversed since the lookup is answered `reversed` without sending (a new document needs an explicit
  `reissue`);
- the lookup's reversed document reported live is `conflict{live}`;
- an *answer* to the leading query that is neither 7 nor a credential code (another szamlazz.hu code, or
  `szlahu_down`) is settled data too, with nothing sent: the handler raises the same `unavailable` the lookup
  step raises for that code, at once, rather than spending the issue policy on a read.

A lost reply is re-queried once, immediately; when nothing landed the step is *unconfirmed* and Restate
re-executes it after the delay (a re-query that fails itself leaves the step unconfirmed naming both the send's
cause and the re-query's failure). When the policy is exhausted (or the invocation is cancelled mid-create) the
handler fails with `TerminalError{outcome_unknown}` naming the order, kind and external id; the next invocation's
lookup finds whatever landed. Correctives take no order-number hint, and a duplicate-order-number answer their
re-query cannot resolve is `rejected`.

**Storno** has the same shape: a read-only lookup of the storno external id (`lookup-storno-{number}`) and a
storno step (`storno-{number}`) under the same issue policy, query-first on every execution, on both
`Szamlazz.Order.storno_invoice` and `Szamlazz.Agent.storno`. The storno request is a pure function of the
verified original (its `telj` as `teljesitesDatum`, its `eszamla` or the account default as the e-invoice flag),
so every execution of the step sends byte-identical bytes; a verified original without a `telj` is `unavailable`
with nothing sent, raised after the answers that need no send. Neither the date nor the form is enforced by
szamlazz.hu (it issues the storno with whatever `teljesitesDatum` and `eszamla` the request carries), so the
derivation from the verified original is what keeps a reversal on its original's date and in its original's form.

A document the verify already sees reversed is `reversed` with a **best-effort** storno number:
`Szamlazz.Order.storno_invoice` from the order-number hint, `Szamlazz.Agent.storno` from the by-number storno
lookup (ours when we issued the storno, unknown after a reversal from the UI). An exhausted read reports the
reversal without the number after a `warn`, while a cancellation of the invocation is never swallowed.

## Testing

### Unit and wiremock tests

`cargo test -p restate-szamlazz` runs:

- the contract, config and identity unit tests: every request type refusing an unknown top-level and nested
  field by name while the documented bodies still deserialize; with `--features schemars`, every request schema
  closed with `additionalProperties: false` and the response schemas open;
- the discovery and binding tests of the adapters: `Body<T>` turning a malformed body (an unknown field, a wrong
  type, a missing field, invalid JSON) into the 400 `invalid_input` fault while discovering exactly as `Json<T>`;
  the storno intent built from a verified document (its `telj` as the fulfillment date, `eszamla` lifted with
  the account default as fallback, an empty `telj` as the 503 `unavailable` fault naming the invoice); the
  `Order` handlers' key parsing refusing an untrimmed key as `invalid_input` while `OrderKey::parse` still trims;
  and the sentinels that the agent key reaches neither the `credentials_rejected` warning nor the body of a
  `credentials_rejected` fault;
- the wiremock tests of the gateway against synthetic szamlazz.hu responses (`tests/gateway.rs`): the lookup
  matrix (`Absent`, `Live`, `Reversed`, `Collision`, `Foreign`, the corrective's exemption from the hint,
  `Unanswered` on a lost reply and `Api` on another code); the create step (`Issued`, `Found` on a re-executed
  step, the open codes and `Unconfirmed`, the 71/152 matrix, the corrective's 71/152 → `Rejected`); the storno
  lookup and step (`AlreadyReversed` on a re-executed step, a lost reply re-queried once, `Unconfirmed` when
  nothing landed, the body carrying `<teljesitesDatum>` and no `<keltDatum>`, two executions after a lost reply
  sending byte-identical bodies); storno validation including the proforma / delivery-note no-op, a verified
  document's `telj` (and its absence) surfaced, 335, 7; the credential codes 3/135/136/164 on every operation;
  every read fn answering a 500, an empty body or `szlahu_down` as `Err(Unanswered)` rather than data; the
  `check_account` probe as exactly one query of the sentinel id with a wrong key as data; and the taxpayer query
  answering a known prefix as `Found` with NAV's registered data, an unknown one as `Found{valid: false}`, a wrong
  key as `CredentialsRejected`, NAV's relayed `funcCode ERROR` or a szamlazz.hu code as `Api`, and a 500, an
  unparseable body or `szlahu_down` as `Err(Unanswered)`.

### Journal compatibility

The same run checks that every type the services journal as a `ctx.run` result is crate-owned and additive-only
(the rule is in the `gateway` module docs), and `tests/journal/<type>/<variant>.json` pins one fixture per
variant. The generator fails when the current code writes a different shape; the compatibility test replays every
fixture ever committed through the current types.

No `szamlazz_agent` response type is journaled: the document outcomes carry the worker's own projections
(`gateway::document::{FoundDocument, IssuedDocument}`: what the handlers read of a queried document or a create
reply, never the buyer block, the seller block, the line items or the PDF), so a change to an agent response type is
a compile error in a `From` impl, not a journal entry the next deployment cannot decode. A guard in the same module
asserts no variant of any journaled type serialises a `supplier`, `buyer`, `items`, `financial_items`, `labels` or
`pdf` key.

After an additive change, regenerate with `UPDATE_JOURNAL_FIXTURES=1 cargo test -p restate-szamlazz journal`; it
writes missing fixtures and archives a differing one beside the new shape. Review the diff as a contract change.
Never regenerate away a rename: an in-flight invocation of the previous deployment would be killed on upgrade. The
twelve `<variant>.1.json` archives of the pre-#127 document outcomes are the one deliberate break, of the pre-go-live
window (ADR 0005, #127 amendment): `DELIBERATE_BREAKS` lists them, and the compatibility test asserts each still
fails to replay. **Once the first production deployment exists, never delete an archived shape
(`<variant>.<n>.json`) of a type the code still journals, and never regenerate a fixture without its archive**: the
archive is the only record of a shape a running deployment may have journaled, and no test can tell a legitimate
deletion from an illegitimate one. A shape that must change beyond additive is a new journaled type under a new
directory; the old type is retired with its directory, archives included, in a deploy that drains first (the rule,
the retirement path and the pre-go-live exceptions are in the `service::journal` module docs).

The registry of pinned types is complete by mechanism: the `Journaled` trait is sealed and implemented through one
`journaled!` list beside it, which the registry test holds the pins to (a type journaled without pins fails by
name), and each enum's pins name its variants through `variants!`, whose list the pins check the samples against
(a variant that compiles but has no fixture fails by name).

The same module's leak guard builds every journaled type around an account whose agent key is a sentinel (the
`account` entry through the static resolver, the two `Transport` write outcomes through a gateway opened with the
sentinel credentials) and asserts the sentinel serialises into none of them.

### End to end

`cargo test -p restate-szamlazz --test e2e -- --ignored` runs `tests/e2e/`: the `Szamlazz.Order` Virtual Object
and `Szamlazz.Agent` end to end against a real Restate server (1.7.8, with the experimental `vqueues`,
`protocol_v7` and `scoped_virtual_objects` flags; `compose.yaml` sets the same three) with wiremock standing in
for szamlazz.hu, in two phases on one server.

**The single-account phase** covers, among others:

- issued → already_issued, `Idempotency-Key` replay, 152 → reconciled;
- storno → reversed (the storno mock matched on `<teljesitesDatum>` equal to the original's `telj`, no
  `<keltDatum>`) → stale create → `reissue`; a `telj`-less original answered 503 `unavailable` naming the order,
  kind and storno external id with only the verify journaled and the storno mock `expect(0)`, after a `telj`-less
  document of another order → `conflict{not_managed}`, a `telj`-less proforma → `rejected{not_stornoable}` and a
  `telj`-less reversed invoice → `reversed` with its storno number; a storno whose first reply is lost
  re-executed with a byte-identical body under one `storno-{number}` entry; `reissue` on live →
  `conflict{live}`; an external reversal;
- proforma auto-link and `consumed` in `get`; `options.proforma` on `create_prepayment` exactly as on
  `create_invoice` (`none` beside a live proforma → `conflict{proforma_live}` after the `proforma-link` read with
  nothing sent; `auto` → `issued` with `dijbekeroSzamlaszam` before `elolegszamla` on the wire; `create_final`
  and `create_proforma` refusing the option 400 `invalid_input` before any call); `options.proforma: {number}`
  checked like every found document (another order's or an order-less proforma → `conflict{not_managed}` naming
  it after the verify alone with the create mock `expect(0)`, this order's → `issued` with `dijbekeroSzamlaszam`
  on the wire whatever its `teszt` says);
- `correct_invoice` issuing a corrective under its `correction_id` with the base named on the wire and finding it
  again; `delete_proforma` deleting the order's live proforma after one send and answering `absent` once it is
  gone;
- a create with a misspelt `options.reissue` answered 400 `invalid_input` naming the field with nothing journaled
  and zero szamlazz.hu requests; a create under an untrimmed key (a `%20` before or after the order number)
  answered 400 `invalid_input` naming the rule likewise;
- an exhausted create step answering a structured `outcome_unknown` within the run policy's delays with the run's
  retries visible on `sys_invocation` while it is in flight; a cancellation (`PATCH /invocations/{id}/cancel`)
  while the create's reply is in flight answered the same `outcome_unknown` naming the SDK's 409, the completion
  releasing the order key and the next call finding the document that landed; a lookup whose reply is lost once
  retried under the read policy and completing `issued` in one invocation with exactly one create on the wire; a
  lookup that never answers as a structured `unavailable` naming the order, kind and external id with zero
  creates; `get` completing after one of its reads is retried;
- a scoped call answered `unknown_account` with zero szamlazz.hu requests; `check_account` unscoped answering the
  account with `credentials: ok` after one sentinel query (and `rejected` as data on code 3); a purged invocation
  querying szamlazz.hu again; a flaky resolver retried under the resolve policy; a failing credential store as a
  terminal `unavailable`; and a positive control for the journal-leak check (a sentinel in a szamlazz.hu
  rejection is found in the hex-decoded `raw` of the create run's result).

**The flag day** then runs as documented (private, drain, register the multi-account revision, public), and
**the multi-account phase** covers:

- the first scoped create for an order invoiced unscoped finding it under the unchanged external id; unscoped →
  `unknown_account`; the same order key under two scopes concurrently → two `issued` with each account's key on
  the create wire exactly once; the same `Idempotency-Key` under two scopes → two invocation ids and two
  documents, each replaying its own completion;
- the order-key lock, same key, same scope (#125), with the first invocation **held** at its credential fetch
  (`hold_fetch`) until the second call is on the server, so the race is the scenario's, not a clock's: two
  `create_invoice` with distinct `Idempotency-Key`s, the second accepted and queued while the first is held,
  the first then released to send into a three-second szamlazz.hu reply → `issued` + `already_issued`, one
  create, the second answered after the first with its runs ending at `lookup-invoice`; the same with the second
  call arriving between the first's two create-step executions (a `szlahu_down` first send, the second execution
  held at its fetch; two sends, both the first call's, the second one received after the second call was on the
  server); and the **same** `Idempotency-Key` sent while the first is held attaching to it: unanswered for as long
  as the hold is held, no second row on `sys_invocation`, then one invocation id and one body on both replies,
  one create;
- `check_account` under each scope → its own account with its key on the probe, unscoped → `unknown_account`; an
  order whose invocations were purged stornoed and reissued;
- `Szamlazz.Agent.storno` under a scope reversing a document whose `teszt` and `szallito/id` are not what the
  account's documents carry (compared with nothing) with that scope's key, reversing one of the account's own,
  and answering an order-bearing document `managed_by_order` with nothing sent; sending `<teljesitesDatum>`
  equal to the original's `telj` and answering a `telj`-less original 503 `unavailable` without an order
  identity, only the verify journaled and nothing sent, after `managed_by_order` and `reversed` on `telj`-less
  documents;
- `Szamlazz.Agent.query` answering the projection with `test` as reported and no `supplier_id`, and code 7 as
  `not_found`; `Szamlazz.Agent.query_taxpayer` under each scope asking NAV with that scope's key and nothing else
  on the wire, the full tax number under one scope and the bare stem under the other both journaling one
  `taxpayer-12345678` step, `valid: false` as a 200, and a malformed tax number answered 400 `invalid_input`
  naming it with nothing journaled and zero szamlazz.hu requests;
- an account change between two executions not reaching the running invocation (the journaled `Account` wins); a
  credential rotation between two executions picked up by the second with the `account` entry byte-identical;
- that no agent key of the run appears in the hex-decoded `raw` of any journal entry of any invocation, nor in
  any `completion_failure`, while the same scan finds the positive control's sentinel;
- and, last, the **run-name pin**: `RUN_NAMES` in the harness lists, per handler of both services, the ordered
  `ctx.run` names of every path it journals, and the scenario asserts over every invocation the server holds that
  its run sequence is a prefix of one of its handler's paths, that every handler seen is pinned and that every
  path was walked in full. A renamed, inserted, reordered or dropped step strands every in-flight invocation on
  replay and fails here instead.

The harness calls through `/restate/call/…` and `/restate/scope/{scope}/call/…`, reports `x-restate-id`, parses
fault bodies out of the ingress envelope (asserting on every fault that the body is
`{code: <status>, message, source: "invocation"}` under `x-restate-error-source: invocation` and that the
worker's fault is the JSON in `message`), and reads `sys_journal` / `sys_invocation` through the SQL
introspection API.

**Where the suite lives.** One integration-test binary: `tests/e2e/main.rs` holds the two tests and the order the
scenarios run in (one server start-up, the server gate decided once). `tests/e2e/harness/` is the szamlazz half of
the harness, one module per concern:

- `mod.rs`: the `Harness` composing the Restate server, the wiremock and the accounts, and the two server specs
  (the main suite's three flags; the canary's without protocol v7);
- `accounts`: the scripted and mutable resolver and store the two deployments run over, and the fetch a scenario
  holds so an account change or a key rotation lands between two executions in sequence;
- `szamlazz`: the document fixture, the selector matchers and the stub helpers;
- `ingress`: a reply with the contract's `Fault` decoded out of the envelope;
- `run_names`: the `RUN_NAMES` table.

The Restate half (the server gate and the launcher, the spawned server, the in-process deployment, `set_public`
and `drain`, the ingress reply and the envelope check, the admin API's SQL, journals, `sys_invocation` rows, kill /
cancel / purge and the in-flight sampler, the run-name matcher) is the workspace's
[`restate-e2e-harness`](../restate-e2e-harness) crate, a path dev-dependency that knows nothing of szamlazz and
never depends on this crate (a dependency back would be a dev-dependency cycle Cargo resolves by compiling this
crate twice, and every type crossing the boundary would then be two types). The harness's own tests (the fetch
hold, the stub helpers against wiremock alone) sit beside what they test and run un-ignored; the server gate's, the
sampler's and the matcher's are the crate's. Every other file is one handler family's scenarios (`create_invoice`,
`create_proforma`, `create_prepayment`, `create_final`, `correct_invoice`, `storno`, `delete_proforma`, `get`,
`policies`, `agent_reads`, `agent_writes`, `faults`, `prologue`, `multi_account`, `pins`), each scenario a
`pub(crate) async fn` taking the harness; a new scenario goes into its handler's file and is called from `main.rs`
in sequence.

**The protocol-v7 canary** (`e2e_check_account_without_protocol_v7`) runs in the same command on a server of its
own with `protocol_v7` off: the ingress accepts the scoped path and keys the invocation by the scope, but the SDK
sees none, so a scoped `check_account` answers `scope: null` with the account on the single-account deployment
and `unknown_account` on the multi-account one. This is what the deploy-time `check_account` under each scope
looks for, provoked once.

**Where the server comes from.** The harness decides once from the environment (the crate's server gate):

- `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a running server with the three flags (the main suite only;
  the canary needs a server of its own shape): `docker compose up -d` at the workspace root starts one;
- `RESTATE_SERVER_BIN` names a `restate-server` binary the harness spawns on the loopback, one process per suite
  (what CI uses). The Dagger `ci` module exports it out of the Restate image:
  `dagger call ci restate-server export --path ./restate-server`, then `RESTATE_SERVER_BIN=$PWD/restate-server`.

`RESTATE_ENDPOINT_HOST` overrides the host the server reaches the in-process endpoint at (`127.0.0.1` for a
spawned server, `host.docker.internal` for a reused one). With neither the suite skips with a message, and
**fails** when `CI` is set, so a CI run never passes by skipping.

**Ports.** A server the harness starts binds ports chosen free at launch, never fixed ones: a listener on port 0
for each of its ingress, admin and node ports (bound, read, released and passed through `RESTATE_*`), bound to the
loopback only (the admin API has no authentication). So two runs on one host
(`cargo test -p restate-szamlazz --all-features --test e2e -- --ignored` twice, concurrently), another Restate on
8080/9070, or anything else on a port collide with nothing; the wiremock and the SDK endpoint take ephemeral ports
likewise. A port chosen free and taken before the server bound it shows as the server failing to start, which the
harness reports at once with the ports it chose and the last lines of the server's log, instead of waiting out the
90 s health deadline. The start-up line names the ports of every server. A reused server's ports are whatever its
URLs say.

**Lifecycle.** A server the harness starts is stopped when the run ends, passes or fails, and when the test process
is told to stop: a SIGINT (Ctrl-C) or SIGTERM runs no `Drop`, so the harness stops every server it started itself
and exits with the signal's status (130 or 143). The spawned binary leads a process group of its own and the group
is killed. A failing test keeps the spawned server's base dir (`$TMPDIR/restate-e2e-{pid}-{main|canary}`, its log
in it) for inspection.

**What CI runs.** `dagger check` (the `Dagger` workflow on every pull request) runs the `rust` module's `build`,
`test` (default features), `clippy`, `doc`, `audit` and `fmt` checks and the workspace's own `ci` module
(`.dagger/modules/ci`): `ci:test` is `cargo test --workspace --all-features --locked` (so the `schemars` contract
tests and the `szamlazz-adatkapcsolat` archiver tests run), `ci:end-to-end` is the ignored suite above with
`restate-server` copied out of the Restate image and `CI` set, run as
`cargo test --workspace --all-features --locked --tests -- --ignored e2e_`: the same selection the container
compiled the tests with, narrowed to the ignored e2e tests by name (this suite's two and the `restate-e2e-harness`
crate's `e2e_smoke`), so the check compiles nothing (`-p` or `--test e2e` would unify dev-dependency features
differently and recompile fifty-odd crates first); when it fails, the kept `restate-server.log` of every server
follows its output. `dagger check ci:end-to-end` runs it locally the same way.

### Go-live

The [go-live checklist](../../docs/szamlazz-hu-behaviour.md#go-live-checklist) re-establishes the verified
szamlazz.hu facts on a target account before the worker is enabled; every step issues real documents there. After
a deploy, call `Szamlazz.Agent.check_account` under each configured scope, then `Szamlazz.Agent.query` a document
known to be the account's and read its `test` and seller block: that is what tells the right key under the right
scope, since the worker holds no account pin.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
