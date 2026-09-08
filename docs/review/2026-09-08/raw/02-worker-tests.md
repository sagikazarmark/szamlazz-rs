# Reviewer 02: test architecture of `restate-szamlazz` (+ `restate-szamlazz-endpoint/tests`)

Research-only. Counts from `cargo test … -- --list` on the warm build; nothing modified, no ignored suite run. One correction to the brief: there is no `crates/restate-szamlazz/tests/readme.rs`; the README test lives at `crates/restate-szamlazz-endpoint/tests/readme.rs` (4 tests).

## Verdict

The suite is honest and dense where it looks: the wiremock gateway layer matrixes every szamlazz.hu answer with `expect(n)`, the journal fixtures are exhaustive per variant with compile-time pinning, the run-name pin cannot pass vacuously, and the e2e reads Restate's own `sys_journal`/`sys_invocation` rather than trusting responses. Since the 2026-09-06 review, every handler decision the reviewers found unreached (`create_final`, `correct_invoice`, `delete_proforma`, `set_payments`, the four storno verify arms, all 12 `ConflictReason`s) is now reached, **but only at the e2e layer**. The design doc's own rule (`docs/design/restate-szamlazz.md:849-851`: "handler decisions are tested end to end or as the pure functions they are extracted into") is applied unevenly: `service/agent.rs` and `service/prologue.rs` extract their decisions into pure functions and unit-test them; `service/create.rs` and `service/storno.rs` interleave ~12 decision sites with `ctx` calls and have **zero** unit tests for them. So by test count the pyramid is right-shaped (168 unit : 72 wiremock : 61 e2e scenarios), but for the `Szamlazz.Order` handler layer specifically it is inverted, and by wall time the e2e (≥ 53 s of fixed `watch` waits plus server start-up) dwarfs everything else (< 3 s). The e2e harness is well built but carries three structural fragilities: fixed 4 s watch windows, hard-coded host ports, and a hidden 10 s budget in the kill scenario created by #114's `CALL_DEADLINE`. The exactly-once race on one key (two creates, same scope, different `Idempotency-Key`s) is still untested (prior F-2).

## Layers

| Layer | Location | Count | Runtime dependency | What it proves |
|---|---|---|---|---|
| Unit (`--lib`) | `src/**` `#[cfg(test)]`: `service::tests` 19, `service::agent` 10, `service::prologue` 8, `service::journal` 6, `service::create` 4, `gateway::{build,tests}` 11, `contract::*` 61, `config` 16, `account::*` 23, `identity` 6, `test_support` 4 | **168** | none (journal reads `tests/journal/`; leak guard opens a socket to `127.0.0.1:1`) | pure decisions (prologue, agent, `respond_to`, `Lookup::classify`, `StornoIntent`), contract closure (`deny_unknown_fields`, schema), config→`RunRetryPolicy`, discovery attributes, fault→status, journal shapes byte-for-byte, key-leak sentinels |
| Gateway (wiremock) | `tests/gateway.rs` (3075 lines, one file) | **72** | loopback HTTP | which selectors/bodies go on the wire, exactly-n sends, classification of every szamlazz.hu answer into outcome enums, `Err(Unanswered)`/`Err(Unconfirmed)` vs data, fresh-client session isolation |
| e2e harness self-tests | `tests/e2e/harness/{gate,run_names,szamlazz}.rs` (not ignored) | **7** | wiremock / pure | the server gate, run-pattern matching, the three stub helpers |
| e2e | `tests/e2e/main.rs:86-157` (60 sequential scenarios in one `#[tokio::test]`) + `:171-232` (canary) | **2 tests / 61 scenarios** | Restate 1.7.8 (docker \| `RESTATE_SERVER_BIN` \| reuse) + wiremock | Restate semantics (replay, run vs invocation retries, per-key lock, scope namespacing, idempotency replay, purge, kill, flag day, journal rows, ingress envelope) **and** every `Szamlazz.Order` handler decision |
| Endpoint crate | `restate-szamlazz-endpoint`: 27 unit + `tests/check_config.rs` 10 + `tests/readme.rs` 4 + `tests/stop.rs` 4 | **45** | spawns the binary | config loader, `--check-config`, identity-key decision, signals, README JSON examples round-trip |

