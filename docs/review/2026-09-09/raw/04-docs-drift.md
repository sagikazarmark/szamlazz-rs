# Documentation-drift review, `szamlazz-rs` @ `8355ff5` (HEAD)

Research only at the time of writing; every STALE item below was then fixed in the working tree with the review (see `REVIEW.md` §5). Verified against code by grep/read; issue states via `gh`.

## Doctest results (per crate, `cargo test --doc -p <crate> --all-features --locked`)

| Crate | Result |
|---|---|
| `szamlazz-agent` | 6 passed (incl. 3 README doctests via `include_str!` in `src/lib.rs:84`) |
| `szamlazz-ipn` | 1 passed |
| `szamlazz-adatkapcsolat` | 4 passed |
| `restate-szamlazz` | 2 passed (`lib.rs:49`, `lib.rs:83`) |
| `restate-e2e-harness` | 1 passed (`lib.rs:58`) |
| `szamlazz-cli` | no library target (nothing to doctest) |

Note: `crates/restate-szamlazz/README.md`'s two `rust` fences are **not** compiled by anything (no `include_str!` doctest like the agent crate's). Read-checked: Quick Start (lines 27–43) matches `lib.rs:49–68`; `identity_key(...)?` matches `EndpointBuilder::identity_key -> Result<Self, KeyError>` (restate-sdk 0.12.0 `endpoint/builder.rs:347`).

---

## README.md (root)

**FINE:** six packages table matches `crates/`; MSRV 1.92 (`Cargo.toml:8`); `ci:test` / `ci:end-to-end` match `.dagger/modules/ci/main.dang` (`test`, `endToEnd`); e2e commands match `main.dang` and the harness gate; `docker compose up -d` as a reuse source is still valid (`compose.yaml` is unchanged in purpose).

No stale items found.

---

## CONTEXT.md

**STALE**
- `CONTEXT.md:190` (*Gateway*): "Every read of account configuration … (**ownership-validation pins**, document defaults, seller block) goes through `Gateway::account()`". No pins exist (ADR 0006 account-pin amendment; `gateway/document.rs:174–179` `is_ours` compares order + `tipus` only; `Account` carries no pin fields). Fix: drop "ownership-validation pins," → "(document defaults, seller block)".

**AMBIGUOUS**
- `CONTEXT.md:229` (*Found document*): "constructed in tests through the wire (`test_support::Doc` renders…)". `Doc` is defined in `tests/common/mod.rs:76` and re-exported by `src/test_support.rs:51`, so the path resolves, but a reader looking for the renderer in `test_support.rs` finds only the `impl Doc` seam. Fix: "`test_support::Doc` (defined in `tests/common/mod.rs`, included by path)".

