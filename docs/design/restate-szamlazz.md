# restate-szamlazz — design and implementation spec (v2, stateless)

Status: accepted for implementation. Supersedes v1 (the ledger design; see ADR 0005 for why). Decisions are recorded
as ADRs 0001–0005, the verified szamlazz.hu behaviour it relies on in [`szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md).

## 1. Goal

Expose the basic szamlazz.hu Számla Agent operations as Restate services with durable execution, so that a caller
can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes,
concurrent callers and reversals — with a JSON API that is a *projection* of the Agent model: deployment constants
moved to config, line totals computed, one handler per document kind, no PDF.

Non-goals: PDF download, receipts, taxpayer query, IPN / Adatkapcsolat ingestion, the proforma → payment → invoice
lifecycle workflow. Several szamlazz.hu accounts in one deployment, selected per request by the Restate scope, are in
scope since #20 (§9, §11).

## 2. Crates

| Crate | Kind | Purpose |
|---|---|---|
| `restate-szamlazz` | library | Contract types, config, the `gateway` module, the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service |
| `restate-szamlazz-endpoint` | binary `restate-szamlazz`, container `ghcr.io/sagikazarmark/restate-szamlazz` | Hosts the services over HTTP for a Restate server; clap + figment config |

`restate-sdk` is an unconditional dependency; the only feature is `schemars`.

Layering (ADR 0001): the `gateway` is a Rust module that speaks to szamlazz.hu on behalf of one account — it owns the
`szamlazz_agent::Client` (the transport it wraps; it is not a second client) and the account, and exposes one plain
async fn per durable step with outcome-as-data. `Szamlazz.Order` calls it inside `ctx.run`; `Szamlazz.Agent` is a thin
stateless facade over it for by-number operations. Every read of account configuration by the services — the
ownership-validation pins, the document defaults, the seller block — goes through `Gateway::account()`. The services
hold no gateway: each holds the `Accounts` bundle (account resolver + credential store) and a `WorkerConfig` with the
deployment-level settings (the namespace of the external ids, the issue and resolve policies), and every handler's
prologue (§4) resolves its account and opens a gateway for its own execution. No Restate service calls another; no
`Order` handler calls a handler on its own key.

## 3. Principle: szamlazz.hu is the source of truth (ADR 0005)

`Szamlazz.Order` keeps **no state**. The Virtual Object exists for its per-key lock: at most one handler runs for an
order at a time. Everything else is answered by querying szamlazz.hu through deterministic external ids:

- **VO key** = the order number, trimmed of leading/trailing whitespace, case preserved (the server trims and is
  case-sensitive — verified). Validation: 1–64 bytes after trim, no control characters, no whitespace runs →
  `invalid_input`.
- **External id** (`szamlaKulsoAzon`), deterministic from the key under the deployment's **namespace** (chosen by the
  operator, opaque to szamlazz.hu, permanent; 1–16 bytes of `[a-z0-9-]`, `:` excluded as the separator), so *any*
  invocation can find what an earlier one issued:
  - slot kinds: `"{namespace}:{order}:{kind}"`, `kind ∈ proforma | invoice | prepayment | final`
  - correctives: `"{namespace}:{order}:corrective:{correction_id}"` (caller-supplied id; several correctives per invoice
    are legitimate)
  - storno: `"{namespace}:{order}:storno:{original_number}"`
  - Ext ids are not unique server-side; a query returns the **newest** holder (verified). That is exactly the
    question we ask — "what is the newest document of this kind we issued for this order?" — and it is why a reissue
    after a storno needs no generation counter: the new document becomes the newest holder, the old one stays
    reachable through the storno's `hivszamlaszam`.
  - Every `Found` document is validated before it is trusted: `rendelesszam == order ∧ tipus ∈ kind-set ∧ teszt ==
    account.mode ∧ (account.supplier_id unset ∨ szallito/id == supplier_id)`; anything else →
    `conflict{external_id_collision}`.
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
| `create_prepayment` | exclusive | `CreateRequest` → `CreateResponse` (v1: one prepayment per order) |
| `create_final` | exclusive | `CreateRequest` → `CreateResponse` (requires a live prepayment; passes `elolegSzamlaszam`) |
| `correct_invoice` | exclusive | `CorrectRequest { invoice_number, correction_id, document }` → `CreateResponse` |
| `storno_invoice` | exclusive | `StornoRequest { invoice_number, comment? }` → `StornoResponse` |
| `delete_proforma` | exclusive | `DeleteProformaRequest { force }` → `DeleteProformaResponse` |
| `get` | shared | `()` → `OrderStatus` (live view) |

Attributes on every handler that calls szamlazz.hu (ADR 0004):

```
invocation_retry_policy(initial_interval = "2m", factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")
inactivity_timeout = "4m"   abort_timeout = "3m"   journal_retention = "3d"   idempotency_retention = "30d"
```

`get`: default retry policy, `max_attempts = 3`, `kill`, `journal_retention = "1d"` (inspectable, nothing to replay).

### The prologue (every handler of both services)

After parsing its key, every handler runs the same four steps before its operation; the handler body then runs on
the resulting *execution* — the gateway opened for this execution plus the deployment settings with the pinned
namespace — and nothing of it (gateway, client, credentials) outlives the execution. No Virtual Object state.

1. **Pin** — `ctx.run("namespace", || namespace)`, a pure durable step: an in-place redeploy with a changed namespace
   cannot make a running invocation issue under a new id.
2. *(The ingress-path guard of #27 slots in here.)*
3. **Resolve** — `ctx.run("account", || resolver.resolve(ctx.scope()))` under the **resolve policy** (§9), an
   explicit run retry policy bounded by duration. The closure returns the resolver's answer as data — the `Account`,
   `unscoped`, `unknown{scope}` — and its unavailability as a retryable error (whose text never echoes the
   resolver's own message), so unscoped/unknown are journaled and never retried while an outage re-executes the
   step and journals nothing. Outside the closure: `unscoped | unknown` → `TerminalError{unknown_account, 400}`;
   exhaustion or cancellation of the run → `TerminalError{unavailable}`. One `account` entry per invocation: the
   invocation finishes on the account it started on, and the Restate UI shows the journaled `Account` (id, mode,
   supplier id, endpoint, defaults, seller, credential reference — never the key) for the retention period.
4. **Fetch** — `store.fetch(account.credential_ref)` **outside the journal**, on every handler execution
   including replays, with a short in-process retry (three attempts, 200 ms apart), then
   `TerminalError{unavailable}`. `gone` is terminal at once. Terminal by decision: a retryable error would route a
   prolonged store outage into the handler's kill-on-five and an unstructured 500, whereas the terminal fault is
   structured and immediate. Documented cost: an outage during a replay of an invocation whose create already
   landed surfaces as `unavailable` although the document exists; `get` or a retry with a new `Idempotency-Key`
   reconciles (`already_issued`). The `Credentials` type has no serde implementation — the compiler rejects any
   attempt to journal it.
5. **Open** — `Gateway::open(account, credentials)` over a fresh Számla Agent client (the default `reqwest::Client`
   keeps szamlazz.hu's `JSESSIONID`; a shared client would carry one account's session into another's request).

Handler-level behaviour is observable only under Restate (the SDK has no mock context), so the prologue's decisions
are pure functions with unit tests (`service::prologue`) and the durable behaviour is asserted end to end (§11).

### `Szamlazz.Agent` (stateless Service, `#[restate_sdk::service(name = "Szamlazz.Agent")]`)

| Handler | Input → Output | Notes |
|---|---|---|
| `query` | `QueryRequest { selector }` → `QueryResponse` | projection of `InvoiceDocument`; 7 → `TerminalError` 404 `not_found`; 3/135/136/164 → `credentials_rejected`; `journal_retention = "1d"` |
| `set_payments` | `SetPaymentsRequest { invoice_number, entries[≤5], additive }` → `SetPaymentsResponse` | `RegisterCreditEntry`; `max_attempts = 2, kill`; run `max_attempts(1)`; 3/135/136/164 → `credentials_rejected` |
| `storno` | `StornoRequest` → `StornoResponse` | verify first; document carries `rendelesszam` → `outcome: managed_by_order{key}`; else the lookup and storno steps of §6 under ext id `"{namespace}:by-number:{number}:storno"` (the storno step under the issue policy; exhaustion → `outcome_unknown`); 3/135/136/164 → `credentials_rejected` |

## 5. Create protocol (`create_invoice`; other kinds analogous)

Steps 0–2 and the lookup are `ctx.run`s with `RunRetryPolicy::max_attempts(1)` whose expected outcomes are data. The
create step is the one `ctx.run` under a retry policy — the **issue policy** (§9) — because it is the one step whose
outcome can be *unknown*. The storno step (§6) is the other write step and runs under the same policy.

0. **Validate (pure).** Order key from `ctx.key()`. Validate buyer, items (≥ 1), dates. Normalise `buyer.name`.
   Compute line totals with `LineItem::calculated_for_currency`. Build `CreateInvoice` from input + the account's
   defaults and seller block (read through the gateway) + per-call overrides,
   `external_id = "{namespace}:{order}:invoice"`, `download_pdf = false`.
1. **Exclusivity.** `ctx.run(query "{namespace}:{order}:prepayment")`: live `ES` → `conflict{prepaid_chain, existing_number}`.
   (`create_prepayment` mirrors this against `…:invoice`.) `Transport` → `TerminalError{unavailable}`. A document under
   the secondary id that fails validation → `conflict{external_id_collision, number}`: the query returns the newest
   holder, so a foreign document may hide a live document of ours behind it, and refusing is the only safe answer.
2. **Proforma link** (`options.proforma`; `create_invoice` only — see kind specifics):
   - `auto` (default): `ctx.run(query "{namespace}:{order}:proforma")` → live `D` → pass `dijbekeroSzamlaszam`; 7 → none.
   - `none`: same query; live `D` → `conflict{proforma_live, existing_number}` (the server links by shared order
     number regardless — verified — so refusing is the only honest answer).
   - `{number}`: `ctx.run(verify number)`; 7 → `conflict{proforma_missing}`; `tipus ≠ D` → `invalid_input`.
   Under `auto` and `none`, a document under `…:proforma` that fails validation → `conflict{external_id_collision,
   number}` (as in step 1).
   Collect the numbers seen in steps 1–2 as `our_numbers` (for foreign detection).
3. **Lookup** — one read-only durable step, `ctx.run("lookup-{kind}", || gateway.lookup(LookupRequest{ external_id,
   kind, order, our_numbers }))`. The gateway validates every found document against its own account (`teszt`,
   supplier pin); the request carries only what identifies the document. In one closure:
   - `QueryInvoiceXml(ExternalId)` → `Ok(doc)`: validate → `Collision(doc)` on mismatch; live → `Live(doc)` (no
     hint: nothing will be created); reversed (`sztornozott == Some(true)`) → remember it and continue. 7 → continue.
     3/135/136/164 → `CredentialsRejected{code, message}`. Transport → `Transport`.
   - The order-number hint, on every lookup **except for correctives**: `QueryInvoiceXml(OrderNumber)` → a live
     `SZ|ES|VS` whose number ∉ `our_numbers` and ≠ the document seen under our ext id → `Foreign(doc)` — also when
     our own document under the id is reversed, since no create may proceed past it; a miss (7) or another API error
     says nothing about foreign documents and continues; 3/135/136/164 → `CredentialsRejected` (conclusive: nothing
     proceeds); the hint's transport failure → `Transport` (nothing may be concluded).
   - Otherwise `Absent`, or `Reversed{doc, storno_number?}` with the storno number when the hint is the `SS` whose
     `hivszamlaszam` is the reversed document (absent otherwise, and for correctives).
   It settles every case that needs no create: `Live` → `reissue ? conflict{live, number} : already_issued{number,
   totals}`; `Reversed` → `reissue ? proceed, remembering the number : outcome: reversed{number, storno_number?}`;
   `Collision` → `conflict{external_id_collision, number}`; `Foreign` → `conflict{foreign, existing_number}`;
   `Transport` → `TerminalError{unavailable}`; `CredentialsRejected` → `TerminalError{credentials_rejected}` (§7);
   `Absent` → proceed.
4. **Create** — one durable step under the issue policy, `ctx.run("create-{kind}", || gateway.create(CreateStepRequest{
   external_id, kind, order, create, reversed }))` with `RunRetryPolicy::new().initial_delay(2m)
   .exponentiation_factor(2.0).max_delay(10m).max_attempts(5).max_duration(1h)` (§9). **Every execution is
   query-first, inside the closure** — a separate journaled pre-query would replay its stale "nothing" on the retry
   and re-send. The gateway returns settled-vs-unconfirmed: every szamlazz.hu answer is `Ok(CreateOutcome)`, and
   `Err(Unconfirmed)` (a plain `std::error::Error`, retryable to the SDK) is the one thing the policy re-executes. The
   closure never returns a `TerminalError` itself.
   - Leading query `QueryInvoiceXml(ExternalId)` → a validated live document that is not `reversed` → `Found(doc)`
     (an earlier execution created it; **nothing is sent**); invalid → `Collision(doc)`; 7 or the reversed document →
     send; 3/135/136/164 → `Ok(CredentialsRejected{code, message})` (settled, nothing sent); transport →
     `Err(Transport)` (never create when the check itself failed).
   - `CreateInvoice` → success with a number → `Issued(r)`; an API rejection → `Rejected{code, message}`; 3/135/136/164
     → `CredentialsRejected{code, message}` — settled data, **not** `Unconfirmed`: re-executing with the same key would
     only repeat the answer, so the run policy is not spent on it.
   - Transport failure or an open code (1, 55, 56 without a number, `szlahu_down`): re-query the external id once,
     immediately (read-your-writes lag ≈ 0) → found live → `Found(doc)`; collision → `Collision`; nothing →
     `Err(Transport | Open)`. The run policy then re-executes the whole handler after the delay: the journal
     replays to the create step and the leading query runs again — the re-check ADR 0002 sizes the 2-minute gap for.
   - 71/152: re-query the external id → live and ours → `Reconciled(doc)`; not ours → `Collision(doc)`; reversed and
     ours, or absent → the duplicate is not ours. For correctives that is `Rejected{code, message}` (exempt from the
     order-number check — verified; no order-number query). Otherwise `QueryInvoiceXml(OrderNumber)` names it:
     the newest document under the order is a live document of our kind → `DuplicateOrderNumber{code, message,
     existing_number}`, another kind or reversed → without `existing_number`, a failed naming query → without it;
     7 (nothing under the order, yet 71/152) → `Err(Contradiction)`, retryable.
   Any `Err` from the run — exhaustion (`TerminalError` 500 carrying the last `Unconfirmed`) or cancellation (409)
   — is mapped by the handler to `TerminalError{outcome_unknown, json{order, kind, external_id}}`; a cancel
   mid-create therefore reports `outcome_unknown`. Nothing is recorded: the next invocation's lookup finds whatever
   landed.
5. **Branch on data.** `Issued(r)` → `outcome: issued` (+ `warnings: [notification_delivery_failed]`); `Found(doc)`
   → `outcome: issued{number, totals}` (the caller asked for this document and has it — ADR 0003); `Reconciled(doc)`
   → `reconciled{number, totals}`; `Collision(doc)` → `conflict{external_id_collision, number}`;
   `DuplicateOrderNumber` → `conflict{duplicate_order_number, code, message, existing_number?}`; `Rejected` →
   `rejected{code, message}`; `CredentialsRejected{code}` → `TerminalError{credentials_rejected, 503, json{order, kind,
   external_id}}` and a warning tagged with the namespace and the code (§7) — the execution that observed the code
   issued nothing, an earlier one may have landed with a lost reply, which is why this is a fault and never `rejected`.
6. **Crash path.** A crash mid-closure leaves no journal entry; Restate re-dispatches after the handler's
   `initial_interval` (2 m > 60 s client timeout + observed stalls) with the journal; completed runs replay; the open
   `create-…` closure re-executes and begins with the external-id query, so a landed create is `Found`, not
   re-issued. Second guard: with the toggle ON, a byte-identical resend while the first document is live is answered
   with the same number.

Kind specifics: `create_proforma` — kind `D`; no exclusivity step; `proforma` option not applicable. `create_prepayment`
— exclusivity against `…:invoice`; `proforma` option not applicable (anything but `auto` → `invalid_input`) and **no
step 2**: the Agent's prepayment invoice cannot carry `dijbekeroSzamlaszam`, and the server converts the order's live
`D` by shared order number regardless (verified — an `ES` issued without the reference shows `hivdijbekszam`), so `get`
derives `consumed` from the `ES`. `create_final` — `ctx.run(query "…:prepayment")` must be a live `ES` (7 →
`conflict{prepayment_missing}`, reversed → `conflict{prepayment_reversed}`, fails validation →
`conflict{external_id_collision}`); passes `elolegSzamlaszam`; the server
enforces one final per prepayment (73 → `rejected`); the server does not net the prepayment into the final's totals.
`correct_invoice` — `ctx.run(verify invoice_number)`: 7 → `invalid_input`, reversed → `conflict{base_reversed}`,
`rendelesszam ≠ key` → `conflict{not_managed}`; ext id `…:corrective:{correction_id}`; the same lookup and create
steps with the corrective exemption (verified): no order-number hint — the live base invoice under the order is
expected — and a 71/152 the re-query cannot resolve is `rejected`, not a conflict; a new `correction_id` issues a new
corrective by contract.

## 6. Storno protocol (`Szamlazz.Order.storno_invoice`)

Storno is natively idempotent on the server (a repeat echoes the existing storno — verified) and an external id on
the storno request attaches to the storno document (verified). It has the shape of issuing (§5): a read-only lookup
step and one write step under the issue policy, query-first on every execution.

1. **Verify.** `ctx.run(verify number)`: 7 → `invalid_input{not_found}`; `rendelesszam ≠ key` → `conflict{not_managed}` (use
   `Szamlazz.Agent.storno`); `teszt` / supplier pin mismatch → `TerminalError{account_mismatch}`; `sztornozott ==
   Some(true)` → `outcome: reversed{storno_number?}` (idempotent; storno number via the hint when the newest document
   is the matching `SS`); `tipus ∉ {SZ, ES, VS, HS}` → `rejected{not_stornoable}`.
2. **Lookup** — one read-only durable step, `ctx.run("lookup-storno-{number}", || gateway.lookup_storno(external_id,
   number))` with `external_id = "{namespace}:{order}:storno:{number}"`: query by the storno ext id → `SS` with
   `hivszamlaszam == number` → `AlreadyReversed{storno_number}` → `outcome: reversed{storno_number}`; 7 or another
   holder → `Absent` → proceed (a storno is idempotent server-side, so a stray holder is not a stop); 3/135/136/164 →
   `TerminalError{credentials_rejected}`; transport → `TerminalError{unavailable}`.
3. **Storno** — one durable step under the issue policy (§9), `ctx.run("storno-{number}", || gateway.storno(StornoStepRequest{
   number, external_id, comment, e_invoice }))`. **Every execution is query-first, inside the closure** (the rule of §5
   step 4): the gateway returns `Ok(StornoOutcome)` for every known answer and `Err(Unconfirmed)` — retryable to the
   SDK — only when szamlazz.hu's answer is not known; the closure never returns a `TerminalError` itself.
   (a) leading query by the storno ext id → the matching `SS` → `AlreadyReversed{storno_number}` (an earlier
   execution sent it; **nothing is sent**); 3/135/136/164 → `Ok(CredentialsRejected)`; transport → `Err(Transport)`
   (never send when the check itself failed);
   (b) send `xmlszamlast{szamlaszam, szamlaKulsoAzon}` **without `keltDatum`** (352 otherwise — verified);
   (c) validate: `invoice_number ≠ requested ∧ gross < 0` → `Reversed`; echo of the requested number →
   `NotStornoable`; API errors → `Rejected{code, message}` with the raw szamlazz.hu code (`14` = storno of a storno,
   `221` = has a corrective — typed in `szamlazz_agent::ErrorCode`, surfaced as the code string); 3/135/136/164 →
   `CredentialsRejected{code, message}`; a transport failure or an open code (1, 55, 56, `szlahu_down`) → re-query
   the storno ext id once, immediately → the matching `SS` → `AlreadyReversed{storno_number}` (what was sent landed);
   nothing → `Err(Transport | Open)`, and the run policy re-executes the step after its delay, beginning again at (a).
   Any `Err` from the run — exhaustion (500) or cancellation (409) — is mapped to `TerminalError{outcome_unknown,
   json{order, kind, external_id}}`; nothing is recorded, the next call's steps 1–2 find the storno if it landed.
4. **Branch on data.** `Reversed | AlreadyReversed` → `outcome: reversed{storno_number}`; `NotStornoable` →
   `rejected{not_stornoable}`; `Rejected` → `outcome: rejected{code, message}`; `CredentialsRejected` →
   `TerminalError{credentials_rejected}` (that execution issued nothing).

`e_invoice` for the storno: the verified document's `eszamla` when known, else the account default.

`Szamlazz.Agent.storno` runs the same lookup and storno steps under `"{namespace}:by-number:{number}:storno"` after its
own verify (§4).

`delete_proforma({force})`: `ctx.run(query "…:proforma")`: 7 → `{deleted: true, reason: absent}` (deleted or consumed —
`get` tells which); a document under our id that fails validation → `{deleted: false, reason: external_id_collision}`;
live `D` with payments ∧ `!force` → `rejected{proforma_paid}` (the server has no guard — verified);
`ctx.run(DeleteProforma{InvoiceNumber})` (`max_attempts(1)`): success | 335 → `{deleted: true}`; 3/135/136/164 →
`TerminalError{credentials_rejected}`; other → `rejected{code}`; `Transport` → `TerminalError{outcome_unknown}`.

`get`: four `ctx.run` queries (`…:proforma|invoice|prepayment|final`) → `OrderStatus { proforma?, invoice?,
prepayment?, final?: DocumentStatus }` where `DocumentStatus { number, state: live | reversed | consumed{by},
gross, net, payments: [amounts], referenced_proforma?, e_invoice? }`. `get` does not look up the storno number (that
would need the order-number hint, which only shows the newest document); the create and storno handlers report it
when the hint yields it. A proforma that is absent while the invoice references it (`hivdijbekszam`) is reported as
`proforma: { state: consumed, by: invoice_number }`. A document under an id that fails validation leaves its slot
absent — a read must not fail; the issuing handlers are the ones that refuse it as `conflict{external_id_collision}`.
`Transport` on any query → `TerminalError{unavailable}`; 3/135/136/164 on any query → `TerminalError{credentials_rejected}`.

## 7. Outcome contract

Domain outcomes are **data** (HTTP 200 through the ingress, typed in the OpenAPI export). `TerminalError` is reserved
for faults.

```
CreateResponse { outcome, conflict_reason?, kind, external_id,
                 invoice_number?, storno_number?, net_total?, gross_total?, outstanding?, customer_account_url?,
                 existing_number?, code?, message?, warnings: [] }
outcome ∈ issued | already_issued | reconciled | reversed | rejected | conflict
conflict_reason ∈ prepaid_chain | live | foreign | duplicate_order_number | external_id_collision
                | proforma_live | proforma_missing | prepayment_missing | prepayment_reversed | base_reversed | not_managed
warnings ∈ notification_delivery_failed
StornoResponse { outcome ∈ reversed | rejected | conflict | managed_by_order, conflict_reason?,
                 invoice_number, storno_number?, order_key?, code?, message? }
DeleteProformaResponse { deleted, reason? }
OrderStatus — see §6
TerminalError codes: outcome_unknown (500) | unavailable (503) | account_mismatch (409) | invalid_input (400)
                   | credentials_rejected (503) | unknown_account (400)
```

`unknown_account`: the request names no account of this deployment — it arrived unscoped where accounts are reachable
by scope only, or under a scope no account is reachable by (on a single-account deployment, any scope). Raised by the
prologue's `account` step before anything is issued; the same request never succeeds, so it is a 400 and the caller
fixes the scope rather than retrying. `unavailable` also covers the prologue's own faults: the resolve policy
exhausted, the credential store gone or unavailable through the in-process retry.

`credentials_rejected`: szamlazz.hu answered 3 (invalid credentials), 135 (browser session active), 136 (login blocked)
or 164 (multiple accounts) to any step of any handler. It is the worker's misconfiguration, not the caller's request —
the same request succeeds once the key is fixed — so it is 503, not a 4xx ("do not retry") or 401/403 ("you are
unauthenticated"). The execution that observed the code issued nothing — szamlazz.hu answers these codes before acting
on a request, and on the 71/152 re-query path the create was already refused as a duplicate; an earlier execution may
have landed with a lost reply, which is why it is a fault under the "every error means outcome unknown" rule and never
`rejected`. Every
occurrence is logged at `warn` with the namespace and the code (never the key).

## 8. Caller contract (documented in the crate READMEs)

1. Send an `Idempotency-Key` per logical request; Restate dedupes retries and attaches concurrent duplicates to the
   in-flight invocation.
2. **Any error** from an issuing or storno handler means "outcome unknown — retry with a **new** key" (the stored
   completion of a failed invocation is replayed for the retention period — verified); the handler reconciles by
   external id, so the retry is safe. Never interpret an error as "no document exists".
3. After a storno — by this service, the UI or anyone — a create returns `outcome: reversed`. Send `reissue: true`
   (with a new key) when a new invoice is actually wanted. `reissue: true` on a live document → `conflict{live}`; the
   flag can never cause a duplicate.

## 9. Configuration (deployment-constant; never in payloads)

Two configuration types, both serde-`Deserialize` only (the host chooses the format). `WorkerConfig` is the
deployment-level part the services hold; `StaticConfig` is the static resolver's account, and everything
account-shaped — credentials, mode, supplier pin, endpoint, document defaults, seller block — lives on the `Account`
it produces (read by the services through `Gateway::account()`). The endpoint binary flattens both into one file:

```toml
namespace = "acct"            # 1–16 bytes of [a-z0-9-]; prefixes every external id; permanent

[issue]      # the issue policy: the run retry policy of the create (§5 step 4) and storno (§6 step 3) steps; shapes no journal entry
max_attempts = 5              # executions of the step, including the first
initial_delay = "2m"          # before the first re-execution; > client timeout + the longest observed server stall
factor = 2.0
max_delay = "10m"
max_duration = "1h"           # the hard bound (the attempt count is not durable across replays — ADR 0004)

[resolve]    # the resolve policy: the run retry policy of the prologue's `account` step; no attempt cap — the duration is the bound
initial_delay = "1s"
factor = 2.0
max_delay = "10s"
max_duration = "1m"

[account]
id = "acme"                   # the resolver's identifier of the account; journaled with every invocation, never a resolution input
agent_key = "..."             # or env RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY; inline in the static resolver, credential_ref = id
endpoint = "https://www.szamlazz.hu/szamla/"   # optional (wiremock in tests)
mode = "live"                 # live | test — validated against <teszt> on every adopted document
supplier_id = 972720          # optional; when set, validated against szallito/id on every adopted document

[account.defaults]   # as v1: e_invoice, language, currency, exchange_rate_bank, template?, send_email?, number_prefix?, extra_logo?, aggregator?, guardian?
[account.seller]     # as v1
```

Both policies are set explicitly on the runs because the SDK's default run policy sends no retry delay and the
server would spend the handler's `invocation_retry_policy` instead. `WorkerConfig::validate` checks the cross-field
invariants (`max_attempts ≥ 1`, `initial_delay ≤ max_delay`, `factor ≥ 1` on both policies);
`StaticResolver::try_from` validates the account (non-blank id and key, an http(s) endpoint). The pre-release layout
(`account.slug`, top-level `[defaults]` / `[seller]`) is refused by name — the crate has never been released, there
is no compatibility shim.

**Multi-account shape.** Instead of `[account]`, a table of `[accounts.<scope>]` with the same fields; the two are
mutually exclusive (both present is a load error) and there is no default account. Each account is reachable under
its scope only (`/restate/scope/{scope}/call/…`); an unscoped request is `unknown_account`, as a scoped request is on
the single-account shape. Load-time validation enforces the checkable half of the resolver's safety contract — one
szamlazz.hu account under exactly one scope, no fan-in: `supplier_id` required on every account (the only
server-side account identity), unique supplier ids, unique `(endpoint, agent_key)` pairs, unique ids (the credential
reference). Scope keys are `[a-z0-9_]`, 1–36 bytes: a strict subset of Restate's scope format (`[a-zA-Z0-9_.-]`,
non-empty, ≤ 36 bytes — a dashed UUID is exactly 36) chosen so that environment overrides can address them
(`RESTATE_SZAMLAZZ_ACCOUNTS__<SCOPE>__AGENT_KEY`; figment lowercases the segment). This is the documented constraint
on the account identifiers a caller uses as scopes with the static resolver. The namespace is one per deployment and
shared by every account.

```toml
namespace = "acct"

[accounts.acme]
id = "acme"
agent_key = "..."
supplier_id = 972720          # required

[accounts.beta_events]
id = "beta"
agent_key = "..."
supplier_id = 972721
```

**Single → multi flag day** (no data migration; the namespace stays, so the first scoped create for an
already-invoiced order finds it under the unchanged external id): make both services private
(`PATCH /services/{name} {"public": false}` — the ingress refuses new calls without creating invocations), poll
`sys_invocation` until no row has `status <> 'completed'`, register the new revision with the switched configuration
(a new deployment URI), point callers at scoped paths, make the services public. The drain is what keeps one
szamlazz.hu account from being reachable unscoped and under its scope at the same time. The same drain–switch–resume
applies to any change of the scope → account mapping, which is append-only. Scripted in the endpoint README and
performed by the e2e suite (§11).

There is no `detect_foreign`: the order-number hint runs on every lookup except for correctives.

Per-call inputs (`DocumentInput`) as v1: `buyer`, `items`, `fulfillment_date`, `due_date`, `payment_method`, `paid`,
`comment?`, `issue_date?`, overrides.

## 10. Endpoint

`restate-szamlazz --config <file> --port 9080`; `RESTATE_SZAMLAZZ_*` env with `__` nesting
(`RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY`, `RESTATE_SZAMLAZZ_ACCOUNT__DEFAULTS__CURRENCY`); `identity_keys`; tracing;
container image on `v*` tags. The start-up log names the namespace and the resolved account's id, mode, endpoint and
supplier id — never the key.

## 11. Testing

- `gateway`: wiremock tests using upstream-shaped responses — the lookup matrix (`Absent`, `Live`, `Reversed` with
  the storno number from the hint, `Collision`, `Foreign`, the corrective's exemption from the hint), the create step
  (`Issued`, `Found` on a re-executed step, `Rejected`, the open codes re-queried once and `Unconfirmed` when nothing
  landed, the 71/152 matrix incl. `existing_number` and the contradiction, the corrective's 71/152 → `Rejected`),
  storno validation incl. the D/SL no-op, 335, 7, and the credential codes 3/135/136/164 as `CredentialsRejected` on
  every operation (both lookup queries, the create's leading query and send, storno, delete, credit entries, query);
  the gateway validates found documents against the account it was opened for.
- `service`: discovery test (names, handler set, attributes), an endpoint build smoke test, `prepare` refusing
  `options.proforma` on every kind but `create_invoice`, the issue policy's field-for-field mapping onto
  `RunRetryPolicy`, the fault → status mapping, and a sentinel test that the agent key reaches neither the
  `credentials_rejected` warning nor the fault body.
- End to end (docker-gated): Restate 1.7.8 with `RESTATE_EXPERIMENTAL_ENABLE_VQUEUES`, `…_PROTOCOL_V7` and
  `…_SCOPED_VIRTUAL_OBJECTS` (the harness asserts them on `/version`; `compose.yaml` matches) + wiremock as
  szamlazz.hu — issued → already_issued (new key) and Idempotency-Key replay (same key, create mock `expect(1)`);
  152 → reconciled; storno → reversed; stale create → reversed; `reissue` → issued as newest holder; `reissue` on
  live → `conflict{live}`; `sztornozott` → reversed; proforma auto-link and `consumed` in `get`; `get` shape; a
  collision on the secondary (`…:prepayment`) lookup → `conflict{external_id_collision}` with the create mock
  `expect(0)` and the slot absent in `get`; `create_prepayment` refusing `options.proforma` and issuing without a
  proforma lookup; an exhausted create step (every reply lost, a short test policy) → a structured `outcome_unknown`
  500 within the run policy's delays, not the handler's, with `sys_invocation.retry_count = 1` and
  `last_failure_related_command_name = create-invoice` observed **while in flight** (attempt state is cleared on
  completion); a scoped `Szamlazz.Agent.query` and a scoped `Szamlazz.Order` call reaching the handlers with the
  scope on `sys_invocation`; a purged `get` invocation querying szamlazz.hu again; and the leak check's positive
  control — a sentinel in a szamlazz.hu rejection found in the hex-decoded `raw` of the create run's
  `Notification: Run` row (under journal v2 the `Command: Run` row carries only the name; the result is in the
  notification that follows), and nowhere else but the output. The prologue (on the same harness, through a
  test-local scripted resolver and store wrapping the static one): every invocation's journal opens with
  `namespace` and exactly one `account` run, and the journaled account carries its id and never the agent key; a
  scoped call on the single-account deployment → 400 `unknown_account` with `namespace`, `account` and nothing
  else journaled and zero szamlazz.hu requests; a resolver that fails twice then answers → the outcome, with the
  `account` run's retries visible on `sys_invocation` (`last_failure_related_command_name = account`, the failure
  text never echoing the resolver's message) within the resolve policy's delays, and still one `account` entry; a
  store that fails every fetch → 503 `unavailable` after three fetches, zero szamlazz.hu requests, and the same
  order issuing once the store is back.
  Then, on the same server, the **flag day** and the **multi-account phase** (two accounts behind a test-local
  mutable resolver and store seeded from the static resolver's `[accounts.<scope>]` shape: `acme` is the phase-1
  account — same key, same supplier id — and `beta` a second one): while private the ingress answers 400 with no
  invocation; after the drain and the switch the first scoped create for the order phase 1 invoiced →
  `already_issued` under the unchanged external id; unscoped → 400 `unknown_account` naming the scoped path, with
  `namespace` and `account` journaled and zero szamlazz.hu requests, and an unknown scope likewise; the same order
  key under `acme` and `beta` concurrently with the **same** `Idempotency-Key` → two invocation ids, two `issued`
  with each account's own key on the wire exactly once (and the key replaying each scope's own completion);
  `create_invoice` → purge → `storno_invoice` → `reversed` → purge → `create_invoice {reissue}` → `issued` on an order
  Restate holds nothing of, then the scoped `get` seeing the new holder; `acme`'s seller bank account changed between
  two executions of a create step (the first loses its reply) → both executions carry the journaled bank account
  and only a new invocation sees the change; `beta`'s key rotated between two executions → the second carries the
  new key while the `account` entry read in flight and after completion is byte-identical; and, over every
  `sys_journal` row of every invocation the server holds (hex-decoded `raw`) plus every
  `sys_invocation.completion_failure`, none of the three agent keys of the run — while the same scan finds the
  positive control's sentinel.
  The harness (`tests/service.rs`) calls through `/restate/call/…` and `/restate/scope/{scope}/call/…`, returns the
  `x-restate-id` and a parsed fault body, reads `sys_journal` (`raw` hex-decoded to bytes — run results are bytes and
  render as integer arrays in `entry_json`) and `sys_invocation`, and purges invocations (`PATCH
  /invocations/{id}/purge`). `get` and `Szamlazz.Agent.query` set `journal_retention = 1d` so their journals are
  inspectable.
- Live: the go-live checklist in `szamlazz-hu-behaviour.md`, to be automated as ignored tests (issue #15).

## 12. What v2 gives up relative to v1 (deliberately)

`request_id` retry identity (→ `Idempotency-Key`), `conflict{payload_mismatch}` (a different payload for a live
document is `already_issued`), flag-free reissue after a service-side storno (→ `reissue: true` after any reversal),
`recorded_document_missing` (a document szamlazz.hu no longer knows is simply absent — live accounts cannot delete
invoices), `payments_before` capture on storno (query before stornoing), the ledger snapshot (`get` is 4 live
queries), operator handlers `record_reversal`/`forget` (nothing to repair), the account fingerprint learned into
state (pin `supplier_id` in config), schema versioning and state migrations.
