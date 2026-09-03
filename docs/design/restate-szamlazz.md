# restate-szamlazz — design and implementation spec (v1)

Status: accepted for implementation. Supersedes the review-round drafts (v1–v3.1) that produced it; the
decisions are recorded as ADRs 0001–0004, the verified szamlazz.hu behaviour it relies on in
[`szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md).

## 1. Goal

Expose the basic szamlazz.hu Számla Agent operations as Restate services with durable execution, so that a
caller can say "issue the invoice for order X" and get exactly one legal document under retries, process crashes,
concurrent callers and reversals — with a JSON API that is a *projection* of the Agent model: deployment
constants moved to config, line totals computed, one handler per document kind, no PDF.

Non-goals for v1: PDF download, receipts, taxpayer query, IPN / Adatkapcsolat ingestion, the proforma → payment →
invoice lifecycle workflow, multiple szamlazz.hu accounts in one deployment.

## 2. Crates

| Crate | Kind | Publishes | Purpose |
|---|---|---|---|
| `restate-szamlazz` | library | yes | Contract types, config, the `steps` module, the `Szamlazz.Order` Virtual Object and the `Szamlazz.Agent` service |
| `restate-szamlazz-endpoint` | binary `restate-szamlazz` | yes (`cargo install`) + container `ghcr.io/sagikazarmark/restate-szamlazz` | Hosts the services over HTTP for a Restate server; clap + figment config |

`restate-szamlazz` depends on `restate-sdk` unconditionally — it is the Restate worker, not a contract package.
Its only feature is `schemars` (`["dep:schemars", "restate-sdk/schemars"]`), which adds typed request/response
schemas to the discovery manifest. Should a caller-side, SDK-free contract package ever be needed (e.g. for a
wasm32 client of the ingress), it becomes a separate `restate-szamlazz-contract` crate rather than a feature.

Layering (ADR 0001): the szamlazz.hu-calling layer is a **Rust module** (`steps`), not a Restate service.

```
restate_szamlazz::steps::Steps                   owns szamlazz_agent::Client + config; one plain async fn per ctx.run;
                                                  outcome-as-data; never returns Err for an expected szamlazz.hu outcome
        ▲                                   ▲
        │ inside ctx.run                    │ inside ctx.run
Szamlazz.Order (Virtual Object, key = order number)   Szamlazz.Agent (stateless Service: query / set_payments / storno)
```

No Restate service calls another. `Order` never calls any handler on its own key (same-key exclusive → exclusive
calls deadlock — verified).

## 3. Identity model (ADR 0002)

- **VO key** = the order number, trimmed of leading/trailing whitespace, case preserved (the server trims and is
  case-sensitive — verified). Validation: 1–64 bytes after trim, no control characters, no internal whitespace
  runs → `invalid_input`. One szamlazz.hu account per deployment, so no account namespace in the key; the account
  slug lives in the external id instead.
- **External id** (`szamlaKulsoAzon`) is the per-document idempotency handle. It is deterministic from ledger
  state, written to state *before* the first szamlazz.hu call, and queryable exactly (`InvoiceSelector::ExternalId`,
  code 7 on miss, read-your-writes lag ≈ 0 — verified):
  - slot kinds: `"{account.slug}:{order}:{kind}:{gen}"`, `kind ∈ proforma | invoice | prepayment | final`
  - correctives: `"{account.slug}:{order}:corrective:{cseq}"`
  - storno: `"{external_id_of_original}:storno"` (an external id sent on `xmlszamlast` attaches to the SS — verified)
  - `gen` bumps only on a *verified* reversal (invoice kinds) or deletion/consumption (proforma). Never on
    transport errors, rejections, 71/152 or foreign detections — nothing of ours was created, or if it was we want
    to find it under the same id. `cseq` increments per accepted corrective `request_id`.
  - Ext ids are not unique server-side (last-writer-wins on query — verified). Every `Found` document is therefore
    **validated** before adoption: `rendelesszam == order ∧ tipus ∈ kind-set ∧ teszt == account.mode ∧ szallito/id ==
    supplier_id` (when known); anything else → `conflict{external_id_collision}`.
- **`request_id`** (caller-supplied, required on every issuing handler; `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`) is the
  *retry identity*: the same id returns the entry's current state forever; a different id is a new logical
  request. It replaces any separate `correction_id`.
- **Fingerprint** `fp = { name_hmac, gross, due?, fulfil?, issue? }` — `HMAC-SHA256(account.fp_secret, NFC(trim(buyer.name)))`,
  the computed gross total, and the date fields **only when caller-supplied**. Used solely to detect caller payload
  drift (`conflict{payload_mismatch}`); it is *not* a model of the server's replay check.
- **Buyer name is serialised byte-identically on every attempt**: normalise once at validation, journal the
  normalised input. (The server's 2-day identical-request replay compares the buyer name byte-exact and the amount,
  and ignores `keltDatum`, external id and comment — verified. It replays only while the first document is live.)

## 4. Ledger (VO state, schema `v = 1`; no PII)

```jsonc
{ "v": 1,
  "supplier_id": 972720,                      // szallito/id learned from the first query response; account fingerprint
  "slots": { "proforma": Slot?, "invoice": Slot?, "prepayment": Slot?, "final": Slot? },
  "correctives": { "<request_id>": CorrectiveEntry },  // cseq, number, corrected_number, fp, status
  "next_cseq": 1,
  "requests": { "<request_id>": { "kind": "invoice", "gen": 0 } },
  "foreign_hint": { "number": "…", "tipus": "SZ" }?,   // last foreign document seen under this order
  "history": [ { "kind": "invoice", "gen": 0, "request_id": "r-2", "number": "SZ-…",
                 "event": "reversed", "by": "SS-…", "origin": "external", "payments_before": [...] } ] }

Slot = { gen, request_id, status, number?, gross?, net?, test?, origin: service|adopted,
         fp, issue_date_requested?, attempts, last_attempt_at? }
status ∈ pending | blocked{existing_number?} | committed | rejected{code, message}
       | reversed{by?, origin: service|external|operator} | reversal_unverified
       | consumed{by} (proforma) | deleted (proforma)
```

Timestamps (`last_attempt_at`) come from `ctx.run("now-…", now)` — journaled, deterministic on replay. No wall
clock outside a run.

## 5. Services and handlers

### `Szamlazz.Order` (Virtual Object, `#[restate_sdk::object(name = "Szamlazz.Order")]`)

| Handler | Kind | Input → Output |
|---|---|---|
| `create_proforma` | exclusive | `CreateRequest` → `CreateResponse` |
| `create_invoice` | exclusive | `CreateRequest` (options: `reissue`, `proforma`) → `CreateResponse` |
| `create_prepayment` | exclusive | `CreateRequest` → `CreateResponse` (v1: one prepayment per order) |
| `create_final` | exclusive | `CreateRequest` → `CreateResponse` (requires a committed prepayment; passes `elolegSzamlaszam` explicitly) |
| `correct_invoice` | exclusive | `CorrectRequest { invoice_number, request_id, document }` → `CreateResponse` |
| `storno_invoice` | exclusive | `StornoRequest { invoice_number, comment? }` → `StornoResponse` |
| `delete_proforma` | exclusive | `DeleteProformaRequest { force }` → `DeleteProformaResponse` |
| `get` | shared | `GetRequest { verify }` → `OrderSnapshot` |
| `record_reversal` | exclusive, `ingress_private = true` | `RecordReversalRequest { invoice_number, result: reversed{storno_number?} \| live }` → `OrderSnapshot` |
| `forget` | exclusive, `ingress_private = true` | `ForgetRequest { kind }` → `OrderSnapshot` |

Every handler that calls szamlazz.hu carries (ADR 0004):

```
invocation_retry_policy(initial_interval = "2m", factor = 2.0, max_interval = "10m", max_attempts = 5, on_max_attempts = "kill")
inactivity_timeout = "4m"     abort_timeout = "3m"     journal_retention = "3d"     idempotency_retention = "7d"
```

Rationale: a crash mid-`ctx.run` re-executes the closure only after `initial_interval` (verified); 2 min exceeds
the 60 s client timeout plus observed server-side stalls (≥ 57 s), so the re-executed closure's external-id
pre-query runs after the first request has resolved server-side. `kill` releases the key; the `pending` slot makes
re-entry safe and kill is what makes it reachable (a paused invocation holds the key — verified).

### `Szamlazz.Agent` (stateless Service, `#[restate_sdk::service(name = "Szamlazz.Agent")]`)

| Handler | Input → Output | Notes |
|---|---|---|
| `query` | `QueryRequest { selector: invoice_number \| order_number \| external_id }` → `QueryResponse` | projection of `InvoiceDocument`: number, `tipus`, `reversed: Option<bool>`, references, order number, dates, totals, payments, `outstanding = gross − Σ payments`, `supplier_id`, `test` |
| `set_payments` | `SetPaymentsRequest { invoice_number, entries[≤5], additive }` → `SetPaymentsResponse` | `RegisterCreditEntry`; default replace semantics; `max_attempts = 2, kill`; run `max_attempts(1)` |
| `storno` | `StornoRequest` → `StornoResponse` | query first; document carries `rendelesszam` → `outcome: managed_by_order{key}` (no call into `Order`); else storno directly with response validation (§7) |

Handler attributes: `query` (read-only) carries `invocation_retry_policy(initial_interval = "10s", factor = 2.0,
max_interval = "1m", max_attempts = 3, on_max_attempts = "kill")` and no retention; `set_payments` and `storno`
carry `invocation_retry_policy(max_attempts = 2, on_max_attempts = "kill")`, `inactivity_timeout = "2m"`,
`abort_timeout = "2m"`, `journal_retention = "3d"`, `idempotency_retention = "7d"`.

## 6. Create protocol (`create_invoice`; proforma / prepayment / final / corrective analogous)

Steps are numbered as they should appear in code; every szamlazz.hu call is inside `ctx.run`.

0. **Validate (pure).** Trim/validate the order number against the key (the body carries no order number). Validate
   `request_id`, buyer, items (≥ 1), dates. Normalise `buyer.name` (trim + NFC). Compute line totals with
   `LineItem::calculated_for_currency`. Build the `szamlazz_agent::CreateInvoice` from input + config defaults +
   per-call overrides; leave `external_id` / `issue_date` to step 3. Compute `fp`.
   **Identity:** `requests[request_id]` known → same kind required (`conflict{request_id_reused}`), `fp` equal
   required (`conflict{payload_mismatch}`); then return that entry's current state: `pending` ⇒ step 2 resume;
   `committed` ⇒ step 2 verify; `reversed` ⇒ `outcome: reversed`; `rejected` ⇒ `outcome: rejected`; `blocked` ⇒
   step 2 blocked. `reissue: true` with a known id ⇒ `invalid_input("reissue requires a new request_id")`.
1. **Exclusivity (check, not state).** `slots.prepayment.status ∈ {pending, committed, blocked}` ⇒
   `conflict{prepaid_chain}`. (`create_prepayment` mirrors this against `slots.invoice`.)
2. **Slot dispatch** on `slots.invoice`:
   - `null | rejected` ⇒ step 3 at the current `gen`.
   - `deleted | consumed` (proforma kind only; `consumed` ⇒ `conflict{proforma_consumed, by}`; `deleted` ⇒ step 3).
   - `pending` ⇒ **pre-sleep** `ctx.sleep(max(0, first_backoff − (now − last_attempt_at)))`; **reconcile-only**
     `ctx.run(query by external id)` (default run retry, `max_duration 2m`): `Found` ⇒ validate ⇒ commit under the
     pending `request_id` ⇒ continue as `committed` with the incoming request; `NotFound` ⇒ same `request_id`:
     reset `attempts = 0` (every invocation is a fresh caller decision and gets a fresh budget; the closure is
     query-first, so a re-send can never duplicate a landed create) ⇒ step 4; **different `request_id`** ⇒ take
     over the slot (history `abandoned{old}`, `requests[old] = abandoned`, `attempts = 0`, same `gen`/external id)
     ⇒ step 4; `Transport` ⇒ `TerminalError{unavailable}`.
   - `committed` ⇒ **verify** `ctx.run(query by invoice number)`: `szallito/id ≠ supplier_id` ⇒
     `TerminalError{account_mismatch}`. `sztornozott == Some(true)` ⇒ slot `reversed{origin: external}`, `gen += 1`,
     history; then `outcome: reversed{number}` unless `reissue: true` (then step 3). Live ∧ `fp` equal ⇒
     `outcome: already_issued{number, totals}` (`reissue: true` ⇒ `conflict{live}`). Live ∧ `fp ≠` ⇒
     `conflict{payload_mismatch, existing_number}`. Code 7 ⇒ `conflict{recorded_document_missing, number}` — never
     reissue; operator `forget`. (Proforma kind: 7 ⇒ disambiguate via the order-number hint — an `SZ`/`ES` with
     `hivdijbekszam == D` ⇒ `consumed{by}` (adopt it if unknown, `origin: adopted`); otherwise `deleted`, `gen += 1`.)
   - `reversed{origin: service}` ⇒ slot is open ⇒ step 3 (flag-free). `reversed{origin: external | operator}` ⇒
     `reissue: true` required, else `outcome: reversed`.
   - `blocked` ⇒ reconcile-only query by external id: `Found` ⇒ commit ⇒ continue as committed; 7 ⇒
     `conflict{duplicate_order_number, existing_number?}` (no new allocation).
3. **Allocate intent.** `issue_date_requested = input.issue_date` (else none — the server dates the document; a
   stale pinned date buys nothing and risks 352). Resolve `options.proforma`: `ledger` ⇒ the proforma slot must be
   `committed`; pre-query it by number (7 ⇒ `consumed`/`deleted` per the rule above ⇒ `conflict{proforma_missing}`);
   `none` ⇒ if the proforma slot is `pending | committed` **or** the order-number hint shows a live `D` ⇒
   `conflict{proforma_live, number}` (the server links by shared order number regardless — verified); `{number}`
   ⇒ pre-query; 7 ⇒ `conflict{proforma_missing}`. Slot = `pending{gen, request_id, fp, attempts: 0}`;
   `requests[request_id] = {kind, gen}`; `ctx.set` — **before any issuing call**.
4. **Attempt loop** while `attempts < issue.max_attempts`: `attempts += 1`, `last_attempt_at = run(now)`, `ctx.set`;
   `out = ctx.run("issue-{kind}-{gen}-{attempts}", || steps.issue(IssueRequest{…})).retry_policy(max_attempts(1))`.
   The module function, in one closure:
   - `QueryInvoiceXml(ExternalId)` ⇒ `Ok(doc)` ⇒ validate ⇒ `Found(doc)` | `Collision(doc)`; 7 ⇒ continue;
     transport ⇒ `Transport` (never create when the check itself failed).
   - hint, on the first attempt when `issue.detect_foreign` or `proforma == ledger`: `QueryInvoiceXml(OrderNumber)` ⇒
     live `SZ | ES | VS` not among our numbers ⇒ `Foreign(doc)`; `SZ | ES` with `hivdijbekszam == our D` ⇒
     `Found(doc, adopted)`; anything else ⇒ continue.
   - `CreateInvoice{external_id, …}` ⇒ `Issued(result)` | `Rejected{code, message}` | `Unknown{code}` (56 without a
     number, 1, 55, `szlahu_down`) | `Dup71{code}` ⇒ **re-query by external id** ⇒ `Found(doc)` or `Dup71{hint?}`
     | `Transport`.
   Branch on data:
   - `Found(doc)`: `doc.sztornozott == Some(true)` or `doc.number ∈ history.reversed` ⇒ slot `reversed{external}`,
     `gen += 1`, history ⇒ **return `outcome: reversed`** (never re-allocate inside the loop — an identical resend
     after a storno issues a new invoice, verified); else commit (`origin: service | adopted`) ⇒ `outcome: reconciled`.
   - `Issued(r)` ⇒ commit ⇒ `outcome: issued` (+ `warnings: [notification_delivery_failed]` when flagged).
   - `Collision(doc)` ⇒ slot unchanged (pending) ⇒ `conflict{external_id_collision, number}`.
   - `Foreign(doc)` ⇒ slot `null` (nothing created), `foreign_hint` ⇒ `conflict{foreign, existing_number}`.
   - `Rejected{code}` ⇒ slot `rejected` ⇒ `outcome: rejected{code, message}`.
   - `Dup71` ⇒ `attempts ≤ 2` ⇒ treat as `Unknown` (sleep, loop — the re-executed closure re-queries); else slot
     `blocked{existing_number?}` ⇒ `conflict{duplicate_order_number}`.
   - `Transport | Unknown` ⇒ attempts remain ⇒ `ctx.sleep(backoff)` (`first_backoff`, ×2, ≤ `max_backoff`) ⇒ loop;
     else step 5.
5. **Exhausted.** Slot stays `pending`; `TerminalError{outcome_unknown, json{order, kind, gen, external_id, request_id}}`.
6. **Crash path.** Restate re-dispatches after `initial_interval` with the journal; completed runs/sets/sleeps
   replay; the open `issue-…` closure re-executes and begins with the external-id query, so a landed request is
   `Found`, not re-issued. Second guard: with the account toggle ON, a byte-identical resend while the first
   document is live is answered with the same number.

Kind specifics: `create_proforma` — slot `proforma`, `tipus D`; after `deleted`, a new `request_id` issues
flag-free (a proforma is not a legal document). `create_final` — requires `slots.prepayment.status == committed`;
verifies the prepayment live before allocating (reversed ⇒ `conflict{prepayment_reversed}`); `slots.final`
committed ⇒ `conflict{final_exists}`; the server does not net the prepayment into the final's totals — the caller
supplies the negative prepayment line. `correct_invoice` — dedupe on `request_id`; refuse when the base is
`reversed` in the ledger (`conflict{base_reversed}`); verify the base live; same loop with
`corrective:{cseq}`; no `Dup71` path (correctives are exempt from the order-number check — verified); a new
`request_id` issues a new corrective by contract.

## 7. Storno protocol (`Szamlazz.Order.storno_invoice`) — idempotent re-send with a query-first guard

Storno is natively idempotent on the server (a repeat storno echoes the existing SS with `sikeres=true`, no
error — verified) and an external id on the storno request attaches to the SS (verified).

1. Locate the slot or corrective by number (unknown ⇒ `invalid_input{not_managed}` — use `Szamlazz.Agent.storno`).
   Pre-checks: a corrective references this number ⇒ `rejected{has_corrective}` (server code 221); slot `reversed`
   ⇒ `outcome: reversed` (idempotent); `pending` ⇒ `outcome: conflict{pending}`; `reversal_unverified` ⇒ continue
   at step 3 (retry).
2. `ctx.run(query by number)`: `sztornozott == Some(true)` ⇒ record `reversed{origin: external}` (storno number via
   the order-number hint if it is an `SS` with `hivszamlaszam == number`), `gen += 1` ⇒ `outcome: reversed`. Otherwise
   capture `payments` for the history event.
3. Loop ≤ 3 attempts (`first_backoff`, ×2): `ctx.run("storno-{number}-{n}", || steps.storno(...)).max_attempts(1)`:
   (a) query by the storno external id ⇒ `Found` SS with `hivszamlaszam == number` ⇒ `Reversed{ss}`;
   (b) send `xmlszamlast{szamlaszam, szamlaKulsoAzon}` **without `keltDatum`** (352 otherwise on e-invoice
   accounts — verified);
   (c) validate: `invoice_number ≠ requested ∧ gross_total < 0` ⇒ `Reversed{by: invoice_number}`;
   `invoice_number == requested` (positive totals) ⇒ `NotStornoable` (the D/SL success-shaped no-op — verified);
   code 14 ⇒ `Rejected{already_storno}`; 221 ⇒ `Rejected{has_corrective}`; other API errors ⇒ `Rejected{code}`;
   `Transport | Unknown` ⇒ backoff, loop.
4. `Reversed` ⇒ slot `reversed{by, origin: service}`, `gen += 1`, history `{payments_before}` ⇒
   `outcome: reversed{storno_number}`. `Rejected` ⇒ `outcome: rejected{code}`. Exhausted ⇒ slot
   `reversal_unverified` (transient: the next `storno_invoice` retries) ⇒ `TerminalError{outcome_unknown}`.

`e_invoice` for the storno: the recorded document's `eszamla` when known, else config.

`Szamlazz.Order.delete_proforma`: `pending` ⇒ reconcile-first (pre-sleep + external-id query): `Found` ⇒ commit then
delete; 7 ⇒ `{deleted: false, reason: pending}`. `committed` ⇒ pre-query `payments`; paid ∧ `!force` ⇒
`rejected{proforma_paid}` (the server has no guard — verified); `DeleteProforma{InvoiceNumber}` in a run
(`max_attempts(1)`): success | 335 ⇒ `deleted`, `gen += 1`; other ⇒ `rejected{code}`. `deleted` ⇒ `{deleted: true}`.
`consumed` ⇒ `conflict{proforma_consumed}`.

## 8. Outcome contract

Domain outcomes are **data** (HTTP 200 through the ingress, typed in the OpenAPI export). `TerminalError` is
reserved for faults.

```
CreateResponse { outcome, conflict_reason?, request_id, kind, gen, external_id,
                 invoice_number?, storno_number?, net_total?, gross_total?, outstanding?, customer_account_url?,
                 existing_number?, code?, message?, warnings: [] }
outcome ∈ issued | already_issued | reconciled | reversed | rejected | conflict
conflict_reason ∈ prepaid_chain | payload_mismatch | request_id_reused | pending | live | foreign
                | duplicate_order_number | external_id_collision | recorded_document_missing | proforma_live
                | proforma_missing | proforma_consumed | prepayment_reversed | final_exists | base_reversed
warnings ∈ notification_delivery_failed | proforma_link_dropped
StornoResponse { outcome ∈ reversed | rejected | conflict | managed_by_order, conflict_reason?,
                 invoice_number, storno_number?, order_key?, code?, message? }
TerminalError codes: outcome_unknown | unavailable | account_mismatch | invalid_input
```

`proforma_link_dropped`: after a create that carried a proforma reference (`options.proforma = ledger | {number}`),
the commit path records `hivdijbekszam` from the issued document when the create response is followed by a verify
(the next call) — if it is absent the warning is attached to that later response and the proforma slot is left
unchanged. The server silently drops references to deleted or already-consumed proformas (verified), so the
absence is informational, not a fault.

Caller contract, documented in the crate README: **any error from an issuing or storno handler means "outcome
unknown — call again with the same `request_id`, or read `Szamlazz.Order.get`"**, never "no document exists". Callers
should not rely on the ingress `Idempotency-Key` (it would replay a `TerminalError` for its retention period —
verified); `request_id` is the retry identity.

## 9. Configuration (deployment-constant; never in payloads)

```toml
[account]
slug = "acct"                 # 1–16 chars [a-z0-9-]; namespaces external ids
agent_key = "..."             # or via env RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY
endpoint = "https://www.szamlazz.hu/szamla/"   # optional (wiremock in tests)
mode = "live"                 # live | test — validated against <teszt> on every adopted document
supplier_id = 972720          # optional pin; otherwise learned from the first query and stored in the ledger
fp_secret = "..."             # required; rotation invalidates stored fingerprints

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

Per-call inputs (`DocumentInput`): `buyer`, `items[{name, quantity, unit, unit_price, vat_rate, id?, comment?}]`,
`fulfillment_date`, `due_date`, `payment_method` (English enum + `other` fallback), `paid`, `comment?`,
`issue_date?`, and overrides `language`, `currency` + `exchange_rate`, `template`, `send_email`, `e_invoice`,
`number_prefix`.

## 10. Endpoint

`restate-szamlazz --config <file> --port 9080`; `CONFIG_FILE` / `PORT` env; figment merges TOML/JSON/YAML by
extension then `RESTATE_SZAMLAZZ_*` env with `__` nesting; `identity_keys` (list or comma-separated) for request
identity; `tracing_subscriber::fmt` + `EnvFilter` (`RUST_LOG`, default `info`); binds `0.0.0.0:{port}`; HTTP/2
only (Restate SDK). Container image built by `Dockerfile` at the repo root, pushed to
`ghcr.io/sagikazarmark/restate-szamlazz` on `v*` tags.

## 11. Testing

- `ledger`: pure transition tests (every branch of §6 step 2 and §7 step 1 without I/O).
- `steps`: `wiremock` tests using the upstream response fixtures under `fixtures/upstream/agent/`
  (`Found` validation, `Dup71` re-query, storno response validation incl. the D/SL no-op, 335, 7).
- `service`: discovery test (`Discoverable::discover()` names, handler set, `ingress_private` flags) and an
  `Endpoint::builder().bind(...).build()` smoke test; optional raw-protocol test as in email-rs.
- Endpoint: figment parse tests; a `wiremock`-backed test wiring config → `steps` module.
- Live: `tests/live.rs`-style ignored tests for the go-live checklist in `szamlazz-hu-behaviour.md`.

## 12. What v1 gives up (deliberately)

Episodes/cooldown budgets (a `pending` slot re-entered by any invocation gets a fresh attempt budget after a
pre-sleep and a reconcile query — the caller decides when to stop retrying), verify-on-hit TTL (every repeat
call verifies live: +1 round trip, always correct), `paid: inherit` from a paid proforma, Adatkapcsolat/IPN-fed
ledger updates, multiple prepayments per order, PDF, receipts.

## 13. Deviations from the review record

- VO key is the bare order number, not `{account}/{order}`: one account per deployment is fixed for v1, and a
  second account would be a second deployment with its own `Szamlazz.Order` service; the slug still namespaces external ids.
- Correctives use `corrective:{cseq}` (a per-order counter) instead of `corrective:{request_id}` to keep the
  external id short; `request_id ↔ cseq` lives in the ledger.
- `recorded_document_missing` fires on the first code 7 (no two-sample rule); it is a pure conflict response that
  changes no state, so a transient 7 costs the caller one retry.

## 14. Implementation notes (as built)

Where the implementation interprets or narrows the protocol above:

- **Run retries.** Every `ctx.run` uses `max_attempts(1)`, including reconcile and verify queries. In SDK 0.12 a
  run-level retry is an invocation retry, so it would be subject to the handler's `initial_interval = 2m` and count
  against `max_attempts = 5`; a `Transport` outcome on a pure query therefore maps to `TerminalError{unavailable}`
  immediately and the caller re-calls.
- **`proforma: ledger` with no proforma slot** falls through to the `none` rule (a plain invoice) instead of
  `conflict{proforma_missing}` — `ledger` is the default and must not fail every plain `create_invoice`.
  `proforma_missing` is reserved for an explicit `{number}` that the server no longer knows.
- **Consumed proforma.** When the pre-query finds the proforma gone and the hint shows an `SZ`/`ES` referencing it,
  the proforma slot becomes `consumed{by}` and the create returns `conflict{proforma_consumed, existing_number}`.
  The consuming document is *not* adopted into the invoice slot (there is no `request_id` to own it); an operator
  can `forget` the invoice slot and re-issue, or the caller accepts the existing number.
- **Proforma consumption on issue.** When the create sent a reference to the ledger's committed proforma
  (`options.proforma = ledger`, or `{number}` naming it), the proforma slot becomes `consumed{by}` eagerly on
  `issued` — the create response carries no `hivdijbekszam`, so the reference we sent is taken as honoured. On
  `reconciled` inside the attempt loop the found document decides: `hivdijbekszam` equal to our proforma ⇒
  `consumed{by}`; absent although we sent a reference ⇒ `warnings: [proforma_link_dropped]` with the proforma
  slot left unchanged (the code-7 path above disambiguates it later); naming another proforma ⇒ nothing. Reconciles
  outside the loop (`pending` / `blocked` resume) carry no reference and leave the proforma slot to that path.
- **Attempt budget.** The `issue.max_attempts` loop budget is per invocation; the slot's `attempts` counter is
  cumulative and informational (surfaced by `get`).
- **Foreign / forget** leave the slot `vacant` (kept, generation preserved or bumped) rather than `null`, so the
  generation history is never lost.
- **`get { verify: true }`** returns `verification: [{ kind, result }]` alongside the snapshot; it never writes state
  (shared handler).
- **`Szamlazz.Agent.storno`** for unmanaged documents uses the external id `"{slug}:by-number:{number}:storno"`.
