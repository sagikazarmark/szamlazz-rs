# szamlazz-rs — Final consolidated review (meta-judge)

Workspace: `/home/laborant/szamlazz-rs2` @ v0.3.0. Inputs: three judge reports (A: protocol crates, B: Restate worker, C: architecture / security-ops / UX-docs) adjudicating eight reviewer reports (≈169 findings). Baseline: suite green (434 passed; the docker-gated e2e and 3 live tests ignored). The three protocol crates are published on crates.io.

Method: every top-10 entry was re-read at the cited lines by the meta-judge; where two judges' notes implied different conclusions the code, `Cargo.lock` or the vendored crate source was consulted. No repo files modified; no cargo commands run. IDs are the judges': `A-nn`/`B-nn`/`J-01` (Judge A; reviewers 01/02), `J1…J44`, `A1…A3` (Judge B; reviewers 03/04/06), `J-05-nn`/`J-07-nn`/`J-08-nn`, `C-A`/`C-B` (Judge C; reviewers 05/07/08).

---

## 1. Executive summary

**Verdict: sound core, fragile edges.** The exactly-once argument of the `restate-szamlazz` worker survived three independent attempts to break it: no reviewer or judge could construct a duplicate-document interleaving under the szamlazz.hu behaviour the repo has verified. The *Create step* is query-first inside the closure, every szamlazz.hu answer is journaled data, and the only timing assumption (≥ 90 s between a send and any re-check) is stated everywhere. Risk sits where the code meets things the repo does not control: the inbound receivers, CI, prose-only invariants, and the caller-facing documentation.

**Three biggest risks.** (1) The Adatkapcsolat receiver loses legal documents deterministically — a transient resolver failure is answered `KEY_ERR` (bank/receipt records are never resent), and ~30 re-validated XSD-required elements plus a strict base64 decode turn a benign omission into a 400 that szamlazz.hu retries for 72 h and drops, with no local artifact (B-04, B-01/B-02/J-01). (2) The worker's handler layer — every `Szamlazz.Order` decision, scope isolation, the leak scan — is exercised only by a docker-gated `#[ignore]` e2e that CI never runs and that passes silently without docker; three handlers are invoked by no test at all; the ≥ 90 s invariant is enforced by nothing and `Szamlazz.Agent.storno` lacks the `initial_interval` the rationale demands; a gap in the exclusivity table can put a live `SZ` beside a live `VS` (J36/J23/J38, J2/J3, A1). (3) The wire shape of every fault is documented wrongly (the JSON body is a string inside the ingress envelope) and the caller contract's headline contradicts its own table, while three load-bearing szamlazz.hu facts — `eszamla` semantics, HS external-id queryability, order-number normalisation — rest on one test account, one day (J-08-01/C-A, A-13, J1, J5).

