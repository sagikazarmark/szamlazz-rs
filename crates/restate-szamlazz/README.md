# restate-szamlazz

[![crates.io](https://img.shields.io/crates/v/restate-szamlazz?style=flat-square&label=crates.io)](https://crates.io/crates/restate-szamlazz)
[![docs.rs](https://img.shields.io/docsrs/restate-szamlazz?style=flat-square&label=docs.rs)](https://docs.rs/restate-szamlazz)

**Restate services issuing and managing szamlazz.hu documents with durable, idempotent execution.**

The `Szamlazz.Order` Virtual Object, keyed by the order number, serializes issuing per key: a caller says "issue
the invoice for order X" and reconciles retries through deterministic external ids. An unanswered send that
remains invisible is retained for read-only reconciliation and pause. A durable **unresolved-write marker**
guards every later mutation after cancellation or kill (see **Unresolved writes** below). szamlazz.hu is the source of truth, reached through deterministic external
ids (`{namespace}:{order}:{kind}`), so any invocation can find what an earlier one issued.

The stateless `Szamlazz.Agent` service exposes by-number operations (query, credit entries, storno of unmanaged
documents), the NAV taxpayer lookup (`query_taxpayer`) and the read-only `check_account` probe over the same
gateway module. It is **unkeyed**: its invocations run concurrently, so two by-number writes on one invoice are
not serialised by the worker the way an order's handlers are. Two replacing `set_credit_entries` (`additive: false`)
race and the last send to land wins, which under reordered webhook deliveries may be the older snapshot. The
caller serialises replacing calls per invoice on its side. Additive registration avoids replacement ordering
but an interrupted run can append the same entries again. Neither mode has Order's send-permission protection;
a caller lock cannot fence a request still processing at szamlazz.hu.

Both services are projections of the Számla Agent model: deployment constants live in configuration, line totals
are computed, domain outcomes are returned as data.

### Protection by operation

| Operation | Protection and recovery |
|---|---|
| Order mutations | Serialized per scope/order. One acknowledged send permit; interrupted or unanswered writes retain a marker that blocks later mutations. Resume reconciles read-only; authorized recovery requires exact-marker evidence. |
| Unmanaged Agent storno | Query-first execution relies on szamlazz.hu's storno idempotence. No Order marker or per-invoice lock; reconcile uncertainty before deliberately renewing. |
| Agent credit-entry registration | An interrupted open run may repeat an additive entry or an older replacement. No Order marker; settle the earlier execution and exclude delayed execution before renewal. A caller lock alone cannot fence vendor processing. |

Worker query results must carry a nonblank document number; by-number results must echo the requested number.
Malformed identity remains an unanswered read, never usable evidence for a mutation or marker clearance.
Storno recovery requires a distinct reversal number and a stornoable, reversed original.

Complete create/storno request validation precedes write arming: unsupported dates and XML-forbidden
text are `invalid_input`, without creating an unresolved-write marker. Create validates before document
reads and again after resolving references; storno validates after deriving the original's facts.

## Quick Start

Bind both services to a Restate endpoint of your own, with the two things Restate's own guidance asks of an
endpoint: the SDK's replay-aware log filter and the request identity key:

```rust
use restate_sdk::filter::ReplayAwareFilter;
use restate_sdk::prelude::{Endpoint, HttpServer};
use restate_szamlazz::account::{StaticConfig, StaticResolver};
use restate_szamlazz::{Accounts, Agent, Order, WorkerConfig};
use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;
use tracing_subscriber::{EnvFilter, Layer as _};

async fn serve(accounts: StaticConfig, worker: WorkerConfig) -> Result<(), Box<dyn std::error::Error>> {
    // `RUST_LOG` selects; `ReplayAwareFilter` drops what a replayed handler emits again
    // (events outside completed durable steps would otherwise repeat on every retry).
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(EnvFilter::from_default_env())
                .with_filter(ReplayAwareFilter),
        )
        .init();

    let worker = worker.validate()?;
    let accounts = Accounts::from(StaticResolver::try_from(accounts)?);
    let order = Order::from_parts(accounts.clone(), worker.clone());
    let agent = Agent::from_parts(accounts, worker);
    let endpoint = Endpoint::builder()
        .bind(order)
        .bind(agent)
        // Refuse requests the Restate runtime did not sign; required in the multi-account shape
        // (see *Securing the Worker*). The server logs this public key at start-up.
        .identity_key("publickeyv1_w7YHemBctH5Ck2nQRQ47iBBqhNHy4FV7t2Usbye2A6f")?
        .build();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:9080").await?;
    // Install SIGTERM handling before serving (SDK 0.12's default handles only Ctrl-C).
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let shutdown = async move {
        #[cfg(unix)]
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = terminate.recv() => {},
        }
        #[cfg(not(unix))]
        let _ = tokio::signal::ctrl_c().await;
    };
    HttpServer::new(endpoint).serve_with_cancel(listener, shutdown).await;
    Ok(())
}
```

This simple hosting example assumes the process/runtime terminates after serving returns. SDK 0.12's
shutdown stops accepting and waits up to ten seconds for connections, but does not abort and join remaining
connection/handler tasks. Return is not a task-completion barrier in a larger application that keeps its
runtime alive. An embedded host requiring component-level shutdown must own its serving tasks, drain them,
then cancel and join remaining tasks before tearing down dependencies. External effects still need reconciliation.

Register the endpoint and call a handler through the ingress (Restate's URL grammar: `/{service}/{key}/{handler}`
for a Virtual Object, `/{service}/{handler}` for a service, `/restate/scope/{scope}/call/…` under a scope):

```sh
restate deployments register http://worker:9080

# Single-account shape, unscoped; the key is the order number, already trimmed.
curl -X POST http://localhost:8080/Szamlazz.Order/ORD-1/create_invoice \
  -H 'Idempotency-Key: 4c8f5a1e-…' -H 'content-type: application/json' \
  -d '{"document": {"buyer": {"name": "Kovács Bt.", "zip": "2030", "city": "Érd", "address": "Tárnoki út 23."},
                    "items": [{"name": "Jegy", "quantity": "1", "unit": "db", "unit_price": "1000", "vat_rate": "27"}],
                    "fulfillment_date": "2026-09-03", "due_date": "2026-09-11", "payment_method": "transfer"}}'

# Multi-account shape: the account's scope on every call.
curl -X POST http://localhost:8080/restate/scope/acme/call/Szamlazz.Order/ORD-1/create_invoice …
curl -X POST http://localhost:8080/restate/scope/acme/call/Szamlazz.Agent/check_account
```

For correlated replay-aware logs, keep the SDK's INFO endpoint span and the worker's
INFO execution span enabled, for example `RUST_LOG=info,restate_szamlazz=debug`.
SDK 0.12.0 can leave its ancestor span marked as replaying when fresh work begins
under an application child span. This crate mitigates that at its actual run-closure
boundary, preserving duplicate suppression and fresh Gateway/credential-warning
events with scope, account and invocation correlation. The mitigation covers these
worker services; another service bound to the same endpoint needs its own handling
until an SDK fix is released. See the [reproduction, candidate upstream patch and
removal condition](../../docs/research/2026-09-10-replay-logging.md).

Amounts (`quantity`, `unit_price`, every total) are decimals serialised as JSON **strings**; a number is accepted on
input without binary-float conversion. Quantities, prices, credit-entry amounts and exchange rates must fit a
Decimal exactly; unrepresentable input is `invalid_input` before the prologue, never implicitly rounded. Use strings
when your caller's JSON tooling would otherwise round the number. Currency rounding happens only during calculation.
Discovery accepts signed exponent notation and the same finite decimal string grammar as the decoder;
exact representability is additionally checked at runtime. Handler JSON rejects object-valued amounts,
including lookalikes of serde_json's private arbitrary-precision representation, before durable work.
Optional response fields, a fault's included, are present as `null` when absent.

Both configuration types only implement `Deserialize`; the host chooses the file format and environment merging
(a TOML file layered with environment overrides through figment, for instance).

- `WorkerConfig` is the deployment-level part: the `namespace` of the external ids and the three run retry
  policies: `[issue]` for unmanaged Agent storno, `[read]` for ordinary reads and operator document
  verification, and `[resolve]` for account resolution. Protected Order reconciliation uses the
  handler invocation policy below. Call `validate()` after parsing.
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
execution. Going from `[account]` to `[accounts.<scope>]` is a flag day: settle external uncertainty and recover
every unresolved marker under its original scope first. Only then is no data migration needed, as scripted in
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
  process crashes and concurrent callers. Same-key handlers run one at a time. After validation and the prologue,
  a target **ownership lookup** settles an existing document before prerequisites for a new send. An absent
  target, or an explicit reissue, proceeds through prerequisites, the full **lookup**, then a **create** step whose every
  execution queries szamlazz.hu by the document's external id *inside the same `ctx.run` closure* before it
  creates. A durable marker and acknowledged one-use permit prevent another send after interruption;
  an invisible result remains unresolved until read-only reconciliation or authorized recovery settles it.
- **Correctives** are issued under a caller-supplied `correction_id`: the same id finds the corrective it
  issued, a new id issues a new one.
- **Storno** (`storno_invoice`) and **proforma deletion** are idempotent. A document reversed by anyone (the UI,
  support, this service) is reported as `reversed` from `<sztornozott>`. The storno carries the original's
  fulfillment date (`teljesitesDatum` = the verified document's `telj`), which NAV requires it to repeat; the
  caller cannot set it.
- **Domain outcomes are data** (HTTP 200) and errors are reserved for faults, so a caller can always tell "the
  document exists" from "the outcome is unknown".
- **`get`** is a non-atomic observation of the order's four external ids. Its separately journaled reads
  can mix observation times and replay ages, and run alongside exclusive writes. See *Polling and invocation completion*.

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
check: use [`examples/verify_seller.rs`](examples/verify_seller.rs) with the deployed `Accounts` resolver and
credential store to compare a known document's `test` and seller block with independent expectations.

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

**Credentials are fetched lazily inside the first external-operation run that executes**, and the fresh
Gateway/client is reused only within that handler execution. Completed runs replay without consulting the store.
The journaled `Account` supplies defaults for deterministic decisions even when no client is opened. Using
credentials inside a run does not persist them: only its result or failure is journaled, and both stay secret-free
(the e2e suite scans every journal entry).

A failed fetch is **terminal** `unavailable` after three bounded attempts, 200 ms apart (`gone` fails immediately).
A Gateway-open failure is the same structured fault. Fetched credentials are checked for XML representability
before opening the Gateway; malformed credentials likewise produce sanitized `unavailable`, preserving
uncertainty about any earlier execution of an interrupted write. They never become caller `invalid_input`.
The failure is recorded on the executing operation's run,
so it cannot replace a recorded run command with a terminal output. It bypasses the operation's read/issue policy;
best-effort storno-number reads still report the known reversal without its number. An unfinished write may have
sent during an earlier execution: `unavailable` preserves that uncertainty. Settle earlier external work before
deliberately renewing a write. Missing credit entries, empty document queries and elapsed time do not establish
non-execution. After settlement, credit-entry callers query again and submit only still-required additive entries
or the current intended replacement. Neither credentials nor sensitive initialization source messages reach the journal or caller.

**Rotation depends on the store.** A dynamic store's rotated value is picked up when the next handler execution
needs an external operation, under the same journaled Account and Credential ref. `StaticResolver` clones startup
credentials: retained immutable deployments keep their original keys unless their store/configuration is updated
operationally. Registering a new deployment alone does not rotate credentials for invocations pinned to an older one.

### The safety contract

The static resolver enforces what can be checked at load time; a resolver of your own, and the operator,
guarantee the rest.

1. One szamlazz.hu account is reachable under exactly one scope value; unscoped counts as a value; no fan-in
   (two scopes reaching one account would split an order's per-key lock across two Virtual Objects). The static
   resolver's single `[account]` is served unscoped and knows no scope; its `[accounts.<scope>]` shape is served
   by scope only and is checked at load time: unique `(endpoint, agent_key)` pairs, the endpoint compared
   normalised (`Endpoint::normalized`), and unique ids. Endpoint validation and comparison use the bundled
   transport's URL parser, including default ports, dot segments (also encoded dots) and equivalent host
   spellings. Comparison additionally drops fragments and deliberately folds trailing slashes; original text
   stays in display, serialized Accounts and client configuration. This detects equivalent URL spellings,
   not arbitrary DNS aliases, redirects, proxies or different keys opening one account. Preventing those
   forms of fan-in remains the resolver's and operator's responsibility. Invalid URL syntax, including invalid
   ports, fails configuration loading; endpoints require HTTP(S), a host and no userinfo.
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
reversal returns `reversed` and issues a replacement only with an explicit
`reissue: {"expected_number": "SZ-A"}` and a new `Idempotency-Key`. Correctives have no reissue option.

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
`CreateOptions` (`reissue`, `proforma: auto | none | {number}`); the response carries the `CreateOutcome`, the identity
(`kind`, `external_id`), the numbers and totals, and `warnings`. `customer_account_url` is set only on the
execution that actually issued (a fresh `issued`), never on `already_issued`, `reconciled` or `get`.
`CorrectRequest` (`invoice_number`, `correction_id`, `document`) is the input of `correct_invoice` and shares the
response. Its base must be this order's live ordinary, prepayment or final invoice. Further corrections name
that original base, not a previous corrective. Other types are `invalid_input` before a write is armed;
an already-issued target is still returned before checking new prerequisites.

Every request type, and every object it nests, is closed (`#[serde(deny_unknown_fields)]`,
`additionalProperties: false` in the schema): a field the contract does not know is refused as `invalid_input`
naming the field, never silently dropped. Response objects tolerate added fields; response tokens and states
preserve unknown values as described below. The `conflict_reason` table is below; the
`schemars` feature puts the full request and response schemas into the discovery manifest.

`service::Body<T>` is how every handler takes its input: a `Json<T>` whose decode runs in the handler, so a
malformed body is the structured `invalid_input` fault instead of the SDK's plain-text 400. Same discovery schema
as `Json<T>`; built with `Body::new` / `From<T>` for calls through the generated clients.

`contract::CreateOutcome` / `ConflictReason`: `issued`, `already_issued`, `reconciled`, `reversed`, `rejected` or
`conflict` with a reason. Both carry `KNOWN` and `as_str` and preserve an unknown string verbatim in
`Other(String)`, as do `StornoOutcome`, `Warning` and `TerminalCode`. `#[non_exhaustive]` requires a default arm
in downstream Rust matches; the serde handling of `Other` is what makes the wire open. An unknown warning
does not undo a known success; an unknown outcome stays unclassified, never assumed successful or safe to retry. The
reasons:

| Reason | When |
|---|---|
| `prepaid_chain` | A plain invoice while the order's own prepayment invoice or final invoice is live, or a prepayment invoice while the order's own invoice or final invoice is. The final invoice keeps the chain closed after its prepayment is reversed. |
| `order_invoiced` | A proforma after the order's own live invoice, prepayment invoice or final invoice. |
| `live` | The expected reissue document is still live. |
| `target_changed` | The expected reissue document is absent or a different owned holder is newest (`existing_number` when known). No send; never automatically substitute that number. |
| `foreign` | A live invoice under the order number that is under none of the order's external ids (another channel's). |
| `duplicate_order_number` | szamlazz.hu refused the protected create's sole permitted send (71/152). An optional diagnostic query may supply `existing_number`; its result does not replace the original refusal. |
| `external_id_collision` | The external id's holder does not carry this order's number and kind. |
| `proforma_live`, `proforma_missing` | `options.proforma` and the order's proforma disagree. |
| `prepayment_missing`, `prepayment_reversed` | `create_final` without a live prepayment invoice. |
| `base_reversed` | `correct_invoice` on a reversed base. |
| `not_managed` | A document named by number does not carry this order's number. |

`contract::TerminalCode` is the set of fault codes a `TerminalError` carries, each with its HTTP status
(`TerminalCode::KNOWN` lists the known codes in the order of the fault table below). `TerminalCode::status`
and `Fault::status` return `Option<u16>`; `TerminalCode::is_outcome_unknown` returns `Option<bool>`:
`Some(true)` for the three outcome-unknown codes, `Some(false)` for the other known codes, `None` for an unknown
token. Unknown codes acquire neither an inferred status nor retry advice; read the actual ingress status.
The known codes are:

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
`CredentialsCheck::Other { state, fields }` preserves an unknown state and all its payload fields without
inferring credential acceptance; `KNOWN` lists `ok` and `rejected`.

`contract::QueryTaxpayerRequest` / `QueryTaxpayerResponse` (`TaxpayerAddress`) is the contract of
`Szamlazz.Agent.query_taxpayer`. In: `tax_number`, the bare eight-digit stem (`12345678`) or the full
`NNNNNNNN-N-NN` form (`12345678-2-42`) and nothing else (`QueryTaxpayerRequest::prefix` derives the prefix or
names what was wrong). Out: `{valid, name?, tax_number?, vat_code?, addresses[]}`, a crate-owned
projection of the agent crate's `TaxpayerInfo` (what the read step journals), with `valid: false` a normal
answer. Not cached by the worker; cache it in the caller with a TTL on the order of a day.

`contract::StornoRequest` / `StornoResponse` (`StornoOutcome`: `reversed`, `rejected`, `conflict`,
`managed_by_order`, `unsupported_order_number`), `DeleteProformaRequest` / `DeleteProformaResponse`, `QueryRequest` (`Selector`) /
`QueryResponse` and `SetCreditEntriesRequest` / `SetCreditEntriesResponse` are the remaining handler contracts.
`Szamlazz.Agent.storno` returns `managed_by_order` only when the reported order number is a supported `OrderKey`.
Otherwise it returns `UnsupportedOrderNumber` (`unsupported_order_number`), preserves the reported string in
`order_key`, and explains the failed rule in `message`. Nothing was sent: reverse in szamlazz.hu and reconcile
in the caller's system, without normalising the number or bypassing the order guard.

`contract::OrderStatus` / `DocumentStatus` is the live view `get` returns: one optional `DocumentStatus` per kind
(`number`, `state`, `gross`, `net`, `credit_entries`, `referenced_proforma`, `e_invoice`) with `DocumentState`
flattened as `{state: live}`, `{state: reversed, storno_number}` or, for a consumed proforma,
`{state: consumed, by}`. `get` never fills `storno_number` (finding the storno would take the order-number hint,
which shows only the newest document); the create and storno handlers report it. A `null` slot is *nothing of
ours* under that external id, which may still be a foreign holder (a create there answers
`conflict{external_id_collision}`). Correctives are not in the view.
`DocumentState::Other { state, fields }` preserves an unknown state and its payload fields;
`DocumentState::KNOWN` lists `live`, `reversed` and `consumed`. Unknown states stay unclassified.

### Identity

`CorrectionId` is the caller-supplied identity of one corrective invoice: `^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$`
and not one of the external-id tokens (`ExternalId::TOKENS`, in any letter case); embedded in the corrective's
external id.

`contract::InvoiceNumber` is an invoice number as the by-number requests take it (`StornoRequest`,
`CorrectRequest`, `SetCreditEntriesRequest`, `Selector::InvoiceNumber`, `ProformaLink::Number`): 1–40 bytes, no
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
  duration is the sole exhaustion threshold when unset), `initial_delay`, `factor`, `max_delay`, `max_duration`.
  The issue policy runs unmanaged `Szamlazz.Agent.storno`, with default thresholds of `5` executions / `1h`
  and delays `2m` → `10m`; the read policy runs ordinary reads and operator document verification,
  `5` / `5m`, `5s` → `60s`; the resolve policy runs the `account` step, with no attempt cap, a `1m`
  duration threshold and `1s` → `10s` delays.

Protected Order writes consume one acknowledged send permit. Their read-only `reconcile-write` run uses
the **Order mutation invocation policy**, not `[issue]` or `[read]`: by default five executions with
`2m` → `10m` doubling delays, then pause. Resume never grants another send. The host can override each
handler's invocation policy through SDK `ServiceOptions` / `HandlerOptions`; retain pause-on-exhaustion
and inspect effective discovery settings after registration. See the
[effective retry controls](../../docs/operations/order-recovery.md#effective-retry-controls).

**Run limits are exhaustion thresholds, not deadlines or external-send caps.**
[Rust SDK 0.12.0](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/struct.RunRetryPolicy.html)
allows both actual execution count and duration to exceed the configured values. Shared core 7.0.3 evaluates
limits after a closure fails; `max_duration` does not interrupt a hung closure. A crash after an external effect
but before its result is journaled can re-execute the closure, even with `max_attempts(1)`. Unkeyed Agent
credit entries disable policy-driven retries but may repeat across crashes. Protected Order writes additionally
require the execution-local permit, which completed-arm replay cannot grant. Keep the
per-call deadlines and execution timeouts described under *Retry policy*.

Durations are written the way Restate's own handler attributes write them (jiff's friendly format: `"90s"`, `"2m"`,
`"1h 30m"`, `"3d"`, `"500ms"`; months and years are refused, having no fixed length) or as a bare integer of seconds,
so a value copied from a `#[handler(...)]` attribute parses. The field names are the SDK's `RunRetryPolicy`'s
(`initial_delay`, `max_delay`, `max_attempts`, `max_duration`; `factor` for its `exponentiation_factor`), not the
handler attribute's `initial_interval` / `max_interval`: those are the *invocation* retry policy, pinned in code; these
are *run* retry policies.

Each policy's `run_retry_policy()` is the `RunRetryPolicy` its steps run under. `validate()` checks the
cross-field invariants (`max_attempts ≥ 1` where set, `initial_delay ≤ max_delay`, a finite `factor ≥ 1`) and one floor:
`issue.initial_delay ≥ IssueConfig::MIN_INITIAL_DELAY`, the Számla Agent client's exported `REQUEST_TIMEOUT`
(60 s) plus a 30 s margin (90 s), because unmanaged Agent storno re-executed sooner would re-check while its send
may still be in flight; the error names the rule. It yields the `ValidatedWorkerConfig` that `Order::from_parts`
and `Agent::from_parts` take, so a deployment cannot run on a policy below the floor (the `test-util` feature's
`ValidatedWorkerConfig::unchecked` is for test harnesses whose szamlazz.hu is a mock).

Nothing account-shaped is in `WorkerConfig`: document defaults and the seller block belong to the `Account`, and
their value types (`account::Defaults`, `account::SellerConfig`, `account::SellerEmailConfig`) are journaled with it
(a *journaled type*'s parts, under no compatibility rule: ADR 0009) and closed to unknown keys themselves, so the
static resolver reads its `[account.defaults]` and `[account.seller]` tables as them directly (`StaticAccount` adds
the `Secret` agent key, whose `Debug` output is redacted).

### Accounts

`account::Account` is one szamlazz.hu account as the worker knows it (never its key). `Accounts` bundles the two
pluggable traits both services hold, `AccountResolver` and `CredentialStore`; `StaticResolver` / `StaticConfig`
is the configuration-backed implementation of both.

The traits are object-safe (`BoxFuture`), require no `Debug`, and carry the checklist a resolver of your own
guarantees: no fan-in, append-only, unique `(endpoint, credentials)` pairs, the right key under the right scope
(the worker holds no account pin: verified at go-live with the actual deployed `Accounts` bundle by
[`verify_seller`](examples/verify_seller.rs), never inferred), a stable `credential_ref` across rotations, never caching `Unscoped` /
`Unknown`. `Accounts`' `Debug` (and so `Order`'s and `Agent`'s) names the trait objects without descending into
them, so a store that derives `Debug` over a key map cannot print its keys through the services.

`StaticConfig` is either `[account]` (`id`, `agent_key`, `endpoint`, `defaults`, `seller`; reachable unscoped) or
a table of `[accounts.<scope>]` (the same fields; each reachable under its scope only, keys `[a-z0-9_]` of at
most 36 bytes so environment overrides can address them), never both. `StaticResolver::try_from` validates it,
and `Accounts::from` bundles it as resolver and store.

### Gateway and services

`gateway::Gateway` is the module that speaks to szamlazz.hu on behalf of one account, over
`szamlazz_agent::Client`: one plain async fn per `ctx.run` (`lookup`, `lookup_ours`, `create`, `verify`, `query`,
`hint`, `lookup_storno`, `storno`, `delete_proforma`, `set_credit_entries`, `query_taxpayer`, `probe`), each returning every
expected szamlazz.hu outcome as data. Two `Err`s say what a run retry policy may re-execute:

- the read fns (`lookup`, `lookup_ours`, `verify`, `query`, `hint`, `lookup_storno`, `query_taxpayer`, `probe`) return
  `Err(Unanswered)` when szamlazz.hu did not answer (a transport or parse failure, `szlahu_down`);
- `create` and `storno` return `Err(Unconfirmed)` for an outcome that is *not* known. An answer to their leading
  query (another code, `szlahu_down`) is data: nothing was sent. An unnumbered unmanaged storno acknowledgement
  is the exception: inconclusive reconciliation returns journaled `StornoOutcome::Unnumbered` data, so this
  reply ends the run rather than entering mutation retry.

It is not a second client: the Számla Agent `Client` is the transport it wraps. Every read of account
configuration by the services uses the journaled `Account` directly; a gateway is opened lazily inside the first
executing operation run (`Gateway::open`) and never outlives that execution. The supplied `Order` and `Agent`
services always use that default transport; they expose no transport factory or injected-client option.
`Gateway::open_with_http` is available to **direct Gateway consumers** for a caller-built `reqwest::Client`
(re-exported as `szamlazz_agent::reqwest`), including custom TLS or proxy configuration. It does not configure
the supplied Restate services. Direct consumers must preserve a fresh cookie jar per execution, the request
deadline, and disabled retries and redirects. Two clients sharing a cookie provider still share a session.
The unit and wiremock tests use this hook to avoid loading root certificates for plain-HTTP mocks.
No Restate service calls another.

#### Diagnostic privacy

Gateway exchange failures cross one allowlisted diagnostic projection before they reach a run failure,
journaled outcome, caller fault or conversion-site warning. It retains the operation (`query`, `query-taxpayer`,
`create`, `storno`, `delete-proforma`, `set-credit-entries`), HTTP status where the client exposes it, and static
parse/transport categories. `szlahu_down` remains distinct, but its arbitrary header text is discarded. Response
excerpts, offending XML values, transport URLs and source chains are never copied or formatted there. Nested
send/re-query failures retain both categories; order, kind, external id and document context remain supplied by
the operation and handler. These diagnostics do not change uncertainty, cancellation or retry semantics.

**Vendor-answer exception:** non-credential `ApiError` codes and messages remain business data under the existing
contract. This includes query and taxpayer faults (`szamlazz_code` and message, NAV tokens included), create/storno
rejections, duplicate-order answers, open-code uncertainty and its re-query combinations, and one-shot
rejections/inconclusive answers. These messages can name order numbers, line items or other document content;
the worker does not claim they are redacted. A not-found answer or an accepted probe discards its message.
Credential codes **3/135/136/164** are deliberately different: every Gateway answer uses a static description
instead of the upstream credential message, including `check_account` and post-send verification/re-query
failures. Local request refusals retain the wire contract's own diagnostic, not an upstream response.

The Számla Agent client's richer diagnostic API remains available to direct consumers. Initialization keeps its
separate credential-hygiene boundary (#200). This change does not scrub already retained journals or introduce
a cross-release replay contract: use immutable deployments and review any exceptional replay under ADR 0009.
`tests/gateway/privacy.rs` exercises hostile diagnostics and conversion-site logs; `tests/e2e/privacy.rs` checks
real run failures, retained run results, completion failures and ingress faults, with positive category controls.

`Order` / `Agent` are the Restate Virtual Object registered as `Szamlazz.Order` and the stateless service
registered as `Szamlazz.Agent`, with generated `OrderClient` and `AgentClient` for typed calls from other
handlers. Both are built `from_parts(Accounts, ValidatedWorkerConfig)`. Every handler decodes its body (`Body<T>`; a
malformed one is `invalid_input` before anything is journaled) and runs the prologue (pin the namespace, resolve
the account in the `account` step) before its operation; credential fetch and gateway open occur inside the first executing operation run.

## Identity Model

Three identities work together.

**The order key** decides which `Order` instance runs; same-key handlers run one at a time, which is what
serializes issuing per order. Its only state is unresolved-write uncertainty. The key is the order number **trimmed by the caller**:
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

The id is queried first by the target ownership lookup, again by the full lookup after prerequisites, and by
every execution of the create step, inside the create's own
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

The SDK starts without keys and warns about nothing, so registering one is the host's responsibility, and it is
**required** in the multi-account shape: the scope that selects the account is protocol data inside the request the
runtime makes, so an endpoint accepting unsigned requests lets any client that reaches its port invoke either service
under any scope, on every account the deployment serves; the gateway in front of the ingress (Caller Contract, rule 6)
does not cover this port. The public key is what the server logs at start-up when it holds the private half
(`RESTATE_REQUEST_IDENTITY_PRIVATE_KEY_PEM_FILE`); it is not a secret.

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
     `credentials_rejected`** fault from an issuing or storno handler means "outcome unknown: reconcile first",
     never "no document exists". Read `Szamlazz.Order.get` or query the relevant external id/number; an empty
     query does not prove the earlier send cannot still land. Use a **new** key only for a deliberately renewed
     operation after the uncertainty is settled (see **Unresolved writes** below). Check cancellation first: `cancelled` (409) or `cause: "cancelled"`
     does **not** authorize automatic retry. For a cancelled write, reconcile through `get` or a
     by-number query, then deliberately renew the operation with a new key only if still intended.
    - **No answer** (your client timed out, or the ingress answered with a 5xx whose source is *not*
      `invocation`) does not establish completion. **Keep the key** and retry with it to attach to the
      invocation or retrieve its retained outcome; a new key can start a second invocation queued behind it.
      With the default policy, Order mutations **pause after five attempts**, retaining their invocation and
      exclusive lock. The roughly 24 minutes of configured delays is not a completion deadline. A paused
      owner needs operator attention; resume it for read-only reconciliation. `get` remains an observation.
    - **A killed invocation** (manual kill, or exhaustion of a handler configured to kill, such as Agent
      storno) can return native error **text** in the envelope `message`, not the worker's `{code, message}`
      JSON. Preserve that native/raw error without inventing a fault code. Kill releases the lock but does
      not settle external effects or clear an Order marker. Order mutation exhaustion pauses rather than kills.
   - **The other known faults are settled**, nothing landed: `invalid_input`, `unknown_account` and `not_found` are
      raised before anything is sent, and `szamlazz_error` is szamlazz.hu answering a read with an error.
      A vendor credit-entry refusal is `outcome_unknown`: it cannot settle an earlier execution of the open run.
      Retrying as is repeats the answer: fix the request, the number,
     the scope or the account, or, for a `szamlazz_error` relaying a NAV outage, retry later with a new key.
3. After a storno (by this service, the UI or anyone) a create returns `outcome: reversed`. Send
   `options.reissue: {"expected_number": "SZ-A"}` (with a new key) when replacing that reversed invoice is
   actually wanted. Obtain `SZ-A` from the reversed response's `invoice_number` or a fresh `get`, and retain it
   with the logical command across retries. The same holder live is `conflict{live}`; another holder or absence
   is `conflict{target_changed}`. A changed number needs a new business decision.
4. A `credentials_rejected` fault (503) means szamlazz.hu refused the worker's agent key (codes 3, 135, 136,
   164) on some step: the deployment is misconfigured, not the request. The request that drew the code was not
   acted on, but the code may have come to a re-query after a send, and an earlier execution may have landed
    with a lost reply, so rule 2 applies: once the key is fixed, reconcile before deliberately renewing. The worker
    logs credential faults and automatic reconciliation's credential answers at `warn` with the namespace and the code, inside the execution's span
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

### Polling and invocation completion

Use `get` to ask **what szamlazz.hu reports**, with a fresh invocation for each new observation. For example,
these are two independent polls (no `Idempotency-Key` header):

```sh
curl http://localhost:8080/restate/call/Szamlazz.Order/ORD-1/get
sleep 5
curl http://localhost:8080/restate/call/Szamlazz.Order/ORD-1/get
```

For scoped calls use `/restate/scope/<scope>/call/Szamlazz.Order/ORD-1/get` on each poll. If your client supplies
keys, use a fresh key per poll; keep that poll's key only when retrying its unanswered request. Reusing a retained
key after completion can replay the old answer. `get` leaves idempotency retention unspecified in discovery;
that means the server's effective setting applies, **not** that deduplication is disabled.

Each poll performs four sequential, separately journaled reads. They can observe different moments; after
replay, earlier reads may be old while later ones execute afresh. Shared `get` runs alongside exclusive writes,
so even a fresh poll is neither an atomic snapshot nor a completion barrier. An absent invoice does not prove
that an in-flight create will never land.

Use native **attach/output** to ask **whether one particular invocation completed**. Record `x-restate-id` from
the call or `invocationId` from `/restate/send/…`, and use the authorized ingress:

```sh
# INVOCATION_ID is the id returned for the original call/send.
curl "http://localhost:8080/restate/attach/$INVOCATION_ID"  # wait for completion
curl "http://localhost:8080/restate/output/$INVOCATION_ID"  # peek without waiting
```

These retrieve that invocation's retained result, including a fault, rather than start another create. Output
can be not ready or not found (including after retention); neither is evidence of external absence. See
[Restate's invocation access guidance](https://docs.restate.dev/services/invocation/http#attach-to-an-invocation).
Completion itself does not settle an `outcome_unknown` write: use the external observations to reconcile it.

### Faults

Faults are `TerminalError`s whose message is the JSON `{ "code", "message", "cause"?, "szamlazz_code"?, "order"?, "kind"?,
"external_id"? }`. **On the wire that JSON is a string inside Restate's ingress envelope**: the body is
`{"code": <HTTP status>, "message": "<the fault JSON>", "source": "invocation"}` under
`x-restate-error-source: invocation`, so a caller parses `message` a second time; the envelope's own `code` is
the status below, never the token (the Rust SDK carries a terminal error as code plus message and offers no other
channel).

For SDK-generated `AgentIngressClient` / `OrderIngressClient` callers,
`service::decode_fault(&ClientError) -> Option<Fault>` checks the actual HTTP error status and
`x-restate-error-source`, decodes the envelope and attempts the inner `Fault`. Its rustdoc contains a
compiling generated-client example. `None` leaves the original SDK error intact, including native
cancellation/kill text, transport failures, non-invocation errors and unfamiliar envelope shapes.
Unknown fault codes and causes decode but remain unclassified; read the actual response status.
**Discovery limitation:** deriving `JsonSchema` for `Fault` does not publish its shape in a handler's
success-output discovery schema. Generated clients still need this error-decoding step.

**Cancellation (ADR 0011):** ordinary reads, account resolution and best-effort reads return `cancelled`
(409). Cancelled create/storno/deletion/credit-entry writes return `outcome_unknown` (500) with
`cause: "cancelled"`. `Fault::is_cancelled() -> Option<bool>` identifies cancellation independently of
`TerminalCode::is_outcome_unknown()`; unknown codes or causes return `None`. An absent cause makes no
machine-readable cancellation claim about an older stored completion. Cancellation is not rollback:
do not automatically retry or reissue. Reconcile a cancelled write before deliberately renewing it.
This changes cancellation behavior in the next breaking release; stored completions retain their old shape.

This worker emits the known tokens of `contract::TerminalCode`; a client preserves a newer token as `Other`.
A szamlazz.hu code never travels in
it, but in `szamlazz_code` beside it, present on every fault a szamlazz.hu answer caused: the `szamlazz_error`
pass-through, `credentials_rejected`, and `unavailable` on a code a read cannot conclude from.

Converting a `Fault` to the SDK's `TerminalError` uses `TryFrom<Fault>` and returns the typed
`service::FaultConversionError`: `UnknownStatus` for an unknown code, `Serialization` if JSON encoding fails.
It never invents a terminal status. Conversion to `HandlerError` keeps known faults terminal and surfaces a
conversion failure as an internal retryable error, not as a classification of the decoded fault.

A malformed body is the same shape: every handler decodes its own body (`service::Body<T>`, not the SDK's
`Json<T>`), so an unknown field, a wrong type, a missing required field or invalid JSON is
`{ "code": "invalid_input", "message": "malformed request body: …" }` with serde's message, naming the field
when there is one, never the SDK's plain-text `Cannot decode input payload`.

| Code | HTTP | Meaning | What to do |
|---|---|---|---|
| `cancelled` | 409 | Intentional cancellation during an ordinary read, account resolution or best-effort read. No write was sent. | Respect the stop; do not automatically retry. |
| `forbidden` | 403 | The host denied operator recovery access. | Use the authenticated operator recovery boundary. |
| `invalid_input` | 400 | The request is malformed: its body carries a field the contract does not know (every request type is closed: ``unknown field `resissue`, expected `reissue` or `proforma` ``), a wrong type, a missing required field, an `invoice_number` or `correction_id` outside its bound (40 bytes; no whitespace or `:`; not an external-id token), or its `Order` key has leading or trailing whitespace or is outside the key alphabet (1–40 bytes, no internal whitespace, no `:`, NFC); refused before anything is journaled or sent. Or it carries a value the operation cannot take: an option the handler does not take, a `{number}` proforma link that is not a proforma, a sixth credit entry on `set_credit_entries`, a replacing `set_credit_entries` (`additive: false`) with no entries (the wire contract takes five, and an empty replace would clear the invoice's credit entries; nothing is sent), or a line item whose arithmetic overflows a decimal (after the prologue's two journal entries, before any read; nothing is sent). | Fix the request. |
| `unknown_account` | 400 | The request names no account of this deployment (rule 5). | Fix the scope; do not retry as is. |
| `not_found` | 404 | The document the request names by number is not known to szamlazz.hu (code 7): `Szamlazz.Agent.query`'s selector, the invoice of `Szamlazz.Agent.storno` / `Szamlazz.Order.storno_invoice`, the base of `correct_invoice`. Nothing was sent. (A missing proforma named by `options.proforma: {number}` is `conflict{proforma_missing}`, an outcome.) | Fix the number; do not retry as is. |
| `szamlazz_error` | 422 | szamlazz.hu answered a read with a code the handler passes through: `Szamlazz.Agent.query` on a code that is neither 7 nor a credential code, or `query_taxpayer` on any `funcCode ≠ OK` (szamlazz.hu's own or NAV's relayed one; `valid: false` is a 200). `szamlazz_code` carries the code, `message` szamlazz.hu's text. | Read `szamlazz_code`; a NAV outage on `query_taxpayer` is retried with a new `Idempotency-Key`. |
| `outcome_unknown` | 500 | Write uncertainty, including cancellation or a later mutation blocked by an unresolved marker. Unmanaged Agent storno can exhaust its issue policy; protected Order writes retain read-only reconciliation and pause instead. | Settle earlier work first. Missing credit entries, empty queries, elapsed time and kill do not authorize renewal. After settlement, deliberately renew with the original expected-document intent; for credit entries, query again and submit only still-required additive entries or the current intended replacement. |
| `unavailable` | 503 | szamlazz.hu did not answer a read-only step through every execution of the read policy (the message names the step and the last failure; the order, kind and external id when the step knows them), or answered it with a code nothing can be concluded from (`szamlazz_code` carries it), or returned a storno's original without a fulfillment date (`telj`), the date the storno must repeat, so it is not sent; or the account resolver or credential store could not answer (reporting so, or silent past the worker's ten-second bound on the call). Nothing was sent by the execution that raised it. | Rule 2, later. |
| `credentials_rejected` | 503 | szamlazz.hu refused the worker's agent key (rule 4; `szamlazz_code` carries the code). | Page the operator; then rule 2. |

A 5xx whose `x-restate-error-source` is `invocation` is an invocation failure, including native kill errors,
not the Restate ingress being down. Restate's HTTP invocation docs say to treat `invocation` errors as non-retryable and to auto-retry a 5xx
only when its source is `ingress` (or absent); do that here: page on an `invocation` 503 instead of retrying into
it (`credentials_rejected` in particular repeats identically until the deployment is fixed). Reconcile any
earlier write before deliberately renewing; a fresh `get` remains an observation, not negative settlement.

### Retry policy

Every handler that calls szamlazz.hu pins its own invocation retry policy.

| Handler | Attempts | Interval | Timeouts (inactivity / abort) | Journal retention |
|---|---|---|---|---|
| `Szamlazz.Order` writes (`create_*`, `correct_invoice`, `storno_invoice`, `delete_proforma`) | 5, pause | 2m → 10m, factor 2 | 4m / 3m | 3d (idempotency 30d) |
| `Szamlazz.Order.get` | 3, kill | 10s → 1m, factor 2 | 2m / 2m | 1d |
| `Szamlazz.Order.recover` | 3, pause | 10s → 1m, factor 2 | 4m / 3m | 30d |
| `Szamlazz.Agent.storno` | 5, kill | 2m → 10m | 4m / 3m | 3d |
| `Szamlazz.Agent.set_credit_entries` | 2 | 2m | 2m / 2m | 3d |
| `Szamlazz.Agent.query`, `query_taxpayer`, `check_account` | 3, kill | 10s → 1m, factor 2 | 2m / 2m | 1d |

`set_credit_entries` gets two attempts because an additive send is at-least-once and every attempt is a potential
second copy of the entries. Inactivity timeout waits for progress before requesting SDK suspension;
**abort timeout starts after that request** and bounds the subsequent wait before aborting the execution.
They are not concurrent timers starting with the external call, nor a terminal invocation kill.
The timeouts follow one sizing rule: a step's szamlazz.hu round trips at the client's 60 s
`REQUEST_TIMEOUT` each, plus the margin a stalling szamlazz.hu needs. `4m` / `3m` where the step is three trips
(the create and storno steps' leading query, send and re-query); `2m` / `2m` where it is one, which is
`set_credit_entries`' send and every read step alike. The reads avoid the server's 1 m inactivity default, which
could request suspension during a slow read; abort follows only if the SDK does not suspend within the further
abort interval. The 2 m retry interval is longer than the client timeout and allows the observed server stall
to settle; a client deadline does not prove that szamlazz.hu has stopped processing a request.

**An invocation attempt is spent on retained read-only reconciliation or a worker-side failure** (the worker unreachable, a rollout cutting the
connection, the abort timeout, an undecodable journal), not on an explicitly delayed run retry: a step re-executed under `[issue]`,
`[read]` or `[resolve]` is re-dispatched by the server without advancing the handler's attempt count (verified end
to end against 1.7.8). Run policies govern ordinary reads and unmanaged Agent storno. Order's retained
reconciliation and infrastructure failures spend its invocation policy (~24 min of delays with defaults).
No retry threshold is a hard send or elapsed-time bound; protected Order writes instead have one-use permission.

**Unresolved writes.** Every Order mutation checks the durable marker before resolving the account or reading
prerequisites. Before a write it records minimal recovery identity, sets the marker, awaits durable arming,
and consumes an execution-local one-use permission. Completed arming replay grants no permission. An open write
replay therefore reconciles without sending. Uncertainty becomes a read-only run whose failures spend the
invocation policy and eventually pause with the lock retained; `[issue]` never authorizes another Order send.
Cancellation returns structured uncertainty and retains the marker. Kill releases the lock but not the marker.

Keep the original Idempotency-Key while unfinished, paused included. Empty queries, elapsed time, cancellation,
kill and a new key do not settle uncertainty. Positive evidence must match the order/kind and corrective or
reissue intent; storno recovery verifies the original reference and reversal. Deletion absence is ambiguous.

`observe_unresolved` is an operator-only shared observation, usable beside a paused owner. `recover` is an
operator-only exclusive action carrying the **exact observed marker** and either document evidence
(`{"type":"document","number":"SZ-1"}`) or audited non-execution attestation
(`{"type":"not_executed","audit_reference":"INC-216","did_not_execute_and_cannot_execute_later":true}`).
An attestation is the operator's assertion, never vendor proof. Recovery records evidence before clearing;
it sends nothing. Stale markers, changed identity and unknown evidence refuse.

Candidate document and issued/reversal attestation numbers use `contract::recovery::EvidenceNumber`:
nonblank XML 1.0 text, preserving the exact vendor spelling without the mutation input's 40-byte bound or
restrictions on whitespace and `:`. Deleted numbers remain bounded mutation targets and must equal the
marker’s pinned number. Recovery never authorizes using an evidence number as a new mutation target.

Alternatively, independently confirmed completion can be submitted as audited positive settlement:

```json
{"type":"completed","audit_reference":"SUPPORT-301",
 "completion":{"type":"deleted","number":"D-1"},
 "completed_and_cannot_execute_later":true}
```

`completion.type` is `issued`, `reversed` or `deleted`, matching the marker's operation. Deletion must name
exactly the pinned proforma; a reversal names the storno, not the original; issuance cannot name the old reissue
target. The audit record must independently establish the exact operation, account and intent, including any
corrective base, and that no delayed execution remains. This is useful for a confirmed deletion or consumed
proforma whose positive evidence is no longer queryable. It is never a substitute for missing evidence.
See the [operator recovery runbook](../../docs/operations/order-recovery.md).

Access defaults to `forbidden` (403). A host enables it with `Order::with_recovery_authorizer` and its
`service::RecoveryAuthorizer`, using authenticated operator metadata from its ingress boundary. Strip caller
identity assertions, enforce the same policy on internal SDK callers, and authenticate runtime requests to
the endpoint. A request-body flag is not authorization. Never enable an allow-all authorizer on public ingress.

Prefer resuming a paused owner on its pinned deployment. Exclusive recovery cannot run behind that owner's
lock: quiesce producers, inspect/cancel queued mutations, deliberately stop the owner, then submit evidence.
Recovery uses the pinned account/endpoint and credential reference; changed scope configuration cannot redirect
it. Keep rotated credentials available behind that stable reference.

**Migration:** quiesce every producer, including delayed sends and internal SDK callers; settle old unmarked
uncertainty and external work before switching. Draining Restate alone is not vendor completion. Old code that
ignores markers must not overlap protected code on the same scope/key. Unknown marker versions or malformed
state fail closed; retain compatible code and review actual prefixes for exceptional replay. Monitor unresolved
age, paused owners, orphaned markers and queued mutations (#45). Unkeyed Agent writes, vendor UI writes and
administrative state deletion are outside the Order protection boundary. See the
[exact command protocol](../../docs/design/order-write-protocol.md).

**The prologue's own waits are bounded too.** One `AccountResolver::resolve` or `CredentialStore::fetch` call
gets ten seconds (a worker constant, not a setting: a resolver that has not answered by then is not going to),
after which the call is dropped and answered as unavailable, retried under `[resolve]`, or by the fetch loop's
three in-process attempts, then the terminal `unavailable`, whose text names the deadline and neither the account
nor the credential reference. A hung database pool behind an embedder's resolver therefore never holds an
execution until the handler's inactivity timeout.

### Issuing, step by step

After validation and the prologue, the first read is the **target ownership lookup** (`lookup-{kind}`),
including `lookup-corrective` for a `correction_id`. It settles a live target as `already_issued` (or
`conflict{live}` with `reissue`), a collision as `conflict{external_id_collision}`, and a reversed target without
`reissue` as `reversed`, **before prerequisites**. A reversed non-corrective gets a best-effort
`hint-storno-{number}` for its storno number; exhaustion leaves the number absent, cancellation propagates.
Correctives take no hint. A consumed proforma, reversed prepayment or changed corrective base therefore cannot
hide an already-issued target.

For explicit reissue, both target reads first require the expected number: absence or a different owned
holder is `conflict{target_changed}`; ownership collisions keep `external_id_collision`.
Only an ordinary create's absent target or a matching, reversed reissue target proceeds through prerequisites (exclusivity, prepayment, proforma
link or corrective base), then the existing full lookup and query-first create. Both target reads are named
`lookup-{kind}`: the first journals `OwnershipOutcome`, the later full lookup journals `LookupOutcome` and
observes changes outside the order's lock as well as foreign documents.

**The lookup** (`lookup-{kind}`) is read-only and settles every case that needs no create: a live document of
ours is `already_issued` (or `conflict{live}` with `reissue`), a reversed one is `reversed` (or proceeds with
`reissue`), an invalid holder is `conflict{external_id_collision}`, a live invoice under the order that is not
ours is `conflict{foreign}`.

Like the ordinary read-only steps of both services (the exclusivity and proforma-link lookups before it, the verifies,
the order-number hint, the storno lookup, `get`'s four queries, `Szamlazz.Agent.query`, `query_taxpayer`'s one
step `lookup-taxpayer-{prefix}`, the `check_account` probe) it runs under the **read policy** (`[read]`: `5` executions
`5s` → `60s`, duration threshold `5m` by default). Every szamlazz.hu *answer* is journaled data, and a query szamlazz.hu
did not answer (a transport or parse failure, `szlahu_down`) is the step's retryable error (`Unanswered`),
re-executed after the policy's delay; a read writes nothing, so re-executing it is safe and its answer is as
fresh as a first one. When the read policy is exhausted the handler fails with `TerminalError{unavailable}`
naming the step, the last failure and, where the step knows it, the order, kind and external id.
`get` stops immediately on a credential or inconclusive vendor answer, preserving that fault and its warning.
Protected `reconcile-write` instead uses the mutation's invocation policy described above.

**The protected create** (`create-{kind}`) consumes one execution-local send permission after durable arming.
Its fresh leading query allows an ordinary create only when the external id holds **nothing**, and reissue
only past **exactly the expected reversed document**. An absent reissue target before the write step is
`conflict{target_changed}`; inside the write step it remains `outcome_unknown`:

- a live document an earlier execution issued is answered `issued` without sending;
- a document reversed since the lookup is answered `reversed` without sending (a new document needs an explicit
  `reissue`);
- the lookup's reversed document reported live is `conflict{live}`;
- a corrective first found in the armed leading query must reference the intended base; a missing or
  different base is `conflict{external_id_collision}`, with no send. Read-only reconciliation applies the
  same base check but retains uncertainty because an earlier send may have landed;
- an *answer* to the leading query that is neither 7 nor a credential code (another szamlazz.hu code, or
  `szlahu_down`) is settled data too, with nothing sent: the handler raises the same `unavailable` the lookup
   step raises for that code, at once, without entering retained reconciliation.

A lost or inconclusive reply is journaled as unresolved data with a safe diagnostic. The separate
`reconcile-write` run only reads, retaining the original cause and latest reason in its failure. Exhaustion of
the invocation policy pauses the owner; it cannot grant another send. A conclusive 71/152 refusal remains
settled even if the diagnostic query fails. Correctives take no duplicate-order hint and retain the refusal.

**Storno** has the same shape: a read-only lookup of the storno external id (`lookup-storno-{number}`) and a
storno step (`storno-{number}`). `Szamlazz.Order.storno_invoice` uses the protected one-use protocol;
`Szamlazz.Agent.storno` is query-first under `[issue]`. The storno request is a pure function of the
verified original (its `telj` as `teljesitesDatum`, its `eszamla` or the account default as the e-invoice flag),
so every execution of the step sends byte-identical bytes; a verified original without a `telj` is `unavailable`
with nothing sent, raised after the answers that need no send. Neither the date nor the form is enforced by
szamlazz.hu (it issues the storno with whatever `teljesitesDatum` and `eszamla` the request carries), so the
derivation from the verified original is what keeps a reversal on its original's date and in its original's form.

**Unnumbered storno acknowledgements.** For unmanaged `Szamlazz.Agent.storno`, a successful reply without a
reported number permits one read-only reconciliation. Positive reversal evidence can settle it; absence or
a failed reconciliation instead completes the run with journaled `StornoOutcome::Unnumbered` data and returns
`outcome_unknown` outside the issue-policy retry loop. This reply cannot trigger a policy-driven resend.
The retained same `Idempotency-Key` repeats that completed fault; settle the earlier reversal before deliberately
renewing. As with other unkeyed writes, a crash before recording the run completion can still re-execute an open
run. Order storno retains its existing unresolved-write marker and read-only reconciliation protection.
Acknowledgement metadata and PDF are excluded from the worker journal.

**Ambiguous storno replies (#196).** A changed number with absent or positive optional gross is now verified
by querying that number for the storno type and reference to the original. It can return `reversed`; if that
query is inconclusive, Order retains the candidate for read-only reconciliation. Without a candidate, a missing
storno external id permits an order hint, still verified against the original's reference, reversal and order.
Unmanaged Agent storno stays `Unconfirmed` under its issue policy and becomes `outcome_unknown` on exhaustion, rather than the previous
`rejected{not_stornoable}` no-op inference. Post-send credential/unavailable failures preserve the uncertainty
and their causes. The same-number echo policy and changed-number/nonpositive-gross fast path remain; zero is
a synthetic comparison control, and neither zero-total nor negative-total originals have been verified live.

Release/journal review under [ADR 0009](../../docs/adr/0009-immutable-deployments-no-journal-compatibility-contract.md):
protected writes and their diagnostic result shapes require a new immutable deployment. Keep in-flight owners
on their pinned code. Review actual command prefixes before exceptional replay; matching step names alone do
not establish compatibility. The persistent marker schema remains version 1; positive operator settlement adds
an evidence variant, not permission to discard or migrate unknown marker state.

A document the verify already sees reversed is `reversed` with a **best-effort** storno number:
`Szamlazz.Order.storno_invoice` from the order-number hint, `Szamlazz.Agent.storno` from the by-number storno
lookup (ours when we issued the storno, unknown after a reversal from the UI). An exhausted read reports the
reversal without the number after a `warn`, while a cancellation of the invocation is never swallowed.

### One-shot deletion and credit entries (#201 release notes)

`delete_proforma` requires `expected_number` and compares it with the owned holder found by the journaled
ownership lookup. A different holder is `target_changed`, and absence is `{deleted: true, reason: "absent"}`
(deleted earlier or consumed, not proof this invocation deleted it). Inside
`delete-proforma-{number}`, each execution queries **that number** and checks its document id,
number, order and proforma type, then its current credit entries. A changed target yields
`{deleted: false, reason: "target_changed"}`; credit entries with `force: false` yield
`proforma_paid`. `force: true` bypasses only the credit-entry guard. Disappearance (query code 7)
or deletion code 335 yields `deleted: true` (already deleted or consumed). A failed fresh read
yields `unavailable` (503), a credential code `credentials_rejected` (503), with no delete sent
by that execution. The step never reselects a replacement by external id. This narrows the
replay gap; the vendor's query and delete are **not atomic** against other writers.

### Expected-document intent (0.4 breaking release notes, #206)

The new request shapes are:

```json
{
  "document": {
    "buyer": {"name": "Kovács Bt.", "zip": "2030", "city": "Érd", "address": "Tárnoki út 23."},
    "items": [{"name": "Consulting", "quantity": "1", "unit": "db", "unit_price": "1000", "vat_rate": "27"}],
    "fulfillment_date": "2026-09-10",
    "due_date": "2026-09-18"
  },
  "options": {"reissue": {"expected_number": "SZ-A"}}
}
```

Deletion takes `{"expected_number": "D-A", "force": false}`. Both expected numbers use the existing
bounded `InvoiceNumber` contract (1–40 bytes, no whitespace, controls or `:`), and both request objects are
closed to unknown fields. Rust callers use `CreateOptions.reissue: Option<Reissue>` and
`DeleteProformaRequest::new(expected_number, force)`; deletion no longer implements `Default` or `Copy`.

**Migration:** omit former `reissue: false`; replace `true` with the object naming the intended reversed
original; add the intended number to every deletion. Booleans and numberless deletion are `invalid_input`
before the prologue. Obtain the number from a create/reversed response or a fresh `get`, and persist it with
the command. `correct_invoice` still has no reissue field: its repeat request reports `reversed`; issuing a
new corrective requires a deliberate new `correction_id` (correctives are not in `get`).

**Response handling:** a retained completion replays success. After expiry or purge, an immediate retry of a
successful reissue sees the replacement and answers `conflict{target_changed}`, even if it is live. After
that replacement is reversed, the old request still conflicts. A repeated deletion sees `absent`, or
`{deleted: false, reason: "target_changed"}` if a replacement exists, even with `force`. Handle these as
observations, not historical success or permission to substitute `existing_number` automatically. The
matching live reissue target is still `conflict{live}`; ownership collisions retain their existing outcomes.

**Separate guarantees:** external ids recover documents after journal expiry; Restate request deduplication
lasts only for its retention period; expected-document intent prevents an old command authorizing a mutation
of a replacement. Longer retention is not indefinite idempotency, and a conflict/empty observation does not
settle an unresolved earlier send (#205). The Order retains only uncertainty state. Query/send races with other writers
remain possible; the mock lifecycle tests establish worker decisions, not vendor atomic compare-and-set.

Drain legacy commands on their original immutable deployment before changing callers. Register this release
as a new deployment; exceptional replay requires reviewing actual inputs and journal under ADR 0009. The
create step adds the journaled `TargetChanged` outcome for an expected holder disappearing before a send,
mapped to `outcome_unknown` because an earlier execution may have sent;
completed old invocations keep their original results. See [ADR 0012](../../docs/adr/0012-expected-document-mutation-intent.md).

Both operations preserve inconclusive send answers as journaled data. Protected Order deletion retains
read-only reconciliation and pauses; cancellation returns `outcome_unknown` and preserves its marker.
Unkeyed Agent credit entries return `outcome_unknown` (500), including the vendor cause and `szamlazz_code`
where supplied (`absent` for a failed verdict without a code). Even credit-registration refusals 53/57/463
remain `outcome_unknown`: they settle the latest exchange but cannot exclude an earlier execution of an
interrupted open run. Local request validation stays `invalid_input`; credential codes 3/135/136/164 stay
`credentials_rejected`. Protected deletion's single permitted send keeps settled XML-input refusals 53/57
and its operation-specific 335 handling.
Invoice-creation codes are not treated as evidence that these writes were refused.

The evidence is the vendor's [error-handling documentation](https://docs.szamlazz.hu/agent/basics/error-handling),
[deletion response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response), and
[behaviour notes D1/D3/D8](../../docs/szamlazz-hu-behaviour.md). No live post-action error for
deletion or credit-entry registration was demonstrated; the uncertainty policy is conservative.

Both runs keep `max_attempts(1)`. Order deletion additionally requires execution-local send permission:
replay of an open closure cannot repeat it. Agent credit entries remain unkeyed and can repeat after a crash;
use their operation-specific query/reconciliation guidance. A completed uncertain fault never alone
authorizes another write.

Release/journal review under ADR 0009: register a new immutable deployment. Step names and
ordered paths stay the same, but deletion now consumes the pinned document and order for its
fresh guard, and both outcome enums gain variants. Review these changed inputs and result
shapes before any cross-deployment resume. Completed old refusals stay completed under their
old key; a new invocation performs the new checks.

## Testing

The shared [testing guide](../../docs/testing.md) lists nextest profiles and
commands. `cargo live` adds two ignored actual-Restate → actual-szamlazz.hu
journeys: ordinary e-invoice consumption/reversal/reissue and EUR
proforma → prepayment → final with an explicit deduction line. These use
validated worker policies. The mocked-vendor suites below remain regular CI.

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
- the wiremock tests of the gateway against synthetic szamlazz.hu responses (`tests/gateway/`), which prove the
  **wire** of each step (which requests it sends and in what order, what it puts in them, and that each answer,
  in headers or in the body alone, is read into the outcome the gateway's classifiers name; the classifiers
  themselves are pure fns table-tested in the module's unit tests): the lookup matrix (`Absent`, `Live`,
  `Reversed`, `Collision`, `Foreign`, the corrective's exemption from the hint, `Unanswered` on a lost reply and
  `Api` on another code); the create step's **leading-query table** (one row per answer the external id can give
  against the number the lookup saw reversed, with the create mock's `expect(0|1)`: a clean miss sends, a live or
  reversed document of an earlier execution settles, another code and `szlahu_down` are data, a credential code
  never sends, a lost reply is the one `Unconfirmed` before a send) and the storno step's twin; the create step
  driven twice after a lost reply (`Found`), the open codes re-queried once and `Unconfirmed` when nothing landed,
  a failed re-query naming both causes, the 71/152 matrix, the corrective's 71/152 → `Rejected`; the storno
  lookup and step (`AlreadyReversed` on a re-executed step, the body carrying `<teljesitesDatum>` and the
  original's `eszamla` and no `<keltDatum>`, two executions after a lost reply sending byte-identical bodies), the
  proforma / delivery-note no-op, a verified document's `telj` (and its absence) surfaced, 335, 7; **one
  credential-code table** with one code per operation in the operation's own shape (the lookup's two queries, the
  create's send, the three reads, the probe, the storno lookup and send, the delete, the credit entries, the
  taxpayer query), `expect(0)` on what must not follow and no re-query after a rejected send (which codes are
  credential codes is the agent crate's `is_credential_error`, unit-tested there); every read fn answering a 500,
  an empty body or `szlahu_down` as `Err(Unanswered)` rather than data; the `check_account` probe as exactly one
  query of the sentinel id; and the taxpayer query answering a known prefix as `Found` with NAV's registered data,
  an unknown one as `Found{valid: false}`, NAV's relayed `funcCode ERROR` or a szamlazz.hu code as `Api`, and a
  500, an unparseable body or `szlahu_down` as `Err(Unanswered)`.

The synthetic szamlazz.hu answers are stated once, in `tests/common/mod.rs`: the `<szamla>` document renderer
(`Doc`), the response templates of every operation and the wiremock selector matchers, shared by the gateway
tests, the e2e suite and (by path) the crate's unit tests, so a fact learned about szamlazz.hu's XML is edited in
one place.

### Journal entries

The same run checks what a `ctx.run` result may hold (`service::journal`): a sample of every variant of every
journaled type round-trips through serde; none carries the agent key (an account resolved from configuration whose
key is a sentinel, and the two `Lost` write outcomes from a gateway opened with the sentinel credentials, are
scanned for it); and none carries the document body: no `supplier`, `buyer`, `items`, `financial_items`, `labels`
or `pdf` key at any depth, since the document outcomes carry the worker's own projections
(`gateway::document::{FoundDocument, IssuedDocument}`: what the handlers read of a queried document or a create
reply) and a journal entry is shown in the Restate UI for the retention period.

There is no cross-version compatibility contract on the journal: a release is a new Restate deployment, and Restate
replays an invocation only on the deployment that started it (ADR 0009, [*Deploying*](#deploying)), so a journaled type may
be reshaped freely between releases.

### End to end

`cargo test -p restate-szamlazz --test e2e -- --ignored` runs `tests/e2e/`: the `Szamlazz.Order` Virtual Object
and `Szamlazz.Agent` end to end against a real Restate server (1.7.8, with the experimental `vqueues`,
`protocol_v7` and `scoped_virtual_objects` flags; `compose.yaml` sets the same three) with wiremock standing in
for szamlazz.hu, in two phases on one server. It is two things and nothing else:

- **the sequence test**: one scenario per handler path of `RUN_NAMES` (the ordered `ctx.run` names each
  handler journals; a handler with two shapes has two paths), proving the steps run in that order under Restate
  and walking every path in full at least once, which the step-name table check at the end demands. A scenario that is the
  only walker of a path stays however plain its decision; the decision itself (what a handler answers to a given
  read) is a unit test of `service`, and the wire of each step is `tests/gateway/`'s;
- **the durable-execution proof**: what only a server can show. The `Idempotency-Key` replaying a stored
  completion, and a stored fault; a run retry re-executing a read and a write with the delay of the run policy
  (`retry_count` and the failing command on `sys_invocation` while in flight, one journal entry per step) and its
  exhaustion as a structured fault; run retries spending no invocation attempts (#87); the create step's leading
  query on a **re-executed** closure meeting a reversal that happened between the two executions; a cancellation
  mid-send answered `outcome_unknown` and releasing the key; the flag day; the scope namespacing the Virtual
  Object key and the `Idempotency-Key`; the per-key lock and the in-flight attach under one scope (#125); the
  `account` entry replayed across a re-execution while the account changes and while the key rotates; an order
  Restate has no memory of after a purge; a stuck invocation killed off the key; the wire faults (a malformed body
  and an untrimmed key as the structured `invalid_input`, a szamlazz.hu code inside the ingress envelope); the
  protocol-v7 canary; and, over the completed journeys, cleared unresolved markers, no agent key in any journal entry
  (the hex-decoded `raw`) with a planted positive control found, and the step-name table check.

**Phase 1** registers the single-account deployment (the static resolver's `[account]`) and runs its
scenarios **concurrently**, unscoped, on one runtime (a `JoinSet`): every scenario owns its order keys, numbers
and `Idempotency-Key`s, every stub is mounted once and discriminated by them (`create_for(order)` matches the
`<rendelesSzam>` the worker puts on every create, a storno or credit entry by its `<szamlaszam>`, a query by its
external id or order), nothing is reset between scenarios, every count is per order or per number, and every
scenario's failure is reported at the end rather than the first one ending the run; the run goes on to phase 2 and
the checks, whose report then notes that their counts are suspect. Restate's per-key lock makes
distinct order keys non-interfering. The scenarios: the first create `issued` then `already_issued` then the key
replayed; a reversal between two executions of the create step; the proforma, the invoice naming it by number and
`get` reporting it `consumed`; the prepayment invoice under `auto` and by number; the final invoice naming its
live prepayment invoice; the corrective under its correction id; storno then reissue; the storno answered from the
hint and a storno re-executed with a byte-identical body; the proforma deleted and then absent; the three
policies (an exhausted create and the key replaying its fault, a flaky read, an exhausted read) on three orders at
once; cancellation mid-create and mid-delete; existing targets settling before changed
prerequisites and the reversed-target hint paths; run retries against `get`'s `max_attempts`; the wire faults; the positive
control.

**Phase 2** performs the documented flag day with ingress-only producers and no pending delayed sends
(private, drain, register the multi-account revision, public; the one
step whose failure ends the run, since everything after it runs on that deployment) and runs, **in sequence** (its
scenarios script the shared resolver and store per scope, `acme` or `beta`), each as a task of its own so that a
failure is recorded under its name, the mock reset without verifying the failed scenario's expectations, and the
next scenario runs: the
first scoped create finding the document issued unscoped under the unchanged external id, unscoped and an unknown
scope → `unknown_account`; the same order key and the same `Idempotency-Key` under two scopes as two objects and
two invocations, each account's key on its create; the order-key lock, same key, same scope (#125), with the first
invocation **held** at its credential fetch (`hold_fetch`) until the second call is on the server (the first then
released into a three-second szamlazz.hu reply → `issued` + `already_issued`, one create; the same with the
second call between the first's two create-step executions; and the **same** `Idempotency-Key` sent while the
first is held attaching to it: one invocation id, one body, one create); every `Szamlazz.Agent` read on the
account its scope selects (`check_account` under each scope with that account's key on the probe, unscoped →
`unknown_account`; `query` with `test` as reported and no `supplier_id`; `query_taxpayer` under each scope with
its key, the full number and the stem one step); the `Szamlazz.Agent` writes on the scoped account (`storno`
with `<teljesitesDatum>` equal to the original's `telj` and no `<keltDatum>`, `set_credit_entries` with the flag, the
entries and the key on the wire and the totals answered, cancellation mid-credit-entry-send as structured
`outcome_unknown`); an order whose invocations were purged stornoed and
reissued; a resolver failing twice then answering, the `account` step re-executed under the resolve policy with
one entry; an invocation held at its fetch after its `account` step, killed, the queued call on the same key
running at once; an account change and a credential rotation between two executions, the journaled `account`
entry winning and staying byte-identical.

**Last**, over every invocation the server holds (run whether or not a scenario failed, and reported with the
scenarios' failures, noting when an earlier failure makes their counts suspect): the `state` table holds no row for
`Szamlazz.Order`; no agent
key of the run appears in the hex-decoded `raw` of any journal entry nor in any `completion_failure`, while the
same scan finds the positive control's sentinel; and the **step-name table check**: `RUN_NAMES` in the harness
lists, per handler of both services, the ordered `ctx.run` names of every path it journals, and the scenario
asserts (through the crate's `Table::check`) that every invocation's run sequence is a prefix of one of its
handler's paths, that every handler the deployments offer (`GET /services`) or an invocation names is in the table
and that every path was walked in full. The table is a regression signal for exceptional replay (*Deploying*),
including deployment-changing resume and retained-prefix restart. A renamed, inserted, reordered or dropped step
shows in its diff; allowed path patterns alone cannot prove that an old invocation takes the same branch or emits
the same exact commands.

The harness calls through `/restate/call/…` and `/restate/scope/{scope}/call/…`, reports `x-restate-id`, parses
fault bodies out of the ingress envelope (asserting on every fault that the body is
`{code: <status>, message, source: "invocation"}` under `x-restate-error-source: invocation` and that the
worker's fault is the JSON in `message`), and reads `sys_journal` / `sys_invocation` through the SQL
introspection API.

**Where the suite lives.** One integration-test binary: `tests/e2e/main.rs` holds the two tests and the order the
scenarios run in (one server start-up, the server gate decided once; phase 1 as a `JoinSet`, phase 2 in
sequence). `tests/e2e/harness/` is the szamlazz half of the harness, one module per concern:

- `mod.rs`: the `Harness` composing the Restate server, the wiremock and the accounts, and the two server specs
  (the main suite's three flags; the canary's without protocol v7);
- `accounts`: the static resolver of phase 1 and the mutable resolver and store of phase 2 (a resolution it fails
  per scope, a fetch it holds per credential reference so an account change or a key rotation lands between two
  executions in sequence, or an invocation stands still where a scenario needs it);
- `szamlazz`: the document-centric stub helpers over the shared fixtures of `tests/common`;
- `ingress`: a reply with the contract's `Fault` decoded out of the envelope;
- `run_names`: the `RUN_NAMES` table, held as the crate's `Table`.

The Restate half (the server gate and the launcher, the spawned server, the in-process deployment, `set_public`
and `drain`, the ingress reply and the envelope check, the admin API's SQL, journals, `sys_invocation` rows, kill /
cancel / purge and the in-flight sampler, the step-name table check) is the published
[`restate-e2e-harness`](https://crates.io/crates/restate-e2e-harness) crate, a crates.io dev-dependency that knows nothing of szamlazz and
never depends on this crate (a dependency back would be a dev-dependency cycle Cargo resolves by compiling this
crate twice, and every type crossing the boundary would then be two types). The harness's own tests (the fetch
hold and the resolution script, the stub helpers against wiremock alone) sit beside what they test and run
un-ignored; the server gate's, the sampler's and the table check's are the crate's. Every other file is one handler
family's scenarios (`create_invoice`, `create_proforma`, `create_prepayment`, `create_final`, `correct_invoice`,
`storno`, `delete_proforma`, `get`, `policies`, `agent_reads`, `agent_writes`, `faults`, `prologue`,
`multi_account`, `invariants`), each scenario a `pub(crate) async fn` taking the harness; a new scenario goes into its
handler's file and is listed in `main.rs`, in phase 1 when it needs only the single-account deployment and its own
order keys, in phase 2 when it needs a scope or scripts the resolver or store. The suite runs in about 20 s
(from about 50 s before the prune and the concurrent phase 1; #134).

**One family at a time.** A developer iterating on one handler sets `E2E_ONLY=<needle,…>`
(`E2E_ONLY=storno cargo test -p restate-szamlazz --test e2e -- --ignored e2e_order`): each needle is a substring of
a scenario's `family::scenario` name, so a family name selects its file's scenarios and any scenario naming it
(`storno` also selects `agent_writes::agent_storno_and_…`), and the run is the selected scenarios plus their
prerequisites: phase 1's run concurrently as always, the flag day runs when a phase-2 scenario is selected and is
skipped with the whole phase otherwise, and the three run-wide checks are skipped under any filter (they count over
the whole run). Every skipped scenario is printed as `skip (E2E_ONLY)`, a needle that selects nothing fails the run
(a typo must not pass as an empty run), and unset or empty the run is unchanged, so CI is unaffected. `E2E_ONLY=storno`
runs in about five seconds.

**The protocol-v7 canary** (`e2e_check_account_without_protocol_v7`) runs in the same command on a server of its
own with `protocol_v7` off: the ingress accepts the scoped path and keys the invocation by the scope, but the SDK
sees none, so a scoped `check_account` answers `scope: null` with the account on the single-account deployment
and `unknown_account` on the multi-account one. This is what the deploy-time `check_account` under each scope
looks for, provoked once.

**Where the server comes from.** The harness decides once from the environment (the crate's server gate):

- `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL` reuse a running server with the three flags (the main suite only;
  the canary needs a server of its own shape): `docker compose up -d` at the workspace root starts one. **One run
  per server**: the suite's `Idempotency-Key`s and order keys are literals and its checks count over every
  invocation the server holds, so the harness refuses a server that already holds `Szamlazz.*` invocations, before
  anything is deployed, naming the fix (`docker compose down -v && docker compose up -d`);
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
is killed. Each launch exclusively claims a fresh base dir (`$TMPDIR/restate-e2e-{pid}-{main|canary}-{sequence}`),
skipping existing candidates independently of port reuse. Failed launches and panic teardown keep their own log
and data for inspection; normal teardown removes only that launch's directory.

**What CI runs.** `dagger check` (the `Dagger` workflow on every pull request) runs the `rust` module's `build`,
`test` (default features), `clippy`, `doc`, `audit` and `fmt` checks and the workspace's own `ci` module
(`.dagger/modules/ci`): `ci:test` is `cargo test --workspace --all-features --locked` (so the `schemars` contract
tests and the `szamlazz-adatkapcsolat` archiver tests run), `ci:end-to-end` is the ignored suite above with
`restate-server` copied out of the Restate image and `CI` set, run as
`cargo test --workspace --all-features --locked --tests -- --ignored e2e_`: the same selection the container
compiled the tests with, narrowed to the worker's ignored e2e tests by name,
so the check compiles nothing (`-p` or `--test e2e` would unify dev-dependency features
differently and recompile fifty-odd crates first); when it fails, the kept `restate-server.log` of every server
follows its output. `dagger check ci:end-to-end` runs it locally the same way.

### Go-live

The [go-live checklist](../../docs/szamlazz-hu-behaviour.md#go-live-checklist) re-establishes the verified
szamlazz.hu facts on a target account before the worker is enabled; every step issues real documents there. After
a deploy, call `Szamlazz.Agent.check_account` through each configured Restate scope to verify routing and protocol
v7. Verify the seller separately with the executable [`examples/verify_seller.rs`](examples/verify_seller.rs):

```sh
cargo run -p restate-szamlazz --example verify_seller < deploy-check.json
```

The example documents the private JSON input. Supply the **actual deployed resolver configuration and credential
source**, plus an independently known invoice number, expected `test`, seller name and seller tax number for every
scope (`null` for the unscoped account). For a custom host, use the example's `verified_endpoint` with the exact
`Accounts` bundle bound to `Order` and `Agent`; the host supplies its complete scope list. The check resolves each
scope, fetches its credentials and opens a fresh Számla Agent client at its resolved endpoint, outside Restate's
journal. A separately copied agent key does not verify the deployed mapping. `Szamlazz.Agent.query` deliberately
omits the seller block and cannot perform this check.

Repeat after key rotation. This is a point-in-time check, not an account pin: a wrong key introduced afterwards
can still issue in another company's name, or in live instead of test, with nothing in the worker failing.

### Deploying

A release is a **new Restate deployment** (ADR 0009): register it under a URI of its own (`restate deployments
register http://worker-v2/`, a new Lambda version, a new `RestateDeployment` under the Kubernetes operator), never
re-register the same URI in place. Restate routes new invocations to the latest deployment and keeps every in-flight
invocation, retries included, on the deployment it started on under normal routing; the worker therefore has no
general cross-release journal compatibility contract. Keep the previous release running until
`restate deployment describe <id> --extra` reports no active invocations on it, then remove it if it is no longer
needed for exceptional replay below; that report, not a clock, is the drain. Run limits can overshoot, same-key invocations queue behind
the one holding the key, a crashed handler is re-dispatched under its invocation retry policy, and a paused invocation
waits for an operator, so a deployment with a backlog can hold invocations for hours. `restate deployments register
--force` is for local development: replacing code behind a pinned URI can strand its invocations.

**Exceptional replay is an operator-selected repair**, both moving a stuck invocation to another deployment
(`pause` / `resume --deployment`) and restarting a completed invocation from a retained journal prefix. Server
**1.7.8 source** keeps the existing pin on resume unless explicitly overridden; prefix restart inherits the old
pin unless patched, creates a new invocation and clears the original idempotency key. These are version-qualified
facts, not defaults inferred from current unversioned docs. Verify the actual server, CLI/API request and selected
deployment before repair; the server's protocol-version check does not prove application replay compatibility.
See [ADR 0009's pinned sources and repair review](../../docs/adr/0009-immutable-deployments-no-journal-compatibility-contract.md#exceptional-replay-204-2026-09-10).

Review the **actual invocation prefix**, old and new branch logic, exact context commands (names and parameters
included), serialization and operation inputs. The step-name table checks allowed current path patterns; its diff
is useful evidence, not a proof that this old invocation replays. A prefix restart can also execute operations
beyond the copied prefix again, so reconcile their external effects before authorizing it, even on the same code.
If this review cannot establish compatibility, retain the original deployment; killing or starting a new invocation
is not rollback or permission to reissue. Completed-result retention and journal retention are separate from
keeping code available, and neither bounds the drain time.

The target-first cleanup inserts an ownership lookup before prerequisites and adds reversed-target hint paths.
Its `ctx.run` sequence differs from the previous deployment's: **do not resume an invocation from that sequence
on this release**, including a restart carrying that incompatible prefix. Keep it on its original deployment;
reconcile external effects before deliberately choosing a new invocation.

**Mapping flag days require quiesced producers.** Making services private blocks ingress calls, but internal SDK
calls still work. Before the [single → multi procedure](../../docs/design/restate-szamlazz.md#9-configuration-deployment-constant-never-in-payloads),
stop internal producers and account for pending delayed sends (drain them under the old mapping or deliberately
cancel and reconcile them); stopping their originator alone does not remove detached sends. Keep producers stopped
through drain and switch. Retain authorized operator ingress under the old mapping while blocking business calls
at the gateway; recover markers before making the services private, which blocks recovery ingress too.
Invocation drain is insufficient: recover every retained marker under its original scope
and independently settle external uncertainty. Run `python3 scripts/check-order-migration.py --admin-url "$RESTATE_ADMIN_URL"`
from the repository root; every Order state row (including unreadable state) blocks switching. Its empty result
is an inventory check, not vendor proof. Then update scope routing and reopen. See the
[recovery runbook](../../docs/operations/order-recovery.md#scope-migration-inventory).
The e2e procedure assumes ingress-only producers.

Related guarantees have their own work: [#45](https://github.com/sagikazarmark/szamlazz-rs/issues/45) owns broader
alerting/runbooks; [#50](https://github.com/sagikazarmark/szamlazz-rs/issues/50) proposes concurrent `get` reads
(current reads are sequential; Rust SDK 0.12 requires immediately awaiting runs before other context calls);
[#195](https://github.com/sagikazarmark/szamlazz-rs/issues/195) owns Számla Agent recovery evidence;
[#200](https://github.com/sagikazarmark/szamlazz-rs/issues/200) credential initialization/rotation;
[#201](https://github.com/sagikazarmark/szamlazz-rs/issues/201) one-shot recovery;
[#202](https://github.com/sagikazarmark/szamlazz-rs/issues/202) replay-aware logging;
[#203](https://github.com/sagikazarmark/szamlazz-rs/issues/203) cancellation;
[#205](https://github.com/sagikazarmark/szamlazz-rs/issues/205) unresolved-write exhaustion/kill; and
[#206](https://github.com/sagikazarmark/szamlazz-rs/issues/206) retention-independent mutation intent. External-id
discovery surviving retention does not mean an old mutation request is deduplicated indefinitely.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