CI (`.dagger/modules/ci/main.dang:37-67`) runs both `cargo test --workspace --all-features --locked` and the ignored e2e with `RESTATE_SERVER_BIN` and `CI=true`; the gate (`harness/gate.rs:42-68`) fails rather than skips under `CI`. Prior F-1/F-5 are closed.

## Findings

### T-01 · high · `Szamlazz.Order` decision logic has no seam and no unit tests

**Evidence.** Every branch below is reached only by `tests/e2e/`:

| Site | Decision | Only test |
|---|---|---|
| `service/create.rs:447-455` `exclusivity` | `Collision`→`external_id_collision`; `Ours` live→`prepaid_chain`/`order_invoiced`; else proceed | `create_final.rs`, `create_prepayment.rs`, `create_proforma.rs` |
| `create.rs:477-490` `prepayment_for_final` | `Absent`→`prepayment_missing`; `Ours` reversed→`prepayment_reversed`; live→refs | `create_final.rs:262` |
| `create.rs:515-547` `proforma_link` auto/none | live `D` + `None`→`proforma_live`; `Auto`→link | `create_invoice.rs:654`, `create_prepayment.rs:26` |
| `create.rs:548-594` `proforma_link` by number | 7→`proforma_missing`; other order→`not_managed`; not `D`→`invalid_input`; `Api`/creds→faults | `create_invoice.rs:501` |
| `create.rs:614-648` `issue` step 3 | `Live`×`reissue`→`conflict{live}`/`already_issued`; `Reversed`×`reissue`→`reversed{storno}`/proceed; `Collision`; `Foreign`; `Api`; creds | many |
| `create.rs:333-338` `correct` base | `!carries_order`→`not_managed`; reversed→`base_reversed` | `correct_invoice.rs:91` |
| `create.rs:722-730` create-step error → `outcome_unknown` text | | `policies.rs:24` |
| `service/storno.rs:115-132` `verify_for_storno` | `not_managed`; reversed→hint→`reversed`; `tipus` ∉ {SZ,ES,VS,HS}→`not_stornoable` | `storno.rs:147,338` |
| `storno.rs:63-78` and **duplicated verbatim** at `service/agent.rs:310-324` | `StornoLookupOutcome` → early answer / fault | `storno.rs`, `agent_writes.rs` |
| `storno.rs:158-167, 180-195` `delete` | `Absent`→absent; `Collision`→`external_id_collision`; paid & `!force`→`proforma_paid`; `DeleteOutcome`→response | `delete_proforma.rs` |
| `storno.rs:230-243` consumed-proforma derivation; `:249-269` `document_status` (already a free fn, untested) | | `create_proforma.rs:15`, `get.rs:30` |
| `agent.rs:276-297` unmanaged storno | trimmed non-empty `rendelesszam`→`managed_by_order`; reversed→best-effort lookup | `agent_writes.rs:22` |

Contrast: `agent.rs:39-150` (`query_response`, `taxpayer_response`, `set_payments_response`, `credentials_check`, `taxpayer_prefix`) and `prologue.rs:184-317` are pure and tested at `agent.rs:340-611`, `prologue.rs:330-621`. `create.rs:96-166` `respond_to` is pure and tested (`create.rs:909`), but its `Issued` arm (number `None` → `outcome_unknown`; `notification_delivery_failed` → `Warning`) is not exercised (`create.rs:105-119`; the test starts at `Found`).

**Why it matters.** A regression in any row is caught only by a ≥ 90 s suite that needs a Restate server, aborts at the first failing scenario, and reports via `eprintln!`. The prior review's mutation 8 (`ProformaLink::None` → `Ok(None)`) is now caught, but only there.

