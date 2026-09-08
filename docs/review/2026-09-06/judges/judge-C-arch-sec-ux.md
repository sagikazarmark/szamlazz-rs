# Judge C, architecture / security-ops / UX-docs

Adjudicating reviewer reports 05 (architecture), 07 (security & ops) and 08 (UX / API / docs) for
`/home/laborant/szamlazz-rs2` at v0.3.0. Every high/medium finding was re-read at the cited
file:line; low/info findings were spot-checked (32 of 49) and the rest passed through. No cargo
commands were run; no repo files were modified. SDK facts were checked in the vendored sources
(`restate-sdk-0.12.0`, `restate-sdk-shared-core-7.0.3`).

## (a) Summary

**Verdict counts (76 findings: 22 + 22 + 32)**

| Verdict | Count |
|---|---|
| CONFIRMED | 36 |
| CONFIRMED-WITH-CORRECTION | 8 |
| DOWNGRADED | 14 |
| UPGRADED | 1 |
| REFUTED | 0 |
| UNVERIFIABLE (standalone) | 0 (two sub-claims flagged unverifiable inside corrections) |
| PASS-THROUGH, not independently verified (low/info) | 17 |

No finding was outright refuted: the three reviewers' factual claims held up nearly every time. The
recurring problem is **severity inflation** (14 downgrades, concentrated in 05's two "high"
architecture items and 08's operator-persona items), and a handful of half-right mechanisms
(a `cfg(test)` guard called weak when CI runs it; a "double tokenization" that is one full pass plus
one partial; an HTTP-probe trap blamed on identity keys when the endpoint is HTTP/2-only anyway).

**Five most important confirmed findings in this domain**

1. **J-08-01 (HIGH, confirmed).** Fault bodies are double-encoded: `From<Fault> for TerminalError`
   serialises the `{code, message, order?, kind?, external_id?}` JSON to a *string* and passes it as
   the `TerminalError` message (`crates/restate-szamlazz/src/service/support.rs:160-166`); the
   ingress wraps that string in its own `{code: <http status>, message}` envelope, which is exactly
   how the e2e harness decodes it (`tests/service.rs:494-502`). Both READMEs and design §7 present
   the inner JSON as "the body" (`crates/restate-szamlazz/README.md:307`,
   `crates/restate-szamlazz-endpoint/README.md:281`). This is the only channel the Rust SDK 0.12
   offers (`TerminalError` is `code: u16` + `message: String`; `TerminalFailure.metadata` is not
   exposed; `restate-sdk-0.12.0/src/errors.rs:110-150,179-187`), so the fix is documentation, not
   code. A caller reading `body.code` per the README gets `400`, never `"invalid_input"`.
2. **J-07-01 (MEDIUM, downgraded from high).** The endpoint accepts unsigned Restate-protocol
   requests when `identity_keys` is empty, logs nothing about it, and binds `0.0.0.0:9080` by default
   (`crates/restate-szamlazz-endpoint/src/main.rs:41,45,184-195`). Because the scope travels inside
   the protocol's StartMessage (`restate-sdk-shared-core-7.0.3/src/lib.rs:72-73`), anyone who can
   reach the port can invoke any handler under any scope. Documented (`README.md:331`) and the SDK's
   own default, so medium, but a start-up `warn!` and a line in the deploy checklist are cheap.
3. **J-07-04 (MEDIUM, confirmed with correction).** The Adatkapcsolat `router()` /
   `router_with_resolver()` disable axum's body limit (`szamlazz-adatkapcsolat/src/axum.rs:154-157`)
   and run `Document::preflight` (one *partial* pass to the root plus one *full* namespace pass)
   before the `X-Szamlazzhu-Key` header is read (`axum.rs:247-259`, `document.rs:54-60,94-125,156-169`).
   An unauthenticated client can make a public receiver buffer and scan an unbounded body.
4. **J-07-13/J-08-05 (MEDIUM, merged & upgraded).** No log line or span carries the account id or
   scope: gateway spans carry `external_id`/`kind`/`number` only (`gateway.rs:813-818,914-921`), and
   the paging `credentials_rejected` warning carries `namespace` + `code`
   (`service/support.rs:119-123`). In a multi-account deployment the one log that says "fix the
   account's agent key" cannot say whose.
5. **J-05-01 (MEDIUM, downgraded from high; churn: later).** The `<szamla>` document is modelled twice
   with divergent types (`szamlazz-agent/src/ops/query_xml.rs:187-268` vs
   `szamlazz-adatkapcsolat/src/document.rs:450-554`: `id: u64` vs `i32`, `document_type: String` vs
   `kind: Option<String>`, `cash_payment: bool` vs `cash: Option<bool>`, `InvoiceNumber` vs `String`)
   and the lenient serde helpers are copied (`agent/src/xml.rs:169-199` vs `document.rs:1495-1592`).
   Real, but the two are siblings, not twins (the receiver side carries incoming-invoice-only fields
   and looser optionality), and both crates are released, unify before 1.0, not now.

Runners-up: J-08-04 (no operator runbook), J-08-08 (agent-key rotation is a restart with the static
resolver; undocumented), J-05-02 (77 `#[non_exhaustive]` on request types, a non-breaking widening
worth doing before 1.0).

**Most consequential corrections (no outright refutations)**

- **07#2** claims the `assert_not_impl_any!` guard "only fires under `cargo test`, not `cargo build`"
  as a weakness. True, but `static_assertions` is a dev-dependency by design
  (`crates/restate-szamlazz/Cargo.toml:36`) and CI's `cargo test --workspace --all-targets` compiles
  the test target, so the guard does fail CI on a regression. The e2e scan being `#[ignore]`d is
  real; the guard's weakness is overstated. Downgraded high → medium.
- **07#6** blames identity keys for a failing `httpGet /health` probe. The endpoint speaks HTTP/2 only
  (`restate-sdk-0.12.0/src/http_server.rs:120`; endpoint README:216), so a kubelet HTTP/1.1 probe
  fails with or without keys. The doc gap is real; the mechanism cited is the lesser one.
- **08#2** "not a single example response body": the endpoint README has one (`check_account`,
  line 166) and design §7 lists every `CreateResponse` field schematically. None for
  create/storno/get/query, which is the point; severity high → medium.
