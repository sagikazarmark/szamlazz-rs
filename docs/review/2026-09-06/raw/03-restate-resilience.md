# Review 03 — Restate worker resilience & exactly-once (restate-szamlazz)

Scope: `crates/restate-szamlazz/src/{gateway.rs, gateway/build.rs, service/{create,storno,handlers,agent,support,prologue}.rs, identity.rs, account.rs, config.rs}`, tests in `tests/gateway.rs` and `tests/service.rs`, read against `CONTEXT.md`, `docs/szamlazz-hu-behaviour.md` and ADRs 0002–0006. I also read the relevant parts of `szamlazz-agent` (`client.rs`, `wire.rs`, `ops/invoice.rs`, `ops/query_xml.rs`, `types.rs`), `restate-sdk 0.12.0`, `restate-sdk-shared-core 7.0.3` and `restate-server v1.7.8`'s `invocation_state_machine.rs` (fetched from GitHub) to verify the retry-budget claims. No files in the repo were modified; no cargo commands were run.

## Summary

The core exactly-once design is sound and carefully implemented: every write step is one `ctx.run` whose closure begins with the external-id query, every szamlazz.hu *answer* is journaled data, only "no answer" is retryable, and the invocation-level policy is `kill`, so the order key is always released. I could not construct an interleaving in which the code, *as written against the verified szamlazz.hu behaviours*, issues a duplicate document of a kind that szamlazz.hu's toggle or the external-id query guards. The residual risks are (a) guards the code depends on that `szamlazz-hu-behaviour.md` never verified — above all that a **corrective (`HS`) is queryable by its external id**, which is the *only* guard correctives have; (b) the 2-minute "wait out the stall" invariant, which is load-bearing for the create/storno steps but is neither validated in `WorkerConfig` nor applied to `Szamlazz.Agent.storno`'s crash retry; (c) `Szamlazz.Agent` being an unkeyed service whose writes are serialized by nothing on the worker side; (d) an order-key alphabet wider than what was verified to round-trip through `rendelesszam`, where a mismatch would strand an issued document behind a permanent `external_id_collision`. I also confirmed, from the server source, that ADR 0004's arithmetic about run retries not consuming the invocation budget is correct for restate-server 1.7.8.

Severity scale: critical = duplicate/lost legal document under verified behaviour; high = duplicate/lost document under a plausible-but-unverified behaviour or a permanent stuck order with a document issued; medium = availability/operability or a safety property that depends on configuration discipline; low/info = inconsistency, hygiene, documentation.

---

## Findings

### 1. Correctives rely solely on external-id queryability of `HS`, which the behaviour doc never verified

- **Severity:** high
- **Confidence:** medium — the consequence (a duplicate corrective on a lost reply) is certain *if* the premise fails; the premise (an `HS` is not found by `szamlaKulsoAzon`) is unverified rather than observed. `szamlaKulsoAzon` is a header field of the same create XML for every kind, so it probably attaches uniformly — but "probably" is the point.
- **Location:** `src/service/create.rs:274-344` (`correct`), `src/gateway.rs:923-969` (`create_inner`), `src/gateway.rs:1014-1017` (corrective 71/152 → `Rejected`); `docs/szamlazz-hu-behaviour.md:24, 102, 149-151`.
- **Evidence:** The behaviour doc verifies external-id queryability for `SZ` (A1, A3, A5), `SS` (B6) and `D` (D1/D4). There is no row showing an `HS`, `ES` or `VS` was queried back by external id. For correctives the doc itself says "the external-id query is the only guard" (line 24) because 71/152 does not apply and the identical-request replay is untested for `HS` (line 149-151). `correct` takes no order-number hint (`lookup_inner` skips it for `IssuedKind::Corrective`, `gateway.rs:847`) and `after_duplicate` turns a 71/152 into `Rejected` without naming.
- **Scenario:** `correct_invoice{correction_id: c1}` → lookup 7 → create step: leading query 7 → send → szamlazz.hu issues `HS-2`, the reply is lost (60 s client timeout during a stall) → immediate re-query 7 (in-flight) → `Unconfirmed` → 2 m later the re-executed closure's leading query returns 7 *again* if the external id did not attach to the `HS` or the query does not surface `HS` by external id → send again → a second `HS-3` for the same correction, no server-side refusal (correctives are exempt from 152; replay unverified). Every later `correct_invoice` with the same `correction_id` would issue another.
- **Why it matters:** A corrective is a numbered legal document reported to NAV; the design has no second guard here, and the worker cannot detect the duplicate afterwards (`get` does not enumerate correctives).
- **Recommendation:** Add an explicit go-live probe: create an `HS` with a `szamlaKulsoAzon`, query it by that id (+0/+2/+60 s), and record the result in `szamlazz-hu-behaviour.md`; do the same for `ES` and `VS`. Until verified, consider taking the order-number hint on correctives too (an `HS` newer than the base under the order that is *not* under `…:corrective:{id}` is a strong "something landed" signal) or refusing `correct_invoice` in production until the probe passes.

