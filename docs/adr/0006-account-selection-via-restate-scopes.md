# One deployment serves many szamlazz.hu accounts, selected per request by the Restate scope

Status: accepted (#20, landed through #21–#31). Amends ADRs [0001](0001-restate-worker-as-module-plus-thin-service.md),
[0002](0002-order-keyed-idempotency-via-external-ids.md), [0004](0004-kill-not-pause-on-exhausted-retries.md) and
[0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md) as listed at the end; the design document is
[`docs/design/restate-szamlazz.md`](../design/restate-szamlazz.md). Every Restate fact below was verified on
**restate-server 1.7.8** with **restate-sdk 0.12.0** (`restate-sdk-shared-core` 7.0.3); a server or SDK upgrade
re-triggers the verification of everything marked *verified*. The szamlazz.hu facts are those of
[`docs/szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md). Scenario names in parentheses are the end-to-end
tests in `crates/restate-szamlazz/tests/service.rs` that prove the fact.

`restate-szamlazz` served exactly one szamlazz.hu account per deployment, baked into configuration. A
second account was a second deployment — which is not even possible inside one Restate environment, since
`Szamlazz.Order` and `Szamlazz.Agent` are fixed service names and a second registration is a *revision* of
the first. An integration that invoices Pretix ticket orders for many organizers, each with its own
szamlazz.hu account and with events that may override the account, needed one deployment to serve N
accounts, selected per request by an opaque identifier the caller controls, with the credentials fetched
from wherever the caller keeps secrets and never written into Restate.

## Decision

One deployment serves any number of szamlazz.hu accounts. The caller selects the account per request with
the **Restate scope** (`/restate/scope/{scope}/call/Szamlazz.Order/{order}/…`, likewise for
`Szamlazz.Agent`). Every handler of both services runs the same **prologue** before its operation: pin the
namespace in a pure durable step (`namespace`); resolve the scope to its **Account** through the pluggable
`AccountResolver` in a durable step named `account` under the **resolve policy**; fetch the account's
credentials through the pluggable `CredentialStore` *outside the journal*, on every handler execution;
open the **Gateway** — the module that speaks to szamlazz.hu for one account — over a fresh Számla Agent
client. The `Account` is journaled once per invocation and carries everything about the account but its
agent key; the credentials are held only for the execution. The Virtual Object key stays the bare, trimmed
order number, and the external ids stay `{namespace}:{order}:{kind}` under one deployment-wide
**namespace** shared by every account. `Szamlazz.Order` keeps no state (ADR 0005); nothing about the account
is stored anywhere in Restate but the journaled `Account` of each invocation.

### The scope is the only transport

The scope is the only channel for the account identifier. It is opaque to the worker, which hands it to the
resolver as is; the resolver's own account `id` is never a resolution input. Restate makes the scope part
of the identity of everything under it (docs, *Flow control → Scopes*: "It becomes **part of the identity
of every invocation and resource inside it**"): the Virtual Object key and the `Idempotency-Key` are
namespaced per scope, so the same order number under two scopes is two `Szamlazz.Order` instances with two
locks, and the same `Idempotency-Key` under two scopes is two invocations (verified end to end:
`same_order_key_under_two_scopes_issues_on_both_accounts`,
`same_idempotency_key_under_two_scopes_is_two_invocations`). The scope is set per call — it is a path segment
of the ingress request, and the worker's two services never call each other, so nothing here relies on
inheritance across calls. A scope value is `[a-zA-Z0-9_.-]`, non-empty, at most
36 characters (ASCII, so bytes) — a dashed UUID fits exactly — which is the constraint on the account
identifiers a caller may use. The scope is also Restate's partition key for scoped invocations
(`crates/types/src/identifiers.rs` at v1.7.8: "When scoped, the partition key comes from the scope"; the
scope is a field of `IdempotencyId` and of `ServiceId`, and is hashed into the deterministic invocation id
of an idempotent request); the worker does not depend on it.

Rejected transports, each because it leaves the Restate identity shared between accounts:

- **A request header.** Not part of the invocation identity: two accounts' orders would share one Virtual
  Object lock and one `Idempotency-Key` space, and a retried request could name a different account under
  the same key. Headers are also the spoofable layer — the ingress appends the caller's headers after its
  own and the SDK keeps the last value of a duplicated name (verified, see the ingress-path note).
- **A body field.** The same identity problem, plus the account would be journaled beside the buyer's PII
  and parsed by every handler, including the input-less `get` and `check_account`.
- **An account prefix on the Virtual Object key** (`{account}:{order}`). Works without any server flag and
  namespaces the lock; the `Idempotency-Key` it namespaces only through the key (`IdempotencyId` carries
  `service_key`, verified), so not at all for `Szamlazz.Agent`, which has no key — and the caller composes
  the account into every key and every by-number call. Kept as the flagless contingency (below), not chosen.
- **The invocation id or `ctx.rand_uuid()`.** Deterministic only within one journal; useless to a later
  invocation.

### Two-phase resolution: the Account is journaled, the credentials never are

Resolution is split so that the *decision* is durable and the *secret* is not.

1. The `account` step journals the resolver's answer **as data**: the `Account` (`id`, `mode`,
   `supplier_id?`, `endpoint`, `defaults`, `seller`, `credential_ref`), or `unscoped`, or `unknown{scope}`.
   Once journaled, an invocation finishes on the account it started on: a remap of the scope, a change of
   the account's bank account or defaults, or a redeploy with a new resolver reaches only new invocations
   (verified: `account_change_between_executions_does_not_reach_the_invocation`). The journaled `Account` is
   visible in the Restate UI for the journal retention period and contains no secret, so that is safe. The
   type is additive-only — a new field gets `#[serde(default)]`, nothing is renamed or removed — so an old
   journal replays on new code. `unscoped` and `unknown` are journaled and never retried; only the
   resolver's *unavailability* is a retryable error, whose display text never echoes the resolver's own
   message (it becomes `last_failure` on `sys_invocation`).
2. The credentials are fetched **on every handler execution, including replays**, outside the journal,
   with three in-process attempts 200 ms apart, and are dropped with the execution. A rotation is picked up
   by the next execution of every in-flight invocation while its `account` entry stays byte-identical
   (verified: `credential_rotation_between_executions_is_picked_up`).

The **compile-time guard** is that `szamlazz_agent::Credentials` and `AgentKey` implement neither
`Serialize` nor `Deserialize` (`assert_not_impl_any!` in `account.rs`), so a `ctx.run` closure cannot
return them and no journaled type can hold them. It is a *narrow* guard: `AgentKey::expose() -> &str` is
one line from a `String` in a journaled struct. The real guarantee is the end-to-end **sentinel scan**:
after the whole run, every `sys_journal` row of every invocation the server holds, hex-decoded from the
`raw` column (run results are bytes and render as integer arrays in `entry_json`, so a text `LIKE` is
vacuous), plus every `sys_invocation.completion_failure`, contains none of the run's three agent keys —
while the same scan finds a sentinel deliberately journaled as a positive control
(`no_agent_key_in_any_journal_of_the_run`, `harness_scoped_call_and_leak_positive_control`).

### The safety contract

Seven rules, listed in full in the library README's "what it relies on" and, rule by rule, on the *Account
resolver*, *Credential store* and *Scope* entries of `CONTEXT.md`; the traits document their own halves
(rules 1–2 on `AccountResolver`, the fetch-every-execution rule on `CredentialStore`). The worker enforces what
it can (the static resolver at load time) and *relies on* the rest.

1. **One szamlazz.hu account is reachable under exactly one scope value.** Unscoped counts as a value.
   No fan-in: two scopes reaching one account would split an order's per-key lock across two Virtual
   Objects, and the worker cannot detect it at runtime — `check_account` only echoes configuration.
2. **The scope → account mapping is append-only.** Moving traffic to another account means a new scope, never
   re-pointing an existing one. Appending a scope cannot create fan-in; any change that could put one account
   under two identities at once — the single → multi flag day above all — is a drain–switch–resume.
3. **The namespace is permanent** for the deployment. Changing it hides every document issued so far.
4. **Order keys are unique within an account** for its lifetime, across all writers.
5. **The caller records the order key and the scope as used**; nothing else is needed to operate on the
   order later (rule of ADR 0005, now per account).
6. **The scope is routing, not authorization.** The ingress sits behind a gateway that sets the scope
   from the authenticated identity, never forwards a caller-supplied scope path, and **strips
   `x-restate-*` request headers** — the ingress lets a caller's copy of one of its own headers win, and
   the SDK keeps the last value of a duplicated name (verified for `x-restate-ingress-path`; the limit key
   therefore travels as the `limit-key` query parameter or is set by the gateway).
7. **Ownership is decided by the external-id query alone.** The order-number query can *name* a document
   but never prove ownership: document bodies carry no external id and the query returns only the newest
   holder (ADR 0002).

**Blast radius.** Before, whoever reached the ingress could issue on one account; now, on every account
the deployment serves, under whichever scope they name. The gateway of rule 6 is the boundary, and it is
the operator's, not the worker's. The static resolver's `[accounts.<scope>]` shape enforces the checkable
half of rule 1 at load time: a `supplier_id` on every account (the only server-side account identity a
found document exposes), unique supplier ids, unique `(endpoint, agent_key)` pairs, unique ids. A
database-backed resolver must guarantee rules 1 and 2 itself.

### Experimental Restate dependencies

The scope reaches the SDK only under **service protocol v7**, and a scoped call to a Virtual Object needs
**vqueues** and **scoped Virtual Objects**. All three are experimental flags on a self-hosted server
(`RESTATE_EXPERIMENTAL_ENABLE_VQUEUES`, `…_PROTOCOL_V7`, `…_SCOPED_VIRTUAL_OBJECTS`; `compose.yaml` and the
e2e harness set them and assert them on the admin `/version` endpoint). The server source at v1.7.8
(`crates/types/src/config/common.rs`) annotates them:

> `vqueues` — "Current in heavy development, do not enable this feature unless you are a contributor"
>
> `scoped_virtual_objects` — "Allow scope on Virtual Object targets. Scoped Virtual Objects are not
> officially supported in v1.7. Requires `vqueues` to be enabled as well."
>
> `protocol_v7` — "Once enabled, you **cannot** rollback back to previous versions where v7 is not
> supported < v1.7"

while the documentation (*Services → Flow control*) presents the same feature as opt-in for users:

> "Flow control is an opt-in feature and is disabled by default. Its configuration and APIs may change in
> future releases."
>
> "Starting with Restate 1.7.3, you can enable flow control on an existing cluster. When you enable
> vqueues, Restate automatically migrates the cluster's existing invocations to vqueues."

Both are recorded because they disagree in tone, and the decision leans on the second while the first is
what a self-hosting operator will read. The versions above are the ones the facts were verified on; an
upgrade re-verifies the flags on `/version`, the scoped-path behaviour of the ingress, and the SDK's
`ctx.scope()`. Before #26 the operator confirmed with Restate that **Restate Cloud** runs these flags as
supported for production; that confirmation is the precondition for a Cloud deployment, and a self-hosted
deployment accepts the source annotation knowingly.

**The runtime canary** is `Szamlazz.Agent.check_account` (#27): the prologue like every handler, then one
read-only query of the sentinel external id `{namespace}:check-account`, answering the scope the SDK saw,
the *configured* account, the namespace and whether szamlazz.hu accepted the credentials (`rejected` on
3/135/136/164 as data). Restate's ingress (1.7.8, `ingress-http/src/handler/service_handler.rs`) refuses a
scoped path while `vqueues` or `scoped_virtual_objects` is off but does **not** gate it on `protocol_v7`
(verified): with v7 off, a scoped call is accepted, the SDK sees no scope, and a single-account deployment
would issue on its one account. `scope: null` under a scoped probe is that misconfiguration, caught by the
deploy pipeline before any order is issued. The probe cannot detect fan-in.

**The flagless contingency**, should the flags be withdrawn: account-prefixed Virtual Object keys
(`{account}:{order}`), inferior because Restate would namespace the `Idempotency-Key` per account only where
a key exists (not for `Szamlazz.Agent`) and the caller would compose the account into every key, but
requiring no server flag.

### `x-restate-ingress-path`: considered and dropped

Restate's ingress sets `x-restate-ingress-path` (the original path and query) on every forwarded request —
present in server 1.6.0 through 1.7.8, **undocumented** (verified in `service_handler.rs`; absent from the
HTTP invocation docs and the changelog). A prologue guard on it was built for #27 — parse the path, drop
the query, percent-decode, recognise `/restate/scope/{scope}/call|send/…`, fail with `unavailable` when a
scoped path arrived with no SDK scope — and **dropped on review**: it hinged on a header that is
undocumented, version-dependent and caller-overridable (the ingress appends the caller's headers after its
own; the SDK keeps the last), a per-request dependency the worker is not willing to take. The worker
therefore has no per-request signal of "was this call scoped?"; the defence is `check_account` per scope
after every deploy, and rule 6's header stripping stays as defence in depth. No upstream request to
document the header or to add a scope header has been filed, by decision.

### Kafka ingress: untested, out of scope

A `kafka_scope` experimental flag exists at 1.7.8 (`crates/types/src/config/common.rs`, beside the three above:
"When enabled, Kafka subscriptions read `x-restate-scope` and `x-restate-limit-key` record headers to drive
vqueue scope and hierarchical limit-key routing. Requires `vqueues` to also be enabled."). So a Kafka
subscription *can* arrive scoped; "arrives unscoped" is **not** the reason it is
unsupported. It is unsupported because it is untested: no e2e scenario exercises it, and the scope of a
record would be set by whoever produces the record, outside rule 6's gateway. A deployment that wants it
verifies it first.

### Why Restate retention is irrelevant

Nothing a later request needs lives in Restate. The journal (`journal_retention = 3d` on the issuing
handlers, `1d` on the reads) exists for replay within one invocation; `idempotency_retention = 30d` exists
to deduplicate one caller's retries. The Virtual Object holds no state (ADR 0005). Walkthrough, months apart,
all under scope `acme` with namespace `acct`:

1. *Month 0.* `create_invoice` for order `EV-1` → lookup finds nothing under `acct:EV-1:invoice`, the create
   step sends → `issued SZ-1`. Three days later the journal is gone; thirty days later the `Idempotency-Key`
   is forgotten.
2. *Month 4.* `storno_invoice{SZ-1}` with a new key → the prologue resolves `acme` again; verify by number:
   `SZ-1` carries `rendelesszam = EV-1` (the key), `teszt` and `szallito/id` match the account; the storno
   lookup under `acct:EV-1:storno:SZ-1` finds nothing; the storno step sends → `SS-2` → `reversed`.
3. *Month 4, later.* `create_invoice{reissue: true}` with a new key → lookup: the newest holder of
   `acct:EV-1:invoice` is `SZ-1` with `sztornozott = true`, the order-number hint shows `SS-2` with
   `hivszamlaszam = SZ-1` → `Reversed{SZ-1, storno SS-2}`; with `reissue` the create step's leading query
   sees the same reversed document and sends → `issued SZ-3`, now the newest holder of the same id. `get`
   shows `invoice: SZ-3 live`.

Restate contributed the lock and, within each invocation, replay. The e2e suite performs exactly this with
the journals purged between the steps (`purged_order_is_stornoed_and_reissued`).

### Issuing is two durable steps; the query lives inside the closure

The hand-rolled attempt loop (one `ctx.run` per attempt, a durable sleep, an attempt counter) became two
steps (#22, ADR 0004 amended): a read-only **lookup** (`lookup-{kind}`) that settles every case needing no
create, and a **create** (`create-{kind}`) under the **issue policy** — a run retry policy, `2m → 10m`,
factor 2, five executions, bounded by one hour — whose closure returns `Err(Unconfirmed)` only when
szamlazz.hu's answer is not known and every known answer as `Ok` data. **Every execution of the create step
is query-first, inside the closure**: a separate journaled pre-query would replay its stale "nothing" on
the retry and the re-executed closure would send again. Storno has the same shape (#30).

**Accepted risk: the attempt count is not durable.** The SDK restores a run's retry count and elapsed
duration from the server's `retry_count_since_last_stored_command` only when the failing run is the *first*
journal entry after replay — true for these handlers, whose code is deterministic and whose open entry on
re-dispatch is the create step (or the storno step). Otherwise the count restarts, so `max_attempts` is
best-effort and `max_duration` is the hard bound (verified: an exhausted create step with a short test
policy returns the structured `outcome_unknown` within the run's delays, not the handler's, with
`retry_count = 1` and `last_failure_related_command_name = create-invoice` visible in flight). No
propagation-lag retry is added inside the closure: read-your-writes lag by external id is measured at ≈ 0.

### The duplicate-order-number answer, settled at once

On 71/152 the create step re-queries the external id **inside the same closure**: a live document of ours
→ `Reconciled` (an earlier send landed with a lost reply); a live document that fails validation →
`Collision`; **reversed and ours, or absent** → the duplicate is not the document under our id — our own is
reversed or never existed, so the server's live document of our kind under this order was issued by
someone else (the UI, another channel, another namespace on the same account) — and the order-number query
*names* it: `conflict{duplicate_order_number, existing_number}` when the newest document under the order is
a live document of our kind, without `existing_number` otherwise (another kind, reversed, or a failed
naming query). The order-number query names, it never adopts (rule 7). If it returns nothing while the
server just said "duplicate", the contradiction is `Unconfirmed` and the step re-executes. Correctives keep
their exemption: no order-number hint in lookup (the live base under the order is expected), and an
unresolvable 71/152 is `rejected`, not a conflict. A foreign document is reported in seconds — by the
lookup's hint or by this branch — never after a retry budget.

### `mode` is required, defaults to `live`, and is always validated

PRD story 16 first read "optional; unset means unchecked". Rejected on review: an operator always knows
whether an account is a test account, and unset-means-unchecked would have removed a default-on guard
that ADR 0002, ADR 0005, design §3 and both READMEs state — `teszt == account.mode` on every found
document. With the default `live`, a test account configured as live fails loudly on its first found
document (`account_mismatch` on a verify, `conflict{external_id_collision}` on a lookup) instead of issuing
on the wrong account.

### A credential-store outage is a terminal `unavailable`

The fetch is not a Restate retry. Three in-process attempts, then `TerminalError{unavailable}` (503),
`gone` at once. Terminal by decision: a retryable error would route a prolonged store outage into the
handler's `invocation_retry_policy` — five attempts, kill — and end as an unstructured 500 that looks like a
transport failure; the terminal fault is structured and immediate. **Documented cost:** an outage during a
*replay* of an invocation whose create already landed surfaces as `unavailable` although the document
exists — which is exactly what the caller contract already says an error means ("outcome unknown"); `get` or
a retry with a new `Idempotency-Key` answers `already_issued` (verified:
`failing_credential_store_is_a_terminal_unavailable`, the same order issuing once the store is back).

### `credentials_rejected` is 503

szamlazz.hu answering 3 (invalid credentials), 135 (browser session active), 136 (login blocked) or 164
(multiple accounts) to any step is the worker's misconfiguration, not the caller's request: the same
request succeeds once the key is fixed, so it is not a 4xx ("do not retry") and not 401/403 ("*you* are
unauthenticated" — the caller authenticated to the gateway, and the worker's key is not the caller's). It is
a fault and never `rejected`: the execution that saw the code issued nothing (szamlazz.hu answers these
codes before acting), but an earlier execution may have landed with a lost reply. Logged at `warn` with the
namespace and the code, never the key. `check_account` alone returns the same codes as data, because
reporting them is its purpose.

### Restate run-policy facts

The SDK's default retry policy for a `ctx.run` is `RetryPolicy::Infinite`, which sends no
`next_retry_delay`; the server then spends *this handler's* `invocation_retry_policy` (five attempts, kill)
on the run's failures. So every step that must not consume the invocation budget sets a policy explicitly —
the issue policy on the create and storno steps, the resolve policy on the `account` step, `max_attempts(1)`
on every read and one-shot write — and builds it with `RunRetryPolicy::new()` (factor 1.0, no caps), **not**
`RunRetryPolicy::default()`, which caps the delay at 2 s and the duration at 50 s. Verified end to end: with
a 1 s test policy the re-execution follows the run's delay, not the handler's 2 m.

### Rulings that shaped the decision

Reviewer and judge rulings during #20–#31, recorded so they are not re-litigated:

- **No default account.** `[account]` and `[accounts.<scope>]` are mutually exclusive; unscoped on the
  multi shape and scoped on the single shape are `unknown_account` (400). A default would make a
  mis-addressed request issue on it.
- **No header transport** (above).
- **No per-account client cache; a fresh client per execution.** The default `reqwest::Client` keeps
  szamlazz.hu's `JSESSIONID` cookie; a shared or cached client would carry one account's session into
  another account's request. The fresh client is a *session boundary*, not a performance choice. A resolver
  may cache accounts internally.
- **Mandatory `supplier_id` in the multi-account shape.** The only server-side account identity a found
  document exposes (`szallito/id`, in query bodies only); required to enforce rule 1 at load time and to
  validate every found document against the account the worker believes it is talking to.
- **Credential failures are faults**, never `rejected` (above).
- **Trait objects, not generics**, for the resolver and store: a type parameter would leak into the
  SDK-generated `OrderClient` / `AgentClient`.
- **The retry envelope stays deployment configuration.** A database row can never turn off the crash
  window or exceed szamlazz.hu's attempt etiquette.

## Considered options

- **N deployments.** Impossible in one Restate environment (fixed service names); and N Restate
  environments for N organizers is the cost the decision removes.
- **Header, body field, key prefix, invocation id as the account channel.** Rejected above.
- **Journal the credentials with the Account** (one durable step). Rejected: the journal is visible in the
  UI for the retention period and is exported by the SQL API; a rotation would not reach in-flight
  invocations; and story 8 demands that no code path can persist a key.
- **Fetch the credentials once per invocation and journal a token.** Rejected: a token is a credential with
  extra steps, and the store would need a second, journal-shaped API.
- **Virtual Object state per account.** Rejected: ADR 0005 — nothing to store that szamlazz.hu or the
  resolver does not already answer, and state would be per *key*, not per account.
- **A retryable credential-fetch error.** Rejected above (kill-on-five, unstructured 500).
- **`mode` optional, unset means unchecked.** Rejected above.
- **Keep the ingress-path guard.** Rejected above.
- **Per-event throttling via scopes.** Rejected for callers: a scope is identity, and one account under two
  scopes breaks rule 1. Limit keys exist for flow control and are not part of the identity (docs: "A limit
  key only influences concurrency. It is **not** part of an invocation's identity").

## Consequences

- Single-account deployments are unchanged for callers: the static resolver's `[account]` is served
  unscoped, `resolve(None)` is the account, any scope is unknown.
- Single → multi is a **flag day** with no data migration: make both services private (the ingress refuses
  new calls without creating invocations), drain `sys_invocation`, register the revision with
  `[accounts.<scope>]` keeping the namespace, point callers at scoped paths, make the services public, probe
  every scope. The first scoped create for an already-invoiced order finds it under the unchanged external
  id (verified: `flag_day_keeps_the_documents_and_refuses_unscoped_calls`). The same drain–switch–resume
  applies to any change of mapping (rule 2). Scripted in the endpoint README.
- Two new terminal codes: `unknown_account` (400), raised by the prologue before anything is issued, and
  `credentials_rejected` (503), whose raising execution issued nothing. `contract::TerminalCode` has six codes
  in all — the faults every handler may raise; `Szamlazz.Agent.query`, `set_payments` and `storno` keep their
  by-number 404 `not_found` and 422 pass-through of szamlazz.hu's own code (design §4) beside them. The
  caller-contract sentence is unchanged.
- The prologue adds two journal entries to every invocation (`namespace`, `account`) and one szamlazz.hu-free
  round trip to the resolver and the store per execution.
- A caller's `order_key` in a `StornoResponse` (`managed_by_order{key}`) is meaningful only under the scope
  the call was made under; `external_id` is the only namespace marker in any response, and no response
  names the account.
- Caller guidance, with the Pretix integration as the worked example, lives in the endpoint README: the
  account is a first-class entity in the caller, scope = the caller's account id, order key = event slug +
  order code, the webhook notification id as `Idempotency-Key`, limit keys for per-event throttling.
- The e2e suite runs against Restate with the three flags and asserts the secrecy guarantee, not assumes it.

## Superseded and amended sections of ADRs 0001–0005

- **ADR 0001.** Amended: neither service holds a gateway or a client; both hold the `Accounts` bundle and
  the `WorkerConfig`, and every handler's prologue opens a `Gateway` for its own execution. The module is
  `gateway` (first `steps`). `Szamlazz.Agent` gains `check_account`. The rest holds.
- **ADR 0002.** Superseded: "one szamlazz.hu account per deployment means the key carries no account
  namespace; the account slug lives in the external id" → the key carries no account namespace *because the
  scope does*; the deployment's **namespace** (then called the slug) lives in the external id and is shared
  by every account. The validation formula's `account.mode` / `supplier_id` are those of the invocation's
  journaled `Account`. The rest that ADR 0005 left standing holds.
- **ADR 0003.** Unchanged in substance; "where live is observed" (#22) already recorded.
- **ADR 0004.** Amended by #22 and #30 in place (the create and storno steps under the issue policy, the
  accepted retry-count risk, the run-policy facts restated above); the prologue's resolve policy and the
  in-process credential fetch are the two retry envelopes it did not have.
- **ADR 0005.** Amended: the validation pins (`teszt == account.mode ∧ (account.supplier_id unset ∨
  szallito/id == supplier_id)`) are read from the resolved `Account`, per invocation, and `supplier_id` is
  required in the multi-account shape; "pin `supplier_id` in config" means on the `Account`. The
  order-number hint is unconditional (the `detect_foreign` setting is gone). The rest holds.

## Historical notes

For readers of older commits and tickets: the module that speaks to szamlazz.hu was `steps` (`Steps`,
`steps::`) and is `gateway` (`Gateway`); the external-id prefix was the `AccountSlug` / `account.slug` /
`{slug}` and is the deployment-level `Namespace`; the `detect_foreign` setting was removed when the
order-number hint became unconditional in the lookup step; the sentences "one deployment serves one
szamlazz.hu account" and "a second account is a second deployment" were removed from the design and both
READMEs. Outside the ADRs' own status lines and superseded text, these words appear nowhere else in the
repository on purpose; the endpoint's configuration loader still names `account.slug` in the error that
refuses the pre-release layout.