- **08#9**'s own recommended wording ("reports `storno_number` only when the newest document is the
  storno") is wrong: `get` never fills it (`service/storno.rs:267-274` hard-codes
  `storno_number: None`).
- **05#15** recommends deleting the `MOVED` pre-release-layout refusal because the endpoint "has never
  been released"; the README documents `cargo install` and a `ghcr.io` image on every `v*` tag, so
  that premise is unverifiable offline and the deletion is not safe to recommend as stated.

## (b) Verdict table

Severity scale: critical / high / medium / low / info. "Churn" only for architecture (05) items:
**now** / **later** / **no**. Merges are noted in the title.

| ID | Source | Title | Reviewer sev/conf | VERDICT | Judge sev/conf | Churn | Justification (file:line) |
|---|---|---|---|---|---|---|---|
| J-05-01 | 05#1 | `<szamla>` modelled twice, divergently | high/high | DOWNGRADED | medium/high | later (pre-1.0) | Same root+ns (`query_xml.rs:599`, `document.rs:147`); divergent `InvoiceInfo` (`query_xml.rs:189-222` vs `document.rs:455-554`); helpers duplicated (`xml.rs:169-199` vs `document.rs:1495-1592`). Receiver side has incoming-only fields (`folyamatostelj`, `elszDatTol/Ig`) and looser optionality, so a shared type is a lossy superset: honest cost is high, breaking two released crates. |
| J-05-02 | 05#2 | Blanket `#[non_exhaustive]` on request structs | high/high | DOWNGRADED | medium/high | now (non-breaking widening) | Counted 77 attrs / 95 pub types in `szamlazz-agent/src` (reviewer: 96). `gateway/build.rs:128-167` is exactly `new()` + 8 header + 6 create assignments; CLI deserialises from JSON (`szamlazz-cli/src/commands/invoice.rs:136`). Ergonomics tax, mitigated by `new()` + pub fields; removing the attr widens, never breaks. |
| J-05-03 | 05#3 | Three field-identical policy config types | medium/high | DOWNGRADED | low/high | later/optional | `IssueConfig`/`ReadConfig` identical incl. `run_retry_policy` bodies (`config.rs:493-536` vs `560-600`); `ResolveConfig` minus `max_attempts` (`618-654`). Per-type docs carry domain meaning; nothing dispatches on the type. Cosmetic. |
| J-05-04 | 05#4 | `CredentialsRejected{code,message}` in 11 enums | medium/high | DOWNGRADED | low/high | now for helper (b); later for (a) | 12 variant decls in `gateway.rs` (146…714 public + 1622 internal), 52 occurrences, 20 `Fault::credentials_rejected` calls in `service/`. `storno_response` returns `Result<_, (String, String)>` (`support.rs:264-289`). Structural, not a bug. |
| J-05-05 | 05#5 | `support.rs` grab-bag; `journal_helpers!` ×3 | medium/high-med | CONFIRMED-WITH-CORRECTION | low/high | split now; macro later | Six concerns in 690 lines; macro at `support.rs:346-686`, stamped at `688-690`. rust-lang/rust#100013 rationale (`support.rs:343-345`) not verifiable without compiling, keep as stated workaround. Organisation only. |
| J-05-06 | 05#6 | Account-shaped types live in `config.rs` | low/high | CONFIRMED-WITH-CORRECTION | info/high | no / later | Placement is deliberate and documented (`config.rs:36-42`, README Key Types, glossary *Namespace*). Sub-claim "nothing external imports them" is wrong: endpoint tests import `AccountMode` (`endpoint/src/config.rs:187`, `config/schema.rs:325`). |
| J-05-07 | 05#7 | `SellerConfig` duplicates agent `Seller` | low/medium | PASS-THROUGH | low/, | later | `to_seller()` used at `build.rs:167`; rationale not independently assessed. |
| J-05-08 | 05#8 | Contract mirror enums (`Selector`, `PaymentMethod`, `TaxpayerStatus`) | low/high | PASS-THROUGH | low/: | no | Reviewer's own verdict (keep; guard catch-all) is sensible; not independently verified. |
| J-05-09 | 05#9 | Four `router*` constructors in adatkapcsolat | low/high | CONFIRMED | low/high | later | 2×2 matrix at `axum.rs:89-136`, all over `build_router`. |
| J-05-10 | 05#10 | `Fanout`/`Archiver`/`raw_xml` in protocol crate | low/high-med | PASS-THROUGH | low/, | no / later | Not independently verified. |
| J-05-11 | 05#11 | `WireRequest.url`/`session_cookie` leak transport into sans-IO | low/high | CONFIRMED | low/high | later | `wire.rs:22,36,54,313`; `client.rs:181` overwrites `url`. |
| J-05-12 | 05#12 | Envelope parsing repeated per op | low/high | PASS-THROUGH | low/, | later | Not independently verified. |
| J-05-13 | 05#13 | `ops/invoice.rs` 2,000 lines w/ waybill types | low/high | PASS-THROUGH | low/, | now (cheap) | Not independently verified. |
| J-05-14 | 05#14 | `tests/service.rs` 4,367 lines, one test | low/high | CONFIRMED | low/high | now | 4,367 lines; single `#[ignore = "needs docker"]` test at `:1458`. File split without changing run order is sound. |
| J-05-15 | 05#15 | Hand-maintained schema tree; delete `MOVED` | low/medium | CONFIRMED-WITH-CORRECTION | low/medium | later; do **not** delete `MOVED` unverified | Tree + `MOVED` exist (`schema.rs:118-127,246`). Premise "crate never released" is unverifiable offline and contradicted by README install/image instructions; ADR 0006:399 keeps the refusal on purpose. |
| J-05-16 | 05#16 | `Body<T>` workaround costs | info/high | PASS-THROUGH | info/, | no | - |
| J-05-17 | 05#17 | Journal fixture framework | info/high | PASS-THROUGH | info/, | no | - |
| J-05-18 | 05#18 | Resolver/store abstraction; fan-in only documented | info/high | PASS-THROUGH | info/, | no / later | Consistent with ADR 0006:127-130. |
| J-05-19 | 05#19 (+08#10) | `TerminalCode` misses 404 `not_found` / 422 pass-through | low/high | CONFIRMED | low/high | now (cheap) | Raw `terminal()` at `support.rs:176-179`; used `agent.rs:131,139,172,212,253`. |
| J-05-20 | 05#20 | "attempt" in caller-facing fault text; "tenant" in adatkapcsolat | low/high | CONFIRMED | low/high | now (5 min) | `support.rs:110,127` ("this attempt issued nothing") vs glossary *Execution* Avoid list; README says "execution". |
| J-05-21 | 05#21 | Handler attribute blocks repeated | info/high | CONFIRMED | info/high | no | 13 `invocation_retry_policy` attrs in `handlers.rs`; discovery test is the mitigation. |
| J-05-22 | 05#22 | Validated newtypes hand-roll impls | info/high | PASS-THROUGH | info/, | later | - |
| J-07-01 | 07#1 | Unsigned requests accepted by default, no warn; binds 0.0.0.0 | high/high | DOWNGRADED | medium/high | - | `main.rs:41,45` defaults; `:184-195` info-only when keys present, silence when absent; README:331 states it; scope is protocol data (`shared-core lib.rs:72-73`). SDK-default behaviour, documented, endpoint is not the ingress, but absent from deploy checklist; recs (a),(c) sound, (b) optional. |
| J-07-02 | 07#2 | Journal leak scan `#[ignore]`d; `assert_not_impl_any!` under `cfg(test)` | high/medium | CONFIRMED-WITH-CORRECTION | medium/medium | - | `#[ignore]` at `tests/service.rs:1458`; dagger check remote (`dagger.toml`), Docker-in-Dagger implausible. But `static_assertions` is a dev-dep (`Cargo.toml:36`) so `cfg(test)` is by design and fires in CI's test build; plus redaction/sentinel unit tests (`account.rs:441-`, `service/tests.rs:539,622`). Cheap sentinel-serialisation test rec is sound. |
| J-07-03 | 07#3 | Two workflows tag-pinned; excluded from dependabot | medium/high | CONFIRMED | medium/high | - | `dagger.yaml:8,10` (`@v7.0.1`, `@v8.4.1`); `release.yml` 16 `@vN` uses, `permissions: contents: write` (`:17-18`); `dependabot.yaml` excludes both. `container.yaml`/scorecard SHA-pinned. |
| J-07-04 | 07#4 (≈02) | Adatkapcsolat: no body limit by default; parse before auth | medium/high | CONFIRMED-WITH-CORRECTION | medium/high | - | `DefaultBodyLimit::disable()` (`axum.rs:156`); preflight before header (`:247` vs `:257`). Correction: `root_kind` stops at the first start tag (`document.rs:156-169`); only `validate_element_namespaces` is a full pass (`:94-125`). Rec (401 before any parse when header missing; root-only before key check) is sound. |
| J-07-05 | 07#5 | Upstream bodies echoed into faults / journaled `Transport(String)` | medium/high | DOWNGRADED | low/high | - | `xml.rs:69-71,77-81` puts whole body in `UnexpectedBody`; `ClientError::Parse` is `#[error(transparent)]` (`client.rs:32-33`); → `Unanswered::Transport(error.to_string())` (`gateway.rs:1251,1537`) → `read_exhausted` fault (`support.rs:185-191`); `DeleteOutcome/SetPaymentsOutcome::Transport` journaled (`:1471,1510`). Hygiene (size, HTML pages), not a secret leak; truncation rec sound. |
| J-07-06 | 07#6 | `/health` behind identity verification; no docs | medium/high | CONFIRMED-WITH-CORRECTION | low/high | - | Order confirmed (`endpoint/mod.rs:242-250`). Bigger reason httpGet fails: HTTP/2-only server (`http_server.rs:120`; README:216). Document TCP probe. |
| J-07-07 | 07#7 | Debian-slim runtime image | low/high | CONFIRMED | low/high | - | `Dockerfile:28` `debian:13-slim@sha256…`. |
| J-07-08 | 07#8 | No file-based secret source | low/high | CONFIRMED | low/high | - | No `_FILE`/`agent_key_file` in `config/sources.rs`, `config.rs`. |
| J-07-09 | 07#9 | `Endpoint` accepts userinfo and plain http; journaled+logged | low/high | CONFIRMED | low/high | - | `account.rs:180-190`; author acknowledges userinfo possibility at `:247-248`; logged `main.rs:162`. |
| J-07-10 | 07#10 (+08#17) | `credential_ref` and account id reach the caller's `unavailable` fault | low/high | CONFIRMED | low/high | - | `prologue.rs:139-148`; `FetchError::Gone` display `account.rs:356`. Contradicts "no response names the account" (CONTEXT *Outcome*, design §7:363). |
| J-07-11 | 07#11 | Fan-in via two keys of one account passes load-time check | low/medium | CONFIRMED | low/medium | - | Uniqueness on `id`/`supplier_id`/`(endpoint, key)`; a wrong `supplier_id` is caught on the first found document. ADR 0006:127-130 already calls it "the checkable half"; making the residual explicit is fine. |
| J-07-12 | 07#12 | `invoice_number` unbounded into step names / external ids | low/high | CONFIRMED | low/high | - | `contract/request.rs:92,106,225` bare `String`; `support.rs:599,629,656` step names; `identity.rs` storno ids. |
| J-07-13 | 07#13 + 08#5 | Logs/spans lack account id, scope, invocation id | low/high (07), medium/high (08) | UPGRADED (merged) | medium/high | - | `gateway.rs:813-818,914-921` spans; `support.rs:119-123` warn; only the prologue fetch warns carry `account.id` (`prologue.rs:122-140`). Unattributable `credentials_rejected` in multi-account mode is an operational gap. |
| J-07-14 | 07#14 | Parse errors echoed in 400 bodies | low/high | CONFIRMED | low/high | - | `axum.rs:249,279`. |
| J-07-15 | 07#15 | IPN mitigation guidance thin | low/high | CONFIRMED | low/high | - | `szamlazz-ipn/README.md:39`. |
| J-07-16 | 07#16 | `RawResponse` derives `Debug` incl. headers | low/high | CONFIRMED | low/high | - | `wire.rs:141-145`. Latent. |
| J-07-17 | 07#17 | CLI secrets as `String` with derived `Debug` | info/high | CONFIRMED | info/high | - | `szamlazz-cli/src/main.rs:22`, `listen.rs:31`. |
| J-07-18 | 07#18 | No zeroization | info/high | PASS-THROUGH | info/, | - |, |
| J-07-19 | 07#19 | No provenance/attestation | info/high | CONFIRMED | info/high | - | No attest/provenance/sbom in any workflow. |
| J-07-20 | 07#20 | Buyer PII journaled 3d; not surfaced to operators | info/high | PASS-THROUGH | info/, | - | Consistent with ADR 0001:71-76. |
| J-07-21 | 07#21 | `--check-config` tests lack key-absence assertion | info/high | CONFIRMED | info/high | - | `tests/check_config.rs` asserts presence strings only. |
| J-07-22 | 07#22 | `cargo-deny` installed, no `deny.toml` | info/medium | CONFIRMED | info/high | - | `devenv.nix:12`; no `deny.toml` at root. |
| J-08-01 | 08#1 | Fault body double-encoded; READMEs describe inner JSON as the body | high/high | CONFIRMED | high/high | - | `support.rs:160-166`; harness `tests/service.rs:494-502`; SDK `TerminalError` is code+message only (`errors.rs:110-150`). READMEs `:307` / `:281`, design §7 never mention the envelope. Rec: document the wire shape; "rename inner field" optional. Exact envelope field names not verified against a live server (harness only reads `message`). |
| J-08-02 | 08#2 | Zero example response bodies | high/high | CONFIRMED-WITH-CORRECTION | medium/high | - | No `"outcome"` literal in any `.md`; `CreateResponse` emits nulls (no `skip_serializing_if`, `response.rs:104-147`; `round_trip` test `:956`). Correction: `check_account` example exists (endpoint README:166); design §7 lists fields; schema is in discovery. |
| J-08-03 | 08#3 (≈04) | Stale "four on `Szamlazz.Agent`" / `handlers=4` | medium/high | DOWNGRADED | low/high | - | README `:122,:240`; five handlers at `handlers.rs:283,301,329,363,386`; `check_config.rs:53-54` checks names only. Cosmetic drift, cheap fix. |
| J-08-04 | 08#4 | No operator runbook | high/high | DOWNGRADED | medium/high | - | Only `sys_invocation` use is the drain (`README.md:190`); no troubleshooting/`x-restate-id` guidance in either README. Fault tables do give caller actions; the "find the invocation" path is missing. |
| J-08-05 | 08#5 | Logs lack account/scope | medium/high | CONFIRMED (merged into J-07-13) | medium/high | - | See J-07-13. |
| J-08-06 | 08#6 | `customer_account_url` only on fresh `issued`; no delivery guidance | medium/high | CONFIRMED | medium/high | - | `create.rs:70-78` (`found`) omits it; `:115` sets it; the URL exists only in the create reply (`invoice.rs:665-666,1118-1120`), never in a queried document. |
| J-08-07 | 08#7 | Conflict reasons have no "what to do" column | medium/high | CONFIRMED | medium/high | - | README `:157-162` comma list vs 4-column fault table. |
| J-08-08 | 08#8 | Agent-key rotation undocumented; static resolver = restart | medium/high | CONFIRMED | medium/high | - | Config loaded once (`main.rs:68`); only identity-key rotation documented (`README.md:325-331`); library claim `:99` is trait-general. |
| J-08-09 | 08#9 | `get` omits correctives; `storno_number` never populated | medium/high | CONFIRMED-WITH-CORRECTION | medium/high | - | `OrderStatus` four slots (`response.rs:608-619`); `document_status` hard-codes `storno_number: None` (`storno.rs:267-274`). Correction: reviewer's proposed wording is wrong; `get` *never* reports a storno number; README `:186` `storno_number?` is misleading for `get`. |
| J-08-10 | 08#10 | 422 pass-through puts numeric code in `code` | low-med/high | CONFIRMED | low/high | - | `agent.rs:139-143`; merged with J-05-19. |
| J-08-11 | 08#11 | `invalid_input (not_found)` vs 404 `not_found` | low/high | CONFIRMED | low/high | - | `create.rs:310-314`; `agent.rs:131-135`. |
| J-08-12 | 08#12 | NAV tax fields unexplained | medium/medium | DOWNGRADED | low/medium | - | Fields exist with one-line docs (`contract/document.rs:151-160`). Domain guidance; reviewer concedes the wording needs an accountant. |
| J-08-13 | 08#13 | `vat_rate` free string, no enumeration in schema | low/high | PASS-THROUGH | low/, | - |, |
| J-08-14 | 08#14 | `Idempotency-Key` guidance scattered | low-med/high | CONFIRMED | low/high | - | Three lists (`design:441`, lib README `:282`, endpoint README `:275`). |
| J-08-15 | 08#15 | `Debug` formatting in logs and `account_mismatch` | low/high | CONFIRMED | low/high | - | `main.rs:161,163` (`?account.mode`, `?supplier_id`); `support.rs:237-242`; README `:120` reproduces `mode=Live supplier_id=Some(…)`. |
| J-08-16 | 08#16 | Go-live checklist references absent probe records | medium/high | DOWNGRADED | low/high | - | `szamlazz-hu-behaviour.md:7-9,203-212`. Steps are followable without the records; copy-paste CLI forms would be nicer. |
| J-08-17 | 08#17 | `unavailable` names account id, contradicting "no response names the account" | low/high | CONFIRMED | low/high | - | `prologue.rs:145-148`; merged with J-07-10. |
| J-08-18 | 08#18 | Agent README example minimal | medium/high | DOWNGRADED | low/high | - | Example compiles against signatures (`client.rs:162`, `credentials.rs:67,39-43`, `invoice.rs:301-307,424-428,617-622`, `item.rs:129-136`, `types.rs:235,354`); suggested additions all exist (`invoice.rs:276,581,595,668`; `types.rs:121`; `error.rs:100`). Enhancement, not defect. |
| J-08-19 | 08#19 | No sans-IO end-to-end example | medium/high | DOWNGRADED | low/high | - | README `:50` prose only. Enhancement. |
| J-08-20 | 08#20 | README examples not compile-tested | low/high | CONFIRMED | low/high | - | No `include_str!("../README.md")` in any `lib.rs`; only TOML blocks tested (`endpoint/src/config.rs:276-281`). |
| J-08-21 | 08#21 | CLI requires hand-computed totals | medium/high | DOWNGRADED | low/high | - | `LineItem` requires `net/vat/gross` (`item.rs:54-59`); example `invoice.json:49-51`; CLI deserialises directly (`invoice.rs:136`). Dev-tool convenience. |
| J-08-22 | 08#22 | CLI `--method` takes Hungarian tokens, silent passthrough | low-med/high | CONFIRMED | low/high | - | `payment.rs:31-33`. |
| J-08-23 | 08#23 | CLI `taxpayer` accepts only the 8-digit stem | low/high | CONFIRMED | low/high | - | `taxpayer.rs:12,19`. |
| J-08-24 | 08#24 | CLI exit codes uniform | low/high | PASS-THROUGH | low/, | - |, |
| J-08-25 | 08#25 | `szamlazz listen` defaults to :8080 = compose ingress port | low/high | CONFIRMED | low/high | - | `listen.rs:23`; `compose.yaml:8`. |
| J-08-26 | 08#26 | No per-persona "start here"; CONTEXT.md is a spec | medium/high | DOWNGRADED | low/high | - | Word counts confirmed (CONTEXT 5,789; design 10,592; READMEs 5,602/5,831; ADR 0006 4,582). Glossary depth is a project style choice (task designates it authoritative); "start here" table is a good, cheap idea. |
| J-08-27 | 08#27 | Caller contract/fault tables triplicated, drifting | medium/high | CONFIRMED | medium/high | - | Three lists (`design:441`, lib `:282-343`, endpoint `:275-294`). See additional finding A for a concrete contradiction. |
| J-08-28 | 08#28 | `reconciled` vs `already_issued` indistinguishable to caller | low/medium | PASS-THROUGH | low/, | - |, |
| J-08-29 | 08#29 | Two kind vocabularies across responses | low/high | PASS-THROUGH | low/, | - |, |
| J-08-30 | 08#30 | `set_payments` vs glossary "avoid payment"; `storno` vs `storno_invoice` | low/high | CONFIRMED | low/high | - | CONTEXT *Credit entry* Avoid list; `handlers.rs:363,386,204`. |
| J-08-31 | 08#31 | No ADR index; "as v1" references | low/high | PASS-THROUGH | low/, | - |, |
| J-08-32 | 08#32 | Dates/payment method rules undocumented | low/medium | PASS-THROUGH | low/, | - |, |