**Three biggest strengths.** (1) Outcome-as-data discipline end to end: *Lookup step*, *Create step*, *Unconfirmed*/*Unanswered*, journaled types pinned by fixtures and replayed in CI. (2) Credential hygiene that is structural, not procedural: a fresh Számla Agent client per execution, `Credentials` un-serialisable by the compiler, `deny_unknown_fields` on every request body, scope isolation delegated to Restate's key namespace. (3) Honest documentation of the unknown: `docs/szamlazz-hu-behaviour.md` and the ADRs distinguish observed from assumed, which is why reviewer factual accuracy was ≈ 100 %.

**Production-ready?** *Single-account*: yes, conditionally — after the S-sized worker fixes (J3, J2, A1, J8, J7) and the docs fix (J-08-01/C-A), and after the go-live checklist is run on the live account with the A-13/J1/A1 probes added; the Adatkapcsolat receiver is **not** ready as a default-configured public receiver until B-04 and B-05 are fixed and B-01 is made lenient (or the loss is consciously accepted). *Multi-account*: not yet — the paging log line cannot name the account (J-07-13), unsigned Restate requests are accepted by default while the scope travels as protocol data (J-07-01), and scope isolation plus the `check_account` canary are proven only by the e2e CI does not run (J36, J42).

---

## 2. Top 10 findings

Ranked by impact × likelihood × confidence. "Verified" = the meta-judge re-read the cited code; "depends on szamlazz.hu" = the consequence needs a live fact the repo cannot settle.

### 1. B-04 — Adatkapcsolat `KeyResolver` is sync and infallible; a transient lookup failure is answered `KEY_ERR` and permanently drops bank/receipt records
- **Source:** reviewer 02 #4 → Judge A CONFIRMED. **Severity: high. Confidence: high** — verified in code and in the vendor XSD; no szamlazz.hu behaviour is assumed beyond what `banktranzvalasz.xsd:7-8` / `nyugtavalasz.xsd:7-8` state ("a Számlázz.hu ilyenkor a … rekordot nem küldi újra").
- **Location:** `crates/szamlazz-adatkapcsolat/src/axum.rs:39-45` (trait), `:260-275` (`None` → `KEY_ERR` Ack for every root kind).
- **Problem:** `fn resolve(&self, key) -> Option<&Handler>` has one non-success value, and the router renders it as the protocol's *control code* "your key is wrong — stop sending". A resolver backed by a database or a secrets service cannot say "I could not check"; its only honest answer for a timeout is indistinguishable from a wrong key. Bank transactions and receipts are never resent; invoices not until they change. The crate's own reasoning for answering a *missing* header with 401 rather than `KEY_ERR` (`:252-256`) is unavailable to resolvers.
- **Recommendation:** Make resolution fallible (and async): `fn resolve(&self, key) -> impl Future<Output = Result<Option<&Handler>, Self::Error>>` or a three-valued `Resolution::{Matched, Unknown, Unavailable}`; map `Unavailable`/`Err` to 503 so the 72 h retry window stays alive; keep `KEY_ERR` for a definite mismatch only. Document the "never resent" consequence at `router()` and in the README (B-07). Breaking trait change → 0.4.0 of a published crate; the `FixedKey` path is unaffected.
- **Effort:** S (code) — the churn is the version bump.

### 2. B-01 (+ B-02, J-01) — Strict required-field, closed-enum and canonical-base64 validation in the Adatkapcsolat receiver turns a benign omission into a deterministic 400, i.e. document loss after 72 h with no local artifact
- **Source:** reviewer 02 #1/#2 → Judge A CONFIRMED-WITH-CORRECTION (B-01), CONFIRMED (B-02); J-01 is Judge A's own. **Severity: high. Confidence: medium** — the code path is verified; the frequency of omissions in production pushes is not settable from the repo. The repo's own test (`tests/protocol.rs:229-235`, "seen_in_official_batches") records one XSD-required element that real batches omit, so the class is not hypothetical.
- **Location:** `crates/szamlazz-adatkapcsolat/src/document.rs:1001-1084` (`InvoiceDocument::validate`, ~30 `required*` calls), `:1087-1106` (`validate_totals`), `:1445-1485` (receipts), `:1136-1145` (closed `TransactionDirection`), `:1124-1129` (`non_negative`), `:1615-1636` (`de::base64_pdf` with `STANDARD` = `RequireCanonical`, no trailing bits — verified in `base64-0.23.1/src/engine/general_purpose/mod.rs:341-342,436`; Judge A cited 0.22.1, a transitive dependency, but the receiver uses 0.23.1 per `Cargo.lock:205-206` and the defaults are identical); consumer path `axum.rs:277-280`, before any `Handler` or `Archiver` runs.
- **Problem:** A `ParseError::Validation`/`DeError` is a 400; szamlazz.hu retries identically for 72 h and drops the push. Meanwhile the Számla Agent crate parses the byte-identical `szamla.xsd` document with only `id`, `szamlaszam`, `tipus`, `eszamla` required and everything else `#[serde(default)]` (`szamlazz-agent/src/ops/query_xml.rs:724-781`, verified) — the workspace already contains the lenient posture. Note Judge A's evidence correction: the vendor example itself passes (`<nev></nev>` deserialises as `Some("")`), so the receipt-batch test is the real evidence.
- **Recommendation:** Reduce hard validation to identity (root, `alap/id`, `alap/szamlaszam`); keep the strict check behind `Document::parse_strict`; `#[serde(other)]` on `TransactionDirection`; drop `non_negative`; decode PDFs with a padding-indifferent engine and degrade a failed decode to `pdf: None` (raw XML is already retained at `:70`). Add the raw-body hook for rejected pushes (B-03's residual). Behaviour change of a published crate: 0.4.0, with the lenient default documented.
- **Effort:** M.

### 3. J36 (+ J23, J38) — The worker's handler layer is verified only by a docker-gated `#[ignore]` e2e that CI never runs and that returns `Ok` when docker is absent; `correct_invoice`, `create_final`, `set_payments` are invoked by no test, `delete_proforma` only with a malformed body, and `storno_invoice`'s four verify `Break` arms are untested
- **Source:** reviewer 06 F-1/F-3/F-4 (+ reviewer 04 #4) → Judge B CONFIRMED. **Severity: high. Confidence: high** — verified: `tests/service.rs:1457-1463` (`#[ignore = "needs docker"]`, `return` on missing docker); `.github/workflows/dagger.yaml` runs `dagger check` with the `rust` module whose `test` defaults are plain `cargo test` (fetched by reviewer 06 and Judge B; no docker in `rust:1.98-slim-trixie`); `grep -c` in `tests/service.rs`: `correct_invoice` 0, `create_final` 0, `set_payments` 0, `delete_proforma` 1 (the malformed-body case).
- **Location:** `crates/restate-szamlazz/tests/service.rs:1457-1505`; `src/service/create.rs:320` (`check_pins` in `correct`), `src/service/storno.rs:132-150` (four `Break` arms).
- **Problem:** The glossary's ownership claims for these handlers — `conflict{not_managed}`, `account_mismatch`, already-reversed, `not_stornoable` on verify; pins on the corrective's base — are pinned by nothing: removing `check_pins(...)?` at `create.rs:320` or `storno.rs:138`, or the `carries_order` guard at `storno.rs:132`, fails no test. Design §11 says handler decisions are tested "end to end or as pure functions"; `issue`, `verify_for_storno`, `correct`, `delete`, `status` are neither.
- **Recommendation:** (a) In CI, fail — do not `return` — when `CI` is set and docker is missing; add a docker-enabled job (or testcontainers) that runs `--ignored e2e_order_protocol`. (b) Add e2e scenarios for `create_final` (`prepayment_missing`/`_reversed`), `correct_invoice` (`base_reversed`, `not_managed`, `account_mismatch`), `delete_proforma` (`proforma_paid`, `run_once` + `outcome_unknown`), `set_payments`, and the four storno verify arms; reviewer 06's proposed scenarios are implementable as written. (c) Set `allFeatures = true` in `dagger.toml` so J39's archiver tests also run.
- **Effort:** M.

### 4. J-08-01 (+ C-A) — Fault bodies are double-encoded and the READMEs describe the inner JSON as "the body"; the caller contract's headline sentence contradicts its own fault table
- **Source:** reviewer 08 #1 → Judge C CONFIRMED (high); C-A is Judge C's own. **Severity: high. Confidence: high** — verified: `From<Fault> for TerminalError` serialises the fault to a *string* and passes it as the message (`support.rs:160-166`); the e2e harness decodes exactly that (`tests/service.rs:494-502`: `body["message"].as_str()` → `from_str`). The only unverified detail is the exact field set of the ingress envelope (`code` = HTTP status?); the double encoding itself is certain.
- **Location:** `crates/restate-szamlazz/src/service/support.rs:160-166,176-179`; `crates/restate-szamlazz/README.md:307`; `crates/restate-szamlazz-endpoint/README.md:281` and the "Every one of them means 'outcome unknown' (rule 2), never 'no document exists'" sentence vs the `invalid_input`/`unknown_account` rows ("Fix the request" / "do not retry as is"); CONTEXT *Outcome* has the same headline.
- **Problem:** A caller implementing the README reads `body.code` and gets `400`, never `"invalid_input"`; the structured fault is `JSON.parse(body.message)`. The Rust SDK 0.12 offers no other channel (`TerminalError` is `code: u16` + `message`), so this is a documentation defect — but it is the *integration* defect every caller hits first. Separately, a caller implementing the headline literally retries a 400 with fresh `Idempotency-Key`s forever.
- **Recommendation:** Document the wire shape once (`{code: <http>, message: "<fault JSON as string>"}`, with the parsing step), in the library README, the endpoint README and design §7; scope rule 2 to `outcome_unknown` / `unavailable` / `credentials_rejected` and state the two 400s as "fix the request". Consider naming the inner field `fault_code` to remove the ambiguity, and add an e2e assertion on the envelope.
- **Effort:** S.

### 5. J2 + J3 — The ≥ 90 s re-check invariant every ADR relies on is enforced by prose only, and `Szamlazz.Agent.storno` has no `initial_interval`, so a crash mid-`storno-{n}` is re-dispatched at the server's ~500 ms default while the first `xmlszamlast` may still be in flight
- **Source:** reviewer 03 #2/#3 → Judge B CONFIRMED-WITH-CORRECTION / CONFIRMED. **Severity: medium. Confidence: high** — verified: `WorkerConfig::validate` (`config.rs:98-139`) checks only `max_attempts ≠ 0`, `initial ≤ max`, `factor ≥ 1`; `handlers.rs:379-385` (`storno`: `max_attempts = 2`, no `initial_interval`) vs `:352-357` (`set_payments`: `initial_interval = "2m"`) and `:44-51` (every `Szamlazz.Order` write handler: `2m`). `szamlazz-hu-behaviour.md:128` states "both 2m, never below ~90 s"; the endpoint README `:230` lists the 500 ms on `storno` as current behaviour. Whether two concurrent `xmlszamlast` produce two `SS` depends on szamlazz.hu (unverified).
- **Location:** `crates/restate-szamlazz/src/config.rs:98-139`; `src/service/handlers.rs:379-385`; `szamlazz-agent/src/client.rs:138` (60 s timeout, not exported).
- **Problem:** ADR 0004 #41 gave `set_payments` its 2 m *because* "a retry that fires while the first send is still in flight could append twice"; the same reasoning applies to a storno send and was not applied. An operator can also set `issue.initial_delay = "5s"` in TOML and nothing refuses it (Judge B's correction: the 1 s policy lives only in the e2e's inline Rust; every shippable TOML says `2m`).
- **Recommendation:** Add `initial_interval = "2m"` to `Szamlazz.Agent.storno` and pin it in the discovery test; add a floor to `validate()` for `issue.initial_delay` (≥ client timeout + 30 s, with a documented test-only override) and export the client timeout from the agent crate so the floor is derived, not duplicated. Reconcile behaviour doc `:128` and endpoint README `:230`.
- **Effort:** S.

### 6. A1 — `exclusive_with` omits the final invoice: once the `ES` is reversed, `create_invoice` can issue a plain `SZ` (and `create_prepayment` with `reissue: true` a new `ES`) beside a live `VS` — double billing
- **Source:** Judge B's own (arising from the refutation of reviewer 03 #17). **Severity: medium** (Judge B: low–medium; promoted — the consequence is the one outcome the worker exists to prevent, the fix is one table row). **Confidence: medium** — the code path is verified; reachability depends on szamlazz.hu allowing the storno of an `ES` that has a `VS`, listed as unverified in `szamlazz-hu-behaviour.md:159`.
- **Location:** `crates/restate-szamlazz/src/service/create.rs:194-204` (`exclusive_with`: `Invoice → [Prepayment]`, `Prepayment → [Invoice]`, `Final → []`), `:416-443` (`exclusivity` refuses only a *live* document under the other id), `:446-478` (`prepayment_for_final`), `src/gateway.rs:1594-1596,1677-1682` (`is_invoice_family` excludes `SS`; `is_foreign` sees only the hint's newest document), `src/service/storno.rs:148` (the type gate admits `ES`).
- **Problem:** State `ES` live → `VS` issued → `ES` reversed (by `storno_invoice` or the UI). `create_invoice`: exclusivity finds `…:prepayment` reversed → proceeds; `lookup-invoice` → 7; the hint's newest document is the `SS` → not invoice-family → `Absent`; the *Create step* sends an `SZ`. The toggle is per kind (`behaviour.md:23`), so nothing server-side refuses. Verified consistent with Judge B's refutation of J17: the `SS` is what the hint returns, so the reviewer's *spurious* `conflict{foreign}` cannot occur — the real consequence is a *missed* refusal.
- **Recommendation:** Add `(Final, PrepaidChain)` to `exclusive_with(Invoice)` and `exclusive_with(Prepayment)`, and `(Final, OrderInvoiced)` to `Proforma` — symmetric with the table, one extra read each, and it puts the `VS` into `our_numbers`. Add "storno of a settled `ES`" and "`SZ` beside a live `VS`" to the go-live checklist.
- **Effort:** S.

### 7. J8 — An adversarial `unit_price × quantity` panics in `rust_decimal` after the *Prologue* has journaled, and the SDK runs handlers on hyper's connection task without `catch_unwind`, so one request tears down the HTTP/2 connection and every in-flight invocation on it is retried after its 2 m `initial_interval`
- **Source:** reviewer 03 #8 → Judge B CONFIRMED-WITH-CORRECTION. **Severity: medium. Confidence: medium-high** — verified: `LineItem::calculated_for_currency` uses operator `*`/`+` (`szamlazz-agent/src/item.rs:140-145,176-178`); `rust_decimal 1.43.0` (`Cargo.lock:2386-2387`) panics "Multiplication overflowed" / "Addition overflowed" in `arithmetic_impls.rs:161,232`; the worker's only path to the wire is `LineItemInput::to_line_item` (`contract/document.rs:336-348`) via `prepare → validate_document → build` (`create.rs:348-405`), which runs *after* `prologue` (`handlers.rs:62-65`) and before any read; no `panic = "abort"` profile. The SDK blast-radius claim rests on Judge B's reading of `restate-sdk-0.12.0/src/endpoint/mod.rs:285-311` and `http_server.rs:125` (not re-read here).
- **Location:** `crates/szamlazz-agent/src/item.rs:139-146,173-178`; `crates/restate-szamlazz/src/contract/document.rs:336-348`; `src/service/create.rs:371-392`.
- **Problem:** No document risk (cut create steps are query-first on re-execution), but a caller bug or a malicious body behind the gateway costs every concurrent order a delay and one of five attempts, repeated at each of the poisoned invocation's attempts, holding its own key throughout.
- **Recommendation:** Bound `unit_price`/`quantity` in `DocumentInput` validation (checked multiplication → `InputError` → `invalid_input`); add a fallible `LineItem::try_calculated_for_currency` to the agent crate (additive). A `catch_unwind` at the handler boundary is defence in depth the SDK does not offer.
- **Effort:** S.

### 8. A-01 — `ErrorCode::is_retryable()` in the published Számla Agent crate marks 1 (`Maintenance`) and 55 (`EInvoiceSigningFailed`) retryable regardless of operation, while the worker classifies exactly these codes on a create as open (*Unconfirmed*)
- **Source:** reviewer 01 #1 → Judge A CONFIRMED. **Severity: medium. Confidence: high** — verified: `error.rs:192-201` (doc: "Only retry on errors the server itself reported"; body: `matches!(Maintenance | EInvoiceSigningFailed)`); the worker's `classify_failure` maps `Maintenance | EInvoiceSigningFailed | InvoiceNotificationDeliveryFailed` to `Failure::Unknown` and re-queries before anything (`gateway.rs:1650-1655`). No workspace caller uses the flag (grep), so this is a public-API footgun, not a live bug.
- **Location:** `crates/szamlazz-agent/src/error.rs:192-201`.
- **Problem:** A library user who loops `while err.is_retryable() { resend }` on an invoice create can issue a duplicate legal document: a 1 or 55 does not say whether the document was created (55 in particular is "created, signing failed"). The doc comment's framing implies the opposite.
- **Recommendation:** Either scope the method (`is_retryable_for(Operation)` returning `false` for creates) or rename/document it as "the *query* may be retried; a create must be re-queried by external id first", pointing at the worker's classification. Doc-only is a patch release; a signature change is a minor bump.
- **Effort:** S.

### 9. B-05 / J-07-04 — The Adatkapcsolat router disables axum's body limit by default and runs a full namespace pass over the body before the `X-Szamlazzhu-Key` header is read
- **Source:** reviewer 02 #5 and 07 #4 → Judge A CONFIRMED, Judge C CONFIRMED-WITH-CORRECTION. **Severity: medium. Confidence: high** — verified: `DefaultBodyLimit::disable()` for `router()`/`router_with_resolver()` (`axum.rs:154-157`); `Bytes` extractor (`:216`); `Document::preflight` at `:247` precedes the header read at `:257`. Judge C's correction stands: `root_kind` stops at the first start tag; only `validate_element_namespaces` is a full pass (`document.rs:94-125`).
- **Location:** `crates/szamlazz-adatkapcsolat/src/axum.rs:154-157,213-216,245-259`.
- **Problem:** An unauthenticated client that finds the receiver URL can make it buffer and scan an unbounded body; the doc comment's rationale (receipt batches are unbounded; `:86-88`) justifies *offering* no limit, not *defaulting* to none. The receiver is meant to sit on the public internet.
- **Recommendation:** Answer 401 before any parse when the header is missing; run only the root-kind scan before the key check and the namespace pass after; default to a generous limit (e.g. 64 MiB) with `router_with_body_limit` for more, or require an explicit choice. Non-breaking.
- **Effort:** S.

### 10. A-13 — `eszamla`: the crate (following the vendor annotation) says `1` = paper; `docs/szamlazz-hu-behaviour.md` says the probe account had e-invoicing enabled with `<eszamla>1</eszamla>`; the worker derives a storno's `e_invoice` from this field
- **Source:** reviewer 01 #13 → Judge A CONFIRMED (contradiction), resolution UNVERIFIABLE. **Severity: medium. Confidence: medium** — the contradiction is verified: `InvoiceAppearance::Paper = 1` (`query_xml.rs:121-163`) matches `fixtures/upstream/adatkapcsolat/szamla_example.xml:32` ("0: not an invoice, 1: paper invoice, 2: e-invoice, 3: e-invoice"); `szamlazz-hu-behaviour.md:5,137` reads `1` as e-invoicing enabled. Which side is right depends on szamlazz.hu.
- **Location:** `crates/szamlazz-agent/src/ops/query_xml.rs:121-163`; `docs/szamlazz-hu-behaviour.md:5,137`; `crates/restate-szamlazz/src/gateway.rs:371-377` (`e_invoice()`: `Paper → Some(false)`); `src/service/storno.rs:49-51` (storno `eszamla` from it, falling back to `defaults.e_invoice`).
- **Problem:** Either the published enum and the worker are wrong — every storno of a real e-invoice is sent with `eszamla=false` — or the behaviour doc's premise (and its "352 may be an e-invoice rule" caveat, echoed in `ops/storno.rs:89-92`) is wrong and the storno `e_invoice = true` path has never been exercised. Judge A's reading (the probe documents were paper) is the more likely, but both outcomes require a fix.
- **Recommendation:** Settle on the go-live account (probe in §6); then fix the doc or the enum, and add an e2e/gateway fixture with a real `eszamla=2` storno. Add `eszamla` to the checklist's "record" line (it is already there — make it a row).
- **Effort:** S after the probe.

**Next in line (not top 10, but medium):** J7 (answered non-7 code on the *Create step*'s leading query is folded into `Unconfirmed::Transport` — ~39 min holding the order key and `outcome_unknown` though nothing was sent; verified at `gateway.rs:1105` vs `:838-841`), J-07-13/J-08-05 (no log line carries account id or scope — the multi-account blocker), J5 (`OrderKey` admits a single internal space, NBSP and `:` while ADR 0002 and the behaviour doc say internal whitespace is rejected — verified at `identity.rs:51-57`), B-10 (IPN strictness), A-02, J-07-01, J1, J22, J39, B-19.

---

## 3. Cross-cutting themes

| Theme | Findings | Single structural change |
|---|---|---|
| **Strictness where leniency is required at an inbound boundary.** A push or an IPN is at-most-N-times delivery from a party that does not read our error bodies; a deterministic non-200 is data loss. | B-01, B-02, J-01, B-10, B-06, A-16 (agent side, lesser), J-05-01's divergent optionality | Separate *parse* (everything optional, raw bytes retained, never fails on content) from *validate* (opt-in, `parse_strict`), and make the receivers' only non-200 answers transport-level (missing header, unreadable root). Archive raw first, judge second. |
| **Load-bearing invariants enforced by prose, not code.** | J2, J3, J5, A2, J16 (run-name sequence), J32, J-07-11, J-07-12, J11 | One "contract test" module in `restate-szamlazz` that pins, from the discovery manifest and `WorkerConfig`, every handler attribute (`initial_interval ≥ 2m` on every write handler), a policy floor, the run-name list per handler, the `OrderKey`/`ExternalId` alphabet and length bounds. |
| **The handler layer is only tested by a CI-skipped e2e.** | J36, J23, J38, J37, J41, J42, J39, B-19 (fragile via feature unification), J40, J22, J-07-02 | A CI job with a docker daemon that runs `--ignored`, fails when docker is absent, tests with `allFeatures = true`, and runs the go-live probes as an opt-in live suite against the test account. |
| **Docs overstate or contradict the contract.** | J-08-01, C-A, J20, J21/J-08-03, J-08-09, J26–J31, J34, J35, C-B, A-09, B-07, B-09, B-14, B-03, J-08-27 | One source of truth for the caller contract: generate the fault table and handler list from `TerminalCode`/discovery (or assert them in a doc-test), and delete two of the three copies. |
| **Duplicate modelling of the same XML with divergent strictness.** | J-05-01 (with Judge A's B-01 cross-observation), J-05-07, J-05-03, J-05-04, J-05-12 | Pre-1.0: a shared lenient `<szamla>` core and shared `de` helpers (the Agent's `AlapXml` is the template), with the receiver's incoming-only fields as an extension. Breaking for two published crates — schedule, do not rush. |
| **Design decisions resting on unverified szamlazz.hu behaviour.** | J1, A-13, J5, A2, A-03, A-04, A1's reachability, J3's consequence, B-01's production shape, B-10's real parameter set | Automate the go-live checklist (J22) as an `#[ignore]`d live suite, add the rows in §6, and record results in `szamlazz-hu-behaviour.md` with date and account. |
| **Observability cannot attribute in multi-account mode.** | J-07-13/J-08-05, J-07-10/J-08-17, A3, J33, J14, J-08-15, J-08-04 | One *Prologue* span carrying `account = %id`, `scope`, and a `Fault` builder that composes causes (send cause + re-query failure) instead of overwriting them; a short operator runbook keyed on `x-restate-id`. |

---

## 4. Contradictions and disagreements

**Refutations and downgrades — position of the meta-judge (code read where marked ✓):**

| Item | Reviewer said | Judge said | Meta-judge |
|---|---|---|---|
| J17 (03 #17) | `create_prepayment` names the order's own live `VS` as `conflict{foreign}` after an `ES` storno | REFUTED: the hint returns the newest document, the `SS`, which `is_invoice_family` rejects | ✓ Agree — `gateway.rs:1594-1596,1677-1682` verified. But the reviewer's *recommendation* (add `…:final` to `exclusive_with(Prepayment)`) is the right fix for the sibling of A1 (a reissued `ES` beside a live `VS`); the reviewer found the right table gap with the wrong consequence. |
| J12 (03 #12) | `get` fails on an `Api` answer "despite the comment" | DOWNGRADED to info: design §6 says exactly that; the comment is about collisions | ✓ Agree — `storno.rs:222-223` read. |
| J20 (04 #1) | Docs claim `Agent.storno` 422 / `set_payments` 404; code never produces them | DOWNGRADED to low: doc-only, code's behaviour is the sensible one | Agree; note it is the third copy of the contract that drifted (theme 4). |
| J37 (06 F-2) | No concurrent same-key test | DOWNGRADED to low: Restate's lock, pinned statically by the discovery test | Agree. |
| J4, J6 (03 #4, #6) | Unkeyed `Szamlazz.Agent`; no timeout on fetch/resolve | DOWNGRADED to low | Agree — proportionate. |
| J5 (03 #5) | high: normalisation strands a document | medium: premise unverified; confirmable doc/code mismatch | ✓ Agree — `identity.rs:51-57` rejects only *runs*; `behaviour.md:27` and ADR 0002 say internal whitespace is rejected. Doc or code must change. |
| B-03 (02 #3) | medium: docs mislead about logging | low: `handler.rs:59` and `axum.rs:349-350` say "log it yourself"; only the `Display` clause is stale | ✓ Agree — `axum.rs:347-350` read. |
| B-09 (02 #9) | high: IPN README lacks "confirm via Agent" | medium: README omission on a surface already labelled unauthenticated | Agree. |
| A-07 (01 #7) | low–medium: taxpayer envelope | low: the fixture shows codes relayed *inside* the NAV envelope; read policy bounds the retry | Agree. |
| A-03 (01 #3) | medium: silent non-HUF non-rounding is a trap | medium, corrected: documented design | Agree with the correction, with one addition: `to_line_item` is the worker's *only* path, so the "documented choice" is imposed on every worker caller without a way to opt out — keep medium. |
| J-07-01 (07 #1) | high: unsigned requests accepted | medium: SDK default, documented | Agree for single-account; for multi-account the scope is protocol data, so it is the item that gates go-live (roadmap bucket 2). |
| J-07-02 (07 #2) | high: `assert_not_impl_any!` only under `cfg(test)` | medium: dev-dep by design; CI's test build compiles it | Agree on the conclusion. Judge C's premise "CI runs `cargo test --workspace --all-targets`" is wrong — reviewer 06 and Judge B verified plain `cargo test` from the dagger module — but plain `cargo test` still compiles the lib's `cfg(test)` module, so the guard fires in CI either way. |
| J-08-09 (08 #9) | `get` reports `storno_number` only when the newest document is the storno | corrected: `get` *never* reports it | ✓ Agree — `storno.rs:271-273` hard-codes `None`. |
| J-05-15 (05 #15) | delete `MOVED` (endpoint never released) | do not delete unverified | Agree. |
| J-05-01 (05 #1) | high | medium, later | Agree; Judge A's B-01 observation (the Agent's lenient `AlapXml`) shows the target shape. |
| J-08-02/04/12/16/18/19/21/26 | medium–high (operator/persona) | low–medium | Agree — enhancements, not defects. |
| J1 (03 #1) | high | UNVERIFIABLE, medium/low, two-sided (HS-vs-HS under the toggle may also *refuse* a legitimate second corrective) | Agree — a probe, not a code change; §6 row 2. |
| J8 (03 #8) | panic "before the prologue" | after `namespace`/`account` are journaled; blast radius is the whole HTTP/2 connection | ✓ Agree — `handlers.rs:62-65` order verified. |
| J16 / 06 F-6 | "guard is presently inert" | not inert: the generator compares byte-for-byte; zero archived shapes exist | Agree. |

**Where two judges imply different conclusions:**
- *CI command line.* Judge C (J-07-02): "`cargo test --workspace --all-targets`". Judge B (J36) and reviewer 06: plain `cargo test` (dagger module defaults `allFeatures=false`, `allTargets=false`). Reviewer 06's local reproduction (`cargo test --locked -- --list`) is authoritative. Consequences: J39 (archiver tests, `opendal`) never run — correct; **B-19's "parse tests behind `axum`" needs a nuance Judge A did not add**: `tests/protocol.rs` *does* run in CI because `szamlazz-cli` enables `axum` and resolver 3 unifies features across members built together — it vanishes only in per-crate runs (`cargo test -p szamlazz-adatkapcsolat`) and in `cargo package`'s verification. Fragile, not absent; B-19 stays medium for the never-parsed vendor example, low for the gating.
- *`base64` version.* Judge A cites `base64-0.22.1`; the receiver depends on the workspace `0.23.1` (`Cargo.lock:205-206`, `adatkapcsolat/Cargo.toml:28`). Verified: `STANDARD` in 0.23.1 is `RequireCanonical` + `decode_allow_trailing_bits: false` (`general_purpose/mod.rs:341-342,436`) — J-01 holds unchanged.
- *A1 severity.* Judge B: low–medium. Meta-judge: medium (see #6) — the disagreement is about weighting an unverified premise against the worst outcome in the domain, not about facts.
- *"attempt" vocabulary.* Judge C (J-05-20) flags "this attempt issued nothing" in caller-facing text against the glossary's *Execution* Avoid list; Judge B's J14 flags the same sentence as potentially *false* after a post-send re-query. Both are right; fix once (`support.rs:127`).

---

## 5. Remaining medium/low findings (verified; top-10 entries and their merged siblings excluded)

Severity/Confidence are the judges' final ratings unless marked. "info" items are listed compactly at the end of each crate.

### szamlazz-agent (Számla Agent)
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| A-02 | `InvoiceKind` cannot carry `dijbekeroSzamlaszam` on a prepayment/final; worker drops `refs.proforma` for prepayments and relies on order-number auto-link | medium | high | `ops/invoice.rs:22-51,829-850`; `restate-szamlazz/src/gateway/build.rs:109` ✓ |
| A-03 | `calculated_for_currency` does no rounding for non-HUF (documented); the worker's only path to the wire | medium | medium | `item.rs:125-146`; `contract/document.rs:336-348` ✓ |
| J-05-02 | Blanket `#[non_exhaustive]` on 77/95 request types; removal is a non-breaking widening | medium | high | `szamlazz-agent/src/**` |
| A-04 | `VatRate::Percent` wire token keeps caller scale (`27.00`); acceptance unverified | low | medium | `types.rs:243`; `contract/document.rs:329-331` |
| A-05 | HUF net rounded before the server's `net = price × qty` check; tolerance unverified | low | low | `item.rs:137-138,173-176` |
| A-06 | Delivery note silently forces `SzlaFuvarlevelesAlap` template | low | low | `ops/invoice.rs:865-868` |
| A-07 | Taxpayer parser accepts no `xmlszamlavalasz` body (speculative) | low | low | `taxpayer.rs:182-220` |
| A-08 | 339 untyped; `TEHK` is a *response-side* `afatipus` token (`Other("TEHK")`) | low | high | `error.rs:31-32`; `query_xml.rs:398-400` |
| A-09 | Final-invoice docs omit the negative prepayment line (behaviour doc C6-2) | low | high | `ops/invoice.rs:38-44`; README:62 |
| A-10 | Empty `szlahu_error_code` treated as an error | low | low | `wire.rs:205-211` |
| A-11 | `RawResponse` carries no HTTP status | low | high | `wire.rs:141-145`; `client.rs:191-203` |
| A-12 | Zero entries + `additive=false` wipes payments; no `validate()` | low | high | `credit_entry.rs:136-144` |
| A-14 / J-07-16 | Derived `Debug` on `RawResponse` prints `Set-Cookie` (`JSESSIONID`) | low | high | `wire.rs:141`; `client.rs:191-200` |
| A-16 | Exchange-rate check stricter than the schema; `is_huf` case-sensitive | low | medium | `ops/invoice.rs:776-787`; `types.rs:379-381` |
| J-07-05 | Whole upstream bodies echoed into faults / journaled `Transport(String)` (hygiene) | low | high | `xml.rs:69-81`; `gateway.rs:1251,1537,1471,1510` |
| J-05-11 | `WireRequest.url`/`session_cookie` leak transport into the sans-IO layer | low | high | `wire.rs:22,36,54,313`; `client.rs:181` |
| J-05-12 / J-05-13 | Envelope parsing repeated per op; `ops/invoice.rs` 2,000 lines | low | — | `ops/*` |
| J40 | Upstream fixture corpus (60 files) referenced by zero tests (licence-excluded from the package; needs a runtime-gated test) | low | medium | `tests/upstream` symlinks; `Cargo.toml:12` |
| J-08-18/19/20 | README quick start minimal; no sans-IO example; examples not compile-tested | low | high | `README.md` |
| (Judge A note) | `AlapXml.teszt` defaults to `false` = live when absent/empty | low | high | `query_xml.rs:777-778`; `xml.rs:184-195` |
| info | A-15 (`hibakod` fabricated `"0"`, `ops/invoice.rs:1268-1297`); A-18 (`+`→space in header decode, `wire.rs:250-257`); J-05-22 (hand-rolled newtype impls) | | | |

### szamlazz-ipn (IPN)
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| B-09 | README lacks "confirm `paid_gross` via the Számla Agent before acting"; `is_fully_paid()` invites acting on the body | medium | high | `lib.rs:17-19,151-157`; README:39 |
| B-10 | Hard failure on non-`%Y-%m-%d` `szlahu_kifizdat`, empty amount, missing `szlahu_fizetesmod` — a deterministic 400 loses the notification after ten deliveries (the file's own rationale at `:240-244`) | medium | medium | `lib.rs:201-222` ✓ |
| B-11 | Retries interleave; payload has no sequence/timestamp | low | high | `lib.rs:79-90,95-123` |
| B-12 / J-07-15 | `SOURCE_IPS` proxy pitfall; list dated 2025-08-01; mitigation guidance thin | low | medium | `lib.rs:68-77`; README:39 |

### szamlazz-adatkapcsolat (Adatkapcsolat receiver)
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| B-19 | Vendor example never parsed by a test; protocol tests run in CI only via the CLI's `axum` feature (fragile); `include_bytes!` would break `cargo package` — must be runtime-gated | medium | high | `tests/protocol.rs:4`; `Cargo.toml:12` |
| J39 | Archiver tests never run in CI (`opendal` enabled by no member) | medium | high | `tests/archive.rs:3` ✓ |
| J-05-01 | `<szamla>` modelled twice, divergently (`id: u64` vs `i32`, `document_type: String` vs `kind: Option`, …); `de` helpers copied | medium | high | `query_xml.rs:187-268` vs `document.rs:450-554`; `xml.rs:169-199` vs `document.rs:1495-1592` |
| B-03 | Handler/Ack errors swallowed to bare 500; stale "`Display` bound" clause; no raw-body hook for rejected pushes | low | high | `axum.rs:288-317`; `handler.rs:57-62` ✓ |
| B-06 | Empty configured key matches an empty header (clap passes an empty env verbatim) | low | high | `axum.rs:89-101,332-345`; `cli/listen.rs:30` |
| B-07 (+B-20 half) | Wrong key → `KEY_ERR` halts bank/receipt streams permanently; README/CLI do not warn | low | high | `axum.rs:74-76,260-275`; README:46 |
| B-08 | `with_registration_number` infallible; `to_xml` fails later → 500 loop | low | high | `ack.rs:70-73,118-121`; `axum.rs:312-317` |
| B-14 | `Deserialize` is wire-shape only; README implies JSON round-trip | low | high | README:35; `archive.rs:391-399` |
| B-15 | "Required" has two meanings (text vs typed) | low | high | quick-xml semantics |
| B-16 | Fan-out: clone per member, `KEY_DEL` wins, first `iktatoszam` wins | low | high | `fanout.rs:192-266` |
| B-17 | Archiver serialises the PDF to base64 then strips it | low | high | `archive.rs:391-399`; `document.rs:309-314` |
| J-05-09 | Four `router*` constructors over one `build_router` | low | high | `axum.rs:89-136` |
| J-05-10 | `Fanout`/`Archiver`/`raw_xml` in a protocol crate | low | — | `fanout.rs`, `archive.rs` |
| J-07-14 | Parse errors echoed in 400 bodies | low | high | `axum.rs:249,279` |
| (Judge A note) | `InvoiceInfo.id: i32` (XSD-exact) vs Agent `u64`; observed ids ~0.9 × 10⁹ | low | high | `document.rs:455` |
| info | B-13 (per-element namespace check), B-18 (401 without `WWW-Authenticate`), B-21 (`Handler` cannot grow non-breaking), B-22 (constant-time compare best-effort), J-05-16 | | | |

### szamlazz-cli
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| A-17 | `invoice storno` skips `CreatedInvoice::reverses()` | low | high | `commands/invoice.rs:209-222` |
| B-20 | `listen` dumps base64 PDFs to stdout | low | high | `listen.rs:44-48` |
| J-08-21/22/23 | Hand-computed totals required; `--method` Hungarian tokens with silent passthrough; `taxpayer` accepts only the 8-digit stem | low | high | `invoice.rs:136`; `payment.rs:31-33`; `taxpayer.rs:12,19` |
| J-08-25 | `listen` defaults to `:8080` = compose ingress port | low | high | `listen.rs:23`; `compose.yaml:8` |
| info | J-07-17 (secrets as `String` with derived `Debug`, `main.rs:22`, `listen.rs:31`); J-08-24 (uniform exit codes) | | | |

### restate-szamlazz (worker)
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| J7 | Answered non-7 code (or `szlahu_down`) on the *Create step*'s leading query → `Unconfirmed::Transport`; issue policy re-executes (~39 min holding the key), ends `outcome_unknown` though nothing was sent; the *Lookup step* answers the same code as `unavailable` at once | medium | high | `gateway.rs:1105` vs `:838-841` ✓ |
| J-07-13 / J-08-05 | No span or log line carries account id or scope; the paging `credentials_rejected` warn names namespace + code only | medium | high | `gateway.rs:813-818,914-921`; `support.rs:119-123` ✓ |
| J5 | `OrderKey` admits a single internal space, NBSP, `:`, non-NFC; ADR 0002 and behaviour doc say internal whitespace is rejected; a server normalisation strands the document behind `external_id_collision` | medium | medium | `identity.rs:40-59` ✓; `gateway.rs:383-385,1124-1126` |
| J1 | Correctives rely solely on HS external-id queryability, never verified; HS-vs-HS under the toggle two-sided | medium | low | `gateway.rs:847,1014-1017`; `behaviour.md:24,149` |
| J22 | Go-live checklist (8 probes) not automated; every "verified" fact is one account, one day | medium | high | `behaviour.md:203-212`; `agent/tests/live.rs` |
| J-07-02 | Journal leak scan only in the ignored e2e; a `String` populated from `expose()` inside a journaled outcome is the unguarded case | medium | medium | `tests/service.rs:1458`; `account.rs:435-436` |
| J-08-04 | No operator runbook (find the invocation, read the journal, which step) | medium | high | READMEs |
| J-08-06 | `customer_account_url` only on a fresh `issued`; never on `already_issued`/`get` | medium | high | `create.rs:70-78,115` |
| J-08-07 | Conflict reasons have no "what to do" column | medium | high | lib README:157-162 |
| J-08-08 | Agent-key rotation undocumented; static resolver = restart | medium | high | `endpoint/main.rs:68`; README:325-331 |
| J-08-09 | `get` omits correctives and never populates `storno_number`; README `:186` misleading | medium | high | `storno.rs:267-274` ✓; `response.rs:608-619` |
| J-08-27 | Caller contract / fault tables triplicated and drifting | medium | high | design:441; lib README:282-343; endpoint README:275-294 |
| A2 | Composed external id unbounded (≤ 157 bytes; 110 verified) | low–med | medium | `identity.rs:31,157-180`; `contract.rs:53` |
| J4 | `Szamlazz.Agent` is unkeyed; two by-number writes race | low | high | `handlers.rs:264` |
| J6 | No timeout around credential fetch / resolver call | low | high | `prologue.rs:113-134`; `support.rs:407-416` |
| J9 | `Szamlazz.Agent.storno` sends for `D`/`SL`/`SS` and relies on the echo | low | high | `agent.rs:285-296` |
| J10 | Read handlers keep the 1 m inactivity default with 60 s reads (`get` = 4 reads) | low | medium | `handlers.rs:248-328` |
| J11 | `:` in `OrderKey`; correction id may equal a kind token → spurious `external_id_collision` | low | high | `identity.rs:40-59,157-166` |
| J13 | `storno_number_of` swallows a cancellation (409) and answers `reversed` | low | high | `support.rs:656-667` |
| J14 / J-05-20 | "this attempt issued nothing" can be false after a post-send re-query; "attempt" is a glossary Avoid word | low | high | `support.rs:110,127` ✓ |
| J15 / J-07-09 | `Endpoint::parse` accepts userinfo and plain `http`; journaled and logged | low | high | `account.rs:180-190,247-248` |
| J16 | Journal fixtures pin type layouts, not the run-name sequence or SDK envelope | low | high | `tests/journal/**` |
| J20 | Docs claim `Agent.storno` 422 and `set_payments` 404; code never produces either | low | high | `agent.rs:252-261,187-224`; README lib:309-310; design §7:360-361; ADR 0006:354-356; CONTEXT *Outcome* |
| J21 / J-08-03 | Endpoint README says four `Szamlazz.Agent` handlers; there are five | low | high | endpoint README:122,240 |
| J24 | Wire-contract violations surface as undocumented code `request` | low | high | `gateway.rs:1484-1509,1666-1669` |
| J25 | `Agent.storno` short-circuits an already-reversed unmanaged document without the storno number | low | high | `agent.rs:285-287` |
| J26–J31, C-B | Stale ADR consequences; "issue and resolve policies" omits read; design §7 key-check order; `set_payments` named as the only pin exemption; 10 s → 1 m back-off partially documented; README gateway lists omit `query_taxpayer` | low | high | ADR 0001:55-57; ADR 0006:297-298; `lib.rs:16`; design §7:405-408; `contract.rs:310`; `service.rs:94-97`; README:216-219 |
| J32 | `(endpoint, agent_key)` uniqueness compares URL text (secondary guard only) | low | high | `static_resolver.rs:396-402` |
| J33 | `Unconfirmed::Open` prints `szlahu_down` for "create succeeded without a number" | low | high | `gateway.rs:274,935-940` |
| J37, J41, J42, J43 | No concurrent same-key test; journal scan never sees `set_payments`/`Transport(String)`; `scope: null` canary never provoked; e2e `watch` window + fixed ports | low | high/med | `tests/service.rs` |
| J-05-03/04/05 | Three field-identical policy configs; `CredentialsRejected{code,message}` in 12 enums; `support.rs` grab-bag with `journal_helpers!` ×3 | low | high | `config.rs:493-654`; `gateway.rs`; `support.rs:346-690` |
| J-05-14 | `tests/service.rs` 4,367 lines, one test fn | low | high | `tests/service.rs:1458` |
| J-05-19 / J-08-10 | `TerminalCode` misses 404 `not_found` and the 422 pass-through (numeric code in `code`) | low | high | `support.rs:176-179`; `agent.rs:131,139,172,212,253` |
| J-07-10 / J-08-17 | `credential_ref` and account id reach the caller's `unavailable` fault, contradicting "no response names the account" | low | high | `prologue.rs:139-148`; `account.rs:356` |
| J-07-11 | Fan-in via two keys of one account passes the load-time check (acknowledged in ADR 0006) | low | medium | `static_resolver.rs:376-402` |
| J-07-12 | `invoice_number` unbounded into step names and external ids | low | high | `contract/request.rs:92,106,225`; `support.rs:599,629,656` |
| J-08-11 | `invalid_input (not_found)` on `Order` vs 404 `not_found` on `Agent` | low | high | `create.rs:310-314`; `agent.rs:131-135` |
| J-08-12/13/14/15/16/26/28–32 | NAV tax fields unexplained; `vat_rate` free string; `Idempotency-Key` guidance scattered; `Debug` formatting in logs/`account_mismatch`; checklist references absent records; no per-persona start-here; `reconciled` vs `already_issued`; two kind vocabularies; `set_payments` vs glossary; no ADR index; dates/payment rules undocumented | low | high/med | various |
| J-05-07/08 | `SellerConfig` duplicates `Seller`; contract mirror enums | low | — | `build.rs:167`; `contract/*` |
| info | J34 (nine step names absent from the design), J35 (7 glossary imprecisions; `gone` is terminal at once, `prologue.rs:131`), J44, A3 (post-send re-query hides the original open cause, `gateway.rs:973-982`), J-05-06, J-05-16/17/18/21, J-07-18/20 | | | |

### restate-szamlazz-endpoint / CI / container
| ID | Title | Sev | Conf | Location |
|---|---|---|---|---|
| J-07-01 | Unsigned Restate requests accepted when `identity_keys` is empty; no start-up `warn!`; binds `0.0.0.0:9080`; scope travels as protocol data | medium | high | `main.rs:41,45,184-195` ✓; README:331 |
| J-07-03 | `dagger.yaml` and `release.yml` actions tag-pinned; excluded from dependabot; `contents: write` | medium | high | `dagger.yaml:8,10`; `release.yml:17-18` |
| J-07-06 | `/health` behind identity verification; server is HTTP/2-only, so `httpGet` probes fail regardless; no probe docs | low | high | `endpoint/mod.rs:242-250`; README:216 |
| J-07-07 / J-07-08 | Debian-slim runtime; no file-based secret source | low | high | `Dockerfile:28`; `config/sources.rs` |
| J-05-15 | Hand-maintained schema tree; do **not** delete `MOVED` unverified | low | medium | `schema.rs:118-127,246` |
| info | J-07-19 (no provenance/attestation), J-07-21 (`--check-config` tests lack key-absence assertion), J-07-22 (`cargo-deny` without `deny.toml`) | | | |

---

## 6. UNVERIFIABLE items — live probes for the go-live checklist

Each row: the claim the repo cannot settle → the exact probe on the **test** account (then repeat the safety-relevant rows on the live account before the first real order). Record request/response pairs in `docs/szamlazz-hu-behaviour.md` with date and `szallito/id`.

| # | Feeds | Claim | Probe |
|---|---|---|---|
| 1 | A-13 | `<eszamla>` 1 = paper (vendor annotation) vs 1 = e-invoice (behaviour doc) | Create one `SZ` with `<eszamla>true</eszamla>` and one with `false`; query both by number; record `<eszamla>`. Then storno each with `eszamla` set to match and to mismatch; record codes (352?) and the `SS`'s `<eszamla>`. |
| 2 | J1 | An `HS` created with `szamlaKulsoAzon` is returned by `xmlszamlaxml` for that id; HS-vs-HS under the toggle | Create `SZ` → `HS` with external id `probe:hs:1`; query by external id at +0/+2/+60 s. Then a second `HS` with different content and a new external id under the same order: 152 or a second `HS`? |
| 3 | A1 | szamlazz.hu allows the storno of an `ES` that has a `VS`; nothing refuses an `SZ` beside a live `VS` | `ES` → `VS` → storno `ES` (expect either `SS` or a 221-like refusal); if reversed, create an `SZ` under the same order via the CLI (not the worker). |
| 4 | J5 | Internal whitespace / NBSP / NFC in `rendelesszam` are stored verbatim (not normalised or rejected) | Create with `"ORD 1"`, `"ORD\u00A01"`, an NFD `"ORD-é"`; query by the exact bytes and by the normalised forms; compare `rendelesszam` in the body. |
| 5 | A2 | `szamlaKulsoAzon` length limit; truncates or rejects | Create with 120-, 160- and 200-char external ids; query back by the full id; record code or truncated value. |
| 6 | J3 | Two `xmlszamlast` for one invoice in flight concurrently produce one `SS` | Fire two stornos of the same number within 100 ms from two clients; count `SS` documents under the order. |
| 7 | B-01, B-02, J-01 | What production Adatkapcsolat pushes omit; whether `<pdf>` base64 is always canonical; whether `TransactionDirection` has values beyond `BE`/`KI` | Register a raw-archiving receiver (store every body before parsing) for a week on the test account; run the corpus through `Document::parse` and `Document::parse_strict`; diff. |
| 8 | B-10 | Real IPN parameter set: date format of `szlahu_kifizdat`, decimal separator, empty amounts, presence of `szlahu_fizetesmod` | Capture raw IPN bodies for a paid proforma, a partially paid invoice, a payment removal; feed to `IpnMessage::parse`. |
| 9 | A-03, A-05 | Non-HUF sub-minor-unit nets (`100.005 EUR`) are accepted/rounded/rejected; HUF rounding tolerance of the 259–261 checks | One EUR line with a 3-decimal net; one HUF line whose rounded net differs from `price × qty` by 0.5. |
| 10 | A-04 | `afakulcs` accepts `27.00` / `27.0` | Create with `27.00`; expect success or the code. |
| 11 | A-07 | `xmltaxpayer` with a wrong agent key answers in header form, body form, or both | Send with a wrong key; dump headers and body. |
| 12 | A-08, A-19, A-20 | `TEHK`/`TAHK`/`TÉTELÁFA` as *request* `afakulcs` tokens; `sendEmail` default when omitted | Create with each token; create with `sendEmail` omitted and observe whether mail is sent. |
| 13 | A-06 | Delivery note requires `SzlaFuvarlevelesAlap` | Create an `SL` with the default template. |
| 14 | J-08-01 | Exact ingress error envelope (`code` = HTTP status? `restate_code`? other fields) | POST a malformed body to `create_invoice` on a real Restate server; dump the raw body. |
| 15 | J42 | `check_account` reports `scope: null` when `protocol_v7` is off | Run the e2e (or one `check_account`) against a server started without the experimental flags. |
| 16 | J18 | Run retries do not consume the invocation budget (`restate-server` invoker) | Read `restate-server v1.7.8` invoker source, or observe `retry_count` across a run-retried step in the e2e (already asserted `= 1`). |
| 17 | behaviour doc | Credential codes 3/135/136/164 are answered before any write, header + body form | Create with a wrong agent key; confirm no document; record form. |
| 18 | B-12 | `SOURCE_IPS` list currency (dated 2025-08-01) | Compare with the szamlazz.hu docs page; record the date. |
| 19 | B-23 | `wasm32-unknown-unknown` builds for `szamlazz-adatkapcsolat`/`ipn` | `cargo check --target wasm32-unknown-unknown -p …` in CI. |
| 20 | J-05-15 | The endpoint was never released (premise for deleting `MOVED`) | Check crates.io and the `v*` tag history before acting. |
| 21 | (Judge A note) | Every queried document carries `<teszt>` (`minOccurs=1`); an absent `teszt` would pass as live | Inspect archived bodies from row 7 for a missing `<teszt>`. |

---

## 7. Prioritized roadmap

**Before next release (blocking).** All S unless marked; the worker items are ADR-consistent one-liners; the receiver items are the published-crate churn worth paying now.
- J3 `initial_interval = "2m"` on `Szamlazz.Agent.storno` + discovery-test pin; J2 floor in `WorkerConfig::validate`.
- A1 `Final` rows in `exclusive_with` (Invoice, Prepayment, Proforma).
- J8 checked arithmetic → `invalid_input`; J7 map an answered code on the leading query to a settled outcome (or `unavailable` at once), keep `Unconfirmed` for unanswered only.
- J-08-01 + C-A wire-shape and rule-2 docs; J20, J21/J-08-03, J-08-09, C-B doc drift (S, same pass).
- J36 (a): fail on missing docker under `CI`; add the docker-enabled CI job (M). J39: `allFeatures = true` in `dagger.toml`.
- B-04 fallible `KeyResolver` (→ `szamlazz-adatkapcsolat` 0.4.0); B-05 401-before-parse, root-only pre-scan, default limit; J-01 lenient PDF decode; B-02 `#[serde(other)]`. Ship together as one breaking release.
- A-01 doc fix now (patch); signature change with the next agent minor.
- J-07-03 SHA-pin the two workflows; add them to dependabot.
- J14/J-05-20 fix the "attempt" sentence once.

**Before 1.0 / before multi-account go-live.**
- B-01 lenient parse + `parse_strict` (M; same 0.4.0 if the schedule allows, else 0.5.0).
- J-07-13/J-08-05 account id + scope on the *Prologue* span and the `credentials_rejected` warn (S). J-07-01 start-up `warn!` when no identity keys + deploy-checklist line (S). J-07-10/J-08-17 strip `credential_ref`/account id from the `unavailable` fault text (S).
- J36 (b) / J23 / J38 e2e scenarios for `create_final`, `correct_invoice`, `delete_proforma`, `set_payments`, the storno verify arms; J42 provoke the `scope: null` canary once (M).
- J22 automate the go-live checklist as an `#[ignore]`d live suite with the §6 rows added (M); settle A-13, J1, J5, A2, A1-reachability and fix accordingly (S each after the probe).
- J5 tighten `OrderKey` to the verified alphabet or fix the docs; J11 forbid `:`; A2 bound the composed id; J-07-12 bound `invoice_number` (S).
- J16 pin the run-name sequence per handler (S). J-07-02 sentinel-serialisation unit test (S).
- A-02 `dijbekeroSzamlaszam` on `Prepayment`/`Final` (M; additive if `InvoiceKind` is `#[non_exhaustive]`, otherwise breaking for the agent crate).
- B-10 lenient IPN parse (S; `payment_method: Option` is breaking → minor). B-09 README trust-model paragraph (S).
- J-05-02 drop `#[non_exhaustive]` from request structs (S, non-breaking widening). J-05-19/J-08-10 add `not_found`/pass-through to `TerminalCode` (S).
- J-08-04 operator runbook; J-08-08 rotation procedure; J-08-06/07 response-field and conflict-reason guidance; J-08-27 collapse the three contract copies (S–M).
- B-19 runtime-gated vendor-example test (S); J40 likewise for the agent corpus.
- A-12 `validate()` on credit entries; A-14 redacted `Debug` on `RawResponse` (S).

**Later / nice to have.**
- J-05-01 shared `<szamla>` core + `de` helpers (L; breaks two published crates — bundle with the 1.0 API freeze).
- J-05-03/04/05, J-05-09, J-05-11/12/13, J-05-14 refactors; J-05-07/08.
- J4 (a `Szamlazz.Document` object is over-engineering — document the limitation instead); J6 trait-boundary timeouts; J10 read-handler timeouts; J13, J25, J33, A3 observability polish.
- J-07-06 probe docs; J-07-07/08/19/22 container and supply-chain hardening; J-07-05 truncate echoed bodies.
- B-03 raw-body hook; B-06/B-07/B-08, B-11, B-14, B-16, B-17, B-20; A-17, J-08-21/22/23/25 CLI.
- A-04/05/06/09/10/11/16 agent polish after the §6 probes.

**Churn honesty.** The Adatkapcsolat crate takes two behaviour-changing releases (B-04 trait, B-01 semantics) — better one. The agent crate's A-01 and A-02 are the only breaking candidates; both can be staged as additive first. The worker is unpublished as a service contract but its *Journaled types* are additive-only by rule — none of the roadmap items above touches a journaled layout except A1 (no new type) and J7 (a new `CreateOutcome` variant is additive).

---

## 8. Process assessment

Factual accuracy on code claims was ≈ 95–100 % across all eight reports (zero outright refutations in A and C, one in B), which says as much about the codebase's legibility as about the reviewers. The errors were in *consequences* (03 #17's interleaving, 03 #8's "before the prologue", 07 #6's mechanism) and in *severity*: 22 of 163 adjudicated findings were downgraded, none upgraded except one merge, concentrated in the operator-persona items of 08, the two "high" architecture items of 05 and the unverified-premise items of 03. Most reliable: 04 (spec coverage — every DEVIATES row correct), 01 (agent protocol — XSD sequence claims verified), 06 (test coverage — the only reviewer who fetched the CI module and reproduced the feature-unification table locally, which later corrected a judge). Least reliable in calibration: 08 (five mediums and one high downgraded; one recommended replacement text itself wrong), 07 (two half-right mechanisms), 03 (two premises unverifiable rated high). Among judges, B was the most rigorous (constructed a refutation from the behaviour doc, fetched SDK and dagger sources, produced the most consequential new finding A1); A was rigorous on library sources but cited `base64-0.22.1` where the crate uses 0.23.1 (same defaults, so harmless); C repeated a CI command line from the README (`--all-targets`) rather than from the dagger module, without effect on its conclusion. Systematic blind spots shared by the reviewers, filled only by the judges: nobody compared the two `szamla.xsd` parsers' strictness side by side (Judge A), nobody followed the exclusivity table gap to its *permissive* consequence (Judge B), and nobody noticed the base64 trap (Judge A) — a pattern of reviewers stopping at the first plausible consequence of a correct observation.