**Recommendation.** Extract each row into a `fn … -> ControlFlow<Response, T>` / `Option<Response>` taking the journaled outcome as data (seams named in *Move down the pyramid*), unit-test them beside `respond_to`, and dedupe the storno-lookup match into one `after_storno_lookup`. Effort: M (one afternoon; no behaviour change; the e2e stays as is).

### T-02 · high · The exactly-once race on one key is untested (prior F-2, still open)

**Evidence.** Every `tokio::join!` in the e2e is cross-scope or call+mutation: `multi_account.rs:155` (same key, two scopes), `:308`, `:412` (call + config change), `faults.rs:265` (two different keys). `prologue.rs:292-359` proves a stuck invocation blocks a queued one on the same key and that `kill` releases it, which is the lock's existence, not the protocol's consequence: no test shows two `create_invoice` calls with different `Idempotency-Key`s on one `(scope, key)` yielding exactly one create on the wire and `issued` + `already_issued`.

**Recommendation.** One e2e scenario: `create()` stub with `set_delay(2s)` and the `create_lands_but_reply_lost`-style flag; `tokio::join!` two calls with keys `k1`, `k2`; assert `create_bodies().len() == 1`, one `issued`, one `already_issued`, both `sys_invocation` rows completed, the second's runs ending at `lookup-invoice`. Effort: S.

### T-03 · medium · `Harness::watch` is a fixed 4 s wait, never an early exit

**Evidence.** `harness/mod.rs:555-600`: `for _ in 0..polls { sleep; query }` with no completion check; `watch()` is 4 s (`:542-544`), `watch_for(12 s)` at `get.rs:144`. Ten `watch()` call sites (`policies.rs:39,207,286,379,462`, `get.rs:74`, `create_invoice.rs:196,454`, `prologue.rs:149`, `storno.rs:404`) → **52 s of deliberate wall time by design**, plus `prologue.rs:313` `sleep(1 s)`. The assertions on it (`failing_commands == ["lookup-invoice"]`) need the back-off to be sampled inside the window; under 1 s policies that is ~10 samples, so the risk is low, but the cost is paid every run.

**Recommendation.** Poll `status` alongside and return as soon as the invocation is not `running`/`suspended`/`backing-off` (keep a max window). Cuts ≈ 40 s. Effort: S.

### T-04 · medium · Hidden 10 s budget in `a_killed_invocation_releases_the_order_key`

**Evidence.** `prologue.rs:296` hangs the *next* resolution via `std::future::pending` (`harness/accounts.rs:100-104`). Since #114, `prologue::bounded` drops that future at `CALL_DEADLINE = 10 s` (`src/service/prologue.rs:98,135-139,170-173`), the `account` run fails retryably, the resolve policy re-executes 1 s later (`accounts.rs:190-195`), `resolver_hangs` is now 0, the resolution succeeds and the "stuck" invocation completes with `["namespace","account","proforma-for-delete"]`. The scenario's `submit → poll → await_status("running") → submit queued → sleep 1 s → kill` (`prologue.rs:297-323`) must all finish inside those 10 s or `h.runs(&stuck) == ["namespace","account"]` (`:336-340`) and `requests_seen == 1` (`:355`) fail. Typically ~1.5 s, so it passes, but the test's premise ("will not finish") is no longer true and nothing says so.

**Recommendation.** Either `hang_next_resolutions(u32::MAX)` semantics for this scenario (hang every resolution until `set`), or assert the deadline explicitly (`elapsed < CALL_DEADLINE`) with a comment. Effort: XS.

### T-05 · medium · Hard-coded host ports; `Reuse` mode is single-shot

**Evidence.** `harness/gate.rs:139-154`: 18080/19070/15122 and 18081/19071/15222. Two concurrent `cargo test --test e2e -- --ignored` on one host, or a container leaked by a killed run (`docker run --rm -d` only removes on stop), collide. In `Reuse` mode the harness registers deployments with `force: true` (`mod.rs:190-193`) but every scenario's `Idempotency-Key` is a fixed literal (`"e2e-1-k1"`, `create_invoice.rs:52`); a second run against the same server replays stored completions and the `expect(1)` create mocks fail at the next `reset`. The pins (`pins.rs:47-100,109-181`) scan *every* invocation the server holds, including a previous run's. Neither is documented (`README.md:733`).