Cross-report overlaps noticed (only 05/07/08 were available): `handlers=4` (08#3; likely also 04);
body limit disabled by default (07#4; also 02 per the brief); logs lacking account/scope (07#13 ≡ 08#5);
account id / credential_ref in the `unavailable` fault (07#10 ≡ 08#17); 422/404 outside `TerminalCode`
(05#19 ≡ 08#10); "attempt" wording (05#20; visible in 08#1's example fault text).

## (c) Detailed notes

### CONFIRMED-WITH-CORRECTION

- **J-05-05 (05#5).** All six concerns are in `support.rs` as listed; the macro is 340 lines stamped
  three times (`:688-690`) with `#[allow(dead_code, unused_imports)]` (`:348-352`). The
  rust-lang/rust#100013 justification cannot be confirmed or refuted without compiling; treat the
  macro as a legitimate workaround until someone tries the generic form. Severity is organisational
  (low), not correctness.
- **J-05-06 (05#6).** The placement contradicts the module doc's first sentence only superficially:
  `config.rs:36-42` explains that the account value types live there "so that any resolver's
  configuration can reuse them", and the README/glossary say the same. The sub-claim that nothing
  outside the crate imports them is false for the endpoint's tests (`config.rs:187`, `schema.rs:325`).
  Info, no churn.
- **J-05-15 (05#15).** The walker and `MOVED` exist as described. The recommendation to delete
  `MOVED` rests on "a crate that has never been released"; the endpoint README instructs
  `cargo install restate-szamlazz-endpoint` and documents a `ghcr.io` image on every `v*` tag, and
  ADR 0006:399 says the refusal is kept on purpose. Unverifiable offline; do not act on it without
  checking crates.io / tag history.
- **J-07-02 (07#2).** The e2e scan is indeed `#[ignore]`d and only runnable with Docker; the dagger
  module is remote, so "not in CI" is a reasonable inference (medium confidence stands). The
  correction: `assert_not_impl_any!` is under `cfg(test)` because `static_assertions` is a
  dev-dependency; CI's `cargo test --all-targets` compiles that module, so a `Serialize` impl on
  `Credentials`/`AgentKey` fails CI today. There are also non-Docker unit tests asserting that the
  key reaches neither renderings nor fault bodies (`account.rs:441-`, `service/tests.rs:539-648`).
  The remaining gap, a `String` field populated from `expose()` inside a journaled outcome, is real
  and the proposed sentinel-serialisation unit test is the right cheap guard. Medium.
- **J-07-04 (07#4).** `Document::preflight` = `root_kind` (returns at the first start tag,
  `document.rs:156-169`) + `validate_element_namespaces` (full pass to EOF, `:94-125`). So "tokenized
  end-to-end twice" is one full pass plus one prefix scan. The unbounded-body point is unaffected and
  is the substantive risk. Note the rationale in the doc comment (`axum.rs:86-88`) (receipt batches
  are unbounded) is a fair reason for *offering* no limit, not for *defaulting* to none. Medium.
- **J-07-06 (07#6).** Verification-before-health is as cited. But `HttpServer` serves HTTP/2 only
  (`http_server.rs:120`), which the README states (`:216`); a kubelet `httpGet` speaks HTTP/1.1 and
  fails regardless of identity keys. Correct recommendation: document a TCP (or exec) readiness
  probe. Low.
- **J-08-02 (08#2).** The absence of `issued`/`conflict`/`reversed`/fault examples is confirmed, and
  the null-emitting flat shape and string `Decimal`s are real surprises. But the endpoint README does
  carry one example body (`check_account`, `:166`), design §7 (`:367-381`) lists the response shapes
  schematically, and the schemars discovery manifest documents the types. Medium.
- **J-08-09 (08#9).** `get` has four slots and never looks up a storno number, `document_status`
  hard-codes `storno_number: None` (`storno.rs:271-273`). The README's `{state: reversed,
  storno_number?}` (`:186`) is therefore misleading for `get`. The reviewer's suggested replacement
  text ("reports `storno_number` only when the newest document under the order is the storno") is
  also wrong; the accurate sentence is "`get` never reports a storno number; the create and storno
  handlers do". Medium (doc inaccuracy on the reconcile path).

### DOWNGRADED

- **J-05-01 (05#1, high → medium).** Facts confirmed in full. Downgraded because (i) the two models
  are siblings, not the same document, the Adatkapcsolat `InvoiceInfo` carries incoming-invoice
  fields (`document.rs:514-534`) and looser optionality, while the Agent's query response asserts
  required fields (`id: u64`, `document_type: String`, `e_invoice: InvoiceAppearance`); a shared type
  must be the lenient superset or keep a two-layer design; (ii) both crates are released, so the fix is
  a breaking release of two crates plus a new crate. "Later, before 1.0" is the right call; unifying the
  `de` helpers first is a good no-API-change step.
- **J-05-02 (05#2, high → medium).** 77/95 confirmed; the `new()`+assignment pattern is exactly what
  `build.rs:128-167` does. It is a tax, but every request type has a `new()` with the required fields
  and public mutable fields, so nothing is impossible or unsafe. Removing the attribute on request
  types is a non-breaking widening; the trade (a new request field becomes a 0.x minor bump) is
  acceptable. Worth doing now.
- **J-05-03 (medium → low)**, **J-05-04 (medium → low).** Duplication confirmed by count; neither is
  a correctness risk and both have small, safe refactors. Low.
- **J-07-01 (07#1, high → medium).** Facts confirmed (defaults, silence, scope-in-protocol). Downgraded
  because the SDK behaves identically for every Restate service, the README states the consequence
  verbatim (`:331`) and documents `--bind 127.0.0.1` (`:216`), and the endpoint is meant to be
  reachable only by the Restate server. What is missing is a start-up `warn!` and a sentence in the
  deploy checklist / multi-account section, cheap and worth doing. Refusing to start without keys
  (rec b) is too opinionated for a self-hosted single-account deployment; make it opt-in if at all.
- **J-07-05 (07#5, medium → low).** Path confirmed end to end. The content is szamlazz.hu's (or a
  CDN's) response, not a secret; the realistic case is a maintenance HTML page in a 503 body and in
  `completion_failure`. The "buyer data in an unexpected root" case requires szamlazz.hu to answer a
  query with a different document root, which is contrived. Truncate/sanitise for hygiene.
- **J-08-03 (medium → low).** A factual drift on two README lines; the log itself is right. Cheap fix.
- **J-08-04 (high → medium).** The fault tables already tell the *caller* what to do; what is missing
  is the *operator* path (find the invocation, read the journal, which step). Real gap, medium.
- **J-08-12, J-08-16, J-08-18, J-08-19, J-08-21, J-08-26 (medium → low).** All are enhancements
  rather than defects: domain guidance the reviewer cannot vouch for (#12), a checklist that is
  followable without the referenced records (#16), a quick start that compiles and could show more
  (#18/#19), a CLI convenience (#21), and a documentation-shape preference against a glossary style
  the project chose deliberately (#26, the "start here" table remains a good idea).

### UPGRADED

- **J-07-13 / J-08-05 (low + medium → medium, merged).** Same finding from two angles. In a
  multi-account deployment the `credentials_rejected` warning is the line the README tells the
  operator to page on (`README.md:297`), and it names namespace and code only. Adding `account =
  %account.id` and the scope to the prologue span costs nothing and leaks nothing (both are already
  journaled). The SDK exposes no invocation id to handlers in 0.12 (08's own question 4), so document
  `x-restate-id` as the caller-side handle.

### UNVERIFIABLE (sub-claims)

- 05#15's "never released" premise (see above).
- 05#1 / 05 question 4: whether the Agent's query response can carry an `xs:date` timezone suffix
  that `jiff`'s `Date` parser rejects (`xml.rs:169-181`). Plausible latent parse failure; no evidence
  either way in the repo.
- 08#1: the exact field set of the ingress error envelope (whether `code` is the HTTP status and
  whether `restate_code` is present). The harness only reads `message`; the double-encoding itself
  is certain.

## (d) Additional findings

**A. The caller contract's headline sentence contradicts its own fault table (low-medium, high
confidence).** Endpoint README `:281`: "Every one of them means 'outcome unknown' (rule 2), never
'no document exists'." Library README rule 2 (`:285-290`) and CONTEXT.md *Outcome* ("always mean
'outcome unknown, retry with a new Idempotency-Key or read get'") say the same. Yet the same table
and the same glossary entry say `invalid_input` and `unknown_account` are refused before anything is
journaled or sent and "the same request never succeeds, so the caller fixes it rather than
retrying" (endpoint README `:285-286`; CONTEXT *Outcome*). A caller implementing the headline
literally retries a 400 with fresh keys forever. Fix: scope rule 2 to "any 5xx / `outcome_unknown` /
`unavailable` / `credentials_rejected`", and say the two 400s are "fix the request". This is the
concrete instance of 08#27's "drifting wording" that none of the three reports pinned down.

**B. `restate-szamlazz` README's `Gateway` entry omits `query_taxpayer` (info).** Both the "one plain
async fn per `ctx.run`" list and the read-fn list at `README.md:216-219` stop at `probe`; the glossary
(*Gateway*, *Unanswered*) and ADR 0001:9 include `query_taxpayer`. Drift from #49.

Nothing else of medium or higher severity was found in `main.rs`, `credentials.rs`, `support.rs` or the
two Restate READMEs that the three reports had not already raised. Positive checks made along the way:
`Secret`/`StaticConfig`/`StaticAccount` are `Deserialize`-only (`config.rs:316-345`,
`static_resolver.rs:71,85`); the fresh client per execution really is fresh (`gateway.rs:766-772`,
`client.rs:137-139` with `cookie_store(true)`, 60 s timeout, no redirects); the library README quick
start matches the SDK/crate signatures (`http_server.rs:91`, `service.rs:64,108`, `account.rs:417`,
`config.rs:98`).

## (e) Reviewer quality ratings

**05, architecture.** Accuracy ≈ 95%: every count I re-did matched or was within one (77 attrs,
95 vs 96 types; 12 variant declarations incl. one internal vs "eleven enums"; 3 macro stamps;
byte-identical policy bodies), the layering claims hold, and the "what is done well" section is
correct and specific. Weaknesses: the two "high" ratings are a notch too high for a 0.x workspace
(neither is a correctness risk), one false sub-claim (nothing imports the config value types), and
one recommendation (delete `MOVED`) rests on an unverified premise. Depth and actionability are the
best of the three, each finding has a concrete alternative and an honest "worth the churn" call,
and the open questions are the right ones.

**07: security & ops.** Accuracy ≈ 90% on facts, lower on mechanisms: the code paths, workflow
pins, Dockerfile, SDK verifier order and secret-handling claims all check out, and the "verified
claims" section is a genuine contribution (fresh client, fail-closed on missing scope, XOR key
compare, no serde on credentials). But the two "high" items lean on framing rather than new
exposure, the unsigned default is the SDK's and is documented; the `cfg(test)` assertion runs in CI,
and two mechanisms are half-right (double tokenization; `/health` vs HTTP/2-only). Good breadth
(supply chain, container, receivers, PII) and clear, proportionate recommendations once severities
are recalibrated.

**08: UX / API / docs.** Accuracy ≈ 90%: signature checks against the agent README were all
correct, the double-encoding catch (#1) is the single most valuable finding across the three reports,
and the stale handler count, missing rotation procedure, `customer_account_url` asymmetry, and
`get`'s missing storno number are all real. Weaknesses: systematic severity inflation for the
operator persona (five mediums downgraded, one high), one overstated headline ("not a single example
response body"), and one recommended replacement text (#9) that is itself inaccurate. Very
actionable (most findings come with the exact paragraph or table to add), and the persona framing
is useful for prioritisation.
