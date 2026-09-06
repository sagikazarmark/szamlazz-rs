# Review 08 — Developer experience: API contract, CLI, configuration, error messages, documentation

Scope: `szamlazz-rs2` at `0e4238c` (merge of #56). Read-only review; nothing built or run. Every signature claim below was checked against the source, not the docs.

## Summary by persona

**A — Rust developer adding `szamlazz-agent` to issue an invoice. Grade: B.**
The crate's public surface is well designed: sans-IO core, typed operations, an open `VatRate`/`ErrorCode`, Hungarian doc aliases, and a `Client` whose defaults (cookie store, 60 s timeout, no redirects) are the right ones and are explained. The README quick start compiles against the current API (checked call by call — no signature mismatch). The gaps are what the quick start does *not* show: the three things every real integrator needs on day one — set an `order_number`/`external_id`, get the PDF, branch on `ClientError::Api` — are absent, the sans-IO path is prose only, and the README is not compile-tested. The CLI is serviceable but makes the user hand-compute line totals and takes Hungarian wire tokens for `--method`.

**B — Platform engineer wiring Pretix → Restate → `Szamlazz.Order`. Grade: B-.**
The contract is thoughtfully shaped (closed request bodies, outcomes-as-data, a small enum to branch on, structured faults with HTTP statuses and "what to do"), and the Pretix section is exactly the right kind of guidance. Two things will cost this person a day: (1) the fault JSON is **double-encoded** — it is the string value of `message` inside Restate's own `{code, message}` envelope, and the READMEs present the inner shape as if it were the body, with a colliding `code` field; (2) there is **not one example response body** in any document, so the shape of `issued`, `conflict`, `reversed` (and the `null`-heavy flat struct) is only discoverable by running it. Conflict reasons have no "what should the caller do" column, `get` silently omits correctives, and the buyer-facing URL is only present on the first `issued`.

**C — Operator onboarding a second account and debugging a failed invoice at 2 a.m. Grade: C+.**
Configuration is the strongest surface in the repo: strict key tree with every unknown key named at once, env overrides read as strings, `--check-config`, mutually exclusive shapes with specific errors, and a genuinely good `check_account` deploy checklist. But there is **no runbook**: the fault table says "Rule 2" or "page", never *how* to find the invocation for order X, what the journal shows, or which szamlazz.hu screen to open. Worker logs carry `external_id` and `kind` but not the account id, scope or invocation id — in multi-account mode the `credentials_rejected` warn cannot tell you *whose* key broke. Agent-key rotation has no procedure, and the go-live checklist references probe records that are not in the repository.

---

## Findings

### 1. The fault body is double-encoded inside Restate's error envelope; the READMEs describe the inner JSON as the response body

- **Persona:** B (and C reading fault bodies)
- **Severity:** high
- **Confidence:** high — the e2e harness itself parses it that way.
- **Location:** `crates/restate-szamlazz/src/service/support.rs:160-166`; `crates/restate-szamlazz-endpoint/README.md:281`; `crates/restate-szamlazz/README.md:307`; `crates/restate-szamlazz/tests/service.rs:494-501`.
- **Evidence:** The fault is serialised to a string and handed to `TerminalError::new_with_code(fault.status(), body)`:
  ```rust
  let body = serde_json::to_string(&fault) …;
  Self::new_with_code(fault.status(), body)
  ```
  Restate's ingress wraps a terminal error's *message* in its own JSON envelope. The harness knows this:
  ```rust
  /// The structured fault inside the ingress error envelope: the handler's
  /// `TerminalError` message is the fault JSON.
  fn fault(&self) -> Fault {
      let message = self.body["message"].as_str()…;
      serde_json::from_str(message)…
  ```
  The README says instead: *"Faults are `TerminalError`s with a JSON body `{ "code", "message", "order"?, "kind"?, "external_id"? }`"*. A caller following that will read `body.code` and get the **numeric HTTP status** (Restate's envelope also has a `code` field), never `"invalid_input"`.
- **Recommendation:** Show the real wire shape once, with a decode step, in both READMEs and the OpenAPI description:
  ```
  HTTP/1.1 503
  x-restate-error-source: invocation
  content-type: application/json

  {"code":503,"message":"{\"code\":\"credentials_rejected\",\"message\":\"szamlazz.hu rejected the agent credentials (code 3: …); this attempt issued nothing — …\",\"order\":\"ORD-1001\",\"kind\":\"invoice\",\"external_id\":\"acct:ORD-1001:invoice\"}"}
  ```
  *"The outer object is Restate's error envelope (`code` is the HTTP status). The worker's fault is the JSON **string** in `message`; parse it a second time. Its `code` is one of the six `TerminalCode` tokens below."* Consider renaming the inner field to `fault` in a future major to end the `code`/`code` collision — or at least name the collision in the doc.

### 2. Not a single example response body exists in any document

- **Persona:** B
- **Severity:** high
- **Confidence:** high — `rg '"outcome"'` over both READMEs and the design doc returns nothing.
- **Location:** `crates/restate-szamlazz-endpoint/README.md:258-273` (request curl, no response shown); `crates/restate-szamlazz/README.md:146-153`.
- **Evidence:** The curl example ends at the request. `CreateResponse` (`contract/response.rs:104-147`) is a flat struct whose ten optional fields have `#[serde(default)]` but no `skip_serializing_if`, so a real `issued` answer is
  ```json
  {"outcome":"issued","conflict_reason":null,"kind":"invoice","external_id":"acct:ORD-1001:invoice","invoice_number":"E-2026-123","storno_number":null,"net_total":"1000","gross_total":"1270","outstanding":"1270","customer_account_url":"https://…","existing_number":null,"code":null,"message":null,"warnings":[]}
  ```
  (`round_trip` test at `response.rs:956` confirms `conflict_reason: null` is emitted.) Nobody can learn from the docs that `Decimal`s arrive as strings, that `conflict_reason` is `null` on success, or what `reversed` carries.
- **Recommendation:** Add a "Reading a response" block directly under the curl example with three bodies — `issued`, `outcome: reversed` (`invoice_number`, `storno_number`), `conflict` with `conflict_reason: "foreign"` and `existing_number` — plus the fault example from finding 1. Add a `documented_bodies_deserialize`-style test that deserialises these response literals into `CreateResponse`, as `request.rs:493` already does for requests.

### 3. Stale handler count: "four on `Szamlazz.Agent`" / `handlers=4` — there are five

- **Persona:** B, C
- **Severity:** medium (doc drift on the exact line an operator compares against the start-up log)
- **Confidence:** high
- **Location:** `crates/restate-szamlazz-endpoint/README.md:122` and `:240`; `crates/restate-szamlazz/src/service/handlers.rs:264-395`.
- **Evidence:** README: *"Eight handlers on `Szamlazz.Order`, four on `Szamlazz.Agent`"* and the sample log `bound Restate service service=Szamlazz.Agent kind=Service handlers=4`. `handlers.rs` registers `check_account`, `query`, `query_taxpayer`, `set_payments`, `storno` — five (`query_taxpayer` landed in #49). `tests/check_config.rs` asserts only that the service names appear, not the counts.
- **Recommendation:** Change both lines to *five* / `handlers=5`, and have `check_config.rs` assert the literal `handlers=5` and `handlers=8` so the README's log excerpt cannot drift again.

### 4. No operator runbook: the fault table says "Rule 2" / "page", never *how* to find and read the invocation

- **Persona:** C
- **Severity:** high
- **Confidence:** high
- **Location:** `crates/restate-szamlazz-endpoint/README.md:283-294`; `crates/restate-szamlazz/README.md:318-331`; `docs/design/restate-szamlazz.md` §7.
- **Evidence:** The only `sys_invocation` query in operator-facing docs is the flag-day drain (`README.md:190`). The Restate UI is mentioned once, for the journaled `Account`. There is no "given order `ORD-1001` under scope `acme`, find the invocation, see which step failed, decide" path, although the columns needed exist and the e2e harness uses them (`tests/service.rs:611`, `:1289`: `status, completion_failure, scope, target_handler_name, retry_count, last_failure, last_failure_related_command_name`).
- **Recommendation:** Add a "Runbook" section to the endpoint README with one block per fault code:
  ```sh
  # 1. Find the invocation(s) for the order (all handlers, this scope)
  curl -s localhost:9070/query -H 'content-type: application/json' -d '{"query":
    "SELECT id, target_handler_name, status, retry_count, last_failure_related_command_name, last_failure, completion_failure
     FROM sys_invocation WHERE target_service_name = '"'"'Szamlazz.Order'"'"' AND target_service_key = '"'"'ORD-1001'"'"' AND scope = '"'"'acme'"'"'"}'
  # 2. Read its journal: `namespace`, `account` (the resolved account — id, mode, supplier pin), then the step names
  #    (lookup-invoice, create-invoice, verify-…, storno-…). The result of a Run is in the Notification row that follows it.
  # 3. Cross-check szamlazz.hu by the external id named in the fault: szamlazz invoice get --external-id acct:ORD-1001:invoice
  ```
  Then per code: `outcome_unknown` → step 3; if a document exists, nothing to do (the next call answers `already_issued`), else re-call with a new key. `account_mismatch` → the message's observed `teszt`/supplier vs configured `mode`/`supplier_id`; fix config, or the caller sent the wrong scope. `conflict{foreign}` → someone issued outside the worker; open `existing_number` in the szamlazz.hu UI, decide storno-and-reissue vs adopt-by-hand. `conflict{external_id_collision}` → a document under our id fails validation; query by `external_id`, expect another order's number or another account. `credentials_rejected` → `check_account` under the scope; fix the key (finding 8). `unavailable` → check `last_failure` for `szlahu_down`; wait; re-call. State explicitly that *the Restate UI's invocation view is the debugging tool*, and that the `x-restate-id` header on the caller's error is the handle.

### 5. Worker logs carry no account id, scope or invocation id — in multi-account mode `credentials_rejected` cannot say whose key broke

- **Persona:** C
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/support.rs:119-123`; `crates/restate-szamlazz/src/gateway.rs:813-818`, `:914-920`; SDK span `restate-sdk-0.12.0/src/endpoint/mod.rs:595-600`.
- **Evidence:** The warn is
  ```rust
  tracing::warn!(namespace = %namespace, code = %code, "szamlazz.hu rejected the agent credentials; fix the account's agent key");
  ```
  Gateway spans carry `external_id`, `kind`, `reversed`. The SDK's span carries `rpc.service` and `rpc.method` only. Nothing in the process logs names the account id or the scope, and nothing names the invocation id. With two accounts, two `credentials_rejected` lines are indistinguishable. The design (§10) promises "never the key", not "never the id" — the id is already journaled and shown in the UI, so logging it leaks nothing.
- **Recommendation:** In the prologue, wrap the handler body in `tracing::info_span!("szamlazz.handler", account = %account.id, scope = ctx.scope().unwrap_or("<unscoped>"), key = ctx.key()?)` (the SDK exposes no invocation id to handlers in 0.12; say so in the README and point at `x-restate-id`). Add `account = %namespace/…id` to the `credentials_rejected` warn. Document in the README's Running section: *"Correlate by `external_id` (which embeds the order) in the worker log, by `x-restate-id` in the caller's error, and by `target_service_key` + `scope` in `sys_invocation`."*

### 6. `customer_account_url` is returned only on a fresh `issued`; every other success path drops it — and there is no "how do I deliver the invoice to the buyer" guidance

- **Persona:** B
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/create.rs:69-77` (`found`) vs `:101-121` (`Issued`); `docs/design/restate-szamlazz.md:14` ("no PDF"); `docs/szamlazz-hu-behaviour.md:64` ("Outstanding is observable only via response headers").
- **Evidence:** `Identity::found` sets `invoice_number`, `net_total`, `gross_total`, `outstanding` from the query body; `customer_account_url` comes from the `szlahu_vevoifiokurl` header of the *create* response only. A caller whose first attempt timed out and retried gets `already_issued` (or `issued` via `Found`) **without** the buyer link, and no response ever carries a PDF or a `document_id`. The endpoint README's Pretix table has no row for "send the buyer the invoice".
- **Recommendation:** (a) Document the asymmetry on the `customer_account_url` field and in the response block of finding 2: *"present on the execution that issued; absent on `already_issued`/`reconciled`. Persist it on first sight."* (b) Add a short "Delivering the document" paragraph: let szamlazz.hu email it (`account.defaults.send_email = true` or `overrides.send_email`, `buyer.email`), or fetch the PDF out of band with `szamlazz-agent`'s `QueryInvoicePdf::new(InvoiceSelector::ExternalId("acct:ORD-1001:invoice"))` / `szamlazz invoice download --external-id …` — the external id is deterministic precisely so a caller can do this without storing the number. (c) Consider adding `document_id: Option<u64>` (`szlahu_id`) to `CreateResponse`; it is stable across paths and useful for support tickets.

### 7. Conflict reasons have no "what should the caller do" column

- **Persona:** B
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/README.md:157-162`; `crates/restate-szamlazz-endpoint/README.md:238`; `crates/restate-szamlazz/src/contract/response.rs:50-87`.
- **Evidence:** Faults get a four-column table with *What to do*; the twelve `ConflictReason`s are a comma list. The rustdoc on the variants explains the cause, and only `NotManaged` says what to do (*"Use the managing order, or `Szamlazz.Agent.storno`"*). A Pretix integrator receiving `conflict{prepaid_chain}` or `conflict{duplicate_order_number}` has to read design §5 to know whether to page, retry, or change the request.
- **Recommendation:** Add a table to the endpoint README's Services section:

  | `conflict_reason` | Means | Caller does |
  |---|---|---|
  | `live` | `reissue: true` but the document is live (`existing_number`) | Nothing to reissue; drop the flag or storno first. |
  | `prepaid_chain` | the order is on the other chain (invoice vs prepayment) | Bug in the caller's flow; do not retry. |
  | `order_invoiced` | proforma requested after the order's own invoice/prepayment | Skip the proforma. |
  | `proforma_live` | `proforma: none` while our proforma is live | Use `auto`, or `delete_proforma` first. |
  | `proforma_missing` / `prepayment_missing` / `prepayment_reversed` / `base_reversed` | the referenced document is gone/reversed | Fix the reference; for a reversed prepayment, `create_prepayment {reissue}` first. |
  | `foreign` | a live invoice under this order number that the worker did not issue (`existing_number`) | **Stop and page**: someone issued outside the worker. Never retry blindly. |
  | `duplicate_order_number` | szamlazz.hu refused 71/152 and the document is not ours | As `foreign`; `existing_number` when known. |
  | `external_id_collision` | another order/kind/account holds our external id | **Page**: namespace or account misconfiguration. |
  | `not_managed` | the named document does not carry this order's number | Use the right order key, or `Szamlazz.Agent.storno`. |

### 8. Agent-key rotation has no operator procedure, and the static resolver's "picked up on the next execution" claim needs a caveat

- **Persona:** C
- **Severity:** medium
- **Confidence:** high — `EndpointConfig::load` runs once in `main`; env is read at start.
- **Location:** `crates/restate-szamlazz-endpoint/README.md:91-100`, `:323-331` (identity-key rotation is documented; agent-key rotation is not); `crates/restate-szamlazz/README.md:98-105`; `crates/restate-szamlazz-endpoint/src/main.rs:68`.
- **Evidence:** The library README says *"a rotation is picked up on the next execution of every in-flight invocation"* — true for a pluggable `CredentialStore`. With the shipped binary the key is in the process environment, read once at start-up (`cli.load_config()` in `main`), so a rotation is a **restart**, which the Stopping section says cuts in-flight create steps and spends one of their attempts. Neither README joins these two facts, and neither says "generate the new key on szamlazz.hu first; the old one stops working the moment you delete it there".
- **Recommendation:** Add "Rotating an agent key" under Configuration:
  1. Create the new key on szamlazz.hu (Számla Agent kulcsok); do not delete the old one yet.
  2. Update `RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY` (or `…ACCOUNTS__<SCOPE>__AGENT_KEY`) in the secret store.
  3. `restate-szamlazz --check-config` with the new environment.
  4. Restart (drain first per *Rolling updates* if orders are issuing).
  5. `check_account` under the scope → `credentials: {state: ok}`.
  6. Delete the old key on szamlazz.hu.
  And qualify the library sentence: *"…with a `CredentialStore` that reads live; the endpoint binary's static resolver reads the environment at start-up, so there a rotation is a restart."*

### 9. `get` silently omits correctives and storno numbers; the caller is never told to keep `correction_id`s

- **Persona:** B
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/contract/response.rs:595-620`; `crates/restate-szamlazz-endpoint/README.md:251`, `:317`.
- **Evidence:** `OrderStatus` has four slots. Correctives live under `{namespace}:{order}:corrective:{correction_id}` with caller-chosen ids, so `get` cannot enumerate them; the README describes `get` as "the order's four documents" without saying correctives are not among them, and the Pretix row for `correct_invoice` does not say "record the `correction_id`, you will need it to find the corrective again". Design §6 notes `get` "does not look up the storno number" but the README's `DocumentState::Reversed { storno_number }` reads as if it will.
- **Recommendation:** On `Szamlazz.Order.get` in the handler table: *"Does not list correctives (their ids are yours — keep them; find one with `Szamlazz.Agent.query {"selector": {"external_id": "acct:ORD-1001:corrective:<id>"}}`) and reports `storno_number` only when szamlazz.hu's newest document under the order is the storno."* Add a Pretix note: persist `correction_id` per corrective.

### 10. `Szamlazz.Agent` 422 pass-through puts szamlazz.hu's numeric code in the same `code` field that carries the worker's symbolic tokens

- **Persona:** B
- **Severity:** low-medium
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/agent.rs:139-143`, `:172-176`, `:212-216`; `support.rs:176-179`.
- **Evidence:** `terminal(422, &code, …)` → `{"code": "259", "message": "szamlazz.hu error 259: …"}` while every other fault has `{"code": "invalid_input" | "not_found" | …}`. A client switching on `code` needs `match code { "invalid_input" => …, c if c.parse::<u32>().is_ok() => szamlazz_error(c), … }`. The `CreateResponse` already models this properly (`outcome: rejected`, `code`, `message`).
- **Recommendation:** Emit `{"code": "szamlazz_error", "szamlazz_code": "259", "message": …}` (additive on the wire since responses are open), or document the rule *"a numeric `code` is szamlazz.hu's own; symbolic codes are the worker's"* in both fault tables.

### 11. `invalid_input` for "document szamlazz.hu does not know" on `Szamlazz.Order` vs 404 `not_found` on `Szamlazz.Agent` for the same situation

- **Persona:** B
- **Severity:** low
- **Confidence:** high — deliberate per design §7, but inconsistent across the two services.
- **Location:** `crates/restate-szamlazz/src/service/create.rs:310-314`; `storno.rs:124-128`; `agent.rs:131-135`.
- **Evidence:** `storno_invoice {invoice_number: "X"}` with code 7 → 400 `invalid_input` *"invoice X is not known to szamlazz.hu (not_found)"*; `Szamlazz.Agent.storno` with the same input → 404 `not_found`. The parenthetical `(not_found)` shows the author felt the mismatch.
- **Recommendation:** Either answer 404 `not_found` on both (it is also "fix the request", and the HTTP class says so), or add a row to the fault table: *"`invalid_input` whose message ends in `(not_found)`: the named document does not exist on the resolved account — check the number and the scope."*

### 12. Hungarian tax fields (`tax_number`, `taxpayer_status`, `eu_tax_number`, `group_id`) are named but never explained for a non-Hungarian caller

- **Persona:** B
- **Severity:** medium
- **Confidence:** medium — I have verified the docs say nothing; the NAV rules themselves are outside this review's evidence.
- **Location:** `crates/restate-szamlazz/src/contract/document.rs:149-160`, `:248-265`; `crates/restate-szamlazz-endpoint/README.md:264-272`.
- **Evidence:** `taxpayer_status` is documented as *"Taxpayer status reported to NAV (`adóalany`)"* with variants `non_eu_business`, `eu_business`, `has_tax_number`, `unknown`, `no_tax_number`; nothing says when a caller must set it, what happens when a domestic company buyer has no `tax_number`, or which of `tax_number` / `eu_tax_number` an EU business gets. The curl example has a company buyer (`Kovács Bt.`) with neither field, which for a real invoice is the case NAV cares about. The schema descriptions inherit the same one-liners.
- **Recommendation:** Add a "Buyer fields for NAV" paragraph next to the curl example and to `BuyerInput`'s rustdoc: *"For a Hungarian business buyer set `tax_number` (`NNNNNNNN-N-NN`) and `taxpayer_status: has_tax_number`; for an EU business set `eu_tax_number` and `eu_business`; for a private individual `no_tax_number` and no tax number; `unknown` when you cannot tell — szamlazz.hu decides NAV reporting from these. `Szamlazz.Agent.query_taxpayer` validates a Hungarian number before you issue."* Show one of them in the curl example.

### 13. `vat_rate` is a free string with no enumeration of the accepted codes in the JSON schema

- **Persona:** B
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/contract/document.rs:296-298`; `crates/szamlazz-agent/src/types.rs:176-231`.
- **Evidence:** *"a numeric percentage such as `27` or a NAV-defined code such as `AAM`. The code set is open."* The agent crate documents 21 codes with meanings; none of that reaches the OpenAPI export.
- **Recommendation:** Keep the string type (the set is open) but list the common codes with one-line meanings in the field doc (`27`, `18`, `5`, `0`, `AAM`, `TAM`, `EUT`, `EUKT`, `F.AFA`, `KBAET`, …) and add `"examples": ["27", "AAM"]` via `#[schemars(example = …)]`.

### 14. `Idempotency-Key` guidance is correct but scattered across four places with three different rule counts

- **Persona:** B
- **Severity:** low-medium
- **Confidence:** high
- **Location:** `docs/design/restate-szamlazz.md:441-459` (5 rules); `crates/restate-szamlazz/README.md:282-305` (6 rules); `crates/restate-szamlazz-endpoint/README.md:275-279` (3 rules) and `:306`.
- **Evidence:** The footgun — *rotate the key after a terminal error, or Restate replays the failure* — appears in all four, worded differently, and the one code-shaped place a caller copies from (the curl block, `README.md:263`) shows `idempotency-key: 8b2f6c4e-0001` with no comment on what to send it as.
- **Recommendation:** Make the endpoint README the canonical caller contract (the other two link to it), and add a callout right under the curl example: *"`idempotency-key`: one value per logical request (Pretix: the webhook notification id). **After any 4xx/5xx with `x-restate-error-source: invocation`, send the retry with a new value** — Restate returns the stored failure for the old one for 30 days."*

### 15. Start-up log and `account_mismatch` message use Rust `Debug` formatting

- **Persona:** C
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz-endpoint/src/main.rs:158-165`; `crates/restate-szamlazz-endpoint/README.md:120`; `crates/restate-szamlazz/src/service/support.rs:236-243`; `crates/restate-szamlazz/src/config.rs:196-205` (no `Display` for `AccountMode`).
- **Evidence:** `mode = ?account.mode, supplier_id = ?account.supplier_id` → `mode=Live supplier_id=Some(972720)` (the README reproduces it). The fault: `it carries teszt = false, supplier Some(972720); the resolved account expects teszt = true, supplier None`. The config file says `mode = "live"`.
- **Recommendation:** Implement `Display` for `AccountMode` (`live`/`test`) and log `mode = %account.mode, supplier_id = account.supplier_id.map_or("<unset>".into(), |id| id.to_string())`; in `check_pins` render `supplier 972720` / `supplier <unset>` and `mode live|test` instead of `teszt = bool`, so the message reads in the vocabulary of the config file the operator is about to edit.

### 16. The go-live checklist references probe records not in the repository and is not runnable

- **Persona:** C
- **Severity:** medium
- **Confidence:** high
- **Location:** `docs/szamlazz-hu-behaviour.md:7-9`, `:195-215`; `crates/szamlazz-agent/tests/live.rs`.
- **Evidence:** *"Probe ids (`A1`, `B4`, …) refer to the review record's probe findings A–D and their raw request/response logs, which are not part of this repository."* Checklist rows are "A1 — create with an external id, query by it at +0/+2/+10/+60 s". An operator must reconstruct each probe as `szamlazz` CLI calls by hand. `tests/live.rs` exists but covers a lifecycle, not the eight checklist steps; "(issue #15)" is cited without a link.
- **Recommendation:** Give each step its copy-paste form with the CLI, e.g. step 1: `szamlazz invoice create -f golive.json --json | jq -r .invoice_number` then `for s in 0 2 10 60; do sleep $s; szamlazz invoice get --external-id golive:$ORDER:invoice --json | jq '{teszt: .info.test, supplier: .supplier.id}'; done`; step 8 with the three order-number spellings. Or land the ignored live test and make the checklist "run `cargo test … --test golive -- --ignored` against the target account and read its report".

### 17. The `unavailable` fault for a failed credential fetch names the account id, contradicting "no response names the account"

- **Persona:** B, C
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/service/prologue.rs:145-148`; `CONTEXT.md` (*Outcome*: "No response names the account"); `docs/design/restate-szamlazz.md:363`.
- **Evidence:** `Fault::unavailable(format!("credentials of account {} could not be fetched ({error}); …", account.id))`.
- **Recommendation:** Either drop the id from the message (the invocation journal has it) or amend the claim in CONTEXT.md/design §7 to "no *domain* response names the account; the prologue's `unavailable` may".

### 18. `szamlazz-agent` README example compiles but shows none of `order_number`, `external_id`, PDF or error branching

- **Persona:** A
- **Severity:** medium
- **Confidence:** high — every call was checked: `Client::new(Credentials::agent_key(&str))` (`credentials.rs:67`, `client.rs:162`), `InvoiceHeader::new(Date, Date, PaymentMethod, Currency, Language)` (`invoice.rs:301-307`), `LineItem::calculated_for_currency(…, &Currency)` (`item.rs:129-136`), `CreateInvoice::new(kind, header, buyer, items)` (`invoice.rs:617-622`), `InvoiceCreationResult.invoice_number: Option<InvoiceNumber>` (`invoice.rs:649`).
- **Location:** `crates/szamlazz-agent/README.md:14-48`.
- **Evidence:** The example issues an invoice with no order number and no external id — the two fields the rest of this workspace is built on — and prints `{:?}`. Getting the PDF (`download_pdf`, `Pdf::save_to`) and the one error every integrator hits first (`ErrorCode::DuplicateOrderNumberNamed`) are not shown; `1.into()` for a `Decimal` is unidiomatic next to the crate's own `dec!` usage in tests.
- **Recommendation:** Extend the quick start (all names verified to exist):
  ```rust
  let mut header = InvoiceHeader::new(…);
  header.order_number = Some("ORD-1001".into());          // szamlazz.hu's rendelésszám; also a query key
  let mut request = CreateInvoice::new(InvoiceKind::invoice(), header, buyer, vec![item]);
  request.external_id = Some("myapp:ORD-1001:invoice".into()); // szamlaKulsoAzon: your handle for later queries
  request.download_pdf = true;
  match client.send(&request).await {
      Ok(created) => {
          println!("issued {}", created.invoice_number.as_ref().map(|n| n.as_str()).unwrap_or("<preview>"));
          if let Some(pdf) = &created.pdf { pdf.save_to("invoice.pdf")?; }
      }
      Err(ClientError::Api(ApiError { code: ErrorCode::DuplicateOrderNumberNamed, message })) => { /* already issued under this order number */ }
      Err(other) => return Err(other.into()),
  }
  ```

### 19. The sans-IO path has no end-to-end example

- **Persona:** A
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/README.md:50`; `crates/szamlazz-agent/src/lib.rs:16-32`; `wire.rs:150-162` (`RawResponse::new`).
- **Evidence:** The README's sans-IO paragraph is prose; the crate docs stop at `to_wire` and `assert_eq!(wire.url, ENDPOINT)`. "Sans-IO … Cloudflare Workers" is the headline feature of the workspace README (`README.md:11-12`), yet nowhere is `parse` shown.
- **Recommendation:** Add to `lib.rs` (doc-tested, no network):
  ```rust
  use szamlazz_agent::wire::{AgentRequest, RawResponse};
  let wire = request.to_wire(&credentials)?;
  // POST wire.body to wire.url with `Content-Type: {wire.content_type}` using your HTTP stack …
  let (status_headers, body_bytes) = (vec![("szlahu_szamlaszam", "E-2026-1")], b"<xml…>".to_vec());
  let raw = RawResponse::new(status_headers, body_bytes);
  let created = request.parse(&raw)?;
  ```
  with a canned body from `tests/synthetic` so it runs as a doc test.

### 20. README code examples are not compile-tested (agent, ipn, adatkapcsolat, restate-szamlazz)

- **Persona:** A
- **Severity:** low
- **Confidence:** high — no `include_str!("../README.md")` in any `lib.rs`; only the endpoint crate tests its README's TOML (`config.rs:274-313`).
- **Location:** all `crates/*/src/lib.rs`.
- **Evidence:** Today they compile (checked above); the next signature change will not be caught. The repo already has the pattern for TOML blocks.
- **Recommendation:** `#[cfg(doctest)] #[doc = include_str!("../README.md")] extern "C" {}` (or the `doc-comment` crate) in each library crate.

### 21. CLI `invoice create` requires the user to hand-compute `net_value`, `vat_value`, `gross_value`

- **Persona:** A, C
- **Severity:** medium
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/examples/invoice.json:45-55`; `crates/szamlazz-cli/src/commands/invoice.rs:135-149`; `crates/szamlazz-agent/src/item.rs:129`.
- **Evidence:** The example carries `"net_value": "10000", "vat_value": "2700", "gross_value": "12700"`. The CLI deserialises `CreateInvoice` directly, so `LineItem`'s explicit totals are required and szamlazz.hu rejects arithmetic mistakes (259/260/261). The library has `calculated_for_currency`; the CLI does not expose it.
- **Recommendation:** Add `--calculate` to `invoice create` (recompute every item with `LineItem::calculated_for_currency(…, &request.header.currency)`), or accept items without the three fields and compute when absent. Mention in the README: *"omit `net_value`/`vat_value`/`gross_value` and pass `--calculate` to let the CLI compute them."*

### 22. CLI `--method` takes Hungarian wire tokens and silently sends anything else verbatim

- **Persona:** A
- **Severity:** low-medium
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/src/commands/payment.rs:31-33`, `:57`; `crates/szamlazz-agent/src/types.rs:569-581`.
- **Evidence:** `--method transfer` → `PaymentMethod::from("transfer")` → `Other("transfer")` → sent as the NAV payment-method text. The Restate contract uses `transfer` for the same thing (`contract/document.rs:359-376`). Two surfaces, two vocabularies, and the CLI's failure mode is silent.
- **Recommendation:** Accept the English tokens as aliases in the CLI (`transfer|cash|card|check|cash_on_delivery|paypal|szep_card`) mapping to the variants, and print `warning: unknown payment method "…" sent verbatim` for anything else.

### 23. CLI `taxpayer` accepts only the 8-digit stem; the Restate handler accepts the full number too

- **Persona:** A
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/src/commands/taxpayer.rs:11-13`, `:19`; `crates/restate-szamlazz/src/contract/request.rs:165-169`.
- **Evidence:** `QueryTaxpayer::new(&args.tax_number_prefix)?` errors on `12345678-2-42`; `Szamlazz.Agent.query_taxpayer` derives the prefix from either form.
- **Recommendation:** Reuse the same derivation in the CLI (`split_once('-')` → stem) and rename the arg to `tax_number`.

### 24. CLI exit codes are uniform and errors are unstructured under `--json`

- **Persona:** A, C
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/src/main.rs:71-83` (`anyhow::Result` from `main`).
- **Evidence:** Every failure — missing key, transport error, szamlazz.hu 152, parse error — exits 1 with `Error: …` on stderr. A script using `--json` cannot tell "duplicate order number" from "network down".
- **Recommendation:** Map `ClientError::Api` → exit 2 with `{"error":{"code":"152","message":"…"}}` on stderr when `--json`, `Transport`/`ServiceUnavailable` → exit 3, usage → 64; document the codes in the README.

### 25. `szamlazz listen` defaults to `127.0.0.1:8080`, the same port the repo's `compose.yaml` publishes for Restate ingress

- **Persona:** A, C
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/szamlazz-cli/src/commands/listen.rs:23-25`; `compose.yaml:6-8`.
- **Evidence:** A developer running `docker compose up` (Restate on `127.0.0.1:8080`) and `szamlazz listen` gets `Address already in use`.
- **Recommendation:** Default to `127.0.0.1:8090` (or `0`), and print the two URLs to paste into szamlazz.hu's settings (`http://<tunnel>/ipn`, `http://<tunnel>/adatkapcsolat/`) with the trailing-slash note from the adatkapcsolat README.

### 26. No per-persona "start here"; `CONTEXT.md` is billed as a vocabulary but is a 5,800-word behavioural spec

- **Persona:** A, B, C
- **Severity:** medium
- **Confidence:** high
- **Location:** `README.md:29-35`; `CONTEXT.md` (top entries: *Outcome* 395 words, *Taxpayer lookup* 314, *Order* 294, *Gateway* 286, *Account* 284, *Scope* 270).
- **Evidence:** The workspace README says *"The Hungarian-to-English vocabulary is documented in CONTEXT.md"*. The *Outcome* entry alone specifies HTTP statuses, retry semantics for six fault codes, the 404/422 behaviour of three handlers, and `invalid_input`'s three sources — i.e. design §7 restated. The design doc (10,600 words), lib README (5,600), endpoint README (5,800) and ADR 0006 (4,600) restate the same protocol; a reader has four places to check for the current truth. Nothing tells persona A to skip everything but `crates/szamlazz-agent/README.md`, or persona B to read endpoint README §Services and §Caller guidance only.
- **Recommendation:** (a) Add a "Start here" table to the workspace README: *Issuing from Rust → `szamlazz-agent` README; Calling the Restate services → endpoint README §Services, §Caller guidance, §Faults; Deploying/operating → endpoint README §Configuration, §Deploy checklist, §Runbook; Why it is built this way → design doc, ADRs; Words → CONTEXT.md.* (b) Cap CONTEXT.md entries at 3–4 sentences each — definition, the one distinction that matters, the type, `See: design §n` — and move the behavioural detail into the design doc where it already lives. The *Avoid* lines are the valuable part; keep them.

### 27. Caller contract and fault semantics are triplicated with drifting wording

- **Persona:** B
- **Severity:** medium
- **Confidence:** high
- **Location:** `docs/design/restate-szamlazz.md:441-459`; `crates/restate-szamlazz/README.md:282-343`; `crates/restate-szamlazz-endpoint/README.md:275-294`.
- **Evidence:** Three "Caller contract" lists (5, 6 and 3 rules) and two fault tables that differ in the `account_mismatch` and `outcome_unknown` rows (the lib README says "Rule 2", the endpoint README spells it out). The `invalid_input` row's meaning text is ~90 words in one and ~60 in the other. Which one a client author is meant to trust is not stated.
- **Recommendation:** One canonical caller contract and fault table in the endpoint README (it is what a caller deploys against); the lib README keeps a two-line summary and a link; the design doc references "endpoint README §Faults" instead of restating.

### 28. `reconciled` is protocol vocabulary a caller cannot act on differently from `already_issued`

- **Persona:** B
- **Severity:** low
- **Confidence:** medium — the distinction is meaningful for the maintainer's metrics, not for the caller.
- **Location:** `crates/restate-szamlazz/src/contract/response.rs:27-33`; `crates/restate-szamlazz-endpoint/README.md:238`.
- **Evidence:** `AlreadyIssued`: *"A live document of this kind already exists under our external id; nothing new was issued."* `Reconciled`: *"szamlazz.hu refused the order number as a duplicate and the external-id re-query found our live document: an earlier attempt had landed."* Both return the same fields with the same meaning for the caller. `Found` on the create step is even reported as `issued` (design §5 step 5), so "the document exists, here is its number" is already spread over three tokens.
- **Recommendation:** Keep the enum (journal/metrics value) but state in the outcome docs and the response block: *"Treat `issued`, `already_issued` and `reconciled` identically: the document exists and `invoice_number`/totals are its current values. The distinction is diagnostic."*

### 29. Two vocabularies for document kind across responses

- **Persona:** B
- **Severity:** low
- **Confidence:** high
- **Location:** `crates/restate-szamlazz/src/contract/response.rs:110-111` (`kind: IssuedKind` → `invoice`), `:482-484` (`document_type: String` → `SZ`), `:653-658` (`DocumentStatus` has neither).
- **Evidence:** `CreateResponse.kind = "invoice"`, `QueryResponse.document_type = "SZ"`, `OrderStatus` slots imply kind by field name. A client mapping `query`'s result back to its own model needs the `SZ/D/ES/VS/SS/HS` table, which appears only in a rustdoc comment.
- **Recommendation:** Add `kind: Option<IssuedKind>` to `QueryResponse` beside `document_type` (additive; `None` for `SS`/`SL`), or at minimum put the code table in the endpoint README's `Szamlazz.Agent.query` row.

### 30. `set_payments` contradicts the glossary's own "avoid *payment*" rule; `storno` vs `storno_invoice` differ by service

- **Persona:** B
- **Severity:** low
- **Confidence:** high
- **Location:** `CONTEXT.md` (*Credit entry*: "_Avoid_: payment (overloaded)"); `handlers.rs:363`, `:386`, `:204`.
- **Evidence:** The handler that registers credit entries is `set_payments` with a `PaymentEntry` type; the glossary says not to call them payments. `Szamlazz.Order.storno_invoice` vs `Szamlazz.Agent.storno` for the same operation on different keys.
- **Recommendation:** Renaming handlers is breaking; instead register the intended names as documented aliases in the handler table (*"`set_payments` — registers credit entries (jóváírás)"*, which the README already does) and add a glossary note that the handler names predate the rule. If a v1.0 rename is ever done: `register_credit_entries`, and `storno` on both services.

### 31. `docs/adr/` has no index and four ADRs are "partially superseded" without saying which parts hold; the design doc references a "v1" that is not in the repo

- **Persona:** B, C (and maintainers)
- **Severity:** low
- **Confidence:** high
- **Location:** `docs/adr/0002…:3`, `0003…:3`, `0004…:3` ("partially superseded"); `docs/design/restate-szamlazz.md:94`, `:500-501`, `:563` ("as v1").
- **Evidence:** `README.md:34` links the directory. A reader opening 0002 is told to read 0005 and 0006 to know what still stands. `[account.defaults]   # as v1: …` and `Per-call inputs (DocumentInput) as v1` point at a document that no longer exists (only `docs/design/restate-szamlazz.md` is present).
- **Recommendation:** Add `docs/adr/README.md` with one row per ADR: title, status, "what still holds" in one sentence, superseded-by. Replace "as v1" with the actual field lists (they are short) or a pointer to `config::Defaults` / `DocumentInput`.

### 32. Required dates and payment method have no defaults and no rules

- **Persona:** B
- **Severity:** low
- **Confidence:** medium — szamlazz.hu's own constraints (e.g. `due_date ≥ issue_date`) are not verified here.
- **Location:** `crates/restate-szamlazz/src/contract/document.rs:32-37`.
- **Evidence:** `fulfillment_date`, `due_date`, `payment_method` are required; `issue_date` optional ("let szamlazz.hu date the document"). Nothing says what a sensible `due_date` is for `transfer` vs `card` (paid at once → `paid: true`, due = fulfillment), or that `issue_date` other than today may be refused (behaviour.md records 352 on storno; "352 on create is untested").
- **Recommendation:** One paragraph under the curl example: *"`fulfillment_date` is the day of delivery/service; `due_date` the payment deadline (for a card order paid at once use the same day and `paid: true`); leave `issue_date` unset — szamlazz.hu dates the document and may refuse a back-dated one on e-invoice accounts."*

---

## What is done well

- **Configuration loader** (`restate-szamlazz-endpoint/src/config/schema.rs`): every unknown key at every level reported at once with path, source and the accepted set; env values read as strings so an all-digit agent key survives; both account shapes detected before serde can produce a misleading "missing field id"; the pre-release layout refused with "where it went". `--check-config` for CI/init containers. This is the best configuration UX I have seen in a Rust service of this size.
- **Closed request bodies via `Body<T>`** turning a misspelt `reissue`/`additive`/`force` into a 400 that names the field instead of a silent default — and the design doc explains *why* each of those defaults would have been dangerous.
- **Outcomes as data with a small, well-named enum**; faults kept to six codes, each with an HTTP status and a stated caller action; `x-restate-error-source` guidance quoted from Restate's docs.
- **`check_account` deploy checklist** with a copy-paste curl, a five-row "answer → meaning" table, and the explicit `scope: null` canary for the protocol-v7 trap.
- **The Pretix worked example** (account as first-class entity, scope = account id, order key = event slug + code, `Idempotency-Key` = notification id, limit keys ≠ scopes) is exactly the guidance an integrator needs.
- **`szamlazz-agent`**: open `VatRate`/`ErrorCode` enums, verbatim Hungarian messages kept, 21 error codes documented with observed messages, `Credentials`/`AgentKey` with redacted `Debug`, `ClientError::Transport` docs warning about duplicate issuance on retry, sensible reqwest defaults explained.
- **IPN and Adatkapcsolat READMEs**: short, correct (examples checked against `from_form_bytes`, `Document::parse`, `InvoiceAck::accept(i32).to_xml(InvoiceDirection)`, `Ack::accept().to_*_xml()`), with the two semantics that matter stated in bold (snapshot-not-delta; KEY_ERR/KEY_DEL are control codes, non-200 means retry).
- **`docs/szamlazz-hu-behaviour.md`** is an unusually honest record: what was observed, how, on which account, and a "Still unverified" list.
- **CLI credential handling**: `--agent-key` documented as "prefer the env var — visible in `ps` and history", `hide_env_values = true`, same for `--adatkapcsolat-key`; PDF-to-stdout routes the summary to stderr.
- No dead relative links anywhere (checked every `](…)` in all READMEs, the design doc and the ADRs; heading anchors resolve).

## Questions I could not resolve

1. **Restate envelope shape at 1.7.8** — I confirmed from the e2e harness that the fault is the string in `body["message"]`; I did not run a server to capture the full envelope (whether it also carries `restate_code`, and its exact `content-type`). Finding 1's example should be verified against a live reply before it goes into the README.
2. **Whether `#[restate_sdk::handler]` doc comments reach the OpenAPI export** — the macro source (`restate-sdk-macros-0.12.0/src/struct_generator.rs:203-233`) collects `documentation`, so the one-line handler docs in `handlers.rs` should appear; I could not check what the admin API's OpenAPI export renders for the `Body<T>` schemas without a server.
3. **NAV rules behind `taxpayer_status`/`tax_number`** (finding 12) — the recommendation's wording is my understanding of szamlazz.hu's documentation, not something verified in this repo; a Hungarian accountant should approve the paragraph.
4. **Does the SDK expose the invocation id to a handler in 0.12?** I found `invocation_id` on the internal context but no public accessor on `ObjectContext`/`Context`; finding 5's span therefore proposes `account`/`scope`/`key` only.
5. **`sys_invocation` column names** in finding 4 are taken from the e2e harness (`status, completion_failure, scope, target_handler_name, retry_count, last_failure, last_failure_related_command_name, target_service_key`); `target_service_name` is from Restate's documented schema and was not exercised by the harness.