### 2. The load-bearing "issue.initial_delay ≥ client timeout + stall" invariant is not validated

- **Severity:** medium (the *consequence* of violating it is a duplicate document; the code definitely does not enforce it)
- **Confidence:** high
- **Location:** `src/config.rs:98-139` (`WorkerConfig::validate`), `src/config.rs:511-520` (defaults), `crates/szamlazz-agent/src/client.rs:136-140` (60 s hard-coded timeout), `tests/service.rs:753-781` (`initial_delay: "1s"` in the e2e config).
- **Evidence:** `validate()` checks only `max_attempts != 0`, `initial ≤ max` and `factor ≥ 1`. ADR 0002/0004/0005 and the behaviour doc (line 128) state that the 2 m `initial_delay` "must exceed timeout plus stall — never below ~90 s". The 60 s client timeout lives in another crate and is not visible to `WorkerConfig`. The e2e suite ships a configuration with `initial_delay = "1s"`.
- **Scenario:** An operator copies the test/dev configuration into production (or "tunes" `[issue].initial_delay` down to speed up retries). Create step: send stalls, client times out at 60 s, immediate re-query misses, `Unconfirmed`; the step re-executes after 1 s, its leading query still misses (server still processing), it sends again; both land → two invoices (identical content → replay *may* save it; different `kelt`/comment does not matter, but the toggle's replay is unverified under concurrency).
- **Why it matters:** This single number is the only thing standing between "lost reply" and "duplicate document" in the stall case; it is currently enforced by documentation.
- **Recommendation:** Enforce a floor in `validate()` (e.g. `issue.initial_delay ≥ 90 s`, derived from a constant the agent crate exports for its client timeout) with an explicit, loudly named override for tests (`allow_unsafe_issue_delay`). Also assert the same relationship for the `Szamlazz.Order` handlers' `initial_interval` (see finding 3) in the discovery test.

### 3. `Szamlazz.Agent.storno` has no `initial_interval`: a crash retry can race the in-flight storno

- **Severity:** medium
- **Confidence:** high that the code differs from the documented rule; medium that szamlazz.hu would issue two `SS` for two *concurrent* stornos (only sequential repeat was verified, B4).
- **Location:** `src/service/handlers.rs:379-385`; compare `set_payments` at `:352-357` (has `initial_interval = "2m"`) and every `Szamlazz.Order` write handler (`:44-51`). `docs/szamlazz-hu-behaviour.md:128` ("the handlers' `initial_interval` (a crash) and the issue policy's `initial_delay` … are both `2m`, never below ~90 s"); ADR 0004 §#41 fixed only `set_payments`.
- **Evidence:** `invocation_retry_policy(max_attempts = 2, on_max_attempts = "kill")` — no interval, so a process crash mid-`storno-{number}` closure is re-dispatched on the server default (`default-retry-policy.initial-interval = 500ms`).
- **Scenario:** `Szamlazz.Agent.storno{SZ-9}` → verify live → lookup absent → storno closure sends → szamlazz.hu stalls (≥ 57 s observed) → the worker process crashes/is redeployed mid-send → ~0.5 s later the retry re-executes the closure → leading query: nothing under `ns:by-number:SZ-9:storno` yet → sends a second `xmlszamlast` while the first is still being processed. Whether szamlazz.hu serializes two concurrent stornos of one invoice is unverified.
- **Why it matters:** The run-policy path (`Unconfirmed`) is protected by the issue policy's 2 m; the *crash* path is not, contradicting the behaviour doc and ADR 0004's stated rule.
- **Recommendation:** Add `initial_interval = "2m"` to `Szamlazz.Agent.storno` (as `set_payments` has), assert it in the discovery test, and amend ADR 0004 §#41.

### 4. `Szamlazz.Agent` is unkeyed: nothing on the worker serializes two writes to the same unmanaged invoice

- **Severity:** medium
- **Confidence:** high that there is no worker-side serialization; medium on the server-side consequence (unverified concurrent behaviour).
- **Location:** `src/service/agent.rs:231-328` (`storno_request`), `:187-224` (`set_payments_request`); `src/service/handlers.rs:264` (`#[restate_sdk::service]`).
- **Evidence:** `Szamlazz.Order` gets its per-key lock from the Virtual Object; `Szamlazz.Agent` is a plain service. Two concurrent `storno` invocations for the same by-number invoice both run verify → lookup → send. The `Idempotency-Key` dedupes retries of *one* logical request only.
- **Scenario:** Two operators (or a webhook fan-out) call `Szamlazz.Agent.storno{SZ-9}` within the same second. Both leading queries miss, both send. Verified behaviour covers a *repeat* storno (echo); concurrent processing is not covered. For `set_payments` with replace semantics the race is last-writer-wins (benign); with `additive: true` both append (intended at-least-once, but now also at-least-twice by design).
- **Why it matters:** Design §6's "query-first storno" assumes one writer per invoice at a time; that assumption holds for `Szamlazz.Order` and not for `Szamlazz.Agent`.
- **Recommendation:** Either move by-number writes onto a Virtual Object keyed by invoice number (e.g. `Szamlazz.Document`), or document the limitation on `Szamlazz.Agent.storno`/`set_payments` and add a go-live probe for two concurrent stornos.

### 5. Order keys admit characters whose `rendelesszam` round-trip is unverified; a mismatch strands an issued document behind a permanent collision

- **Severity:** high (if the premise holds: a legal document exists and the order is permanently `conflict{external_id_collision}`); medium-likelihood premise.
- **Confidence:** medium — the code path is certain; whether szamlazz.hu normalizes internal whitespace, NBSP, or non-NFC Unicode in `rendelesszam` is listed as unverified in the doc (line 169-170).
- **Location:** `src/identity.rs:40-59` (`OrderKey::parse` accepts a single internal space, `\u{a0}` when not adjacent to other whitespace, any non-control Unicode), `src/gateway.rs:383-385` (`carries_order`: exact trimmed comparison), `src/gateway.rs:395-405` (`is_ours`), `src/gateway.rs:1124-1126` (`Seen::Collision`).
- **Evidence:** ADR 0002 says "internal whitespace and control characters are rejected rather than collapsed" but only *runs* of whitespace are rejected; `"a b"` and `"rendelés #42"` are accepted (`identity.rs:241-243` tests). The behaviour doc verified only edge whitespace and case (C4).
- **Scenario:** `create_invoice` on key `ORD 42` (single space). Lookup 7 → create sends `<rendelesszam>ORD 42</rendelesszam>` → issued `SZ-5`. If szamlazz.hu stores/echoes `ORD42` or `ORD&#160;42` differently, the immediate/any later external-id query returns `SZ-5` with `rendelesszam ≠ "ORD 42"` → `is_ours` false → `Seen::Collision` → every future call answers `conflict{external_id_collision, SZ-5}`; `get` shows the slot empty; a storno via `storno_invoice` is refused as `not_managed` (`storno.rs:132-137` uses `carries_order` too). The document is real, unmanageable through the worker, and the order looks "collided".
- **Why it matters:** The validation formula is the whole ownership model; any server-side normalization of the key alphabet breaks it in a way the worker cannot recover from.
- **Recommendation:** Add the internal-space / non-ASCII / NBSP cases to the go-live checklist (create + query by external id + compare `rendelesszam` byte-exact), or tighten `OrderKey` to the verified alphabet (`[A-Za-z0-9._/#-]`, no internal whitespace) until verified. Consider an operator-facing `Szamlazz.Agent.query` hint in the collision fault that names the observed `rendelesszam`.

### 6. No timeout around the credential fetch or the resolver call

- **Severity:** medium (availability: holds the order key for up to inactivity+abort per attempt)
- **Confidence:** high
- **Location:** `src/service/prologue.rs:113-134` (`fetch_credentials`: three attempts, 200 ms pause, no per-call deadline), `src/service/support.rs:405-421` (`account` step: `accounts.resolve(...).await` inside `ctx.run` with no deadline).
- **Evidence:** The static resolver is in-memory, but the traits are designed for database/vault-backed implementations. A `fetch` that never returns runs *outside* the journal: the inactivity timer (4 m) then abort (3 m) fire, the server re-dispatches per the invocation policy (2 m → 10 m, 5 attempts, kill). A `resolve` that hangs inside the run defeats the resolve policy's `max_duration = 1m` (the policy only evaluates on closure *failure*).
- **Scenario:** Vault/DB network partition. Every in-flight `Szamlazz.Order` invocation holds its key for ~7 m per attempt × 5 attempts before kill, instead of the intended immediate terminal `unavailable`.
- **Why it matters:** ADR 0006 chose a *terminal* `unavailable` precisely to avoid routing store outages into the kill-on-five path; a hang re-routes them there.
- **Recommendation:** Wrap `accounts.fetch` and `accounts.resolve` in `tokio::time::timeout` (a few seconds), mapping the elapsed timeout to `FetchError::Unavailable` / `ResolverUnavailable`.

### 7. A failed *leading* query inside the create/storno closure costs an issue-policy attempt and a 2 m delay although nothing was sent; an answered API code there is retried as if unanswered

- **Severity:** medium (availability/consistency; safe)
- **Confidence:** high
- **Location:** `src/gateway.rs:1068-1107` (`settled_by_query`: `Err(error) => Err(Unconfirmed::Transport(error.to_string()))` at `:1105` for *every* non-credential `QueryError`, including `Api{code}` and `Unavailable`), `src/gateway.rs:1404-1419` (`storno_settled_by_query`, same at `:1417`), vs `src/gateway.rs:838-841` (lookup maps `Api` to `LookupOutcome::Api` → immediate `unavailable`).
- **Evidence:** The create closure's first line is the external-id query. If it fails (transport, `szlahu_down`, *or* an answered code such as 57), the closure returns `Unconfirmed` and the issue policy waits `initial_delay` (2 m, doubling) and consumes one of five attempts — although nothing was sent and a fast retry would be safe. An answered code 57 on the leading query is repeated five times (~39 min) and ends as `outcome_unknown`, whereas the same answer one step earlier (the lookup) is an immediate `unavailable`.
- **Scenario:** szamlazz.hu answers the leading query with a transient 5xx twice in a row: the caller waits 2 m + 4 m for a step that never sent anything; three such blips exhaust the budget → `outcome_unknown` with zero documents.
- **Why it matters:** The issue policy is sized for the *post-send* uncertainty window; applying it to a pre-send read wastes the budget and misreports (an `Api` answer is not "outcome unknown").
- **Recommendation:** Inside the closure, retry the *leading* query under a short in-process policy (safe: nothing has been sent), and map an answered `Api` code on the leading query to a settled data variant (`CreateOutcome::Api`/`Inconclusive`) rather than `Unconfirmed`. Keep post-send failures on the issue policy.

### 8. Adversarial numeric input can panic before any journal entry, holding the order key for the whole invocation retry budget

- **Severity:** medium (availability; no document risk)
- **Confidence:** medium — `rust_decimal`'s `Mul`/`Add` panic on overflow ("Multiplication overflowed"); I did not execute it.
- **Location:** `crates/restate-szamlazz/src/contract/document.rs:336-348` (`to_line_item` → `LineItem::calculated_for_currency`), `crates/szamlazz-agent/src/item.rs:137-145` (`unit_price * quantity`, `net_value * rate / 100`, `net + vat`), reached from `src/service/create.rs:371-392` (`validate_document`, step 0, before the prologue) and `src/gateway/build.rs:155-159`.
- **Evidence:** `Decimal` deserializes 28-digit mantissas; multiplying two large values overflows and panics. A panic in a handler is a transport-level failure to the server, retried under the handler's `invocation_retry_policy` (2 m → 10 m, 5 attempts, kill) while the Virtual Object key is held.
- **Scenario:** A buggy or hostile caller sends `unit_price: "79228162514264337593543950335", quantity: "10"` for order `ORD-1`. Every attempt panics in step 0; `ORD-1`'s legitimate creates queue behind it for ≈ 39 minutes; nothing is journaled, nothing is sent.
- **Why it matters:** Cheap denial of service against a specific order; also every such event looks like an infrastructure failure in `sys_invocation`.
- **Recommendation:** Use checked arithmetic in `LineItem::calculated_*` and surface overflow as `InputError` → `invalid_input`; add a contract test with extreme decimals.

### 9. `Szamlazz.Agent.storno` does not pre-check the document type; it sends and relies on the echo

- **Severity:** low
- **Confidence:** high
- **Location:** `src/service/agent.rs:285-296` (no `document_type` check) vs `src/service/storno.rs:148-150` (`Szamlazz.Order` refuses `D`/`SL`/`SS` before sending); `src/gateway.rs:1353-1360` (`reverses()` echo check).
- **Evidence:** For a proforma or delivery note without an order number, `Szamlazz.Agent.storno` sends `xmlszamlast`; szamlazz.hu answers a success-shaped no-op (B5), `reverses()` is false → `rejected{not_stornoable}`. For an `SS` → server 14. Correct in the end, but a write is attempted and the classification depends on the echo shape (`invoice_number ≠ requested ∧ gross ≤ 0`), which the doc lists as partially unverified for zero-gross documents.
- **Recommendation:** Mirror `verify_for_storno`'s type check in `storno_request` before `lookup_storno`.

### 10. Read-only handlers keep the server-default 1 m inactivity timeout while a single read can take 60 s

- **Severity:** low
- **Confidence:** high on configuration; medium on the practical effect (depends on how the server treats a run that completes during the suspension window).
- **Location:** `src/service/handlers.rs:248-251` (`get`), `:273-282` (`check_account`), `:291-300` (`query`), `:319-328` (`query_taxpayer`) — no `inactivity_timeout`/`abort_timeout`; `crates/szamlazz-agent/src/client.rs:138` (60 s client timeout).
- **Evidence:** `get` runs four sequential reads, each up to 60 s. A stall on one read hits the 1 m default inactivity timeout; the server then asks to suspend and, after the default 10 m abort, aborts. ADR 0004 sized `4m/3m` for `Szamlazz.Order` but the read handlers were left at defaults.
- **Recommendation:** Set `inactivity_timeout = "2m"` (or more) on the read-only handlers, or document why the default is acceptable.

### 11. External-id segment ambiguity: order keys may contain `:` and a correction id may equal a kind token

- **Severity:** low (detected as a collision → safe refusal, but two orders can block each other)
- **Confidence:** high
- **Location:** `src/identity.rs:40-59` (no `:` restriction on `OrderKey`), `:157-180` (`for_kind`, `for_corrective`, `for_storno`, `for_unmanaged_storno`), `CorrectionId` pattern `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`.
- **Evidence:** Order `X` with `correction_id = invoice` → `acct:X:corrective:invoice`; order `X:corrective` kind invoice → `acct:X:corrective:invoice`. Two different documents share one external id; the newest hides the other, and each order sees the other's document as `conflict{external_id_collision}`.
- **Recommendation:** Reject `:` in `OrderKey` (the namespace already excludes it for this reason) and reserve the kind tokens / `storno` / `corrective` / `by-number` / `check-account` as forbidden `CorrectionId` values — or escape segments.

### 12. `get` is four independent reads on a shared handler — not a snapshot — and an `Api` answer fails it despite the "must not fail on an answer" comment

- **Severity:** low
- **Confidence:** high
- **Location:** `src/service/storno.rs:224-263` (`status`), `src/service/support.rs:315-338` (`Lookup::classify` maps `QueryOutcome::Api` to `Fault::inconclusive_answer` → 503).
- **Evidence:** `get` runs concurrently with exclusive handlers; a conversion between the `proforma` and `invoice` reads shows both live. The doc comment says "a read must not fail on an answer", yet a code-57 answer on any of the four reads is a 503 `unavailable`.
- **Recommendation:** Document that `get` is eventually consistent; consider reporting an `Api` answer as a per-slot `unknown` rather than failing the whole read.

### 13. `storno_number_of` swallows a cancellation and completes the invocation successfully

- **Severity:** low
- **Confidence:** high
- **Location:** `src/service/support.rs:650-667`.
- **Evidence:** `hint()` returns the `Fault` for *any* run failure, including the SDK's 409 cancellation; `storno_number_of` logs and returns `Ok(None)`, so `storno_invoice` on an already-reversed invoice returns `200 reversed` to a caller who cancelled it. Harmless here (nothing further is written), but it establishes a pattern of continuing past a cancel.
- **Recommendation:** Distinguish `error.code() == 409` in `read_exhausted`/`storno_number_of` and propagate cancellation.

### 14. `credentials_rejected` wording ("this attempt issued nothing") can be false on the re-query

- **Severity:** low (contract stays correct: a fault means "outcome unknown")
- **Confidence:** high
- **Location:** `src/service/support.rs:112-130` (message text), `src/gateway.rs:1102-1104` (`CredentialsRejected` on the *re-query* after a send), `CONTEXT.md` Outcome entry.
- **Evidence:** Send lands → reply lost → key rotated in the same window → the immediate re-query answers 3 → `CreateOutcome::CredentialsRejected` is journaled and the fault says the attempt issued nothing. It did.
- **Recommendation:** Reword to "may have issued; read `get` after fixing the key", or carry a `sent: bool` into the fault.

### 15. `Endpoint` accepts userinfo, so a journaled `Account.endpoint` could carry a password

- **Severity:** low
- **Confidence:** high
- **Location:** `src/account.rs:180-190` (`Endpoint::parse` checks scheme and host only; the `InvalidEndpoint` doc at `:247-248` even notes userinfo), journaled via `Resolution::Account` (`src/service/prologue.rs:36-47`).
- **Recommendation:** Reject URIs with userinfo in `Endpoint::parse`.

### 16. Journal compatibility protects type layouts, not the *sequence and names* of journal entries

- **Severity:** low / info
- **Confidence:** high
- **Location:** `src/service/journal.rs` (fixtures per type), `docs/adr/0005-…md` §#47.
- **Evidence:** Inserting, removing or renaming a `ctx.run` in a handler (e.g. a new read before `lookup-{kind}`, or renaming `exclusivity-{other}`) makes an in-flight invocation's replay non-deterministic on the next deployment; the fixtures cannot catch it. The README's "drain first" guidance covers it operationally only.
- **Recommendation:** Pin the ordered list of run names per handler in a test (a golden "journal shape" per handler), or at least note the gap in ADR 0005.

### 17. Foreign-document false positives name the order's own final invoice as "another channel's"

- **Severity:** low (safe refusal, misleading reason)
- **Confidence:** high
- **Location:** `src/service/create.rs:194-204` (`exclusive_with`: `Invoice` checks only `Prepayment`, `Prepayment` only `Invoice`, `Proforma` both, `Final` none), `src/gateway.rs:1677-1682` (`is_foreign`: `SZ|ES|VS` live and not in `our_numbers`).
- **Evidence:** Order with `ES` reversed and `VS` live (the prepayment stornoed after the final — allowed by the server as far as verified). `create_prepayment` (new `ES`) → exclusivity finds no live `SZ` → lookup hint returns the live `VS`, which is ours but not in `our_numbers` → `conflict{foreign, VS-…}`.
- **Recommendation:** Add `…:final` to `exclusive_with(Prepayment)` and `exclusive_with(Proforma)`, or push the found `VS` number into `our_numbers`.

### 18. (Positive verification) Run retries do not consume the invocation retry budget; the attempt count is durable at every run site in this crate

- **Severity:** info
- **Confidence:** high — read from `restate-server v1.7.8 crates/invoker-impl/src/invocation_state_machine.rs::handle_task_error` and `restate-sdk-shared-core 7.0.3 src/vm/transitions/journal.rs:815-830`, `src/vm/context.rs:247-265`.
- **Evidence:** The server computes `next_retry_interval_override.or_else(|| self.retry_policy_state.retry_iter.next())`: when the SDK supplies `next_retry_delay` (which `RunRetryPolicy::new()`-built policies always do), the invocation policy's iterator is *not* advanced, so `max_attempts = 5` on the handler is untouched by run retries — ADR 0004's "issue executions + invocation attempts − 1" arithmetic holds. The SDK restores `retry_count`/`retry_loop_duration` from `StartMessage` only when the failing run is the first entry committed after replay; in this crate every `ctx.run` is awaited sequentially with no non-journaled context actions between runs (the credential fetch is not a journal action), so that condition holds for every write and read step. `max_attempts(1)` on `run_once` is exactly one execution (`max_attempts <= retry_count` after the first failure).
- **Note:** ADR 0004's "accepted risk: the attempt count is not durable" is therefore more conservative than needed; the *duration* bound is subject to the same condition (both are restored together), so the ADR's "max_duration is the hard bound" is not more durable than `max_attempts` — they are equally durable here.

### 19. (Verified) Client timeouts, response-version mismatch, 200-with-error-header and `szlahu_down` all funnel into query-first re-execution

- **Severity:** info
- **Confidence:** high
- **Location:** `crates/szamlazz-agent/src/client.rs:136-140` (60 s total timeout, no redirects, cookie store), `src/gateway.rs:1633-1672` (`classify_failure`: reqwest/parse errors → `Transport`, `szlahu_down`/1/55/56 → `Unknown`, 71/152 → `Duplicate`), `:973-982` (`settle_or`), `crates/szamlazz-agent/src/ops/invoice.rs:1045-1130` (`parse_creation_result` reads headers and body; a version-1 or HTML body is a `Parse` error).
- **Evidence:** A reqwest timeout is `ClientError::Transport` → `Failure::Transport` → immediate re-query → `Unconfirmed::Transport` if nothing → issue policy. A 200 with `szlahu_error_code` is `Api`; `sikeres=false` in the body is `Api`; `szlahu_down` is `ServiceUnavailable` → `Unknown{code: None}`. HTTP status is ignored by design (`client.rs:183-205`), so a 5xx HTML page is a parse failure → `Transport`. All paths end in "re-query, then let the policy re-execute query-first", which is the right shape.

### 20. (Verified) Credentials never reach the journal, faults or `Debug` output

- **Severity:** info
- **Confidence:** high
- **Location:** `src/account.rs:435-436` (`assert_not_impl_any!` on `Credentials`/`AgentKey`), `:441-497` (renderings test incl. `Gateway` `Debug`), `crates/szamlazz-agent/src/credentials.rs:27-30, 86-97` (redacted `Debug`), `src/service/prologue.rs:26-30` (`Execution: Debug` → `Arc<Gateway>` → redacted), `src/service/support.rs:112-130` (warn carries namespace + code only). `WireRequest`'s `Debug` prints `body_len` only (`wire.rs:39-49`). The agent crate has no `tracing`/`log` calls. Only the `Resolution` (with `Account`, no key) and outcome types are journaled.

---

## Answers to the specific questions

1. **Exactly-once interleavings.** Crash after send before journal → invocation retry after 2 m → replay to the create run → leading query finds the document → `Found` → `issued` (no duplicate; `tests/service.rs:1914` covers the lost-reply variant, the crash variant is covered indirectly by "the closure re-executes"). Crash between lookup and create → replay uses the stale lookup but the create's own query is fresh; a *foreign* document appearing in that window is caught by 152 (same kind) and named via the hint (`after_duplicate`). Replay under a new deployment → protected for types (fixtures), not for entry sequence (finding 16). Two invocations on the same key → serialized for exclusive handlers; `get` is shared but read-only; `Szamlazz.Agent` has no lock (finding 4). Same order under two scopes → two VOs on two accounts by contract; under fan-in the toggle + external-id `Reconciled` path still prevents a duplicate for *sequential* sends, concurrent sends are unverified. Reply lost after the server received → re-query, then 2 m → found (finding 2 is the configuration dependency). 200 + error header / version mismatch / timeout → finding 19; a timeout is `Unconfirmed`. Retry racing a still-in-flight original → protected only by the 2 m gap (findings 2, 3).
2. **Create step send condition** (`gateway.rs:1068-1107`): send iff the external-id query is code 7, **or** returns a document that `is_ours`, is reversed, and whose *number* equals `request.reversed` (the number the lookup saw). Comparison is by invoice number (`Some(found.number()) != request.reversed`). Newest holder changed to a live document → `Found`/`issued`; to a reversed document with another number → `Reversed`; the lookup's reversed document reported live → `LiveAgain`/`conflict{live}`; a different `tipus`, order, `teszt` or supplier → `Collision` (checked before liveness, `gateway.rs:1124`).
3. **Lost-reply re-query.** The +771 ms lag evidence (doc line 50) is measured *after the create returned*; in the lost-reply case the create has not returned, so "once, immediately" is expected to miss during a stall and safety rests entirely on the 2 m `initial_delay` (finding 2). Under the verified stall (≥ 57 s) and a 2 m gap, the re-executed leading query runs ≥ ~181 s after the send began — adequate margin; a stall > ~3 min would produce a duplicate, and nothing in the code can prevent that.
4. **Foreign detection** (`gateway.rs:845-874, 1677-1682`): excludes `SS`, `HS`, `D`, `SL`, reversed documents and `our_numbers` (tested at `tests/gateway.rs:585`). False positive: the order's own `VS` (finding 17). False negatives: only the newest document under the order is seen; a foreign document without `rendelesszam` is invisible (doc line 166-168); the hint is taken only in the lookup, not in the create, so the 152 toggle is the guard in the lookup→create window.
5. **Policies.** Issue `5 × 2m→10m, 1h`; read `3 × 5s→30s, 2m`; resolve `1s→10s, 1m, no attempt cap` — all via `RunRetryPolicy::new()` (`config.rs:529-536, 593-600, 647-653`). Exhaustion → `TerminalError` from the run → `outcome_unknown` (writes) / `unavailable` (reads); cancel (409) is mapped the same way. `on_max_attempts = "kill"` is present on every handler (`handlers.rs`). Kill/terminal both complete the invocation and release the key. Interaction with the invocation policy verified from server source (finding 18). Gaps: findings 2, 3, 6, 7.
6. **Storno.** `verify-storno-{n}` → `carries_order` → `check_pins` → `sztornozott` → type gate → `lookup-storno-{n}` → `storno-{n}` (query-first, `reverses()` validates the echo). Repeat-storno echo → `Reversed(existing SS)`; proforma/delivery-note echo → `NotStornoable` (Order refuses pre-send, Agent post-send — finding 9); another order's invoice → `conflict{not_managed}`; `storno_number` hint is best-effort (`support.rs:650-683`, swallows even cancellation — finding 13).
7. **Proforma link / consumed / delete / set_payments / get.** `options.proforma: {number}` verifies order, pins, then `tipus == D` (`create.rs:535-582`). Consumed is derived from `hivdijbekszam` of `invoice` or `prepayment` when the proforma slot is empty *or a collision* (`storno.rs:248-261`). `delete_proforma` deletes the number the lookup found; 335 → `deleted` (safe, as the doc argues). `set_payments` has no query, no lock, no pins; `additive` is at-least-once and its crash retry waits 2 m (correct); its only worker-side hazards are the missing serialization (finding 4) and the documented cross-account/number case. `get` is not a snapshot (finding 12).
8. **Prologue.** `account` is journaled once (`support.rs:405-418`); credentials are fetched every execution outside the journal (`:421`) — rotation semantics as documented and e2e-tested. No path journals or logs the key (finding 20). Missing deadlines (finding 6).
9. **Journaled types.** Mechanism is sound: `Journaled` bound on all run helpers, exhaustive per-variant pins, archived old shapes replayed with a superset check. `VatRate` serializes as its wire token (`types.rs:330-342`), so a new NAV code is not a shape change; `Decimal` is a string; a `rust_decimal` feature-unification change would fail both the generator and the compatibility test. Gap: entry sequence (finding 16). `pdf` fields are always `None` (`build.rs:163`, `QueryInvoiceXml::new` sets `include_pdf: false`, `StornoInvoice::new` sets `download_pdf: false`).
10. **HTTP client.** 60 s total timeout (no separate connect timeout), no redirects (correct for a multipart POST), cookie store on, TLS per reqwest defaults, proxy env honoured by reqwest defaults. A fresh `reqwest::Client` per execution gives a fresh cookie jar and pool: the `JSESSIONID` isolation claim holds; the cost is one TLS handshake and one szamlazz.hu login per execution (the doc's "first of a session 4.8 s" is paid once per execution, not per call). The 60 s constant is invisible to `WorkerConfig` (finding 2).
11. **Code vs behaviour doc.** Contradiction: `Szamlazz.Agent.storno`'s crash-retry interval (finding 3). Assumed-but-unverified: `HS`/`ES`/`VS` by external id (finding 1); order-key alphabet round-trip (finding 5); concurrent stornos/creates (findings 3, 4); credential codes being pre-write (doc says unverified; code and wording rely on it — finding 14).

---

## What is done well

- **Query-first inside the closure, for every write** (`gateway.rs:927-929, 1336-1338`), with the send condition stated precisely and enforced by number (`settled_by_query`). The `Reversed`-since-lookup and `LiveAgain` cases (#36) close the last "send past a reversal" hole.
- **Outcome-as-data discipline**: every szamlazz.hu answer, including 71/152, credential codes and the 71/152-with-nothing contradiction, is journaled and never re-sent for; only "no answer" is retryable, split cleanly into `Unanswered` (reads) and `Unconfirmed` (writes).
- **`kill` on every handler that calls szamlazz.hu**, so a stuck invocation never holds an order key indefinitely; explicit `RunRetryPolicy::new()` everywhere so the server never spends the invocation budget on a run — and the arithmetic checks out against the server source.
- **Credential hygiene**: compile-time `assert_not_impl_any!`, redacted `Debug` at every layer, fetch-outside-journal, fresh client per execution, and an end-to-end journal scan with a positive control.
- **Journal compatibility fixtures** with archived shapes and a superset check — a genuinely useful CI guard for a stateless-but-journaled design.
- **The e2e harness** (`tests/service.rs`) drives real Restate with scripted failure injection (lost reply flipped on the create request itself, reversal between executions, flaky reads, resolver/store outages, rotation, two scopes) — the scenarios that matter are exercised, not mocked at the unit level only.
- Ownership validation reads `teszt` and `szallito/id` on *every* found document, by id and by number, including in `Szamlazz.Agent.query`/`storno`.

## Questions I could not resolve

1. Does an `HS` (and `ES`, `VS`) issued with `szamlaKulsoAzon` come back from `xmlszamlaxml` by that external id? (Finding 1 — the single most valuable probe to add.)
2. How does szamlazz.hu behave under two *concurrent* creates for the same order/kind, and two concurrent stornos of the same invoice? The toggle and the storno echo were verified sequentially only (findings 3, 4).
3. Does szamlazz.hu preserve `rendelesszam` byte-exact for internal whitespace, NBSP and non-NFC input? (Finding 5.)
4. On cancellation (409) while the create closure's HTTP send is in flight, does the Rust SDK drop the closure future (aborting the reqwest request client-side) or let it complete before surfacing the terminal error? Either way the contract ("outcome unknown") holds, but the operational narrative differs.
5. Whether the server's `should_bump_start_message_retry_count_since_last_stored_command` is true for SDK-suggested retries — the e2e assertion `retries.max_retry_count >= 1` suggests yes, meaning crash attempts and run attempts share the count the SDK restores (conservative; not a safety issue).
6. Whether the `Szamlazz.Order` handlers' 4 m inactivity timeout has ever been exercised against a real 3-call, 180 s closure on a live account (the doc's stall was one observation).