**Recommendation.** Bind ephemeral host ports (`-p 127.0.0.1::8080` + `docker port`; `RESTATE_*__BIND_ADDRESS=127.0.0.1:0` is not available, so pick a free port via `TcpListener` for the binary). Document `Reuse` as "fresh server per run" or salt keys with a run id. Effort: S.

### T-06 · medium · Scenarios are order-dependent and only partially independent

**Evidence.** Shared state across scenarios: one `MockServer` (reset per scenario, `mod.rs:631-634`; a previous scenario's `expect(n)` failure is reported at the *next* scenario's `reset`), one Restate server (idempotency keys, invocations, retention), cumulative counters on `ScriptedAccounts` (`accounts.rs:84-90`), the multi-phase `MutableAccounts` whose rotation persists (`accounts.rs:233-238`; `KEY_B_V2` is live for the rest of the run). Deliberate chains: `policies.rs:115` depends on `:24` having stored a fault under `e2e-11-k1`; all of phase 2 depends on `multi_account.rs:25`; the three pins depend on the whole run (`pins.rs:19,52,58,95` assert minimum counts). Key reuse: `E2E-29` is used by `policies.rs:355` and `get.rs:62`. Ordering-dependent stubs: `holds()` relies on "first mounted match answers" (`mod.rs:88-91`). A failure in scenario *k* aborts *k+1…n*; progress is `eprintln!` only.

**Recommendation.** Keep one server, but (a) give each scenario a declared `after: &[..]` and run the rest independently, or at minimum (b) wrap each in a labelled `Result` so one run reports every failure; (c) name keys per scenario. Effort: M.

### T-07 · low · `tests/gateway.rs` is one 3075-line file mixing plumbing with decision logic

**Evidence.** It already has 11 `// -----` sections (`:421,840,1454,1692,1808,1943,1970,2090,2620,2813,2902`). It tests both: wire plumbing (selectors, `szamlaKulsoAzon`/`rendelesSzam`/`dijbekeroSzamlaszam`/`helyesbitettSzamlaszam` on the body, `expect(n)`) and classification of answers that is private pure logic in `src/gateway.rs` (`settled_by_query` `:1201-1240`, `after_duplicate` `:1121-1181`, `classify_failure` `:1811-1840`, `is_foreign` `:1845-1850`, `outcome` `:1854-1865`). The ten `credential_codes_on_*` tests (`:1694-1806, 1945, 2027, 2452, 2686, 2706, 2985`) each loop four codes and start a fresh `MockServer` per code (≈ 40 servers) to prove `is_credentials_rejected` (unit-tested at `src/gateway.rs:2020,2046`) is consulted at each call site. `unconfirmed_displays_name_their_cause` (`:1248`) is a pure `#[test]` living in the integration file.

**Recommendation.** See *Grouping*; move the display test to `src/gateway.rs`; keep one code per op at wiremock, the 4-code loop belongs to the unit test. Effort: S (mechanical).

### T-08 · low · Three `Doc` renderers, all defaulting to an `eszamla` value never observed