**FINE (checked):** seven `TerminalCode`s and statuses (`contract.rs:163–175`); `Fault` fields + `Fault::new`/setters public, service constructors crate-private in `service::support` (`support.rs:106–214`); `ExternalId::MAX_LEN=110`, `SEPARATOR`, `TOKENS: [&str; 8]`, compile-time proof (`identity.rs:706–835`); `Namespace::MAX_LEN=16`, `OrderKey`/`CorrectionId`/`InvoiceNumber` `MAX_LEN=40`; `IssuedKind::ALL` five, `DocumentKind::ALL` four; `Gateway` has exactly 11 pub async fns, 7 read fns returning `Unanswered` (`gateway.rs:896–1611`); `Unanswered{Transport,Unavailable}`, `Unconfirmed{Transport,Open,Unavailable,ReQueryFailed}`; `SzamlazzAnswer`, `Rejection`, `RejectionCode::{Szamlazz,Request}`, `RejectionCode::REQUEST` (`gateway.rs:99–198`); `CreateOutcome::{Api,Unavailable,DuplicateOrderNumber}`, `StornoOutcome::{Api,Unavailable}`; `Transport` gone from `LookupOutcome`/`QueryOutcome`/`StornoLookupOutcome`/`ProbeOutcome` (kept on `DeleteOutcome`/`SetPaymentsOutcome` writes); `TaxpayerOutcome::Found(QueryTaxpayerResponse)`; `RetryPolicyConfig<T: Table>`, `config::table::{Issue,Read,Resolve}` sealed, defaults 5/2m→10m/1h, 5/5s→60s/5m, none/1s→10s/1m (`config.rs:310–350`); `IssueConfig::MIN_INITIAL_DELAY = REQUEST_TIMEOUT(60s)+RE_CHECK_MARGIN(30s)`, `WorkerConfigError::IssueDelayBelowFloor`; `ValidatedWorkerConfig::unchecked` behind `test-util`, only caller `tests/e2e/harness/accounts.rs:122`; `prologue::CALL_DEADLINE=10s`, `FETCH_ATTEMPTS=3`, `FETCH_PAUSE=200ms`; `support::run_prologue`, `support::RunCtx` (3 SDK contexts), `support::execute`, `prologue::execution_span`, `prologue::record_account`, `prologue::Execution` with handler bodies as `impl Execution` in `create.rs/storno.rs/agent.rs`; `support::best_effort`; `journaled!` list in `support.rs:83`, `variants!` in `journal.rs:84`, `service::journal` scans `supplier/buyer/items/financial_items/labels/pdf`; `Endpoint::{production,is_cleartext,normalized}`, `CredentialRef`, scope keys `[a-z0-9_]` ≤36 (`static_resolver.rs:73`); `AccountResolver::resolve(Option<&str>)`, `CredentialStore::fetch`; `Accounts` custom `Debug` (`account.rs:668`); `szamlazz_agent::reqwest` re-export, `ClientBuilder::http_client`, `Gateway::open_with_http`; session-isolation test `tests/gateway.rs:2572`; `RUN_NAMES` in `tests/e2e/harness/run_names.rs`, `run_pattern`, check in `tests/e2e/pins.rs`; `restate_e2e_harness::run_names::{RunPath,RunPatterns::pattern,is_prefix_of_path}`; `Launcher::launch(&ServerSpec)` (`gate.rs:184`), `server_gate` in `gate` (`gate.rs:43`), `Reply::fault` (`ingress.rs:40`), `Restate::deploy -> Deployment` (`server.rs:419`), `plain_http()` (`lib.rs:119`), `Admin` methods incl. `Watch`/private `Sampler`, `JournalEntry`/`Invocation`/`run_result`; harness deps exactly reqwest/restate-sdk/serde/serde_json/tokio/nix, no tracing; temp dir `restate-e2e-{pid}-{name}-{admin}` (`server.rs:273`), `ServerSpec::is_valid_name` `[a-z0-9-]`, `/version` check, SIGINT/SIGTERM handler; `e2e_smoke` shape; `tests/common/mod.rs` shared by path; `InvoiceAppearance::{NotInvoice,Paper,Electronic,Unknown}`; `LineItem::try_calculated`, `Rounding`, `ArithmeticError`, `calculated_for_currency`; `OutcomeClass`, `outcome_class()` on `ErrorCode`/`ResponseError`/`ClientError`, `ErrorCode::is_credential_error`; `InvoiceKind::Prepayment{proforma_number}` / `Final{prepayment_number, proforma_number}`; adatkapcsolat `Document::{parse,parse_strict,validate}`, `ParseError::Validation`, `TransactionDirection::Other`, `raw_xml()`; `StornoRequest{invoice_number, comment}` (no date/e_invoice); design §10 has *Request identity* and *Go-live* bullets as cited.

---

## crates/restate-szamlazz/README.md

