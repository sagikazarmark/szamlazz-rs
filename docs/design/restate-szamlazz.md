# restate-szamlazz — design and implementation spec (v2, stateless)

Status: accepted for implementation. Supersedes v1 (the ledger design; see ADR 0005 for why). Decisions are recorded
as ADRs 0001–0005, the verified szamlazz.hu behaviour it relies on in [`szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md).

## 1. Goal

Expose the basic szamlazz.hu Számla Agent operations as Restate services with durable execution, so that a caller
can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes,
concurrent callers and reversals — with a JSON API that is a *projection* of the Agent model: deployment constants
moved to config, line totals computed, one handler per document kind, no PDF.

Non-goals for v1: PDF download, receipts, taxpayer query, IPN / Adatkapcsolat ingestion, the proforma → payment →
invoice lifecycle workflow, multiple szamlazz.hu accounts in one deployment.

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
ownership-validation pins, the document defaults, the seller block — goes through `Gateway::account()`; the services
hold only the gateway and a `WorkerConfig` with the deployment-level settings (the namespace of the external ids, the
issue policy). No Restate service calls another; no `Order` handler calls a handler on its own key.

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

`get`: default retry policy, `max_attempts = 3`, `kill`.

### `Szamlazz.Agent` (stateless Service, `#[restate_sdk::service(name = "Szamlazz.Agent")]`)

| Handler | Input → Output | Notes |
|---|---|---|
| `query` | `QueryRequest { selector }` → `QueryResponse` | projection of `InvoiceDocument`; 7 → `TerminalError` 404 `not_found` |
| `set_payments` | `SetPaymentsRequest { invoice_number, entries[≤5], additive }` → `SetPaymentsResponse` | `RegisterCreditEntry`; `max_attempts = 2, kill`; run `max_attempts(1)` |
| `storno` | `StornoRequest` → `StornoResponse` | query first; document carries `rendelesszam` → `outcome: managed_by_order{key}`; else storno with ext id `"{namespace}:by-number:{number}:storno"` |

## 5. Create protocol (`create_invoice`; other kinds analogous)

Every szamlazz.hu call is inside a `ctx.run` with `RunRetryPolicy::max_attempts(1)`; expected outcomes are data.

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
3. **Attempt loop** while `attempts < issue.max_attempts`:
   `out = ctx.run("issue-{kind}-{attempt}", || gateway.issue(IssueRequest{ external_id, kind, order, create, reissue,
   check_hint: attempt == 1 && (detect_foreign || proforma linked), our_numbers }))`. The gateway validates every found
   document against its own account (`teszt`, supplier pin); the request carries only what identifies the document.
   The step, in one closure:
   - `QueryInvoiceXml(ExternalId)` → `Ok(doc)`: validate → `Collision(doc)` on mismatch; live → `Found(doc)`;
     reversed (`sztornozott == Some(true)`) → `reissue ? continue : FoundReversed(doc)`. 7 → continue. Transport →
     `Transport` (never create when the check itself failed).
   - Hint, if `check_hint`: `QueryInvoiceXml(OrderNumber)` → live `SZ|ES|VS` whose number ∉ `our_numbers` and ≠ any
     document already seen under our ext id → `Foreign(doc)`; otherwise continue.
   - `CreateInvoice` → `Issued(r)` | `Rejected{code, message}` | `Unknown{code?, message}` (56 without number, 1, 55,
     `szlahu_down`) | 71/152 → re-query ext id → `Found(doc)` (live) or `DuplicateOrderNumber{message}` | `Transport`.
   Branch on data:
   - `Found(doc)` → `outcome: already_issued{number, totals}` if the query preceded a create attempt in this
     invocation… — precisely: `Found` from the *pre-query* → `reissue ? conflict{live} : already_issued`; `Found`
     from the *71/152 re-query* → `reconciled`. (The step tags which.)
   - `FoundReversed(doc)` → `outcome: reversed{number, storno_number?}` (storno number from the hint if the newest
     document under the order is an `SS` with `hivszamlaszam == number`, else absent).
   - `Issued(r)` → `outcome: issued` (+ `warnings: [notification_delivery_failed]`).
   - `Collision(doc)` → `conflict{external_id_collision, number}`.
   - `Foreign(doc)` → `conflict{foreign, existing_number}`.
   - `Rejected{code}` → `outcome: rejected{code, message}`.
   - `DuplicateOrderNumber` → `attempt ≤ 2` → treat as `Unknown` (sleep, loop; the re-executed closure re-queries),
     else `conflict{duplicate_order_number}`.
   - `Transport | Unknown` → attempts remain → `ctx.sleep(backoff)` (`first_backoff`, ×2, ≤ `max_backoff`) → loop.
4. **Exhausted.** `TerminalError{outcome_unknown, json{order, kind, external_id}}`. Nothing to record: the next
   invocation's pre-query finds whatever landed.
5. **Crash path.** Restate re-dispatches after `initial_interval` (2 m > 60 s client timeout + observed stalls) with
   the journal; completed runs replay; the open `issue-…` closure re-executes and begins with the ext-id query, so a
   landed create is `Found`, not re-issued. Second guard: with the toggle ON, a byte-identical resend while the first
   document is live is answered with the same number.

Kind specifics: `create_proforma` — kind `D`; no exclusivity step; `proforma` option not applicable. `create_prepayment`
— exclusivity against `…:invoice`; `proforma` option not applicable (anything but `auto` → `invalid_input`) and **no
step 2**: the Agent's prepayment invoice cannot carry `dijbekeroSzamlaszam`, and the server converts the order's live
`D` by shared order number regardless (verified — an `ES` issued without the reference shows `hivdijbekszam`), so `get`
derives `consumed` from the `ES`. `create_final` — `ctx.run(query "…:prepayment")` must be a live `ES` (7 →
`conflict{prepayment_missing}`, reversed → `conflict{prepayment_reversed}`, fails validation →
`conflict{external_id_collision}`); passes `elolegSzamlaszam`; the server
enforces one final per prepayment (73 → `rejected`); the server does not net the prepayment into the final's totals.
`correct_invoice` — `ctx.run(verify invoice_number)`: 7 → `invalid_input`, reversed → `conflict{base_reversed}`,
`rendelesszam ≠ key` → `conflict{not_managed}`; ext id `…:corrective:{correction_id}`; same loop without the 71/152
path (correctives are exempt from the order-number check — verified); a new `correction_id` issues a new corrective
by contract.

## 6. Storno protocol (`Szamlazz.Order.storno_invoice`)

Storno is natively idempotent on the server (a repeat echoes the existing storno — verified) and an external id on
the storno request attaches to the storno document (verified).

1. `ctx.run(verify number)`: 7 → `invalid_input{not_found}`; `rendelesszam ≠ key` → `conflict{not_managed}` (use
   `Szamlazz.Agent.storno`); `sztornozott == Some(true)` → `outcome: reversed{storno_number?}` (idempotent; storno
   number via the hint when the newest document is the matching `SS`); `tipus ∉ {SZ, ES, VS, HS}` → `rejected{not_stornoable}`.
2. Loop ≤ 3 attempts (`first_backoff`, ×2): `ctx.run("storno-{number}-{n}", || gateway.storno(StornoAttempt{ number,
   external_id: "{namespace}:{order}:storno:{number}", comment, e_invoice }))`:
   (a) query by the storno ext id → `SS` with `hivszamlaszam == number` → `AlreadyReversed{storno_number}`;
   (b) send `xmlszamlast{szamlaszam, szamlaKulsoAzon}` **without `keltDatum`** (352 otherwise — verified);
   (c) validate: `invoice_number ≠ requested ∧ gross < 0` → `Reversed`; echo of the requested number →
   `NotStornoable`; API errors → `Rejected{code, message}` with the raw szamlazz.hu code (`14` = storno of a storno,
   `221` = has a corrective — typed in `szamlazz_agent::ErrorCode`, surfaced as the code string); `Transport |
   Unknown` → backoff, loop.
3. `Reversed | AlreadyReversed` → `outcome: reversed{storno_number}`; `Rejected` → `outcome: rejected{code}`;
   exhausted → `TerminalError{outcome_unknown}` (the next call is safe: step 1 and 2(a) find the storno if it landed).

`e_invoice` for the storno: the verified document's `eszamla` when known, else config.

`delete_proforma({force})`: `ctx.run(query "…:proforma")`: 7 → `{deleted: true, reason: absent}` (deleted or consumed —
`get` tells which); a document under our id that fails validation → `{deleted: false, reason: external_id_collision}`;
live `D` with payments ∧ `!force` → `rejected{proforma_paid}` (the server has no guard — verified);
`ctx.run(DeleteProforma{InvoiceNumber})` (`max_attempts(1)`): success | 335 → `{deleted: true}`; other →
`rejected{code}`; `Transport` → `TerminalError{outcome_unknown}`.

`get`: four `ctx.run` queries (`…:proforma|invoice|prepayment|final`) → `OrderStatus { proforma?, invoice?,
prepayment?, final?: DocumentStatus }` where `DocumentStatus { number, state: live | reversed | consumed{by},
gross, net, payments: [amounts], referenced_proforma?, e_invoice? }`. `get` does not look up the storno number (that
would need the order-number hint, which only shows the newest document); the create and storno handlers report it
when the hint yields it. A proforma that is absent while the invoice references it (`hivdijbekszam`) is reported as
`proforma: { state: consumed, by: invoice_number }`. A document under an id that fails validation leaves its slot
absent — a read must not fail; the issuing handlers are the ones that refuse it as `conflict{external_id_collision}`.
`Transport` on any query → `TerminalError{unavailable}`.

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
TerminalError codes: outcome_unknown | unavailable | account_mismatch | invalid_input
```

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

```toml
[account]
slug = "acct"                 # the namespace: 1–16 bytes of [a-z0-9-]; prefixes every external id; permanent
agent_key = "..."             # or env RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional (wiremock in tests)
mode = "live"                 # live | test — validated against <teszt> on every adopted document
supplier_id = 972720          # optional; when set, validated against szallito/id on every adopted document

[defaults]   # as v1: e_invoice, language, currency, exchange_rate_bank, template?, send_email?, number_prefix?, extra_logo?, aggregator?, guardian?
[seller]     # as v1
[issue]
max_attempts = 5
first_backoff = "2m"
max_backoff = "10m"
detect_foreign = true         # the hint is mandatory when a proforma is linked, regardless
```

Per-call inputs (`DocumentInput`) as v1: `buyer`, `items`, `fulfillment_date`, `due_date`, `payment_method`, `paid`,
`comment?`, `issue_date?`, overrides.

## 10. Endpoint

Unchanged from v1: `restate-szamlazz --config <file> --port 9080`; `RESTATE_SZAMLAZZ_*` env with `__` nesting;
`identity_keys`; tracing; container image on `v*` tags.

## 11. Testing

- `gateway`: wiremock tests using upstream-shaped responses (`Found`/`FoundReversed` with and without `reissue`,
  `Dup71` re-query, storno validation incl. the D/SL no-op, 335, 7, `Collision`, `Foreign`); the gateway validates
  found documents against the account it was opened for.
- `service`: discovery test (names, handler set, attributes), an endpoint build smoke test, and `prepare` refusing
  `options.proforma` on every kind but `create_invoice`.
- End to end (docker-gated): Restate 1.7.8 + wiremock as szamlazz.hu — issued → already_issued (new key) and
  Idempotency-Key replay (same key, create mock `expect(1)`); 152 → reconciled; storno → reversed; stale create →
  reversed; `reissue` → issued as newest holder; `reissue` on live → `conflict{live}`; `sztornozott` → reversed;
  proforma auto-link and `consumed` in `get`; `get` shape; a collision on the secondary (`…:prepayment`) lookup →
  `conflict{external_id_collision}` with the create mock `expect(0)` and the slot absent in `get`; `create_prepayment`
  refusing `options.proforma` and issuing without a proforma lookup.
- Live: the go-live checklist in `szamlazz-hu-behaviour.md`, to be automated as ignored tests (issue #15).

## 12. What v2 gives up relative to v1 (deliberately)

`request_id` retry identity (→ `Idempotency-Key`), `conflict{payload_mismatch}` (a different payload for a live
document is `already_issued`), flag-free reissue after a service-side storno (→ `reissue: true` after any reversal),
`recorded_document_missing` (a document szamlazz.hu no longer knows is simply absent — live accounts cannot delete
invoices), `payments_before` capture on storno (query before stornoing), the ledger snapshot (`get` is 4 live
queries), operator handlers `record_reversal`/`forget` (nothing to repair), the account fingerprint learned into
state (pin `supplier_id` in config), schema versioning and state migrations.