**Evidence.** `src/test_support.rs:55`, `tests/gateway.rs:82`, `tests/e2e/harness/szamlazz.rs:32` (rationale at `test_support.rs:13-22`: a `#[cfg(test)]` module is invisible to `tests/`). All three default `eszamla` to `2` for non-proformas (`szamlazz.rs:93-95`, `gateway.rs:127`, `test_support.rs:160`), while CONTEXT.md records `1` (paper) / `3` (e-invoice) and "`2` was never observed". Every default fixture is therefore an e-invoice by a code szamlazz.hu does not emit, and a P73-style fact must be edited in three places (plus `journal.rs`'s `document()`). None renders an empty `<rendelesszam></rendelesszam>`, so the `.map(str::trim).filter(!is_empty)` at `agent.rs:280-281` and `gateway.rs:472-474` are untested (prior mutation 9).

**Recommendation.** Default to `1`; add one `order: Some("")` / `Some(" ")` case at unit level once T-01's seam exists. Consider a `#[doc(hidden)] pub mod test_support` behind a `__test-support` feature only if the triplication keeps biting. Effort: XS/S.

### T-09 · low · RUN_NAMES is not tied to the discovered handler set

**Evidence.** `pins.rs:136-138` marks a handler "unpinned" only when an invocation of it exists; a handler added to `handlers.rs` with neither a `RUN_NAMES` row nor a scenario is invisible to the pin. The discovery test (`service/tests.rs:61-73`) catches the new handler, but updating that list gives no reminder about `run_names.rs`.

**Recommendation.** In `pins.rs`, assert `{(service, handler) in RUN_NAMES} == {discovered handlers of Order ∪ Agent}` via `<Order as Discoverable>::discover()` (already used by `get.rs:137`). Effort: XS.

### T-10 · low · Journal compatibility guard is process-dependent today

**Evidence.** `tests/journal/**` holds no `<variant>.<n>.json` (0 archives), so `every_pinned_fixture_replays_through_the_current_types` (`journal.rs:779-844`) is currently a round-trip of the present shape. The guard works through the workflow: `Verify` fails byte-for-byte (`:994-996`), `Update` archives before overwriting (`:1001-1006`), and the archive fails compat forever unless deleted. It is defeated by hand-editing a fixture, or deleting and regenerating. Git is the only backstop. Samples are dense (`resolution/account.json` sets every optional field; `create-outcome/issued.json` fills all nine fields), so the fixtures themselves are not vacuous.

**Recommendation.** Accept, or add a CI grep that a modified `tests/journal/**/<variant>.json` in a PR is accompanied by a new `<variant>.<n>.json`. Effort: XS.

## Move down the pyramid

Behaviours provable without Restate (or without wiremock), with the seam to cut:

| Today | Where it should live | Seam |
|---|---|---|
| e2e `create_final.rs:262` (`prepayment_missing`, `prepayment_reversed`) | unit | `fn prepayment_for_final_verdict(found: Lookup, identity: &Identity, refs: &mut Refs) -> Option<CreateResponse>` (from `create.rs:477-490`) |
| e2e `create_final.rs:30`, `create_prepayment.rs:194`, `create_proforma.rs:97` | unit | `fn exclusivity_verdict(found: Lookup, identity: &Identity, reason: ConflictReason) -> Option<CreateResponse>` (`create.rs:447-455`) |
| e2e `create_invoice.rs:654`, `create_prepayment.rs:26` (`proforma_live`, auto-link) | unit | `fn proforma_link_verdict(found: Lookup, link: &ProformaLink, identity, refs) -> Option<CreateResponse>` (`create.rs:527-546`) |
| e2e `create_invoice.rs:501` (`proforma_missing`, `not_managed`, "not a proforma") | unit | `fn proforma_by_number_verdict(found: QueryOutcome, number, order, identity, refs, namespace) -> Result<Option<CreateResponse>, Fault>` (`create.rs:561-592`) |
| e2e `create_invoice.rs:280,310`, `storno.rs:55` (`conflict{live}`, `already_issued`, `reversed`, reissue) | unit | `fn after_lookup(outcome: LookupOutcome, reissue: bool, identity, namespace) -> Result<ControlFlow<CreateResponse, Option<String>>, Fault>` (`create.rs:614-648`) |
| e2e `correct_invoice.rs:91` (`not_managed`, `base_reversed`) | unit | `fn base_verdict(found: &InvoiceDocument, order, number, identity) -> Option<CreateResponse>` (`create.rs:333-338`) |
| e2e `storno.rs:147,338` (`not_managed`, `not_stornoable`, already reversed) | unit | `fn storno_verdict(found: &InvoiceDocument, order) -> ControlFlow<StornoVerdict>` with `enum StornoVerdict { NotManaged, AlreadyReversed, NotStornoable }`; handler does the best-effort read for `AlreadyReversed` (`storno.rs:115-132`) |
| `storno.rs:63-78` = `agent.rs:310-324` | unit, once | `fn after_storno_lookup(outcome: StornoLookupOutcome, number: &str, namespace) -> Result<Option<StornoResponse>, Fault>` |
| e2e `delete_proforma.rs` | unit | `fn delete_guard(found: Lookup, force: bool) -> ControlFlow<DeleteProformaResponse, Box<InvoiceDocument>>` and `fn delete_response(outcome: DeleteOutcome, order, id, namespace) -> Result<DeleteProformaResponse, Fault>` (`storno.rs:158-167,180-195`) |
| e2e `create_proforma.rs:15`, `get.rs:30` (consumed derivation, `get` projection) | unit | `fn derive_consumed(status: &mut OrderStatus)` (`storno.rs:230-243`); test the existing `document_status` (`:249`) |
| e2e `agent_writes.rs:22` (`managed_by_order`, trimming) | unit | `fn unmanaged_storno_verdict(found: &InvoiceDocument) -> ControlFlow<StornoResponse>` (`agent.rs:276-297`) |
| e2e `policies.rs:24` fault text | unit | `fn create_unconfirmed(error: &TerminalError, ...) -> Fault` beside `read_exhausted` (`create.rs:722-730`) |
| wiremock `credential_codes_on_*` × 4 codes | unit | `classify_failure(ClientError) -> Failure` (`gateway.rs:1811`) and `outcome(Result<_, QueryError>)` (`:1854`) over `ClientError::Api` samples; keep one wiremock test per op for the header-vs-body parse path |
| wiremock `create_*` send/no-send matrix (`tests/gateway.rs:923-1040`) | unit + one wiremock | `fn settle(seen: Seen, reversed: Option<&str>) -> Option<CreateOutcome>` (`gateway.rs:1209-1234`) |
| wiremock `duplicate_order_number_*` `existing_number` cases (`:1529-1616`) | unit | `fn name_duplicate(newest: Result<InvoiceDocument, QueryError>, kind) -> Result<Option<String>, CreateOutcome>` (`gateway.rs:1149-1175`) |
| wiremock `lookup_hint_ignores_…`, `…foreign` (`:615-696`) | unit | `is_foreign` (`gateway.rs:1845`) is already pure; test it directly with `test_support::Doc` |
| wiremock `unconfirmed_displays_name_their_cause` (`:1248`) | `src/gateway.rs` unit | it is already pure |

Rightly e2e (Restate semantics, keep): the create step's leading query on a *re-executed* closure (`create_invoice.rs:375,440`); run-retry delay vs handler `initial_interval` and `retry_count`/`last_failure_related_command_name` in flight (`policies.rs`, `get.rs:120`); exhaustion → structured 500 stored under the key and replayed (`policies.rs:115`); same key under two scopes / same `Idempotency-Key` under two scopes (`multi_account.rs:135,210`); flag day (`:25`); purge (`get.rs:196`, `storno.rs:499`); kill releasing the key (`prologue.rs:292`); journaled account stable across executions while the key rotates (`multi_account.rs:273,367`); `Body<T>` decode through the SDK and `%20` in the ingress path (`faults.rs:24,106`); the ingress error envelope (`every_fault_carries…`); the journal-row shape and leak scan (`pins.rs`); the protocol-v7 canary (`main.rs:171`).

## Redundancy

Tested at 2–3 layers:

| Behaviour | Layers | Verdict |
|---|---|---|
| Credential codes 3/135/136/164 | unit `is_credentials_rejected` · 10 wiremock tests × 4 codes · e2e `check_account` rejected, `faults.rs` | **Wasteful at wiremock**: reduce to one code per op |
| Malformed body → 400 with serde's message | 61 contract unit + `service::tests:376,436` · e2e `faults.rs:24` | Deliberate: e2e proves the SDK does not intercept; keep one |
| Untrimmed key | `service::tests:1147` · e2e `faults.rs:106` | Deliberate (URL-decoding path); keep |
| Storno repeats `telj`, lifts `eszamla`, no `keltDatum` | unit `StornoIntent` (`tests.rs:1208,1284`) · wiremock body (`gateway.rs:2207,2252`) · e2e body (`storno.rs:147`, `agent_writes.rs:256`) | Deliberate: e2e proves verify→intent→send through the journal; the wiremock one could be trimmed to a single body assertion |
| 71/152 reconciled / corrective rejected | wiremock `:1479`, `:1638` · unit `respond_to(Reconciled/Rejected)` · e2e `create_invoice.rs:136`, `correct_invoice.rs:283` | **e2e adds nothing Restate-specific** (single execution): drop after T-01 seams exist, provided `create_invoice`/`correct_invoice` paths stay walked (they are, by `create_invoice.rs:23`, `correct_invoice.rs:21`) |
| `sztornozott` → `reversed{storno_number}`; `reissue` on live | wiremock `lookup_of_our_reversed_document_names_its_storno_from_the_hint` · e2e `create_invoice.rs:280,310` | Same: candidates once `after_lookup` is unit-tested |
| Fault → status / `szamlazz_code` field | unit `service::tests:496,547` · e2e `faults.rs:315` (4 cases) · endpoint `readme.rs` | Keep e2e for the envelope, one case suffices |
| Taxpayer stem/full → one step, forms refused, NAV code pass-through | unit `agent.rs:511,525,440` · wiremock ×5 · e2e `agent_reads.rs:225` | Fine: e2e's point is the scoped key on the wire |
| Journal-type shapes | unit fixtures · e2e byte scan | Complementary (leak vs layout); keep |

## Harness fragility (e2e)

- **Server start**: `Harness::start` polls `/health` every 500 ms up to 90 s (`mod.rs:128-140`), asserts `/version` features match the spec's flags (`:153-161`), serves the endpoint on an ephemeral port and registers with `force: true` retrying up to 60 s (`:179-215`). Docker: `docker run --rm -d --add-host=host.docker.internal:host-gateway -p 18080:8080 -p 19070:9070` (`gate.rs:212-243`), Linux-daemon-specific. Binary: `RESTATE_*` env, log + data under `$TMP/restate-szamlazz-e2e-{pid}-{admin_port}` kept on panic (`gate.rs:249-300,314-323`). The two ignored tests run concurrently on two servers.
- **Waits**: mostly polling with deadlines (`drain` 200 ms/60 s `:258-273`, `await_status` 100 ms/30 s `:458-480`, `purge` 200 ms/30 s, `wait_for_creates` 25 ms/30 s). Fixed sleeps: `watch` (T-03) and `prologue.rs:313`. Upper-bound assertions are loose (`< 60 s`), lower bounds exist only in `policies.rs:52` and `storno.rs:412` (`>= 1 s`).
- **Fake szamlazz.hu**: not a fake, canned wiremock stubs (`szamlazz.rs`). `holds()` mounts one body on number/order/external-id selectors (`:357-366`); `holds_after_misses(n)` uses `up_to_n_times(n)` (`:385-398`); `create_lands_but_reply_lost` flips an `AtomicBool` from the create stub's responder (`:409-434`), the only stateful helper. Verified in the next scenario's `reset()` (`mod.rs:631-634`; wiremock guards double-panic on drop). Response templates hand-write header + body (`:146-218`), so header-vs-body precedence is the agent crate's problem, tested there.
- **Ports/keys**: T-05. Idempotency keys are literals per scenario; VO keys `E2E-N` are reused in one place (`E2E-29`).
- **Cleanup**: `Restate::drop` `docker rm -f` / `kill`+`wait` (`gate.rs:303-325`); leaks on SIGKILL of the test binary.
- **Timing assumptions**: T-03 (4 s windows), T-04 (10 s deadline), `answered_code_on_the_create_leading_query…` asserts `max_retry_count <= 1 && failures.is_empty()` (`policies.rs:407-409`), so any transient SDK/server hiccup during that invocation fails it.
- **Run length by design**: 52 s of `watch` + 1 s sleep + policy delays outside windows (≈ 2 s) ≈ **55 s fixed**, plus server start (2–10 s) and ~60 scenarios × 2–8 HTTP round trips. Measured by the lead: 82.9 s wall on 4 cores with docker.

## Grouping / selective running

Today: `cargo test -p restate-szamlazz --lib` (168, ~0.2 s), `--lib service::` (name filter works), `--test gateway` (72, ~2 s), `--test e2e` (7 harness tests), `--test e2e -- --ignored` (both suites; `-- --ignored e2e_order_protocol` selects one). **Not possible**: one scenario, one handler family, phase 1 only. `main.rs:95-156` is a straight-line list; a developer iterating on `storno.rs` pays the whole run and loses the pins if anything before them fails.

Proposals:
1. `tests/gateway/` as a directory: `main.rs`, `harness.rs` (Doc, templates, matchers, `Harness`), then `lookup.rs`, `create.rs`, `duplicate.rs`, `credentials.rs` (the consolidated one-code-per-op tests), `reads.rs`, `probe.rs`, `storno.rs`, `delete_credit.rs`, `open.rs`, `taxpayer.rs`: the same family names as `tests/e2e/`, so `storno` is found in the same place at both layers. Filtering then works by module path (`--test gateway storno::`).
2. e2e scenario filter: a table `&[(&str, Family, fn)]` in `main.rs` and `E2E_ONLY=storno,create_final` (substring on family/name); when a filter is set, skip the three pins and the two chained dependents unless their predecessor is selected, and print the skip. Phase-2 scenarios need the flag day, so `E2E_ONLY=multi_account` implies it.
3. `E2E_PHASE=1` to stop before the flag day.
4. Cargo aliases in `.cargo/config.toml`: `test-unit`, `test-gateway`, `test-e2e`.

## Missing tests (prioritised)

1. **Two creates on one key, same scope, different `Idempotency-Key`s** → one send, `issued` + `already_issued` (T-02). Variant: the second arrives while the first's create step is in its 1 s back-off. e2e. **High.**
2. **`respond_to` `Issued` arms**: `invoice_number: None` → `outcome_unknown`; `notification_delivery_failed: true` → `warnings: ["notification_delivery_failed"]` (`create.rs:105-119`); and at wiremock, code 56 *with* a number → `CreateOutcome::Issued` with the flag (only 56-without-number is tested, `tests/gateway.rs:1302`). Unit + one wiremock. **High** (a documented "issued, but…" path with zero coverage).
3. **Handler decision unit tests** for every row of T-01, after extraction, including the `ProformaLink::None`, `not_stornoable` by `tipus` ∈ {D, SL, SS}, `proforma_paid` with/without `force`, `Collision` in each of exclusivity/prepayment/proforma-link/delete/get. **High.**
4. **Cancellation mid-write** (`PATCH /invocations/{id}/cancel` during a delayed create/storno send): CONTEXT says "any `Err` from the run, exhaustion (500) or cancellation (409), is `outcome_unknown`" (`create.rs:687-692`); `best_effort`'s 409 is unit-tested (`tests.rs:1005`), the write path's is not. e2e. **Medium.**
5. **RUN_NAMES ⊇ discovered handlers** (T-09). **Medium**, trivial.
6. **Empty / whitespace `<rendelesszam>`** at `agent.rs:280-281` and `gateway.rs:472-474`. Unit. **Low-medium.**
7. **Two-revision replay** (ADR 0005's actual failure mode): revision A suspends in a create step (wiremock `set_delay` > run delay), register revision B from the same tree, assert completion on B. Pins the SDK envelope around `Json<T>` that the fixtures do not. e2e. **Medium**, expensive.
8. **Reuse-mode idempotency**: either salt keys per run or assert a fresh server (T-05). **Low.**