**STALE**
- `README.md:361` "Both are built `from_parts(Accounts, WorkerConfig)`" → code takes `ValidatedWorkerConfig` (`service.rs:84,153`); the README itself says so at 309–311. Fix: `from_parts(Accounts, ValidatedWorkerConfig)`.
- `README.md:821` "the *step-name table* **below** is the diff to read": the step-name table section is above (lines 708–716). Fix: "above".
- `README.md:254` "a crate-owned **additive-only** projection of the agent crate's `TaxpayerInfo`" and `README.md:315` "so they stay permissive and **additive-only**": the additive-only rule is retired (ADR 0009 decision 2; CONTEXT *Journaled type* `_Avoid_`). Fix: drop "additive-only"; for 315 say the value types are journaled with the `Account` (no compatibility rule). (Same wording survives in rustdoc `contract/agent.rs:285, 918`: #183.)

**AMBIGUOUS**
- `README.md:749` "The suite runs in about 20 s": not verifiable without a server; fine to leave.
- `README.md:186` "(a schema test pins it)": "pin" as a plain verb, not the retired journal term; fine.

**FINE (checked):** `TerminalCode::ALL` order = fault table order; retry-policy table (lines 504–510) matches every `#[handler]` attribute in `service/handlers.rs` (Order writes 5/kill/2m→10m/4m/3m/3d/30d; `get` 3/2m/2m/1d; `Agent.storno` = Order's; `set_payments` 2/2m/2m/2m/3d; `query`/`query_taxpayer`/`check_account` 3/10s→1m/2m/2m/1d); `CorrectionId` regex (`identity.rs:383`); `ConflictReason::ALL` 12 reasons match the table; `StornoOutcome` four variants; `CheckedAccount`/`CredentialsCheck`/`QueryTaxpayerRequest::prefix` exist; `Body::new`/`From<T>`; config defaults and floor text match `config.rs`; `Defaults`/`SellerConfig`/`SellerEmailConfig`, `StaticDefaults`/`StaticSeller`/`StaticSellerEmail`/`StaticAccount`/`Secret` exist; Gateway fn lists; phase 1 "fourteen scenarios" = 14 in `main.rs:205–219`; phase-2 sequence matches `main.rs:230–255`; `agent_reads` asserts no `supplier_id` (`agent_reads.rs:129`); `get::run_retries_do_not_spend_invocation_attempts`; harness module list (`mod.rs`, `accounts`, `szamlazz`, `ingress`, `run_names`); `{main|canary}` spec names (`harness/mod.rs:60,68`); "two `Transport` write outcomes" in the journal scan (`journal.rs:231,243,551–568`); *What CI runs* matches `main.dang` verbatim; *Deploying* consistent with ADR 0009.

---

## docs/design/restate-szamlazz.md

**STALE**
- `:3–4` "Decisions are recorded as ADRs 0001–0006" → there are 0001–0009. Fix: "ADRs 0001–0009".
- `:86` "the operator's go-live check (§9)" → Go-live is §10 *Hosting* (line 838); §9 is Configuration. Fix: "(§10)".
- `:227` (`query_taxpayer` row) "**No account check**: with `set_payments` one of the two handlers exempt from it: … it carries no pins": no handler has an account check any more (ADR 0006 account-pin amendment; §3:80). Fix: delete the sentence or say "no account check, like every handler since the account-pin amendment".
- `:538` "the **eight** codes of `TerminalCode`" → seven (`contract.rs:89–130`; the code block at 561–562 lists seven). Fix: "seven".
- `:601` "`options.proforma` on any kind but `create_invoice`" and `:953–954` "`prepare` refusing `options.proforma` on every kind but `create_invoice`" → since #69 `create_prepayment` takes it (`service/create.rs:591–594`: "applies to create_invoice and create_prepayment only"); the doc contradicts itself at :414. Fix: "on any kind but `create_invoice` and `create_prepayment`".
- `:601–605` lists "an invalid Virtual Object key (§3)" among faults "raised **after** the prologue" → `order_key(ctx.key())` runs before `execute` (`handlers.rs:65–69`, `support.rs:284–292`), i.e. before the prologue, as :599 and CONTEXT say. Fix: move it to the second source.
- `:906–907` "the gateway validates found documents against the account it was opened for" → validation is order + `tipus` only (`gateway/document.rs:177`). Fix: "against the order and kind it was asked for".
- `:1148` "to be automated as ignored tests (issue #15)" → #15 is CLOSED (NOT_PLANNED: "Go-live reduces to a settings audit…"). Fix: drop the issue reference or state it was closed not-planned; the live tests that exist are `szamlazz-agent/tests/live.rs` (4, `#[ignore]`).

**AMBIGUOUS**
- `:39` "(the ownership-validation pins, the document defaults, the seller block) goes through `Gateway::account()`": same retired "pins" wording as CONTEXT:190.
- `:703` "permissive for replay" and `:756–757` "which stay permissive so that an `account` entry of an earlier deployment replays": under ADR 0009 nothing replays across deployments; the permissiveness now has no replay rationale. Fix: "permissive (journaled value types; no compatibility rule, ADR 0009)".
- `:23–25` §2 *Crates* table lists only `restate-szamlazz`; `restate-e2e-harness` (a workspace crate the design describes at :1108) is absent. Add a row or note it is a test-only crate.
- `:273` "so the **pin** would misread": retired word for the step-name table; harmless.
- `:1119` "59 scenarios in sequence" vs REVIEW.md's "61": historical, both dated.

**FINE (checked):** §4 handler tables and attributes; *Durable step names* table = `RUN_NAMES` row for row; `prologue::CALL_DEADLINE`, 3×200 ms fetch loop; `support::execute` / `RunCtx`; §5 step 4 policy literal = issue defaults; `StornoIntent::from_verified`, `CreatedInvoice::reverses` (`invoice.rs:782`); §7 `Fault` shape and `From<Fault> for TerminalError` in `service::support` (`support.rs:214,222`); §9 TOML keys/defaults match `config.rs` and `StaticDefaults` fields (`static_resolver.rs:190–209`), `BothShapes` exists; §10 *Releases* subsection exists (cited by :239); §11 decide fns `decide_lookup/decide_exclusivity/decide_prepayment_for_final/decide_proforma_link/decide_proforma_by_number/decide_base/respond_to/prepare/create_outcome_unknown` (`create.rs`), gateway classifiers `settle_create/settle_storno/is_foreign/classify_failure/answered` (`gateway.rs:626–1840`), `assert_not_impl_any!` (`account.rs:731`), `holds/holds_after_misses/create_lands_but_reply_lost` (`harness/szamlazz.rs`), `/restate/scope/{scope}/send/…` (`harness/mod.rs:349`), `verify()` on `reset` (`mod.rs:452`); *What CI runs* matches `main.dang`; endpoint-crate history (:27–30, :843–845) is explicitly past tense.

---

## docs/adr/

**STALE**
- `0001…md:75–78` "*Amended (#12):* the runs journal the agent crate's response types as they are, so a queried document … is journaled with the buyer block": superseded by #127 (`FoundDocument`/`IssuedDocument` projections, `gateway/document.rs`) and ADR 0009 item 3, but ADR 0001 carries no supersession note and ADR 0006:447–449 says "The rest holds". Fix: add "*Superseded (#127, ADR 0009): the outcomes carry crate-owned projections; no buyer block is journaled.*"
- `0001…md:56–57` "The lookup, storno and delete runs use `RunRetryPolicy::max_attempts(1)`": lookup runs under the read policy (#37), storno under the issue policy (#30); only delete/set_payments and `namespace` are `max_attempts(1)` (`support.rs:726`). ADR 0004 records the amendments; ADR 0001 is not annotated. Fix: add an amendment note pointing at ADR 0004 #30/#37.
- `0004…md:3–12` status line lists amendments #22, #30, #37, #41, #61, #87 but omits **#114**, whose section exists at `:248`. Fix: add "#114 (every wait has a bound)".
- `0005…md:3–4` status: "the validation pins are read from that journaled `Account` (below)": pins gone (ADR 0006 account-pin amendment, noted in the body at :75–77 but not in the status line). Fix: strike the clause.

**AMBIGUOUS**
- `0001…md:7–9` lists the gateway's fns as `lookup, create, storno, delete_proforma, set_payments, query, query_taxpayer, probe`: misses `verify`, `hint`, `lookup_storno` (11 total). Historical text; add the full list.
- `0009…md:101–102` "The e2e's redeploy scenario, which deploys twice into one harness, keeps proving…": after #171 there is no scenario named so; the flag day (`Harness::switch_to_multi_account`, `harness/mod.rs:194–201`) is the second `deploy`. Fix: name the flag day.
- `0008…md:59` cites `tests/sans_io.rs`; the file is `tests/custom_http_client.rs`.

**FINE (checked):** 0002 status and `#64` *Bounded inputs* match `identity.rs`; 0002:73–74 and :133–139 carry account-pin supersession notes; 0003 status; 0004 #114 section names `ResolverUnavailable::TimedOut`/`FetchFailure::TimedOut`/`CALL_DEADLINE` (all in `prologue.rs`); 0005 marks #47/#125/#127 superseded by 0009; 0006 *Superseded and amended* + *Historical notes* consistent with code (`steps`→`gateway`, slug→namespace, `detect_foreign` gone); 0007/0008 stand; 0009 decisions match `service/journal.rs` module doc and `RUN_NAMES` rationale.

---

## docs/szamlazz-hu-behaviour.md

**AMBIGUOUS**
- `:240` "`create_proforma` looks up `…:invoice` and `…:prepayment` first": since #62 it also looks up `…:final` (`RUN_NAMES` create_proforma row; design §5 step 1). Fix: "…:invoice, …:prepayment and …:final".

**FINE (checked):** `eszamla_semantics` test exists (`live.rs:170`); `RequestError::{EmptyCreditEntryReplace, MissingExchangeRate}`, `ExchangeRate::automatic_mnb()`; account-pin rows consistent with ADR 0006 amendment; go-live checklist referenced by both READMEs.

---

## docs/review/2026-09-08 (ticket table only)

The table in `REVIEW.md:96–118` carries no status column; measured against the live tracker (#135 body + `gh`):
- **Done / closed:** #136 (PR #157), #137 (#159), #138 (#158), #152 (folded into #134, closed), #140 (folded into #123, closed), #142 (folded into #125, closed), #143 (folded into #132; #132 still open).
- **Open but description no longer holds:** #144 "the endpoint binary compiled once": the endpoint crate was removed (#169); only the debuginfo-profile half remains. #153: the "one answered-code → fault mapping" half is largely delivered by #128's `SzamlazzAnswer` (`gateway.rs:99`); the rustdoc-drift half is what remains.
- **Open, description still holds:** #139, #141, #145–#150, #151, #154, #155, #156 (`tests/common/mod.rs:22` still defaults `eszamla` to `2` and cites #156).
- `README.md:6–7` / `REVIEW.md` §1, §3, P3 speak of the endpoint crate, "Restate 1.7.8 via docker", "fixed host ports 18080/19070", "journal-fixture archive": dated snapshot text (explicitly "at `4950e1f`"); not counted as drift.

---

## compose.yaml, dagger, devenv, dist, workflows

**STALE**
- `compose.yaml:13–15` comment: "The e2e harness runs its own **container** with the same three." → the docker launcher is gone (#167); the harness spawns a `restate-server` binary. Fix: "The e2e harness's spawned `restate-server` runs with the same three."
- `.dagger/modules/ci/main.dang:48` "the **run-name pin**": retired term (ADR 0009 / CONTEXT *Step-name table* `_Avoid_`). Fix: "the step-name table check".

**FINE (checked):** `dagger.toml` modules (`rust`, `ci`, `dagger-gha`); `main.dang` `test`/`endToEnd`/`restateServer` match both READMEs' *What CI runs*; `.github/workflows/dagger.yaml` runs `dagger check` only; `release.yml` is cargo-dist boilerplate with no crate names; `dist-workspace.toml` `members = ["cargo:."]` (no explicit crate list, so nothing stale); `dependabot.yaml` has no docker ecosystem; no `Dockerfile`/`.dockerignore` remain; `devenv.nix` unremarkable.

---

## crates/restate-e2e-harness (README + `src/lib.rs`, `src/run_names.rs` docs)

**AMBIGUOUS**
- `README.md:22–24` "a consumer **pins** … a renamed, inserted or reordered step **strands every in-flight invocation on the next deploy**"; `src/run_names.rs:7–12` "An in-flight invocation replays the *previous* deployment's entries … strands it"; `src/lib.rs:23–25` "a consumer pins its handlers' `ctx.run` names with". This is the pre-ADR-0009 rationale (in-place redeploy). The crate is generic (true for a consumer that `--force` re-registers), but the workspace's own vocabulary retired "pin" and reframed the table as the sequence half of pause-and-resume (CONTEXT:233–235, `tests/e2e/harness/run_names.rs:14–20`). Fix: reword to cover both premises.

**FINE (checked):** module list, env vars (`RESTATE_ADMIN_URL`/`RESTATE_INGRESS_URL`/`RESTATE_SERVER_BIN`/`RESTATE_ENDPOINT_HOST`/`CI`), three `FLAG_*` consts and `FEATURES` (`gate.rs:111–124`), unix-only `compile_error!`, `Reuse::{Allowed,Never}`, `e2e_smoke` contents (deploy twice, named run, JSON `TerminalError`, kill, purge, set_public), `Cargo.toml` deps and own versioning.

---

## crates/szamlazz-agent, szamlazz-ipn, szamlazz-adatkapcsolat, szamlazz-cli READMEs + lib.rs docs

**FINE (checked):** agent README code is doctested (3 blocks pass); `client-reqwest` is the only feature (`Cargo.toml:14–17`); ipn features `axum`/`serde`, `SOURCE_IPS`, `PaymentNotification::from_form_bytes`; adatkapcsolat features `axum`/`opendal`, `Document::parse/parse_strict/validate`, `TransactionDirection::Other(String)`, `ParseError::Validation(ValidationError)`, `nest_at`, 64 MiB default; CLI commands `invoice/proforma/payment/receipt/taxpayer/listen` exist (`main.rs`, `commands.rs`), `/ipn` + `/adatkapcsolat` routes (`listen.rs:107–113`), `--adatkapcsolat-key`. No retired terms (`endpoint`, `Dockerfile`, `supplier_id`, `slug`, `InvoiceDocumentExt`, `REQUEST_CODE`, `Contradiction`, `journal_helpers!`, `worker.validate()?;` statement form, 18080/19070) appear in any crate README or `lib.rs` module doc. (The adatkapcsolat README's quick start does not compile as an external crate: see `03-receivers-harness.md` A-S2 and #176.)

---

## Summary of STALE items (count: 21; all fixed with the review)

| Doc | Lines | Fix in one line |
|---|---|---|
| CONTEXT.md | 190 | drop "ownership-validation pins," |
| restate-szamlazz/README.md | 361 | `from_parts(Accounts, ValidatedWorkerConfig)` |
| restate-szamlazz/README.md | 821 | "below" → "above" |
| restate-szamlazz/README.md | 254, 315 | drop "additive-only" (ADR 0009) |
| design | 3–4 | "ADRs 0001–0009" |
| design | 86 | "(§9)" → "(§10)" |
| design | 227 | delete the "No account check… exempt" sentence |
| design | 538 | "eight" → "seven" |
| design | 601, 953–954 | "…but `create_invoice` and `create_prepayment`" |
| design | 601–605 | move "invalid VO key" to the before-prologue source |
| design | 906–907 | "against the order and kind", not "the account" |
| design | 1148 | #15 is closed not-planned; drop or reword |
| ADR 0001 | 56–57, 75–78 | add supersession notes (ADR 0004 #30/#37; #127/ADR 0009) |
| ADR 0004 | 3–12 | add #114 to the status line |
| ADR 0005 | 3–4 | strike "the validation pins are read from that journaled `Account`" |
| compose.yaml | 13–15 | "container" → spawned `restate-server` binary |
| .dagger/modules/ci/main.dang | 48 | "run-name pin" → "step-name table check" |
