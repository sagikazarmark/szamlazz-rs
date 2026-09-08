# restate-szamlazz, design and implementation spec (v2, stateless)

Status: accepted for implementation. Supersedes v1 (the ledger design; see ADR 0005 for why). Decisions are recorded
as ADRs 0001–0006, the verified szamlazz.hu behaviour it relies on in [`szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md).
Since #20 one deployment serves any number of szamlazz.hu accounts, selected per request by the Restate scope
([ADR 0006](../adr/0006-account-selection-via-restate-scopes.md)).

## 1. Goal

Expose the basic szamlazz.hu Számla Agent operations as Restate services with durable execution, so that a caller
can say "issue the invoice for order X on account A" and get exactly one legal document under retries, process
crashes, concurrent callers and reversals, with a JSON API that is a *projection* of the Agent model: account
constants moved to the resolved account, deployment constants to config, line totals computed, one handler per
document kind, no PDF.

Non-goals: PDF download, receipts, IPN / Adatkapcsolat ingestion, the proforma → payment → invoice lifecycle
workflow, Kafka ingress (untested in multi-account mode, §4). The NAV taxpayer lookup (`xmltaxpayer`) *is* in scope
since #49, not because it issues anything, but because it is a per-account read that needs the account's agent
key, and the worker is the one place that holds one (§4, `Szamlazz.Agent.query_taxpayer`).

## 2. Crates

| Crate | Kind | Purpose |
|---|---|---|
| `restate-szamlazz` | library | Contract types, deployment config, the account model with the resolver and credential-store traits and the static resolver, the `gateway` module, the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service |
| `restate-szamlazz-endpoint` | binary `restate-szamlazz`, container `ghcr.io/sagikazarmark/restate-szamlazz` | Hosts the services over HTTP for a Restate server; clap + figment config in the single-account (`[account]`) or multi-account (`[accounts.<scope>]`) shape |

`restate-sdk` is an unconditional dependency; the features are `schemars` and `test-util` (the unchecked configuration
constructor the e2e harness builds its sub-floor policies with; never enabled by a deployment).

Layering (ADR 0001): the `gateway` is a Rust module that speaks to szamlazz.hu on behalf of one account; it owns the
`szamlazz_agent::Client` (the transport it wraps; it is not a second client) and the account, and exposes one plain
async fn per durable step with outcome-as-data. `Szamlazz.Order` calls it inside `ctx.run`; `Szamlazz.Agent` is a thin
stateless facade over it for by-number operations. Every read of account configuration by the services (the
ownership-validation pins, the document defaults, the seller block) goes through `Gateway::account()`. The services
hold no gateway: each holds the `Accounts` bundle (account resolver + credential store) and a `WorkerConfig` with the
deployment-level settings (the namespace of the external ids; the issue, read and resolve policies), and every handler's
prologue (§4) resolves its account and opens a gateway for its own execution. No Restate service calls another; no
`Order` handler calls a handler on its own key.

## 3. Principle: szamlazz.hu is the source of truth (ADR 0005)

`Szamlazz.Order` keeps **no state**. The Virtual Object exists for its per-key lock: at most one handler runs for an
order at a time. Everything else is answered by querying szamlazz.hu, the account the invocation resolved to (§4),
through deterministic external ids:

- **VO key** = the order number, trimmed of leading/trailing whitespace, case preserved (the server trims and is
  case-sensitive, verified). Validation: 1–40 bytes after trim (a dashed UUID fits), no control characters, no
  internal whitespace of any kind, no `:` (the external-id separator), Unicode NFC → `invalid_input` naming the
  rule; nothing is collapsed or normalised, because the server's handling is unverified and a `rendelesszam` it
  stored differently from the key would strand the order behind `conflict{external_id_collision}` (ADR 0002, #64). The caller trims: a key whose trimmed form differs from the raw key is refused as `invalid_input`
  naming the rule, before the prologue, because Restate's per-key lock is on the *raw* key, `ORD-1` and ` ORD-1`
  would be two instances with two locks mapping to one szamlazz.hu order and identical external ids, and two
  concurrent creates under them would both pass their lookup and both send, leaving the order-number-repetition
  toggle as the only guard. The order-key *type* stays lenient (it trims) for the places that parse an order number
  rather than a Virtual Object key: its `FromStr`, `TryFrom<String>` and serde implementations, which a caller
  building keys from its own order numbers uses. The key carries no account marker: Restate
  namespaces it per **scope** (ADR 0006), so the same order number under two scopes is two `Szamlazz.Order`
  instances with two locks.
- **External id** (`szamlaKulsoAzon`), deterministic from the key under the deployment's **namespace** (chosen by the
  operator, opaque to szamlazz.hu, permanent; 1–16 bytes of `[a-z0-9-]`, `:` excluded as the separator; one per
  deployment, shared by every account), so *any* invocation can find what an earlier one issued:
  - slot kinds: `"{namespace}:{order}:{kind}"`, `kind ∈ proforma | invoice | prepayment | final`
  - correctives: `"{namespace}:{order}:corrective:{correction_id}"` (caller-supplied id; several correctives per invoice
    are legitimate; `^[A-Za-z0-9][A-Za-z0-9._-]{0,39}$` and not one of the external-id tokens)
  - storno: `"{namespace}:{order}:storno:{original_number}"`
  - Bounded at **110 bytes** (`ExternalId::MAX_LEN`, the length verified accepted and queryable) by bounding the
    parts: namespace 16, order key 40, correction id 40, the caller's invoice number 40 (`contract::InvoiceNumber`:
    no whitespace, no control character, no `:`), so the longest shape, the corrective, is 109; proven at compile
    time in `identity.rs` (#64).
  - Ext ids are not unique server-side; a query returns the **newest** holder (verified). That is exactly the
    question we ask ("what is the newest document of this kind we issued for this order?"), and it is why a reissue
    after a storno needs no generation counter: the new document becomes the newest holder, the old one stays
    reachable through the storno's `hivszamlaszam`.
  - Every `Found` document is validated before it is trusted: `rendelesszam == order ∧ tipus ∈ kind-set`;
    anything else → `conflict{external_id_collision}`. **Nothing about the account.** The worker holds no account
    pin (ADR 0006, account-pin amendment): 0.3 compared `teszt` with the account's `mode` and `szallito/id` with a
    configured `supplier_id`, but a create's reply carries neither (only a query body does), so either check
    could fire only *after* the first document of a fresh order had been issued into whatever account the key
    opens, and its reference value had to be read off the very account being checked (`szallito/id` is besides
    undocumented and of unverified stability). Which account a key opens, and whether it is a test account, is
    the operator's go-live check (§9): query a known document under each scope and read `test` and the seller
    block. `Szamlazz.Order`'s verifies require the found document to carry this order's number
    (`conflict{not_managed}` otherwise), so no handler can act on (or link into this order's invoice) a document
    another order manages.
- **Retry identity** is Restate's ingress `Idempotency-Key` (caller-side, recommended; see §8). The service does not
  know whether one was used, so it never relies on it for safety.
- **Buyer name is serialised byte-identically on every attempt**: normalised once (trim + NFC) at validation. The
  server's identical-request replay compares the name byte-exact (verified); this makes it a stable second guard.
- **Account precondition**: "Rendelésszám ismétlődés tiltása" (Disable order number repetition) ON. It is the
  server-side guard against a second live document of the same kind under one order number (71/152).

## 4. Services and handlers

### `Szamlazz.Order` (Virtual Object, `#[restate_sdk::object(name = "Szamlazz.Order")]`)

| Handler | Kind | Input → Output |
|---|---|---|
| `create_proforma` | exclusive | `CreateRequest` → `CreateResponse` |
| `create_invoice` | exclusive | `CreateRequest` (options: `reissue`, `proforma`) → `CreateResponse` |
| `create_prepayment` | exclusive | `CreateRequest` (options: `reissue`, `proforma`) → `CreateResponse` (v1: one prepayment per order) |
| `create_final` | exclusive | `CreateRequest` → `CreateResponse` (requires a live prepayment; passes `elolegSzamlaszam`; the caller supplies the negative prepayment line) |
| `correct_invoice` | exclusive | `CorrectRequest { invoice_number, correction_id, document }` → `CreateResponse` |
| `storno_invoice` | exclusive | `StornoRequest { invoice_number, comment? }` → `StornoResponse`; the storno repeats the verified original's `telj` as `teljesitesDatum` (ADR 0007), never a caller's date |
| `delete_proforma` | exclusive | `DeleteProformaRequest { force }` → `DeleteProformaResponse` |
| `get` | shared | `()` → `OrderStatus` (live view) |

Attributes on every handler that calls szamlazz.hu (ADR 0004):

```
invocation_retry_policy(initial_interval = "2m", factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")
inactivity_timeout = "4m"   abort_timeout = "3m"   journal_retention = "3d"   idempotency_retention = "30d"
```

`get`: default retry policy, `max_attempts = 3`, `kill`, `journal_retention = "1d"` (inspectable, nothing to replay),
and the reads' timeouts:

```
inactivity_timeout = "2m"   abort_timeout = "2m"
```

One rule sizes every handler's timeouts (#114): a step's szamlazz.hu round trips at the Számla Agent client's
`REQUEST_TIMEOUT` (60 s) each, plus the margin a stalling szamlazz.hu needs, `4m` / `3m` where the step is three trips
(the create and storno steps' leading query, send and re-query), `2m` / `2m` where it is one: `set_payments`' send
(§4, `Szamlazz.Agent`) and every read step, so `get`, `Szamlazz.Agent.query`, `query_taxpayer` and `check_account`
carry `2m` / `2m` too. Never the server's defaults (1 m / 1 m): a read step is one 60 s-bounded trip, and szamlazz.hu
has been observed to stall for a minute at a time and still answer, so a stalled read lands exactly on the default
inactivity boundary; suspended, then aborted, an invocation attempt spent on a read that would have completed. The
discovery test pins all four.

The timeouts bound one handler execution, not an invocation: a run retry policy's delay, the issue policy's, the
read policy's (§9) or the resolve policy's, is returned to the server with the retryable failure and the invocation
yields (the SDK sleeps nowhere in-process), so a failed step ends the execution it fails in and the delays are spent
between executions. The per-execution worst case is therefore one read step of at most two 60 s calls (the lookup's
external-id query and hint) or the create step's three (ADR 0004), well inside `inactivity_timeout`; the read policy
(#37) changed neither budget.

### The prologue (every handler of both services)

After decoding its body (`service::Body<T>`: a malformed one is `TerminalError{invalid_input}` here, before anything
is journaled; §7) and parsing its key, every handler runs the same four steps before its operation; the handler body
then runs on the resulting *execution* (the gateway opened for this execution plus the deployment settings with the
pinned namespace), and nothing of it (gateway, client, credentials) outlives the execution. No Virtual Object state.

1. **Pin**: `ctx.run("namespace", || namespace)`, a pure durable step: an in-place redeploy with a changed namespace
   cannot make a running invocation issue under a new id.
2. **Resolve**: `ctx.run("account", || resolver.resolve(ctx.scope()))` under the **resolve policy** (§9), an
   explicit run retry policy bounded by duration. The closure returns the resolver's answer as data (the `Account`,
   `unscoped`, `unknown{scope}`), and its unavailability as a retryable error (whose text never echoes the
   resolver's own message), so unscoped/unknown are journaled and never retried while an outage re-executes the
   step and journals nothing. The `resolve` call itself runs under a **deadline** of ten seconds
   (`prologue::CALL_DEADLINE`, a worker constant, a resolver that has not answered by then is not going to; #114):
   at the deadline the future is dropped and the closure fails with the same retryable error, its text naming the
   deadline, so a hung database pool behind an embedder's resolver is re-executed by the resolve policy rather than
   holding the execution until the handler's inactivity timeout (the policy bounds re-executions, not one hung
   call). Outside the closure: `unscoped | unknown` → `TerminalError{unknown_account, 400}`;
   exhaustion or cancellation of the run → `TerminalError{unavailable}`. One `account` entry per invocation: the
   invocation finishes on the account it started on, and the Restate UI shows the journaled `Account` (id,
   endpoint, defaults, seller, credential reference, never the key) for the retention period.
3. **Fetch**: `store.fetch(account.credential_ref)` **outside the journal**, on every handler execution
   including replays, with a short in-process retry (three attempts, 200 ms apart, each attempt one call under the
   same ten-second deadline; a store silent past it is retried like one that reported itself unavailable, so the
   loop ends within `3 × 10 s` plus the pauses), then
   `TerminalError{unavailable}`. `gone` is terminal at once. Terminal by decision: a retryable error would route a
   prolonged store outage into the handler's kill-on-five and an unstructured 500, whereas the terminal fault is
   structured and immediate. The fault's text tells `gone`, `unavailable` and the deadline apart but names neither
   the account nor the
   credential reference, no response names the account (§7), and a store's reference may be internal topology (a
   secret path); the operator's `warn` carries both (#65). Documented cost: an outage during a replay of an
   invocation whose create already landed surfaces as `unavailable` although the document exists; `get` or a retry
   with a new `Idempotency-Key` reconciles (`already_issued`). The `Credentials` type has no serde implementation,
   the compiler rejects any attempt to journal it.
4. **Open**: `Gateway::open(account, credentials)` over a fresh Számla Agent client (the default `reqwest::Client`
   keeps szamlazz.hu's `JSESSIONID`; a shared client would carry one account's session into another's request).

The four steps and the handler body run inside one tracing span, **`execution{scope, order, restate.invocation.id,
account.id}`** (`support::{object, shared, service}::execute`): `scope` is what the SDK saw (`<unscoped>` when none),
`order` the Virtual Object key (absent on `Szamlazz.Agent`), `restate.invocation.id` the value the ingress returns as
`x-restate-id`, and `account.id` the resolved account's id, recorded once the `account` step has answered. Every log
line the execution emits, the prologue's warnings, the gateway steps' `gateway.*` spans and events, the paging
`credentials_rejected` warning (whose own fields stay `namespace` and `code`), is thereby attributable to a scope,
an account and an invocation from the worker's log alone, which a multi-account deployment's alerting needs (#65).
Never the key: the account id is journaled and shown in the Restate UI already.

The scope reaches the worker only under protocol v7. Restate's ingress (1.7.8, `ingress-http/src/handler/service_handler.rs`)
refuses a scoped path while `vqueues` or `scoped_virtual_objects` is off but does **not** gate it on `protocol_v7`:
with the other two on and v7 off, the scoped call is accepted, the SDK sees `scope = None`, and a single-account
deployment would issue on its one account. The worker has no second signal of "was this call scoped?" that it is
willing to depend on, the ingress's `x-restate-ingress-path` header would be one, but it is undocumented and
caller-overridable, so a guard on it was considered and dropped (#27), and the defence is operational:
`Szamlazz.Agent.check_account` under each scope after every deploy, whose `scope` field is what the SDK saw; `null`
under a scoped call is that misconfiguration, caught before any order is issued. Kafka ingress is untested and
unsupported in multi-account mode: the server's `kafka_scope` flag can scope a record from an `x-restate-scope`
record header, so "arrives unscoped" is not the reason, no scenario exercises it, and the record's scope would be
set outside the gateway of the safety contract (ADR 0006).

Handler-level behaviour is observable only under Restate (the SDK has no mock context), so the prologue's decisions
are functions of their inputs with unit tests (`service::prologue`; the resolution is a pure function of the
resolver's answer, the pre-amendment `warn` on a scoped request resolving to an account without a `supplier_id`, #43,
went with the pin, and the two deadlines run under a paused tokio clock: a resolver that never answers is the
retryable error at exactly `CALL_DEADLINE`, a store that never answers is the terminal fault after three bounded
attempts, its text naming neither the account nor the reference) and the durable behaviour is asserted end to end
(§11).

### `Szamlazz.Agent` (stateless Service, `#[restate_sdk::service(name = "Szamlazz.Agent")]`)

**Unkeyed, so unserialised.** A stateless service's invocations run concurrently; nothing in the worker orders two
by-number writes on one invoice the way `Szamlazz.Order`'s per-key lock orders everything on one order. Two
`set_payments` with `additive: false` both send their full replacement and the last to land is the invoice's payment
state, under reordered webhook deliveries of one payment stream, the older snapshot; two `storno`s of one invoice
both pass their leading query and both send, and szamlazz.hu's storno being idempotent per original (a repeat echoes
the existing `SS`; verified for a sequential repeat, B4; two sends in the same instant are unverified) answers the
repeat with the existing storno number. **Decision (J4, #115):** a keyed `Szamlazz.Document` Virtual Object per
invoice number (the shape that would serialise these) was judged over-engineering for two writes whose only hazard
is a replace. The remedy is the caller's, stated in both READMEs: serialise
`set_payments` per invoice on its side, or send `additive: true` and let szamlazz.hu sum, which no ordering can
corrupt.

| Handler | Input → Output | Notes |
|---|---|---|
| `check_account` | `()` → `CheckAccountResponse { scope, account: { id }, namespace, credentials }` | the read-only probe for onboarding and deploy pipelines, and the deploy-time canary for the experimental flags: the prologue as every handler, then one step (`probe`); a query of the sentinel external id `{namespace}:check-account`, which nothing the service issues carries (two segments; every issued id has three or more); expecting code 7, under the read policy (§9); `credentials` is `{state: ok}` on any answer but a credential code, `{state: rejected, code, message}` on 3/135/136/164 (**data, not a fault**: reporting it is the probe's purpose); an exchange that produced no answer is the read's `Unanswered`, re-executed by the read policy, and `TerminalError{unavailable}` when it is exhausted. `scope` is what the SDK saw: `null` under a scoped call means the server did not forward the scope (protocol v7 off). Credential acceptance is the only szamlazz.hu-verified fact it returns, `account.id` echoes the configuration; *which* account the key opens, and whether it is a test account, no operation answers and the worker checks nowhere (§3), so the deploy checklist follows the probe with a `query` of a document known to be the account's and reads its `test` and seller block. Issues nothing. Called under each configured scope after a deploy, it proves the scope reaches the worker, resolves to the configured account and its key works. `max_attempts = 3, kill`; `journal_retention = "1d"` (explicit, so the leak assertion can scan it); `inactivity_timeout = 2m, abort_timeout = 2m` (one read; #114) |
| `query` | `QueryRequest { selector }` → `QueryResponse` | one step (`query`) under the read policy, journaling the document as found (the same `QueryOutcome` a verify writes: the worker's `FoundDocument` projection, never the agent crate's `InvoiceDocument`; #127); the `QueryResponse` projection of it, `test` as szamlazz.hu reported it (compared with nothing, what the go-live check reads off a known document; `null` when the document carries no `teszt`, which the schema forbids, never an invented `false`, #70); 7 → `TerminalError{not_found}` (404); another code → `TerminalError{szamlazz_error}` (422), the code in `szamlazz_code`; 3/135/136/164 → `credentials_rejected`; a query szamlazz.hu never answered through the read policy → `unavailable`; `max_attempts = 3, kill`; `journal_retention = "1d"`; `inactivity_timeout = 2m, abort_timeout = 2m` (one read; #114) |
| `query_taxpayer` | `QueryTaxpayerRequest { tax_number }` → `QueryTaxpayerResponse { valid, name?, tax_number?, vat_code?, addresses[] }` | the NAV taxpayer lookup (`xmltaxpayer`) on the account the scope resolves to, so an embedder needs no second credential path for this one read. `tax_number` is the bare eight-digit stem (`12345678`) or the full `NNNNNNNN-N-NN` form (`12345678-2-42`), nothing else, no whitespace, no other separator; the handler derives the prefix, and a number in neither form is `TerminalError{invalid_input}` naming the input and the accepted forms, refused **before the prologue** like a malformed body (nothing journaled, nothing sent). Then the prologue as every handler and one step, `taxpayer-{prefix}` (the prefix, not the number as sent, so the stem and the full number name the same entry) under the read policy (§9), journaling the crate-owned projection (`TaxpayerOutcome::Found(QueryTaxpayerResponse)`), never the agent crate's `TaxpayerInfo`. **Every answer is data**: `valid: false` (NAV knows no taxpayer under the prefix) is a normal 200; 3/135/136/164 → `credentials_rejected`; any other `funcCode ≠ OK` (szamlazz.hu's own code or NAV's relayed `errorCode`) is an *answer*, passed through as `TerminalError{szamlazz_error}` (422, the code in `szamlazz_code`) like `query`'s and never retried, so a NAV outage surfaces as a terminal 422 the caller may retry with a new `Idempotency-Key`; an exchange that produced no answer is the read's `Unanswered`, re-executed, `unavailable` on exhaustion. **No account check**: with `set_payments` one of the two handlers exempt from it: it finds no document, and a taxpayer record is NAV's, not the account's, so it carries no pins. No caching in the worker (ADR 0005: nothing to store that szamlazz.hu does not answer); the caller caches, with a TTL on the order of a day. `max_attempts = 3, kill`; `journal_retention = "1d"`; `inactivity_timeout = 2m, abort_timeout = 2m` (one read; #114) |
| `set_payments` | `SetPaymentsRequest { invoice_number, entries[≤5], additive }` → `SetPaymentsResponse` | `RegisterCreditEntry` without a preceding query: a verify round trip (about a second per credit entry) would establish nothing the send does not, and a credit entry is not a legal document; run `max_attempts(1)`; a write with no retry of its own, so a lost reply is `outcome_unknown`; a sixth entry never reaches szamlazz.hu (the wire contract takes five) and is `TerminalError{invalid_input}`, the caller's request; szamlazz.hu refusing the entries → `TerminalError{szamlazz_error}` (422), the code in `szamlazz_code`; 3/135/136/164 → `credentials_rejected`. **`additive: true` is at-least-once**: every send that reaches szamlazz.hu appends the entries, and the handler cannot tell a lost reply from a lost request, so the `outcome_unknown` message is conditional on `additive` ("query the invoice before re-sending" rather than "call set_payments again"), and the handler's retry policy is explicit, `initial_interval = 2m, max_attempts = 2, kill`: the one retry after a crash waits out the 60 s client timeout (never the server's ~500 ms default) so that it cannot re-send while the first send is still in flight. `inactivity_timeout = 2m, abort_timeout = 2m` (one send). Not serialised per invoice (unkeyed, above) |
| `storno` | `StornoRequest` → `StornoResponse` | verify first (`verify-{number}`, under the read policy; 7 → `TerminalError{not_found}` (404) naming the invoice); then document carries `rendelesszam` → `outcome: managed_by_order{key}` (an `Order`'s document); `sztornozott` → `outcome: reversed{storno_number?}`; the storno number from the by-number storno lookup (`lookup-storno-{number}`, under the read policy: the `SS` under `"{namespace}:by-number:{number}:storno"` when the storno was ours, unknown when nothing is under the id, a reversal from the UI, or another code answered), **best effort** like `Szamlazz.Order`'s hint: an exhausted read reports the reversal without the number after a `warn`, a cancellation propagates (J25, #65); then a document without a `telj` → `TerminalError{unavailable}` naming the invoice, without `order` / `kind` / `external_id` like this handler's other faults (ADR 0007; the storno must repeat that date, so nothing is sent; no document-type pre-check here, the echo tells); else the lookup and storno steps of §6 under ext id `"{namespace}:by-number:{number}:storno"` with `teljesitesDatum` = the original's `telj` (the lookup under the read policy, exhaustion → `unavailable`; the storno step under the issue policy, exhaustion → `outcome_unknown`); 3/135/136/164 → `credentials_rejected`. `Szamlazz.Order`'s policy throughout (`initial_interval = 2m, factor = 2.0, max_interval = 10m, max_attempts = 5, kill` and `inactivity_timeout = 4m, abort_timeout = 3m`), because the storno step is the same closure `Szamlazz.Order` runs (query, send, re-query at 60 s each; ADR 0004): the 2 m interval waits out the 60 s client timeout so the leading query cannot look before a cut send has landed, anything shorter than 4m/3m suspends a slow storno mid-step, and an invocation attempt is spent only on a worker-side failure, so nothing about an unmanaged storno justifies a shorter budget than the managed one's (#87) |

### Durable step names

Every `ctx.run` of both services, per handler path, in the order the handler journals them. **The authority is the
run-name pin**: `RUN_NAMES` in the e2e harness (`tests/e2e/harness/run_names.rs`), verified against a live
`sys_journal` whenever the suite runs (§11; CI, on every pull request), and this table follows it: a step added,
renamed or reordered in the code fails the pin first, and the table is then brought to match, never the other way
round. The names are what the Restate UI shows, what a `sys_invocation.last_failure_related_command_name` names, and
what an `unavailable` fault's message means by "the step". `{kind}` is the document kind the handler issues or reads:
`proforma | invoice | prepayment | final`, and `corrective` on `correct_invoice`'s lookup and create; `{number}` is an
invoice number, the caller's as sent on every step but `delete-proforma-{number}`, where it is the found proforma's
(`delete_proforma` takes no number); `{prefix}` the eight-digit taxpayer prefix. A handler with two shapes has two
rows; a handler that answers early (a conflict, a refusal, `unknown_account`) journals a prefix of its row.

| Service | Handler | Path |
|---|---|---|
| `Szamlazz.Order` | `create_proforma` | `namespace`, `account`, `exclusivity-invoice`, `exclusivity-prepayment`, `exclusivity-final`, `lookup-proforma`, `create-proforma` |
| `Szamlazz.Order` | `create_invoice` (`proforma: auto | none`) | `namespace`, `account`, `exclusivity-prepayment`, `exclusivity-final`, `proforma-link`, `lookup-invoice`, `create-invoice` |
| `Szamlazz.Order` | `create_invoice` (`proforma: {number}`) | `namespace`, `account`, `exclusivity-prepayment`, `exclusivity-final`, `verify-proforma-{number}`, `lookup-invoice`, `create-invoice` |
| `Szamlazz.Order` | `create_prepayment` (`proforma: auto | none`) | `namespace`, `account`, `exclusivity-invoice`, `exclusivity-final`, `proforma-link`, `lookup-prepayment`, `create-prepayment` |
| `Szamlazz.Order` | `create_prepayment` (`proforma: {number}`) | `namespace`, `account`, `exclusivity-invoice`, `exclusivity-final`, `verify-proforma-{number}`, `lookup-prepayment`, `create-prepayment` |
| `Szamlazz.Order` | `create_final` | `namespace`, `account`, `prepayment-for-final`, `lookup-final`, `create-final` |
| `Szamlazz.Order` | `correct_invoice` | `namespace`, `account`, `verify-base-{number}`, `lookup-corrective`, `create-corrective` |
| `Szamlazz.Order` | `storno_invoice` (a live original) | `namespace`, `account`, `verify-storno-{number}`, `lookup-storno-{number}`, `storno-{number}` |
| `Szamlazz.Order` | `storno_invoice` (the verify sees it reversed) | `namespace`, `account`, `verify-storno-{number}`, `hint-storno-{number}` |
| `Szamlazz.Order` | `delete_proforma` | `namespace`, `account`, `proforma-for-delete`, `delete-proforma-{number}` |
| `Szamlazz.Order` | `get` | `namespace`, `account`, `get-proforma`, `get-invoice`, `get-prepayment`, `get-final` |
| `Szamlazz.Agent` | `check_account` | `namespace`, `account`, `probe` |
| `Szamlazz.Agent` | `query` | `namespace`, `account`, `query` |
| `Szamlazz.Agent` | `query_taxpayer` | `namespace`, `account`, `taxpayer-{prefix}` |
| `Szamlazz.Agent` | `set_payments` | `namespace`, `account`, `set-payments-{number}` |
| `Szamlazz.Agent` | `storno` | `namespace`, `account`, `verify-{number}`, `lookup-storno-{number}`, `storno-{number}` |

Which policy runs each: `namespace` is pure (`max_attempts(1)`); `account` runs under the resolve policy; every
`exclusivity-*`, `prepayment-for-final`, `proforma-link`, `verify-*`, `lookup-*`, `hint-storno-*`, `proforma-for-delete`,
`get-*`, `probe`, `query` and `taxpayer-*` step is a read under the read policy (§9); `create-*` and `storno-*` run
under the issue policy; `delete-proforma-*` and `set-payments-*` are one-shot writes (`max_attempts(1)`) whose lost
reply is `outcome_unknown`. Two reads do not fault on exhaustion, the best-effort storno-number reads after a verify
that saw the document reversed: `hint-storno-{number}` (§6 step 1) and `Szamlazz.Agent.storno`'s
`lookup-storno-{number}` on that path (§4, its row; the same entry name the storno protocol's lookup step writes,
which the reversed path never reaches). The parametrized names are read by their prefix (`verify-storno-…` is
`verify-storno-{number}`, never `verify-{number}`), so the pin would misread only an invoice number that itself began
with a pinned stem (`verify-storno-1` for a `Szamlazz.Agent.storno` of `storno-1`); none of the suite's do, and the
misread would be the test's, not the worker's: the names themselves are unambiguous to the journal.

## 5. Create protocol (`create_invoice`; other kinds analogous)

The reads of steps 1–3 are `ctx.run`s under the **read policy** (§9): every szamlazz.hu *answer* is data, and a query
szamlazz.hu did not answer (a transport or parse failure, `szlahu_down`) is the closure's retryable error
`Unanswered`, re-executed by the policy and `TerminalError{unavailable}` when it is exhausted (a read writes nothing,
so a re-executed closure's answer is exactly as fresh as a first one). The create step is the `ctx.run` under the
**issue policy** (§9) because it is the one step whose outcome can be *unknown*. The storno step (§6) is the other
write step and runs under the same policy. Every step runs on the account the prologue resolved (§4), through the
gateway opened for this execution.

0. **Validate (pure).** Order key from `ctx.key()`. Validate buyer, items (≥ 1), dates. Normalise `buyer.name`.
   Compute line totals with `LineItem::try_calculated` rounded to the currency's minor unit
   (`Rounding::minor_unit`: whole forints for HUF, cents for EUR); a value that overflows is `invalid_input`. Build
   `CreateInvoice` from input + the account's
   defaults and seller block (read through the gateway) + per-call overrides,
   `external_id = "{namespace}:{order}:invoice"`, `download_pdf = false`.
1. **Exclusivity.** `ctx.run(query "{namespace}:{order}:prepayment")`: live `ES` → `conflict{prepaid_chain, existing_number}`;
   then `ctx.run(query "{namespace}:{order}:final")`: live `VS` → the same `conflict{prepaid_chain, existing_number}`.
   (`create_prepayment` mirrors this against `…:invoice` and `…:final`; `create_proforma` runs it against **all three**
   of `…:invoice`, `…:prepayment` and `…:final`, a live one → `conflict{order_invoiced, existing_number}`: a proforma
   after the invoice makes no sense, and without these lookups the order-number hint of step 3 would report the
   order's own invoice as `foreign`, which claims another channel issued it.) The final invoice has its own row
   because it is the prepayment chain's settled end and outlives its `ES`: after `ES` → `VS` → storno of the `ES`,
   `…:prepayment` is reversed, `…:invoice` is absent and the newest document under the order is the `ES`'s `SS` (not
   invoice-family, so the hint says absent), and szamlazz.hu's repetition toggle is per kind (verified), so without
   the row a plain `SZ` (or a second `ES` under `reissue`) landed beside the live `VS` (#62). A reversed `VS` refuses
   nothing.
   Another szamlazz.hu code (`Api`) → `TerminalError{unavailable}`
   (an answer nothing can be concluded from); no answer → `Unanswered`, retried by the read policy. A document under
   the secondary id that fails validation → `conflict{external_id_collision, number}`: the query returns the newest
   holder, so a foreign document may hide a live document of ours behind it, and refusing is the only safe answer.
2. **Proforma link** (`options.proforma`; the kinds that convert a proforma, `create_invoice` and `create_prepayment` (#69),
   see kind specifics):
   - `auto` (default): `ctx.run(query "{namespace}:{order}:proforma")` → live `D` → pass `dijbekeroSzamlaszam`; 7 → none.
   - `none`: same query; live `D` → `conflict{proforma_live, existing_number}` (the server links by shared order
     number regardless, verified, so refusing is the only honest answer).
   - `{number}`: `ctx.run(verify number)`; 7 → `conflict{proforma_missing}`; then the found document is checked like
     every other document found by number (§3), in the order the other verifies use: `rendelesszam ≠ order` (or
     absent) → `conflict{not_managed, existing_number}`, another order's live proforma cannot be linked into this
     order's invoice, nothing sent and the verify the last step journaled; `tipus ≠ D` → `invalid_input`.
   Under `auto` and `none`, a document under `…:proforma` that fails validation → `conflict{external_id_collision,
   number}` (as in step 1).
   Collect the numbers seen in steps 1–2 as `our_numbers` (for foreign detection).
3. **Lookup**: one read-only durable step under the read policy, `ctx.run("lookup-{kind}", || gateway.lookup(LookupRequest{
   external_id, kind, order, our_numbers }))` → `Result<LookupOutcome, Unanswered>`. The gateway validates every found
   document against the order and kind it should have; the request carries only what identifies the document.
   In one closure:
   - `QueryInvoiceXml(ExternalId)` → `Ok(doc)`: validate → `Collision(doc)` on mismatch; live → `Live(doc)` (no
     hint: nothing will be created); reversed (`sztornozott == Some(true)`) → remember it and continue. 7 → continue.
     3/135/136/164 → `CredentialsRejected{code, message}`; another code → `Api{code, message}`. No answer →
     `Err(Unanswered)`.
   - The order-number hint, on every lookup **except for correctives**: `QueryInvoiceXml(OrderNumber)` → a live
     `SZ|ES|VS` whose number ∉ `our_numbers` and ≠ the document seen under our ext id → `Foreign(doc)`, also when
     our own document under the id is reversed, since no create may proceed past it; a miss (7) or another API error
     says nothing about foreign documents and continues; 3/135/136/164 → `CredentialsRejected` (conclusive: nothing
     proceeds); a hint szamlazz.hu did not answer → `Err(Unanswered)` (nothing may be concluded; the whole step is
     re-executed).
   - Otherwise `Absent`, or `Reversed{doc, storno_number?}` with the storno number when the hint is the `SS` whose
     `hivszamlaszam` is the reversed document (absent otherwise, and for correctives).
   It settles every case that needs no create: `Live` → `reissue ? conflict{live, number} : already_issued{number,
   totals}`; `Reversed` → `reissue ? proceed, remembering the number : outcome: reversed{number, storno_number?}`;
   `Collision` → `conflict{external_id_collision, number}`; `Foreign` → `conflict{foreign, existing_number}`;
   `Api` → `TerminalError{unavailable}`; `CredentialsRejected` → `TerminalError{credentials_rejected}` (§7);
   `Absent` → proceed. The run's own `Err`, the read policy exhausted (500, the last `Unanswered`) or a cancel (409),
   is `TerminalError{unavailable, json{order, kind, external_id}}` naming the step and the last failure.
4. **Create**: one durable step under the issue policy, `ctx.run("create-{kind}", || gateway.create(CreateStepRequest{
   external_id, kind, order, create, reversed }))` with `RunRetryPolicy::new().initial_delay(2m)
   .exponentiation_factor(2.0).max_delay(10m).max_attempts(5).max_duration(1h)` (§9). **Every execution is
   query-first, inside the closure**, a separate journaled pre-query would replay its stale "nothing" on the retry
   and re-send. The gateway returns settled-vs-unconfirmed: every szamlazz.hu answer is `Ok(CreateOutcome)`, and
   `Err(Unconfirmed)` (a plain `std::error::Error`, retryable to the SDK) is the one thing the policy re-executes. The
   closure never returns a `TerminalError` itself.
   - Leading query `QueryInvoiceXml(ExternalId)` → a validated live document that is not `reversed` → `Found(doc)`
     (an earlier execution created it; **nothing is sent**); a validated **reversed** document that is not `reversed`
     → `Reversed(doc)` (issued by an earlier execution and reversed since: a reversal the caller has not
     acknowledged; **nothing is sent**, the handler answers `outcome: reversed`); `reversed` itself reported live →
     `LiveAgain(doc)` (a server inconsistency; **nothing is sent**, answered `conflict{live}`); invalid →
     `Collision(doc)`; 7 or `reversed` still reversed → send; 3/135/136/164 → `Ok(CredentialsRejected{code,
     message})` (settled, nothing sent); **another code → `Ok(Api{code, message})` and `szlahu_down` →
     `Ok(Unavailable{message})`**; answers, settled with nothing sent (#63): the handler raises
     `TerminalError{unavailable}` at once, for a code, the same fault the lookup step raises for it; for
     `szlahu_down`, without the read policy's retries the lookup step gives it, since the issue policy this step runs
     under is sized for the post-send window and would otherwise be spent on a read, ending `outcome_unknown` ~39
     minutes later although nothing was ever sent; only a transport failure of the leading query → `Err(Transport)`
     (never create when the check itself failed, an exchange without an answer). **The rule (ADR 0003, #36): the step sends only when the
     external id holds nothing, or exactly the document the lookup step saw reversed.**
   - `CreateInvoice` → success with a number → `Issued(r)`; an API rejection → `Rejected{code, message}`; 3/135/136/164
     → `CredentialsRejected{code, message}`: settled data, **not** `Unconfirmed`: re-executing with the same key would
     only repeat the answer, so the run policy is not spent on it.
   - Transport failure, an open code (1, 55, 56 without a number, a code the agent crate does not know,
     `szamlazz_agent::OutcomeClass::Unknown`, because it may be a refusal or a new "issued, but…" code like 55/56,
     and `rejected` would assert that no document exists (#13), or a success without a document number) or
     `szlahu_down`: re-query the external id once,
     immediately (read-your-writes lag ≈ 0) → found live → `Found(doc)`; found reversed (reversed between the send
     and the re-query) → `Reversed(doc)`; collision → `Collision`; nothing → `Err(Transport | Open | Unavailable)`;
     **the re-query itself failed** (lost reply, another code, `szlahu_down`) → `Err(ReQueryFailed{sent, re_query})`,
     naming how the send ended *and* how the re-query failed, the re-query's failure never hides that a send
     happened (#63). Each variant's display names its own cause: `Open` without a code is the success without a
     document number, never `szlahu_down` (#63). The run policy then re-executes the whole handler after the delay:
     the journal replays to the create step and the leading query runs again, the re-check ADR 0002 sizes the
     2-minute gap for.
   - 71/152: re-query the external id → live and ours → `Reconciled(doc)`; not ours → `Collision(doc)`; reversed and
     ours but not `reversed` → `Reversed(doc)`; `reversed` still reversed, or absent → the duplicate is not ours; the
     re-query itself failed → `Err(ReQueryFailed{sent: "duplicate order number …", re_query})` (whether the duplicate
     is ours is what it was to settle). For correctives that is `Rejected{code, message}` (exempt from the
     order-number check, verified; no order-number query). Otherwise `QueryInvoiceXml(OrderNumber)` names it:
     the newest document under the order is a live document of our kind → `DuplicateOrderNumber{code, message,
     existing_number}`, another kind or reversed → without `existing_number`, a failed naming query → without it;
     7 (nothing under the order, yet 71/152) → a contradiction, logged at `warn` and **settled** without
     `existing_number` all the same: the refusal is an answer szamlazz.hu already gave, and re-sending the create
     for it (up to five times, as the pre-#41 `Err(Contradiction)` did) would only repeat it.
   Any `Err` from the run, exhaustion (`TerminalError` 500 carrying the last `Unconfirmed`) or cancellation (409),
   is mapped by the handler to `TerminalError{outcome_unknown, json{order, kind, external_id}}`; a cancel
   mid-create therefore reports `outcome_unknown`. Nothing is recorded: the next invocation's lookup finds whatever
   landed.
5. **Branch on data.** `Issued(r)` → `outcome: issued` (+ `warnings: [notification_delivery_failed]`); `Found(doc)`
   → `outcome: issued{number, totals}` (the caller asked for this document and has it; ADR 0003); `Reversed(doc)` →
   `outcome: reversed{number}` (no storno number: the next call's lookup reports it); `LiveAgain(doc)` →
   `conflict{live, number}`; `Reconciled(doc)`
   → `reconciled{number, totals}`; `Collision(doc)` → `conflict{external_id_collision, number}`;
   `DuplicateOrderNumber` → `conflict{duplicate_order_number, code, message, existing_number?}`; `Rejected` →
   `rejected{code, message}`; `Api{code, message}` → `TerminalError{unavailable, 503, szamlazz_code, json{order,
   kind, external_id}}` and `Unavailable{message}` → `TerminalError{unavailable, 503, json{order, kind, external_id}}`
   without a `szamlazz_code` (`szlahu_down` is a header, not a code); the leading query's answers, nothing sent (#63);
   `CredentialsRejected{code}` → `TerminalError{credentials_rejected, 503, json{order, kind,
   external_id}}` and a warning tagged with the namespace and the code (§7); the request that drew the code was not
   acted on, but the code may have come to a post-send re-query and an earlier execution may have landed with a lost
   reply, which is why this is a fault and never `rejected`; its message says the outcome is not known, never that
   "this attempt issued nothing" (#63).
6. **Crash path.** A crash mid-closure leaves no journal entry; Restate re-dispatches after the handler's
   `initial_interval` (2 m > 60 s client timeout + observed stalls) with the journal; completed runs replay; the open
   `create-…` closure re-executes and begins with the external-id query, so a landed create is `Found`, not
   re-issued. Second guard: with the toggle ON, a byte-identical resend while the first document is live is answered
   with the same number.

Kind specifics: `create_proforma`, kind `D`; exclusivity against `…:invoice`, `…:prepayment` and `…:final` (a live
one → `conflict{order_invoiced, existing_number}`, never `foreign`); `proforma` option not applicable.
`create_prepayment`: exclusivity against `…:invoice` and `…:final`; **step 2 as for `create_invoice`** (#69): the
Agent's prepayment invoice carries `dijbekeroSzamlaszam` (the XSD lists it beside `elolegszamla` as an independent
element), so the live `D` of ours is linked explicitly under `auto`, `none` is `conflict{proforma_live}`, the
server converts the order's live `D` by shared order number regardless (verified, an `ES` issued without the
reference shows `hivdijbekszam`), and `{number}` is verified like every found document; `get` derives `consumed`
from the `ES`. `create_final`: no exclusivity row of
its own (a live `SZ` cannot coexist with the live `ES` it requires); `ctx.run(query "…:prepayment")` must be a live
`ES` (7 → `conflict{prepayment_missing}`, reversed → `conflict{prepayment_reversed}`, fails validation →
`conflict{external_id_collision}`); passes `elolegSzamlaszam`; the server enforces one final per prepayment (73 →
`rejected`); the server does not net the prepayment into the final's totals, the caller's document deducts it as a
negative line (behaviour note C6-2); `proforma` option not applicable (anything but `auto` → `invalid_input`): the
Agent's final invoice can carry `dijbekeroSzamlaszam` too, but the order's `D` was consumed by the `ES` and
`create_proforma` is refused once the `ES` is live, so there is nothing for the final to link.
`correct_invoice`: `ctx.run(verify invoice_number)` under the read policy: 7 → `TerminalError{not_found}`, reversed → `conflict{base_reversed}`,
`rendelesszam ≠ key` → `conflict{not_managed}`; ext id `…:corrective:{correction_id}`; the same lookup and create
steps with the corrective exemption (verified): no order-number hint (the live base invoice under the order is
expected), and a 71/152 the re-query cannot resolve is `rejected`, not a conflict; a new `correction_id` issues a new
corrective by contract.

## 6. Storno protocol (`Szamlazz.Order.storno_invoice`)

Storno is natively idempotent on the server (a repeat echoes the existing storno, verified) and an external id on
the storno request attaches to the storno document (verified). It has the shape of issuing (§5): a read-only lookup
step and one write step under the issue policy, query-first on every execution, on the account the prologue
resolved for this invocation; a storno request under the wrong scope finds nothing under the number (what szamlazz.hu
answers when one account names another's invoice number is unverified, behaviour notes).

1. **Verify.** `ctx.run(verify number)` under the read policy: 7 → `TerminalError{not_found}` (404, with the order, kind and storno external id attached); `rendelesszam ≠ key` → `conflict{not_managed}` (use
   `Szamlazz.Agent.storno`); `sztornozott ==
   Some(true)` → `outcome: reversed{storno_number?}` (idempotent; storno number via the hint when the newest document
   is the matching `SS`; best effort: a hint the read policy could not get answered reports the reversal without the
   number, after a `warn` naming the step, rather than failing a handler whose answer is already known; a
   **cancellation** of the hint is never swallowed; the SDK's 409 propagates, so a cancelled invocation does not
   complete as `reversed` (J13, #65; `support::best_effort`)); `tipus ∉ {SZ, ES, VS, HS}` →
   `rejected{not_stornoable}`. **Then, last**, a document without a `telj` → `TerminalError{unavailable,
   json{order, kind, external_id}}` naming the invoice (ADR 0007): the storno must repeat that date and no default
   can be right (szamlazz.hu's query schema has `telj` mandatory, so an absent one is szamlazz.hu breaking its
   schema, the same class as an `Api` answer a read cannot conclude from), and nothing is sent. It comes after every
   answer above so that a `telj`-less document that is already reversed, not managed or not stornoable still gets
   that answer.
2. **Lookup**: one read-only durable step under the read policy, `ctx.run("lookup-storno-{number}", ||
   gateway.lookup_storno(external_id, number))` with `external_id = "{namespace}:{order}:storno:{number}"`: query by the
   storno ext id → `SS` with `hivszamlaszam == number` → `AlreadyReversed{storno_number}` → `outcome:
   reversed{storno_number}`; 7 or another holder → `Absent` → proceed (a storno is idempotent server-side, so a stray
   holder is not a stop); 3/135/136/164 → `TerminalError{credentials_rejected}`; another code (`Api`) →
   `TerminalError{unavailable}`; no answer → `Err(Unanswered)`, retried by the read policy, exhaustion →
   `TerminalError{unavailable, json{order, kind, external_id}}`.
3. **Storno**, one durable step under the issue policy (§9), `ctx.run("storno-{number}", || gateway.storno(StornoStepRequest{
   number, external_id, comment, e_invoice, fulfillment_date }))`, built from the *storno intent*, `{ number,
   storno_id, comment?, e_invoice, fulfillment_date }`, one pure function of the verified document and the resolved
   account (`StornoIntent::from_verified`) that both storno handlers share; the step rebuilds the request from it
   on every execution, so every send is byte-identical, and nothing beyond the verify's result is journaled.
   **Every execution is query-first, inside the closure** (the rule of §5
   step 4): the gateway returns `Ok(StornoOutcome)` for every known answer and `Err(Unconfirmed)` (retryable to the
   SDK) only when szamlazz.hu's answer is not known; the closure never returns a `TerminalError` itself.
   (a) leading query by the storno ext id → the matching `SS` → `AlreadyReversed{storno_number}` (an earlier
   execution sent it; **nothing is sent**); 3/135/136/164 → `Ok(CredentialsRejected)`; another code →
   `Ok(Api{code, message})` and `szlahu_down` → `Ok(Unavailable{message})`: answers, settled with nothing sent, the
   twins of §5 step 4's (#63); only a transport failure → `Err(Transport)` (never send when the check itself
   failed);
   (b) send `xmlszamlast{szamlaszam, szamlaKulsoAzon, teljesitesDatum}`; `teljesitesDatum` = the verified
   original's `telj`, which NAV requires the storno to repeat (ADR 0007; szamlazz.hu defaults to it when the element
   is omitted and accepts any date silently when it is not, verified, so the explicit date is what fails loudly),
   **without `keltDatum`** (352 otherwise; verified);
   (c) validate: `invoice_number ≠ requested ∧ gross ≤ 0` → `Reversed` (`CreatedInvoice::reverses`; `≤`, not `<`:
   the storno of a 0-HUF invoice, a free ticket, lands as a new number with a gross of `0`, and `< 0` reported it
   `not_stornoable` although the storno document had landed); echo of the requested number →
   `NotStornoable`; API errors → `Rejected{code, message}` with the raw szamlazz.hu code (`14` = storno of a storno,
   `221` = has a corrective; typed in `szamlazz_agent::ErrorCode`, surfaced as the code string); 3/135/136/164 →
    `CredentialsRejected{code, message}`; a transport failure, an open code (1, 55, 56, a code the agent crate
    does not know) or `szlahu_down` → re-query
   the storno ext id once, immediately → the matching `SS` → `AlreadyReversed{storno_number}` (what was sent landed);
   nothing → `Err(Transport | Open | Unavailable)`; the re-query itself failed → `Err(ReQueryFailed{sent, re_query})`
   naming both (#63); and the run policy re-executes the step after its delay, beginning again at (a).
   Any `Err` from the run, exhaustion (500) or cancellation (409), is mapped to `TerminalError{outcome_unknown,
   json{order, kind, external_id}}`; nothing is recorded, the next call's steps 1–2 find the storno if it landed.
4. **Branch on data.** `Reversed | AlreadyReversed` → `outcome: reversed{storno_number}`; `NotStornoable` →
   `rejected{not_stornoable}`; `Rejected` → `outcome: rejected{code, message}`; `Api{code, message}` →
   `TerminalError{unavailable, szamlazz_code}` and `Unavailable{message}` → `TerminalError{unavailable}` (the leading
   query's answers, nothing sent; #63); `CredentialsRejected` →
   `TerminalError{credentials_rejected}` (the request that drew the code was not acted on; the outcome is not known).

`e_invoice` for the storno: the verified document's `eszamla` when known (`1` paper → `false`, `2`/`3` e-invoice →
`true`; the vendor annotation, confirmed on the test account, #73), else the account default, an open code set
for which the account's own default is a legitimate choice. The derivation is the only guard: szamlazz.hu does not
require a storno's `eszamla` to match its original's, a mismatch is accepted silently and the storno document takes
the *request's* flag (P73), so a caller-supplied or default flag would put a paper storno on an e-invoice without a
word from the server. `fulfillment_date` has no such fallback: it is a fiscal
fact of the document (ADR 0007). No post-storno read verifies the `SS`'s `telj` (the storno document is immutable and
cannot be stornoed, so a mismatch found afterwards is un-actionable); the go-live checklist verifies it once per
account.

`Szamlazz.Agent.storno` runs the same lookup and storno steps under `"{namespace}:by-number:{number}:storno"` after its
own verify (§4): an order-bearing document is answered `managed_by_order` off the verified document, compared with
nothing (the worker holds no account pin; ADR 0006, account-pin amendment), and builds the same storno intent from
what it found, so its storno carries the original's `telj` too and a `telj`-less original is the same `unavailable`,
without an order identity. **It checks no document type before sending** (J9: deliberate, stated in the code beside
the intent: "the echo tells"): `Szamlazz.Order.storno_invoice` refuses `tipus ∉ {SZ, ES, VS, HS}` up front because it
knows what the order issued, while an unmanaged document is whatever the caller named, and szamlazz.hu's echo of the
requested number on a proforma or delivery note is the verified, success-shaped answer that becomes
`rejected{not_stornoable}` at no cost but one send that changes nothing. The one consequence: a `telj`-less proforma or
delivery note reaching it is `unavailable` rather than `rejected{not_stornoable}`, accepted, twice theoretical (ADR
0007).

`delete_proforma({force})`: `ctx.run(query "…:proforma")` under the read policy: 7 → `{deleted: true, reason: absent}` (deleted or consumed,
`get` tells which); a document under our id that fails validation → `{deleted: false, reason: external_id_collision}`;
live `D` with payments ∧ `!force` → `{deleted: false, reason: proforma_paid}` (the server has no guard, verified);
`ctx.run(DeleteProforma{InvoiceNumber})` (`max_attempts(1)`): success | 335 → `{deleted: true}`; 3/135/136/164 →
`TerminalError{credentials_rejected}`; other → `{deleted: false, reason: <code>}`; `Transport` → `TerminalError{outcome_unknown}`.
The response is `DeleteProformaResponse { deleted, reason? }` throughout, never a `rejected` outcome.

`get`: four `ctx.run` queries under the read policy (`get-{kind}`, `…:proforma|invoice|prepayment|final`) → `OrderStatus { proforma?, invoice?,
prepayment?, final?: DocumentStatus }` where `DocumentStatus { number, state: live | reversed | consumed{by},
gross, net, payments: [amounts], referenced_proforma?, e_invoice? }`. `get` does not look up the storno number (that
would need the order-number hint, which only shows the newest document); the create and storno handlers report it
when the hint yields it. A proforma that is absent while the invoice references it (`hivdijbekszam`) is reported as
`proforma: { state: consumed, by: invoice_number }`. A document under an id that fails validation leaves its slot
absent: a read must not fail on an answer; the issuing handlers are the ones that refuse it as
`conflict{external_id_collision}`. A query szamlazz.hu never answered through the read policy → `TerminalError{unavailable}`;
another code on any query → `TerminalError{unavailable}`; 3/135/136/164 on any query → `TerminalError{credentials_rejected}`.

## 7. Outcome contract

Domain outcomes are **data** (HTTP 200 through the ingress, typed in the OpenAPI export). `TerminalError` is reserved
for faults: the eight codes of `TerminalCode` below, and nothing else, every fault either service raises carries one
of them in `code`, with the status `TerminalCode::status` pins; a szamlazz.hu code never travels in `code` but in the
fault's own `szamlazz_code` field, present on every fault a szamlazz.hu answer caused (`szamlazz_error`,
`credentials_rejected`, `unavailable` on an inconclusive code). Three of the codes mean "outcome unknown"
(`outcome_unknown`, `unavailable`, `credentials_rejected`); the rest are settled: the same request never succeeds
(`invalid_input`, `unknown_account`, `not_found`) or szamlazz.hu's answer is passed through
(`szamlazz_error`) (§4). No response names the account: `external_id` (with its namespace) is the only deployment
marker a response carries, and `order_key` in a `StornoResponse` (`managed_by_order{key}`) is meaningful only under
the scope the call was made under.

```
CreateResponse { outcome, conflict_reason?, kind, external_id,
                 invoice_number?, storno_number?, net_total?, gross_total?, outstanding?, customer_account_url?,
                 existing_number?, code?, message?, warnings: [] }
outcome ∈ issued | already_issued | reconciled | reversed | rejected | conflict
conflict_reason ∈ prepaid_chain | order_invoiced | live | foreign | duplicate_order_number | external_id_collision
                | proforma_live | proforma_missing | prepayment_missing | prepayment_reversed | base_reversed | not_managed
warnings ∈ notification_delivery_failed
StornoResponse { outcome ∈ reversed | rejected | conflict | managed_by_order, conflict_reason?,
                 invoice_number, storno_number?, order_key?, code?, message? }
DeleteProformaResponse { deleted, reason? }
OrderStatus, see §6
Fault { code, message, szamlazz_code?, order?, kind?, external_id? }   (contract::Fault, the TerminalError body)
TerminalError codes: invalid_input (400) | unknown_account (400) | not_found (404) | szamlazz_error (422)
                   | outcome_unknown (500) | unavailable (503) | credentials_rejected (503)
On the wire (Restate 1.7.8 ingress), a TerminalError is the JSON *string* in `message` of Restate's own envelope:
  { "code": <HTTP status>, "message": "<the Fault JSON above>", "source": "invocation" }
  + header x-restate-error-source: invocation
```

The envelope is Restate's, not ours: the Rust SDK 0.12 carries a terminal error as `(code, message)` and offers no
other channel, so the worker serialises the fault into the message and the caller parses `message` a second time
(`From<Fault> for TerminalError` in `service::support`). The fault body is a public contract type,
`contract::Fault` (`Serialize + Deserialize`, open like every response type, `#[non_exhaustive]`, built with
`Fault::new` and its setters; the service-side constructors, `Fault::not_found`, `Fault::credentials_rejected`, …, are
a crate-private inherent impl in `service::support`), so a Rust caller decodes `message` into it rather than
re-declaring the shape: the e2e harness (`Reply::fault`) and the endpoint crate's `tests/readme.rs` both did until
#128. A caller reading the envelope's `code` sees the HTTP status, never the token. The endpoint README (*Faults*)
shows one body per case, a structured fault, a killed invocation (the same envelope with the last retryable error's
text in `message`), an ingress error (`source: ingress`), held to the contract types by `tests/readme.rs` (a fault
example must re-serialise from `Fault` to exactly what it shows); the e2e harness asserts the envelope on every fault
it receives.

`invalid_input`: the caller's request, which the same request never gets past, a 400 and "fix the request". Three
sources. A **malformed body**: every request type and every object it nests (`CreateRequest`, `CreateOptions`,
`DocumentInput`, `BuyerInput`, `PostalAddressInput`, `LineItemInput`, `DocumentOverrides`, `ExchangeRateInput`,
`CorrectRequest`, `StornoRequest`, `DeleteProformaRequest`, `QueryRequest`, `QueryTaxpayerRequest`,
`SetPaymentsRequest`, `PaymentEntry`) is
closed; `#[serde(deny_unknown_fields)]`, `additionalProperties: false` in the discovery schema, so a field the
contract does not know is refused, never dropped: `{"options": {"resissue": true}}` is not `reissue: false` (which
would answer `reversed` on a document the caller asked to reissue), `{"aditive": true}` is not `additive: false`
(which would *replace* the invoice's credit entries), `{"froce": true}` is not `force: false`, a misspelt
`buyer.tax_number` is not an invoice without the tax number. Response types stay open: a client must tolerate fields
added later. The body is decoded **by the handler, not the SDK**: every handler with an input takes it as
`service::Body<T>`, whose SDK `Deserialize` never fails (it keeps the verdict), and whose discovery metadata is
exactly `Json<T>`'s, so the schemas do not change with the wrapper; the handler's first act, before the prologue, is
to turn a refused body into this fault. Every malformed body (an unknown field, a wrong type, a missing required
field, invalid JSON) is therefore the same `{ "code": "invalid_input", "message": "malformed request body: …" }`
with serde's message, naming the field when there is one (``unknown field `resissue`, expected `reissue` or `proforma` ``, ``missing
field `entries` ``, `invalid type: string "yes", expected a boolean`), and never the SDK's plain-text `Cannot decode
input payload: …`, which no handler of either service can return. Nothing is journaled and nothing is sent. The
second source is a **well-formed body with a value the handler cannot take before it has anything to journal**: a
`query_taxpayer` tax number in neither accepted form (the bare eight-digit stem or the full `NNNNNNNN-N-NN`), an
untrimmed Virtual Object key (§3), refused at the same point as a malformed body, before the prologue, with the same
consequences (nothing journaled, nothing sent), by the handler's own check rather than serde's. The third source is
a request **the operation cannot take**: `options.proforma` on any kind but `create_invoice`, a `{number}` proforma
link that is not a `D` document, an empty `buyer.name`, an invalid Virtual Object key (§3), a sixth credit entry on
`set_payments` (the Számla Agent wire contract takes five, so the gateway refuses it before anything is sent, the
`request` pseudo-code of its `Rejected` outcome, which the handler maps here rather than to `szamlazz_error`, since
szamlazz.hu answered nothing). These are raised after the prologue, by the handler's own validation or the gateway's.

`not_found`: the request names a document **by number** that szamlazz.hu does not know, code 7 on
`Szamlazz.Agent.query`'s selector, on the invoice of `Szamlazz.Agent.storno` or `Szamlazz.Order.storno_invoice`, on the
base of `correct_invoice`. A 404, settled: nothing was sent and the same request never succeeds; the caller fixes the
number. One vocabulary across both services: `Szamlazz.Order` attaches the order, kind and external id it was working
under, `Szamlazz.Agent` carries none (a by-number fault). Not the answer to a missing proforma named by
`options.proforma: {number}`, which is the create's own outcome `conflict{proforma_missing}` (§5 step 2), nor to
`query_taxpayer` (NAV knowing no taxpayer is `valid: false`, a 200).

`szamlazz_error`: szamlazz.hu answered with an error code of its own that the handler **passes through** rather than
concludes from; `Szamlazz.Agent.query` on a code that is neither 7 nor a credential code, `query_taxpayer` on any
`funcCode ≠ OK` (szamlazz.hu's own or NAV's relayed `errorCode`), `set_payments` on szamlazz.hu refusing the credit
entries. A 422 whose `code` is the symbolic token and whose `szamlazz_code` is szamlazz.hu's (a caller branching on
`code` never meets a numeric code there) with szamlazz.hu's message. An answer, so it is journaled and never retried
by a read policy; settled from the worker's side, although a NAV outage relayed through `query_taxpayer` is one the
caller retries with a new `Idempotency-Key`. The handlers that *can* conclude from a szamlazz.hu code do not use it:
a code on a verify or lookup is `unavailable` (nothing may be concluded, and the step's caller had a write in view),
a code on a create or storno send is the `rejected` outcome.

`unknown_account`: the request names no account of this deployment; it arrived unscoped where accounts are reachable
by scope only, or under a scope no account is reachable by (on a single-account deployment, any scope). Raised by the
prologue's `account` step before anything is issued; the same request never succeeds, so it is a 400 and the caller
fixes the scope rather than retrying. `unavailable` is the answer to an *exhausted read policy* (§9) (szamlazz.hu did
not answer a read through every execution the policy allows, or the invocation was cancelled mid-read) naming the
step and the last failure, about the document when the step knows one; a szamlazz.hu code a read cannot conclude
from (`Api`) is the same fault without a retry, so is the same code, or `szlahu_down`, answered to a write step's
*leading* query (§5 step 4, §6 step 3: settled data, nothing sent, never `Unconfirmed`, #63), and so is a verified
storno original **without a `telj`** (§6 step 1,
ADR 0007: szamlazz.hu breaking its own schema on a date the storno must repeat; nothing is sent; the message names
the invoice, and `Szamlazz.Order.storno_invoice` attaches the order, kind and storno external id). It also covers the
prologue's own faults: the resolve policy
exhausted, the credential store gone or unavailable, reporting so, or silent past the ten-second deadline on each
call (#114), through the in-process retry. One of the three "outcome unknown"
codes: a read that fails may sit before a create that an earlier execution already landed, so the caller retries with a
new `Idempotency-Key` or reads `get`, never concludes that no document exists.

`credentials_rejected`: szamlazz.hu answered 3 (invalid credentials), 135 (browser session active), 136 (login blocked)
or 164 (multiple accounts) to any step of any handler. It is the worker's misconfiguration, not the caller's request
(the same request succeeds once the key is fixed), so it is 503, not a 4xx ("do not retry") or 401/403 ("you are
unauthenticated"). The request that drew the code was not acted on (szamlazz.hu answers these codes before acting
on a request, and on the 71/152 re-query path the create was already refused as a duplicate), but the code may have
come to the re-query after a send with an open code, and an earlier execution may have landed with a lost reply,
which is why it is the third "outcome unknown" code and never `rejected`; its message
says the outcome is not known, never "this attempt issued nothing" (#63). Every
occurrence is logged at `warn` with the namespace and the code (never the key).

## 8. Caller contract (documented in the crate READMEs)

The **endpoint README** is the canonical caller reference, the request and response reference with one example per
outcome, the `conflict_reason` table, the fault envelope, the guidance for calling from a webhook handler and what a
caller stores per order; its
JSON examples and tables are held to the contract types by the endpoint crate's `tests/readme.rs`. The library README
carries the same rules for an embedder. The rules:

1. Send an `Idempotency-Key` per logical request; Restate dedupes retries and attaches concurrent duplicates to the
   in-flight invocation. Deduplication is per scope: the same key under two scopes is two invocations.
2. Tell a **fault** from **no answer** before deciding what to do with the key (ADR 0004, #87). A fault, a 4xx/5xx
   with `x-restate-error-source: invocation` and a body the worker wrote, inside Restate's envelope (§7); is a
   completed invocation whose stored completion is replayed under the same key for the retention period (30 days on
   the write handlers;
   verified): an **`outcome_unknown`, `unavailable` or `credentials_rejected`** fault from an issuing or storno handler
   means "outcome unknown; retry with a **new** key, or read `get`"; the handler reconciles by external id, so the
   retry is safe. Never interpret one of these as "no document exists". No answer (a client timeout, an ingress 5xx
   whose source is not `invocation`) is an invocation still in flight, re-dispatched by Restate for as long as the
   handler's invocation retry policy allows: **keep the key** (a retry with it attaches to the in-flight invocation) or
   read `get`. A killed invocation is a fault whose envelope `message` is the last retryable error's text, not `{code, message}`;
   treat it as `outcome_unknown`. The one exception to "retry with a new key" is `Szamlazz.Agent.set_payments` with
   `additive: true`, which has nothing to reconcile by: every send that reached szamlazz.hu appended the entries, so
   query the invoice before re-sending (the fault's message says so). The other faults are settled (§7): nothing
   landed: `invalid_input`, `unknown_account` and `not_found` are raised before anything is sent, and
   `szamlazz_error` is szamlazz.hu answering with an error (to a read, or refusing the credit entries it was sent).
   Retrying as is repeats the answer: the caller fixes the request, the number, the scope or the account, or, for a
   `szamlazz_error` relaying a NAV outage, retries later with a new key.
3. After a storno (by this service, the UI or anyone) a create returns `outcome: reversed`. Send `reissue: true`
   (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document → `conflict{live}`; the
   flag can never cause a duplicate.
4. On a multi-account deployment, **set the scope on every call** (`/restate/scope/{scope}/call/…`) (it is a path
   segment of the request, not a session), and record the order key *and the scope as used* per order; nothing else is needed to storno,
   correct, reissue or inspect the order later. `order_key` in a storno response is meaningful only under the same
   scope. Never send a tenant or event identifier as the scope: one szamlazz.hu account ⇔ one scope (ADR 0006).
5. A 5xx whose `x-restate-error-source` is `invocation` is this worker's fault (`outcome_unknown`, `unavailable`,
   `credentials_rejected`): page, do not auto-retry into it; then rule 2. Auto-retry a 5xx only when the source is
   `ingress` or absent.
6. From a webhook handler: a client timeout of about 90 s (longer than szamlazz.hu's 60 s request timeout); on
   timeout, re-send with the **same** key or poll `get`, a create can legitimately take minutes while szamlazz.hu is
   flaky (the read and issue policies, §9); or `/restate/send/…` and read the result with `get`. Always acknowledge
   the webhook and own the retry queue: a provider retries with the *same* notification id, which after a fault
   replays the stored failure for the retention period, and any **changed** request needs a new key.

## 9. Configuration (deployment-constant; never in payloads)

Two configuration types, both serde-`Deserialize` only (the host chooses the format). `WorkerConfig` is the
deployment-level part the services hold, the namespace and the three run retry policies; `StaticConfig` is the static resolver's account, and everything
account-shaped (credentials, endpoint, document defaults, seller block) lives on the `Account`
it produces (read by the services through `Gateway::account()`). The endpoint binary reads one file with both side by
side: its own layout type has one explicit field per top-level key and assembles the two library types from them,
so a parse error keeps the key path and the source figment attaches (a `#[serde(flatten)]` would drop both):

```toml
identity_keys = ["publickeyv1_…"]   # the Restate server's request identity public keys (§10); `[]` written out is the local-development opt-out, the key unmentioned a start-up warn
namespace = "acct"            # 1–16 bytes of [a-z0-9-]; prefixes every external id; permanent

[issue]      # the issue policy: the run retry policy of the create (§5 step 4) and storno (§6 step 3) steps; shapes no journal entry
max_attempts = 5              # executions of the step, including the first
initial_delay = "2m"          # before the first re-execution; > client timeout + the longest observed server stall
factor = 2.0
max_delay = "10m"
max_duration = "1h"           # the hard bound (the attempt count is not durable across replays, ADR 0004)

[read]       # the read policy: the run retry policy of every read-only step (lookups, verifies, hints, `get`, `Szamlazz.Agent.query`, `query_taxpayer`, the probe); shapes no journal entry
max_attempts = 5              # executions of the step, including the first
initial_delay = "5s"
factor = 2.0
max_delay = "60s"
max_duration = "5m"           # the hard bound; a szamlazz.hu outage is tolerated for this long, not for the handlers' attempts

[resolve]    # the resolve policy: the run retry policy of the prologue's `account` step; no attempt cap; the duration is the bound
initial_delay = "1s"
factor = 2.0
max_delay = "10s"
max_duration = "1m"

[account]
id = "acme"                   # the resolver's identifier of the account; journaled with every invocation, never a resolution input
agent_key = "..."             # or env RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY; inline in the static resolver, credential_ref = id
endpoint = "https://www.szamlazz.hu/szamla/"   # optional (wiremock in tests)

[account.defaults]   # as v1: e_invoice, language, currency, exchange_rate_bank, template?, send_email?, number_prefix?, extra_logo?, aggregator?, guardian?
[account.seller]     # as v1
```

All three policies are set explicitly on the runs because the SDK's default run policy sends no retry delay and the
server would spend the handler's `invocation_retry_policy` instead. Durations are `"90s"`, `"2m"`, `"1h"` or a bare
non-negative integer of seconds. `WorkerConfig::validate` checks the cross-field
invariants (`max_attempts ≥ 1` where set, `initial_delay ≤ max_delay` and `factor ≥ 1` on all
three) and the one floor: `issue.initial_delay ≥ IssueConfig::MIN_INITIAL_DELAY`, the Számla Agent client's exported
`REQUEST_TIMEOUT` (60 s) plus a 30 s margin, a create or storno step re-executed sooner would query for the cut execution's
send while it may still be in flight (the ~90 s rule of ADR 0002 and the behaviour notes, in code since #61; the read
and resolve policies have no floor, and the e2e suite's 1 s policies are built with
`ValidatedWorkerConfig::unchecked` behind the `test-util` feature and never pass through `validate`; `validate` is the
one way to the `ValidatedWorkerConfig` that `Order::from_parts` / `Agent::from_parts` take, #128);
`StaticResolver::try_from` validates the account (non-blank id and key, an http(s) endpoint).

**The endpoint's loader is strict.** The layout and every library type it is made of are closed
(`#[serde(deny_unknown_fields)]`: `WorkerConfig` and its `RetryPolicyConfig` tables, `StaticConfig`, `StaticAccount` and
its `StaticDefaults` / `StaticSeller` / `StaticSellerEmail`), so an unknown key at any level is a parse error that
figment attaches the key path and the source to (the file, or the environment variable that set it); a typo such as
`mod = "test"` or `[isue]` fails at start-up instead of silently running a test account as live or leaving a policy at
its default. The input types are distinct from the journaled value types they are built into (`Defaults`,
`SellerConfig`, `SellerEmailConfig` in `account`, which stay permissive so that an `account` entry of an earlier
deployment replays), mirror them field for field and convert with `From`; a round-trip test holds the two sides to each
other (#128; the pre-#128 loader kept a hand-maintained key tree instead, J-05-15). The one shape rule serde cannot
express is checked first, on the merged figment value: both account shapes present is refused naming each with its
source (a stray `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` on a multi-account file would otherwise surface as the partial
account's `missing field id`). The pre-release layout (`account.slug`, top-level `[defaults]` / `[seller]`) is refused
as any unknown key is: the crate has never been released, there is no compatibility shim and no longer a named
refusal. Environment override values are
read as **strings** and the field's type decides (`extract_lossy`: `"3"` is `3` on a count, `"true"` on a flag), so an
all-digit agent key keeps its leading zeros; figment's own environment provider would parse it as a number.
`--check-config` runs the loader and builds the endpoint, then exits 0 without listening.

**Multi-account shape.** Instead of `[account]`, a table of `[accounts.<scope>]` with the same fields; the two are
mutually exclusive (both present is a load error) and there is no default account. Each account is reachable under
its scope only (`/restate/scope/{scope}/call/…`); an unscoped request is `unknown_account`, as a scoped request is on
the single-account shape. Load-time validation enforces the checkable half of the resolver's safety contract, one
szamlazz.hu account under exactly one scope, no fan-in: unique `(endpoint, agent_key)` pairs and unique ids (the
credential reference). The endpoint is compared on its normalised form (`Endpoint::normalized`: scheme and host
case-folded, a default port written out dropped, trailing slashes trimmed), so `…/szamla/` defaulted beside `…/szamla`
typed with one key is refused as the one account it is rather than admitted as two (J32); the account keeps and posts
to the text as written. *Which* szamlazz.hu account a key opens is not checkable, no operation answers "which account
am I?", `check_account` finds no document, and a found document exposes no account identity the worker could verify
against configuration (0.3's optional `supplier_id` pin on `szallito/id` is gone: ADR 0006, account-pin amendment),
so a key under the wrong scope is the operator's go-live check to catch: under each scope, `Szamlazz.Agent.query` a
document known to be the account's and read its seller block. Scope keys are `[a-z0-9_]`, 1–36 bytes: a strict subset of Restate's scope format (`[a-zA-Z0-9_.-]`,
non-empty, at most 36 characters; ASCII, so bytes; a dashed UUID is exactly 36) chosen so that environment overrides can address them
(`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY`; figment lowercases the segment). This is the documented constraint
on the account identifiers a caller uses as scopes with the static resolver. The namespace is one per deployment and
shared by every account.

```toml
identity_keys = ["publickeyv1_…"]   # required in this shape (§10): the scope inside an unsigned request selects any account
namespace = "acct"

[accounts.acme]
id = "acme"
agent_key = "acme-key"        # distinct per account: a shared (endpoint, agent_key) pair is refused at load

[accounts.beta_events]
id = "beta"
agent_key = "beta-key"
```

**Single → multi flag day** (no data migration; the namespace stays, so the first scoped create for an
already-invoiced order finds it under the unchanged external id): make both services private
(`PATCH /services/{name} {"public": false}`; the ingress refuses new calls without creating invocations), poll
`sys_invocation` until no row has `status <> 'completed'`, register the new revision with the switched configuration
(a new deployment URI), point callers at scoped paths, make the services public. The drain is what keeps one
szamlazz.hu account from being reachable unscoped and under its scope at the same time. The same drain–switch–resume
applies to any change of the scope → account mapping, which is append-only. Scripted in the endpoint README and
performed by the e2e suite (§11).

The order-number hint runs on every lookup except for correctives; it is not configurable.

Per-call inputs (`DocumentInput`) as v1: `buyer`, `items`, `fulfillment_date`, `due_date`, `payment_method`, `paid`,
`comment?`, `issue_date?`, overrides.

## 10. Endpoint

`restate-szamlazz --config <file> --bind 0.0.0.0 --port 9080`; `RESTATE_SZAMLAZZ_*` env with `__` nesting
(`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY`, `RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY`;
`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY` in the multi-account shape), every value a string the key's type
reads; `identity_keys`; tracing; `--check-config` for CI and init containers (loads, validates, builds the endpoint,
logs the start-up summary, exits 0 without listening, non-zero with the error otherwise);
container image on `v*` tags, running as a non-root user (uid 65532) with `STOPSIGNAL SIGTERM`. The start-up log
names the namespace, whether the deployment is scoped, and per account its scope (or `<unscoped>`), id and
endpoint (never the key), then whether request identity verification is on, then the bound address
and the signals that stop the process. An
account's `endpoint` is an `http` or `https` URL with a host and no userinfo (`user:password@` is a load error: the
endpoint is journaled with the account and printed here); plain `http` is allowed (a local mock, a proxy), and one
on a host other than loopback is logged at `warn` as sending the agent key in cleartext (`Endpoint::is_cleartext`;
#65). Request identity is read as a three-way decision (`RequestIdentity`; #96): `identity_keys` with at least one
key is `Verified` (the SDK refuses unsigned requests; `info … enabled keys=N`); the empty list **written out**
(`identity_keys = []`) is `Unsigned { deliberate: true }`; the local-development opt-out, an `info`; the key not
mentioned is `Unsigned { deliberate: false }`, a `warn` naming the consequence (any client reaching `{bind}:{port}`
can invoke the services under any scope; the scope is protocol data inside the request, and identity keys are what
enforce ADR 0006's assumption that only the Restate runtime speaks to the endpoint) and the two remedies. A delimited
string yielding no key (an empty `RESTATE_SZAMLAZZ_IDENTITY_KEYS`) is folded to "not mentioned" by the
deserializer, because that is what a template with a missing secret renders, and it overrides a file's keys or its
`[]` the same way; only the list literal opts out. The endpoint does not refuse to start without keys: the warn, the
README's deploy checklist (keys required wherever anything but the runtime reaches the port, and in the multi-account
shape) and `--check-config`, which prints the same line, are the guards. `SIGTERM`
or `SIGINT` stops it cleanly through the SDK's `serve_with_cancel` (the SDK's own `serve` waits for `SIGINT` alone,
and an unhandled `SIGTERM` would end PID 1 on the spot): accepting stops, open connections get the SDK's 10 s connection
drain, the process exits 0. An invocation the drain cuts resumes on Restate's next dispatch after the handler's retry
interval, query-first (ADR 0004); the README's Running section gives the grace-period recommendation and the
drain-first rolling update. The endpoint README also carries the caller guidance with the Pretix integration as the
worked example (ADR 0006), the deploy checklist around `check_account`, and the flag-day and drain–switch–resume
scripts.

## 11. Testing

szamlazz.hu is stood in for by **wiremock, not a fake**: every test states the answer szamlazz.hu gives, byte for
byte, so a test is reviewable against the verified facts in `docs/szamlazz-hu-behaviour.md` rather than against a
second model of the server that could be wrong while every test passes. What the protocol's hard cases need (a lost
reply, `szlahu_down`, a specific code, "exactly n creates on the wire", "this account's key on this request") is
what canned responses and `expect(n)` do natively. The cost is that the stubs of one scenario must agree with each
other (a document is reachable by number, order number and external id with one body). The e2e harness keeps that
behind a thin consistency layer over wiremock. A scenario states what szamlazz.hu *holds* per document:
`holds(doc)` mounts the one body on every selector the document is reachable by (its number, its order number when
it carries one, the external id when the test states it), so the stubs cannot disagree; `holds_after_misses(n, doc)`
is the document appearing under its external id after `n` code-7 answers. The one failure the create protocol is
designed around (the create lands, the reply is lost) is `create_lands_but_reply_lost(doc)`: its transition is
driven by the create request being received, not by a hand-counted number of queries (one flag for one document,
flipped by the create stub's responder and read by the external id's). Raw selector stubs, `expect(n)` and
`up_to_n_times(n)` stay where a scenario is about a specific wire sequence. The layer holds no state beyond that flag
and does not grow into a fake. A stateful fake would be reconsidered for one capability only: property tests of
the exactly-once invariant (random handler sequences under two scopes, "at most one live document per kind per
order, the newest holder under every external id"), which no stub can express. Neither approach exercises a handler
without Restate (the SDK has no `ObjectContext` harness), so handler decisions are tested in the **decision layer**,
the decide fns: each handler body is `read → decide → (answer | proceed) → next read`, where every `decide` is a pure
function of the journaled outcome the read returned and the request, beside its async shell, and the shell is held to
holding no `match` on a gateway outcome that returns a response. On `main` the storno, delete and `get` shells
(`service/storno.rs`), `Szamlazz.Agent`'s (`service/agent.rs`), the shared after-lookup decision and the responses
(`service/support.rs`) and the prologue's (`service/prologue.rs`) are in that shape; the create side's
(`service/create.rs`) has `respond_to`, `exclusive_with` and `prepare` pure and the rest of its decisions in flight
(#137). The decision functions are unit-tested branch by branch with `test_support::Doc`; the gateway's own
classifiers (which answer is settled, which document is foreign, which failure is which class) are the same kind of
function one layer down, table-tested in `gateway`'s unit tests; and the e2e is left with what only a server can
show: the durable sequence, replay, the per-key lock and the journal.

- `gateway`: wiremock tests using upstream-shaped responses; the lookup matrix (`Absent`, `Live`, `Reversed` with
  the storno number from the hint, `Collision`, `Foreign`, the corrective's exemption from the hint), the create step
  (`Issued`, code 56 *with* a number as `Issued` with `notification_delivery_failed` after one send and no re-query
  (in the shape the agent crate accepts; szamlazz.hu's own shape for 56 is unverified),
  `Found` on a re-executed step, `Rejected`, the open codes re-queried once and `Unconfirmed` when nothing
  landed, a code the agent crate does not know among them, on the create and the storno send , an answered code
  or `szlahu_down` on the leading query settled as `Api` / `Unavailable` with the create and storno mocks seeing
  zero requests (#63), a failed post-send re-query as `ReQueryFailed` naming both causes, the `Unconfirmed`
  displays, the 71/152 matrix
  incl. `existing_number` and the contradiction settled after one send, the corrective's
  71/152 → `Rejected`), storno validation incl. the D/SL no-op, the zero-gross storno as `Reversed`, the storno body
  carrying `<teljesitesDatum>` equal to the step request's date and no `<keltDatum>`, the two sends of a
  re-executed storno step byte-identical (`assert_eq!` on the bodies), a verified document's `telj` surfaced as
  `fulfillment_date` and a document without the element as `None`, 335, 7, and the
  credential codes 3/135/136/164 as `CredentialsRejected` on
  every operation (both lookup queries, the create's leading query and send, storno, delete, credit entries, query,
  taxpayer query, probe); every read fn (`lookup`, `verify`/`query`/`hint`, `lookup_storno`, `query_taxpayer`,
  `probe`) answering a 500, an empty body or `szlahu_down` as `Err(Unanswered)` (the step's retryable error, never
  data), and another API code as `Ok(Api)` data (the probe: `Accepted`); the probe as exactly one query of the
  sentinel id and nothing else, with a wrong key as data; the taxpayer query as exactly one `xmltaxpayer` request of
  the prefix, a known prefix `Found` with NAV's registered data, an unknown one `Found{valid: false}`, NAV's relayed
  `funcCode ERROR` and a szamlazz.hu header code both `Api`; the gateway validates found documents against the
  account it was opened for. The classifiers behind the async steps are pure functions with a table test each in
  the module's unit tests (#124), so a branch is pinned without a wire exchange: `is_foreign` (a live `SZ`/`ES`/`VS`
  that is neither the document seen under our id nor a number known to be ours; a reversed one, and `D`, `HS`, `SS`,
  `SL`, are not), `classify_failure` (on representative codes of each outcome class, each asserting its class
  first, since the exhaustive code → class table is `szamlazz-agent`'s under its own tests: the credential codes
  before their class; 71/152 the duplicate; seven `Rejected`-class codes and 7 as `Rejected`; 1, 55, 56 and an
  unknown code as `Unknown` through the arm a class the agent
  crate adds later falls into; `szlahu_down` as `Unavailable`; a request the wire contract refused as `Rejected`
  under the `request` pseudo-code; a parse and a transport failure as `Transport`), `QueryError::answered` (7, a
  credential code and another code are answers, `szlahu_down` and a transport failure `Unanswered`) with the
  `outcome` fold every read fn applies, and the send rule of the two write steps as a function of what the leading
  or re-query saw against the number the lookup saw reversed: `settle_create` (nothing, or exactly the lookup's
  reversed document still reversed, proceeds; a live document that is not it is `Found`, it live again `LiveAgain`,
  a reversed one that is not it `Reversed`, a collision and rejected credentials settle; every other failure is
  handed back for the caller to place) and `settle_storno` (the `SS` settles as `AlreadyReversed`, nothing proceeds,
  rejected credentials settle, the rest is handed back).
- `contract`: every request type and every object it nests refuses one unknown top-level and one unknown nested field
  with serde's error naming the field, the externally tagged enums refuse a second key, and every documented body
  (the endpoint README's curl example, the e2e scenarios' literal bodies) still deserializes; under `schemars`, every
  request schema and each of its `$defs` objects carries `additionalProperties: false` while the response schemas
  carry none.
- `service`: discovery test (names, handler set incl. `check_account`: read-only, `max_attempts = 3`, kill, explicit
  `journal_retention`, no input, and `query_taxpayer` with `query`'s read-only attributes, and attributes:
  `Szamlazz.Agent.storno` on `Szamlazz.Order`'s policy and timeouts, `set_payments` at two attempts,
  `inactivity_timeout = 2m` / `abort_timeout = 2m` on `get`, `query`, `query_taxpayer` and `check_account`, the
  reads' one-trip rule, the same as `set_payments`' (#114), and
  `initial_interval = 2m` on both `Szamlazz.Agent` writes and every `Szamlazz.Order` write, asserted to clear
  `IssueConfig::MIN_INITIAL_DELAY`), the
  taxpayer decisions (the stem and the full number name one `taxpayer-{prefix}` step; a number in neither form is the
  400 `invalid_input` fault naming it and the accepted forms; an exhausted read is `unavailable` naming the step), the
  storno, delete and `get` decisions as the pure functions their handlers call after each read (#138): the
  order's storno verdict on the verified document (`SZ`/`ES`/`VS`/`HS` proceed; another order's, an absent, empty or
  whitespace-only `rendelesszam` → `conflict{not_managed}` before anything else is read; reversed by anyone →
  the hint is read, whatever the kind; `D`, `SL` and `SS` → `rejected{not_stornoable}`), the by-number storno's
  verdict (a trimmed non-empty `rendelesszam` → `managed_by_order` with that number as `order_key`; an empty or
  whitespace-only one is no order; reversed → the by-number lookup is read; no kind pre-check), the after-lookup
  decision both services share (`Absent` proceeds, `AlreadyReversed` answers `reversed{storno_number}`, a credential
  code and another code are the two faults carrying `szamlazz_code`), the delete guard (absent → `absent`; a
  collision → `external_id_collision` with or without `force`; a credit entry → `proforma_paid` without `force` and
  the document to delete with it) and the delete response (deleted and 335 → `deleted`; a refusal → its code;
  rejected credentials; a lost reply → `outcome_unknown`), the `get` projection (`live` / `reversed{None}`, totals,
  credit entry amounts, `referenced_proforma`, `e_invoice` as the storno lifts it) and the `get` fold (ours fills
  the slot, absent and a collision leave it empty, an absent proforma referenced by the invoice, by the prepayment,
  by both (the invoice's) or by neither, and a returned proforma never derived), an
  endpoint build smoke test, two integration tests of the
  endpoint binary (spawned on an ephemeral port with an environment-only configuration, `SIGTERM` and `SIGINT` each
  end it with status 0 within seconds, the start-up log names both and honours `--bind`; `--check-config` on each
  fixture exits 0 with the start-up summary and without listening, and on an unknown key (in the file and in the
  environment) an invalid identity key or a missing file exits non-zero with the error), the loader (every unknown
  key at every level named with its path and source (`[isue]`, `mod`, `supplyer_id`, `curency`, `bnk`, a misspelt
  environment variable), and every one at once; a wrong type naming key and source; both shapes naming both sources
  rather than the partial account's missing field; environment values as strings the type reads, the all-digit agent
  key byte-exact through to a wiremock szamlazz.hu; the known-key tree matched field for field against the library
  types' `Serialize` output; every TOML example of the endpoint README, design §9 and `fixtures/` loading and building
  its accounts; the request-identity decision, keys as a list or a delimited string are `Verified`, `identity_keys`
  unmentioned is unsigned by omission, `[]` written out in TOML, JSON or YAML is deliberate, and a blank string or a
  blank `RESTATE_SZAMLAZZ_IDENTITY_KEYS` is not the opt-out even over a file's keys or its `[]`), `Body<T>` (a well-formed body
  decodes, a misspelt option / a wrong type / a missing field / an empty body each leave the handler as the 400
  `invalid_input` fault naming the field, and its schema and input metadata are `Json<T>`'s, in the discovery
  manifest too) `prepare` refusing
  `options.proforma` on every kind but `create_invoice`, the create-side decisions of `Szamlazz.Order` as pure
  functions of the journaled outcome beside their async shells (`service::create`, #137): `decide_lookup` (a live
  document as `already_issued` or `conflict{live}` under `reissue`, a reversed one as `reversed{storno_number}` or
  proceeding under `reissue` carrying its number, `Absent` proceeding, a collision and a foreign document refusing
  either way, an answered code as the `unavailable` / `credentials_rejected` fault), `decide_exclusivity` (a live
  other-kind document with the table's reason, a collision as `external_id_collision`, a *reversed* other-kind
  document and `Absent` passing), `decide_prepayment_for_final` (live recorded as the reference, `Absent`, reversed,
  collision), `decide_proforma_link` under `auto` and `none` (live linked or `proforma_live`, reversed and `Absent`
  linking nothing, collision), `decide_proforma_by_number` (a proforma of ours linked, 7 as `proforma_missing`,
  another order's or an order-less one as `not_managed`, this order's non-proforma as `invalid_input` without a
  document identity, an answered code as a fault about the create), `decide_base` (`not_managed` before
  `base_reversed`), `create_outcome_unknown` (exhaustion and cancellation as `outcome_unknown` repeating the last
  failure) and `respond_to`'s `Issued` arms (56 as the `notification_delivery_failed` warning on an `issued`, a
  number-less success as `outcome_unknown`), found documents built with `test_support::Doc`, create replies parsed
  off the wire as the gateway parses them, and nothing recorded in the references on a refusal; the
  issue and read policies' field-for-field mapping onto
  `RunRetryPolicy` and `WorkerConfig::validate` on all three tables, the fault → status mapping incl. the exhausted
  read → `unavailable{step, last failure}` about the document and the missing-`telj` fault as a 503 `unavailable`,
  the storno intent built from a verified document (`telj` present → `fulfillment_date` equals it with `e_invoice`
  lifted from `eszamla` or the account default; `<telj></telj>` → the fault naming the invoice, `.about(..)` adding
  the order, kind and external id), the `set_payments` fault's message differing by
  `additive`, `Lookup::classify` on `Api`, the probe outcome →
  `credentials` mapping, the handler's key parsing refusing a key with leading
  or trailing whitespace (`" ORD-1"`, `"ORD-1 "`, `"\tORD-1"`) as `invalid_input` naming the rule while
  `OrderKey::parse` itself still trims, two sentinel tests that the agent key reaches
  neither the `credentials_rejected` warning nor its fault body, and the prologue's credential fetch through a
  scripted store under a paused tokio clock (#124): a store reporting itself unavailable is asked `FETCH_ATTEMPTS`
  times `FETCH_PAUSE` apart with no deadline spent and then the terminal `unavailable` fault naming the cause and
  neither the store's message, the account nor the reference; a gone reference is the fault after one fetch and no
  pause; a store that never answers is bounded per attempt by `CALL_DEADLINE`; and one that recovers within the
  attempts (unavailable, then silent, then the key) answers the credentials it gave after two pauses and one
  deadline.
- `service::journal` (journal compatibility, ADR 0005 #47): one pinned JSON fixture per variant of every type the
  services journal as a `ctx.run` result, `Namespace`, `Resolution` (with an `Account` carrying every optional
  field), `QueryOutcome`, `LookupOutcome`, `CreateOutcome`, `StornoLookupOutcome`, `StornoOutcome`,
  `DeleteOutcome`, `SetPaymentsOutcome`, `ProbeOutcome`, `TaxpayerOutcome`, under
  `tests/journal/<type>/<variant>.json`, the document-carrying ones with every element the `szamla` XML can hold
  (postal addresses, ledger blocks, a financial item, labels, two payments, a PDF) and the taxpayer one with a
  detailed and a simple address. The generator asserts the JSON the current code writes equals the committed
  fixture byte for byte and never writes unless `UPDATE_JOURNAL_FIXTURES=1`, which writes missing fixtures and
  archives a differing one as `<variant>.<n>.json` before writing the new shape; the compatibility test replays
  every fixture in every directory (current and archived) through the current types, requiring each to decode
  and re-encode to a superset of itself, and refuses a fixture directory no journaled type claims. The `Journaled`
  marker trait bounds the run helpers, and each enum's pins name its variants exhaustively, so a new variant
  fails to compile until pinned. Harness tests cover the superset check and the verify / update / archive
  behaviour on a scratch directory. The same module's leak guard serialises every variant of every journaled type
  built around an account whose agent key is a sentinel: the `account` step's entry through the static resolver
  from configuration carrying the key, `DeleteOutcome::Transport` and `SetPaymentsOutcome::Transport` from a gateway
  opened with the sentinel credentials against an endpoint that refuses connections, the registry's samples for the
  rest, and asserts the sentinel is in none of them: the cheap complement to the `assert_not_impl_any!` guard and
  to the e2e's byte scan, which needs a server.
- End to end (`tests/e2e/`, ignored; a Restate server from one of three sources: see "What CI runs" below):
  Restate 1.7.8 with `RESTATE_EXPERIMENTAL_ENABLE_VQUEUES`, `…_PROTOCOL_V7` and
  `…_SCOPED_VIRTUAL_OBJECTS` (the harness asserts on `/version` exactly the features the server's flags enable;
  `compose.yaml` matches) + wiremock as
  szamlazz.hu, issued → already_issued (new key) and Idempotency-Key replay (same key, create mock `expect(1)`);
  152 → reconciled; 152 whose external-id re-query finds nothing of ours → `conflict{duplicate_order_number}`
  settled after one send with no run failure recorded (#41); `existing_number` absent with nothing under the order,
  and naming the live invoice another channel issued between the lookup step and the create when the naming query
  finds one; storno → reversed with the storno mock matched on `<teljesitesDatum>` equal to the fixture's
  `telj` and no `<keltDatum>` on the wire; a `telj`-less original → 503 `unavailable` naming the invoice with
  `order`, `kind` and the storno external id, only `namespace`, `account` and `verify-storno-{number}` journaled and
  the storno mock `expect(0)`, after a `telj`-less document of another order → `conflict{not_managed}`, a
  `telj`-less proforma and a `telj`-less delivery note → `rejected{not_stornoable}` and a `telj`-less reversed invoice → `reversed` with its storno
  number from the hint, nothing sent in any case; a storno whose first reply is lost re-executed under the issue
  policy with two byte-identical bodies and one `storno-{number}` entry; szamlazz.hu's 14 and 221 on the storno →
  `rejected{code, message}` after one send carrying the caller's comment; an exhausted storno step (every send's
  reply lost) → a structured `outcome_unknown` naming the storno's identity with `storno-{number}` the failing
  command in flight and one send per execution, and the next call (a new key) whose storno lookup finds the `SS`
  under the storno external id → `reversed{storno_number}` through `verify-storno-{number}` and
  `lookup-storno-{number}` alone, nothing sent; stale create → reversed; `reissue` → issued as newest holder; `reissue` on
  live → `conflict{live}`; `sztornozott` → reversed; a document reversed in the UI between two executions of the
  create step (the first loses its reply, a short test policy) → `reversed` with exactly one create on the wire; a
  create whose reply is lost and whose immediate re-query finds the document → `issued` within the one execution,
  no run failure recorded, exactly one create on the wire; proforma auto-link and `consumed` in `get`;
  `options.proforma: {number}` checked like every found document, another order's proforma and one carrying no
  order number → `conflict{not_managed}` naming it with `verify-proforma-{number}` the last step journaled and the
  create mock `expect(0)`, this order's → `issued` with `dijbekeroSzamlaszam` on the wire whatever its `teszt`; `correct_invoice` on
  a live invoice of the order → `issued` under `…:corrective:{correction_id}` with `helyesbitettSzamlaszam` naming
  the base on the wire through `verify-base-{number}`, `lookup-corrective`, `create-corrective`, and the same
  `correction_id` again → `already_issued`; `correct_invoice` on a base szamlazz.hu does not know → 404 `not_found`
  carrying the corrective's identity, on a reversed base → `conflict{base_reversed, existing_number}`, on another
  order's → `conflict{not_managed, existing_number}`, each after `verify-base-{number}` alone with the create mock
  `expect(0)`; two `correction_id`s on one live base → two correctives, each under `…:corrective:{correction_id}` on
  the wire (`szamlaKulsoAzon`) beside `helyesbitoszamla` and `helyesbitettSzamlaszam`, with the order-number query
  mock `expect(0)`; correctives take no hint, so a live foreign invoice under the order is never met; 152 on a
  corrective → `rejected{152}` after the external-id re-query and no order-number query, never
  `conflict{duplicate_order_number}`; `delete_proforma` on the order's live proforma → `deleted` after one
  `delete-proforma-{number}` send, and again → `deleted{reason: absent}` with nothing sent; a proforma with
  registered credit entries → `{deleted: false, reason: proforma_paid}` after the lookup alone and, with `force`,
  `deleted` after one send; another order's proforma under `…:proforma` → `{deleted: false, reason:
  external_id_collision}`, nothing sent; szamlazz.hu's 335 → `deleted`; a lost reply → 500 `outcome_unknown` about
  the proforma after exactly one send (the delete has no retry of its own); `get` shape; a
  collision on the secondary (`…:prepayment`) lookup → `conflict{external_id_collision}` with the create mock
  `expect(0)` and the slot absent in `get`; `create_prepayment` taking `options.proforma` as `create_invoice` does
  (`none` beside a live `D` of ours → `conflict{proforma_live}` after `proforma-link` with nothing sent, `auto` →
  `issued` with `dijbekeroSzamlaszam` before `elolegszamla` on the wire) while `create_final` and `create_proforma`
  refuse it 400 `invalid_input` before any call; `create_proforma` on an order whose own invoice, then whose own prepayment invoice, is live under
  `…:invoice` / `…:prepayment` → `conflict{order_invoiced, existing_number}`, and on an order whose live invoice is
  under none of our ids → `conflict{foreign}`, the create mock `expect(0)` in every case; a live `VS` under `…:final`
  beside a reversed `ES` (its `SS` the newest document under the order) → `create_invoice` and `create_prepayment`,
  with and without `reissue`, `conflict{prepaid_chain, existing_number}` and `create_proforma`
  `conflict{order_invoiced, existing_number}`, each refused by `exclusivity-final` with no lookup step journaled and
  the create mock `expect(0)`; a reversed `VS` beside a reversed `ES` → `create_invoice`, and `create_prepayment`
  with `reissue`, `issued` under their own external ids, and `create_final` after a reversed `VS` →
  `reversed{storno_number}` then, with `reissue`, `issued` carrying `elolegSzamlaszam` (#62); a live `ES` under
  `…:prepayment` → `create_invoice` `conflict{prepaid_chain, existing_number}` and a live `SZ` under `…:invoice` →
  `create_prepayment` likewise, each from the first exclusivity row with one request seen, while a reversed `ES`
  refuses nothing and the invoice issues past it; `create_invoice` with `none` beside a live `D` of ours →
  `conflict{proforma_live}` after `proforma-link`, with `{number}` on 7 → `conflict{proforma_missing}` (an outcome,
  never `not_found`), and with `{number}` naming an `SZ` of the order → 400 `invalid_input` naming the number and
  its `tipus`, nothing sent in any case; `create_final` with nothing under `…:prepayment` →
  `conflict{prepayment_missing}` without `existing_number`, with a reversed `ES` → `conflict{prepayment_reversed,
  existing_number}`, both after `prepayment-for-final` alone, with a live `ES` → `issued` carrying `vegszamla`,
  `elolegSzamlaszam` and `…:final` on the wire (the `ES`, the newest live invoice-kind document under the order,
  never foreign), and szamlazz.hu's 73 → `rejected{73}` after one send; a create whose body
  carries `options.resissue`, and one with `buyer.tax_numer`, and a
  `delete_proforma` with `force: "yes"`; answered 400 with the structured `invalid_input` fault naming the field,
  the create mock `expect(0)`, zero szamlazz.hu requests and no run journaled (refused before the prologue); a create
  under an untrimmed key; a leading, a trailing and both `%20` around the order number, → 400 `invalid_input`
  naming the trimming rule with nothing journaled and zero szamlazz.hu requests; an
  exhausted create step (every reply lost, a short test policy) → a structured `outcome_unknown`
  500 within the run policy's delays, not the handler's, with `sys_invocation.retry_count = 1` and
  `last_failure_related_command_name = create-invoice` observed **while in flight** (attempt state is cleared on
  completion), and after it the same key → the stored fault without a request, a new key → `already_issued` from
  the lookup step with nothing sent; the read policy (a 1 s test policy of three executions): a lookup whose external-id query answers 500
  once → `issued` in one invocation with `last_failure_related_command_name = lookup-invoice` in flight, one journal
  entry per step and exactly one create on the wire; a lookup that never answers → a structured `unavailable` 503
  naming the order, kind and external id within the read policy's delays, `lookup-invoice` journaled, `create-invoice`
  not, zero creates; the create step's leading query answered with code 57 (the lookup's own query having missed
  cleanly) → a structured `unavailable` 503 with `szamlazz_code: "57"` at once; `retry_count` at the first
  execution's 1, no failure and no failing command recorded, `create-invoice` journaled as data, zero creates (#63); the
  lookup step's two queries answering code 57: on the order-number hint it is inconclusive (the hint looks for a
  foreign document and a code says nothing about one), so the create proceeds to `issued` in one execution with no
  run failure and the hint queried once, while on the external-id query it is the lookup's own `unavailable` 503
  with `szamlazz_code: "57"`, the hint never asked and nothing sent; `get` with one of its four reads answering 500 once → the status, with `get-proforma` the
  failing command in flight; `get` with **all four** reads answering 500 once → the status in one invocation, with
  `sys_invocation.retry_count` observed past the handler's `max_attempts = 3` (read from discovery); run retries
  spend no invocation attempts (ADR 0004, #87); a scoped `Szamlazz.Agent.query` and a scoped `Szamlazz.Order` call reaching the handlers with the
  scope on `sys_invocation`; a purged `get` invocation querying szamlazz.hu again; and the leak check's positive
  control; a sentinel in a szamlazz.hu rejection found in the hex-decoded `raw` of the create run's
  `Notification: Run` row (under journal v2 the `Command: Run` row carries only the name; the result is in the
  notification that follows), and nowhere else but the output. The prologue (on the same harness, through a
  test-local scripted resolver and store wrapping the static one): every invocation's journal opens with
  `namespace` and exactly one `account` run, and the journaled account carries its id and never the agent key; a
  scoped call on the single-account deployment → 400 `unknown_account` with `namespace`, `account` and nothing
  else journaled and zero szamlazz.hu requests; a resolver that fails twice then answers → the outcome, with the
  `account` run's retries visible on `sys_invocation` (`last_failure_related_command_name = account`, the failure
  text never echoing the resolver's message) within the resolve policy's delays, and still one `account` entry; a
  store that fails every fetch → 503 `unavailable` after three fetches, zero szamlazz.hu requests, and the same
  order issuing once the store is back; an invocation stuck in its `account` step (a resolver that never answers)
  holding the order key with a second exclusive call queued behind it → after `PATCH /invocations/{id}/kill` the
  stuck one completes as killed with `namespace` and `account` journaled and the queued `delete_proforma` completes
  at once from szamlazz.hu, a killed invocation releases the key. `check_account` unscoped → the configured account with `scope: null` and
  `credentials: ok` after exactly one szamlazz.hu request (the sentinel query carrying the account's key) and
  `namespace`, `account`, `probe` journaled; szamlazz.hu's code 3 on the probe → `credentials: rejected` as a 200;
  scoped on the single-account deployment → 400 `unknown_account`, zero requests.
  Then, on the same server, the **flag day** and the **multi-account phase** (two accounts behind a test-local
  mutable resolver and store seeded from the static resolver's `[accounts.<scope>]` shape: `acme` is the phase-1
  account, same key, and `beta` a second one): while private the ingress answers 400 with no
  invocation; after the drain and the switch the first scoped create for the order phase 1 invoiced →
  `already_issued` under the unchanged external id; unscoped → 400 `unknown_account` naming the scoped path, with
  `namespace` and `account` journaled and zero szamlazz.hu requests, and an unknown scope likewise; the same order
  key under `acme` and `beta` concurrently → two `issued` with each account's own key on the create wire exactly
  once; the **same** `Idempotency-Key` under the two scopes → two invocation ids and two documents, and the key
  replayed under either scope returning that scope's own completion without a call; `check_account` under `acme`
  and under `beta` → each its own account with `scope` as the SDK saw it, `credentials: ok`, and each account's
  key on its probe query exactly once; unscoped → 400 `unknown_account`;
  `create_invoice` → purge → `storno_invoice` → `reversed` → purge → `create_invoice {reissue}` → `issued` on an order
  Restate holds nothing of, then the scoped `get` seeing the new holder; `Szamlazz.Agent.storno` under `acme` on a
  document whose `teszt` is `false` and whose seller block carries another `szallito/id` → `reversed` with `acme`'s
  key on the storno (compared with nothing); a document as `acme`'s own → `reversed` through verify, lookup and
  storno; an order-bearing document → `managed_by_order{key}`, nothing sent; `Szamlazz.Agent.storno` under `acme`
  → `reversed` with `<teljesitesDatum>` equal to the fixture's `telj` and `acme`'s key on the
  storno and no `<keltDatum>`, on a `telj`-less document → 503 `unavailable` naming the invoice without `order`,
  `kind` or `external_id`, only the verify journaled and the storno mock `expect(0)`, after a `telj`-less
  order-bearing document → `managed_by_order` and a reversed one → `reversed`;
  `Szamlazz.Agent.query` under `acme` → the projection (`test` as reported, `false` on a live account's document ,
  totals; no `supplier_id`), 404 `not_found` on 7; `Szamlazz.Agent.set_payments` under `acme` → `<additiv>false</additiv>`
  (replacing) and `<additiv>true</additiv>` (additive) on the wire with `acme`'s key and the entries as sent,
  answered with the invoice's totals as szamlazz.hu reported them (`outstanding` distinct from `gross_total`) after
  the one `set-payments-{number}` step and no query before it; a replacing call with no entries → 400
  `invalid_input` before the wire; a lost reply → 500 `outcome_unknown` after exactly one send, its advice by
  `additive` (repeat as is, or query the invoice first);
  `acme`'s seller bank account changed between
  two executions of a create step (the first loses its reply) → both executions carry the journaled bank account
  and only a new invocation sees the change; `beta`'s key rotated between two executions → the second carries the
  new key while the `account` entry read in flight and after completion is byte-identical; the `state` table holding
  no row for `Szamlazz.Order` after the run's invocations on both deployments (the object keeps no state); and, over every
  `sys_journal` row of every invocation the server holds (hex-decoded `raw`) plus every
  `sys_invocation.completion_failure`, none of the three agent keys of the run, while the same scan finds the
  positive control's sentinel; and, last, the **run-name pin**: for every invocation the server holds, the `ctx.run`
  names in journal order are a prefix of one of its handler's paths in the table `RUN_NAMES` (the durable steps of
  every handler of both services, parametrized names, `verify-storno-{number}`, `taxpayer-{prefix}`, pinned by
  their prefix), every handler seen is in the table, and every path in the table was walked in full by at least one
  invocation. The type fixtures pin what an entry holds; this pins which entries a handler writes and in what order,
  the other half of what an in-flight invocation replays across a deploy (ADR 0005). A renamed, inserted, reordered
  or dropped step fails here rather than stranding the invocation.
  The harness (`tests/e2e/harness/`) calls through `/restate/call/…` and `/restate/scope/{scope}/call/…`, submits
  without waiting through `/restate/send/…`, returns the
  `x-restate-id` and a parsed fault body, reads `sys_journal` (`raw` hex-decoded to bytes, run results are bytes and
  render as integer arrays in `entry_json`) and `sys_invocation`, purges and kills invocations (`PATCH
  /invocations/{id}/purge`, `…/kill`), and verifies every mounted mock's `expect(n)` at the next scenario's reset
  (wiremock checks the counts on `verify`, never on `reset`). `get`, `Szamlazz.Agent.query`, `Szamlazz.Agent.query_taxpayer` and
  `Szamlazz.Agent.check_account` set `journal_retention = 1d` so their journals are inspectable. Kafka ingress is not exercised (§4).
  The suite is one integration-test binary, `tests/e2e/main.rs`, which holds the two tests and the order the scenarios
  run in; `harness/` is one module per concern (`gate`, the server gate and the launcher; `accounts`, the scripted
  and mutable resolver and store; `szamlazz`: the document fixture, selector matchers and stub helpers; `ingress`;
  `introspection`; `run_names`, the `RUN_NAMES` table and its matching), with the harness's own tests (the server
  gate, the run-pattern matching, the stub helpers against wiremock alone) beside what they test; and every
  other file is one handler family's scenarios (`create_invoice`, `create_proforma`, `create_prepayment`,
  `create_final`, `correct_invoice`, `storno`, `delete_proforma`, `get`, `policies`, `agent_reads`, `agent_writes`,
  `faults`, `prologue`, `multi_account`, `pins`), each a `pub(crate) async fn` per scenario taking the harness. A new
  scenario of a handler goes into that handler's file and is called from `main.rs` in sequence.
- The protocol-v7 canary (`tests/e2e/main.rs`, a second ignored test on a server of its own, on its own ports, with
  vqueues and scoped Virtual Objects on and protocol v7 **off**): the ingress accepts a scoped path and the server
  keys the invocation by the scope (`sys_invocation.scope = acme`), but the SDK never sees it, a scoped
  `check_account` on the single-account deployment answers 200 with the account and `scope: null`, the signal a
  deploy pipeline reads, and on the multi-account deployment (after the flag day on the same server) 400
  `unknown_account` naming the unscoped case with nothing sent: every scoped call fails closed. What §4 and ADR 0006
  say about the flag, provoked once against a server without it.
- **What CI runs.** `dagger check` (`.github/workflows/dagger.yaml`) runs the `rust` module's `build`, `test`
  (default features), `clippy`, `doc`, `audit` and `fmt` checks and, from the workspace's own `ci` module
  (`.dagger/modules/ci`, wired onto `rust:container`), `ci:test` (`cargo test --workspace --all-features --locked`,
  so the `szamlazz-adatkapcsolat` archiver tests behind `opendal` and the `schemars` contract tests run), and
  `ci:end-to-end`: the ignored `tests/e2e` suite, both e2e tests, on every pull request. The Dagger
  container has no docker daemon, and a Dagger service cannot reach back into the container that binds it, so the
  harness starts `restate-server` itself: the check copies the binary out of the Restate image and sets
  `RESTATE_SERVER_BIN`, and the harness spawns one process per suite on the loopback (bind addresses, base
  directory and the experimental flags through Restate's `RESTATE_*` environment; log and data under a temp
  directory of its own, kept when the test fails), registers the endpoint at `127.0.0.1` and kills the process on
  drop. The **server gate** decides the source once from the environment, in this order: `RESTATE_ADMIN_URL` /
  `RESTATE_INGRESS_URL` (a running server with the three flags; the main suite only, the canary needs a server of
  its own shape), `RESTATE_SERVER_BIN`, the docker daemon (a container of the image, the endpoint registered at
  `host.docker.internal`; `RESTATE_ENDPOINT_HOST` overrides the host in every mode). With none of them the suite
  skips with a message on a developer machine and **fails** when `CI` is set (the check sets it), because a run
  that passed by skipping proves nothing. The gate is a pure function under its own tests; the docker mode is the
  developer default and is exercised on developer machines, not in CI.
- Live: the go-live checklist in `szamlazz-hu-behaviour.md`, to be automated as ignored tests (issue #15).

## 12. What v2 gives up relative to v1 (deliberately)

`request_id` retry identity (→ `Idempotency-Key`), `conflict{payload_mismatch}` (a different payload for a live
document is `already_issued`), flag-free reissue after a service-side storno (→ `reissue: true` after any reversal),
`recorded_document_missing` (a document szamlazz.hu no longer knows is simply absent; live accounts cannot delete
invoices), `payments_before` capture on storno (query before stornoing), the ledger snapshot (`get` is 4 live
queries), operator handlers `record_reversal`/`forget` (nothing to repair), the account fingerprint learned into
state, and, since ADR 0006's account-pin amendment, any account pin at all (0.3's `mode` against `teszt` and
optional `supplier_id` against the undocumented `szallito/id`: both dropped), schema versioning and state
migrations.
