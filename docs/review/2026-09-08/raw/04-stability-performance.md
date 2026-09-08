# Reviewer 04: test-suite stability and performance

**Environment:** 4 vCPU, 7 GiB RAM, rustc/cargo 1.98.0 stable, linker = bundled `ld.lld` (default since 1.90; verified via `-C link-arg=-Wl,--version`). `cargo-nextest` not installed; no `.cargo/config.toml` (only `audit.toml`), no `.config/nextest.toml`, no `justfile`/`Makefile`.

**Measurement caveat:** another agent was editing source files in this tree throughout (e.g. `crates/szamlazz-agent/src/item.rs` at 00:21:40, e2e harness files every ~30 s from 00:40–00:45). Every `cargo test` run that shows `Compiling …` lines was contaminated by an unrelated rebuild (20–27 s). Build was separated from test time by (a) checking `Compiling` counts, (b) running the test executables directly, and (c) repeating until a zero-compile run was obtained. Timings below are from clean runs unless flagged.

No tracked file was modified (the `touch` in §7 changes mtime only; the e2e and live `#[ignore]` suites were not run by this reviewer; the lead ran the e2e once: 82.9 s wall, 61 scenarios, green).

---

## 1. Per-binary runtime

Clean warm run (`cargo test --workspace --all-features --locked`, 0 crates compiled): **8.24 s wall** (user 9.0 s, sys 2.7 s), of which
- 1.46 s cargo freshness check,
- **2.90 s test execution** (sum of `finished in`; cargo runs the 20 binaries *sequentially*),
- ~3.4 s doctests (13 doctests across 4 crates; `--doc` alone = 3.6 s wall; `--tests` alone = 5.3 s),
- ~0.5 s process spawn / cargo overhead.

| crate | binary | kind | tests | ignored | `finished in` | direct exec |
|---|---|---|---:|---:|---:|---:|
| restate-szamlazz | `tests/gateway.rs` | integration | 72 | 0 | **1.41 s** | 1.35 s |
| szamlazz-adatkapcsolat | `tests/protocol.rs` | integration | 29 | 0 | **0.66 s** | 0.65 s |
| restate-szamlazz | `tests/e2e/main.rs` (harness unit tests only) | integration | 7 | 2 | 0.24 s | 0.24 s |
| szamlazz-agent | `tests/client.rs` | integration | 6 | 0 | 0.20 s | 0.20 s |
| restate-szamlazz | `src/lib.rs` | unit | 168 | 0 | 0.19 s | 0.17 s |
| restate-szamlazz-endpoint | `tests/check_config.rs` | integration | 10 | 0 | 0.06 s | 0.06 s |
| restate-szamlazz-endpoint | `src/main.rs` | unit | 27 | 0 | 0.05 s | 0.05 s |
| restate-szamlazz-endpoint | `tests/stop.rs` | integration | 4 | 0 | 0.04 s | 0.03 s |
| szamlazz-adatkapcsolat | `tests/document.rs` | integration | 17 | 0 | 0.04 s | 0.05 s |
| szamlazz-agent | `src/lib.rs` | unit | 173 | 0 | 0.01 s | 0.01 s |
| szamlazz-agent | `tests/upstream.rs` | integration | 9 | 0 | 0.00 s (0.47 s on cold page cache) | 0.01 s |
| szamlazz-adatkapcsolat | `tests/archive.rs` / `fanout.rs` / `src/lib.rs` | — | 8 / 6 / 10 | 0 | ≤0.01 s | — |
| szamlazz-agent | `custom_http_client.rs` / `literals.rs` / `live.rs` | — | 1 / 2 / 0 | 0/0/**4** | 0.00 s | — |
| szamlazz-ipn | `src/lib.rs` | unit | 13 | 0 | 0.00 s | — |
| szamlazz-cli | `src/main.rs` | unit | 0 | 0 | 0.00 s | — |
| 4 crates | doctests | doctest | 13 | 0 | 0.00 s (compile ≈3.4 s) | — |
| **TOTAL** | 24 binaries | | **579** | **6** | **2.90 s** | |

The gateway binary alone is 49 % of test execution; gateway + protocol + e2e-harness + client + restate-szamlazz lib = 93 %.

---

## 2. Top slowest individual tests and why

Method: every non-ignored test of the 10 non-trivial binaries run alone (`<exe> --exact <name> --test-threads=1`, min of 2), 327 tests timed; "net" = minus process start-up (~2–3 ms).

| # | net | binary | test | cause (file:line) |
|--:|--:|---|---|---|
| 1 | **659 ms** | protocol | `default_body_limit_is_64_mib_and_unlimited_is_explicit` | `oversized_body(64 MiB+1)` (`protocol.rs:609-615`, memset 64 MiB) then **4×** `request()` → `body.to_vec()` copies 64 MiB each (`protocol.rs:341`); two of the four go through `BodyLimit::Unlimited` so axum buffers and the parse scans 64 MiB of `<!--xxx…` (`protocol.rs:641-660`). ≈320 MiB of alloc/copy + 128 MiB of XML scanning. |
| 2 | 315 ms | protocol | `body_limit_applies_after_the_header_check_and_before_the_key_check` | same 64 MiB body, 2× `to_vec()` copies (`protocol.rs:666-673`), both refused early: pure memory traffic (~192 MiB). |
| 3 | 242 ms | gateway | `credential_codes_on_the_storno_are_credentials_rejected` | `for code in CREDENTIAL_CODES` × 2 `Harness::start()` = **8 `Gateway::open`** (`gateway.rs:2456-2496`). |
| 4 | 238 ms | e2e (harness) | `harness::szamlazz::create_lands_but_reply_lost_…` | `query_by()` builds a fresh `reqwest::Client::new()` per call (`harness/szamlazz.rs:441-442`, ~7 calls) + `:585`. |
| 5 | 231 ms | e2e (harness) | `harness::szamlazz::holds_answers_every_selector_…` | same: one `reqwest::Client::new()` per selector queried. |
| 6 | 213 ms | gateway | `lookup_hint_ignores_our_documents_non_invoices_and_its_own_failure` | 7-case loop, `Harness::start()` per case = 7 `Gateway::open` (`gateway.rs:668-694`). |
| 7 | 198 ms | client | `classifies_every_failure_by_outcome` | ~7 `Client::builder()…build()` (`client.rs:171-197`), each a fresh reqwest client. |
| 8 | 177 ms | e2e (harness) | `harness::szamlazz::holds_after_misses_answers_code_7_n_times_…` | as #4/#5. |
| 9 | 164 ms | gateway | `storno_lookup_reports_rejected_credentials_another_code_and_no_answer` | 4 codes × 3 harnesses = 12 `Gateway::open` (`gateway.rs:2162`). |
| 10 | 146 ms | gateway | `a_failed_post_send_re_query_names_both_the_send_and_its_own_failure` | 3-case loop + 2 more harnesses = 5 `Gateway::open` (`gateway.rs:1279-1381`). |
| 11–20 | 111–121 ms each | gateway | the ten `credential_codes_on_*` / `*_reports_a_wrong_key_*` tests | 4 codes × 1 harness = 4 `Gateway::open` each. |

### Root cause for #3–#20 (and 63 % of all test execution): system root-CA loading in `reqwest::Client::build()`

`Gateway::open` (`crates/restate-szamlazz/src/gateway.rs:862-867`) → `szamlazz_agent::Client::builder().build()` → `default_http_client()` (`crates/szamlazz-agent/src/client.rs:172-180`) → `reqwest::Client::builder().cookie_store(true)…build()`. reqwest 0.13's `rustls` feature = `rustls-platform-verifier`, which on Linux runs `rustls-native-certs` and PEM-parses **all 121 certificates** of `/etc/ssl/certs/ca-certificates.crt` (182 KB) on *every* client build; ~28 ms CPU each (linear fit over the 71 gateway tests: 31 ms fixed + 17 ms per harness at 4-way parallelism; sequential: 1 client+1 server+1 request = 31 ms, 2 clients = 62 ms).

Verified experimentally by pointing `SSL_CERT_FILE` at a 1-certificate PEM (reqwest honours it via rustls-native-certs):

| binary | system CA (121 certs) | 1-cert CA | saving |
|---|--:|--:|--:|
| gateway (72 tests, 4 threads) | 1.367 s | **0.088 s** | 15.5× |
| client | 0.199 s | 0.010 s | 20× |
| e2e harness tests | 0.242 s | 0.013 s | 19× |
| restate-szamlazz lib | 0.166 s | 0.037 s | 4.5× |
| protocol / check_config / stop | unchanged | unchanged | (not TLS-bound) |

Total: ≈1.85 s of the 2.90 s sequential test time is CA parsing. wiremock's `MockServer::start` is *not* the cost: it pools `BareMockServer`s (`wiremock-0.6.5/src/mock_server/pool.rs:24`) and costs ~1–2 ms.

**Side finding (correctness, not speed):** with an *empty* CA store `Gateway::open` fails: `reqwest::Error { kind: Builder, source: General("No CA certificates were loaded from the system") }`. The gateway/client/e2e-harness tests, and the production `Gateway::open`, therefore require host CA certificates even though every test endpoint is plain `http://`. A slim container without `ca-certificates` fails 72+6+7 tests and every handler execution. (The Dockerfile installs `ca-certificates`, so production is fine today.)

**Production note:** the same 28 ms is paid on every handler execution (fresh client per *Prologue*, by design for cookie isolation). Cookie isolation does not require rebuilding the TLS verifier: build one `rustls::ClientConfig` once and pass it through `reqwest::ClientBuilder::tls_backend_preconfigured` (0.13; `use_preconfigured_tls` is the deprecated name), keeping a fresh cookie jar per client.

Other slow-test causes:
- **check_config (10 tests, ~20 ms each):** spawns the 200 MiB endpoint binary and polls `try_wait()` with `thread::sleep(20 ms)` (`check_config.rs:371-385`); the process exits in a few ms, so the 20 ms quantum dominates. Same in `stop.rs:196-210` (~30 ms each).
- **journal leak guard** `service::journal::no_journaled_type_serialises_the_agent_key` (38 ms): one `Gateway::open` (CA load) + two requests to `http://127.0.0.1:1/` (`journal.rs:874,903-911`), which get ECONNREFUSED immediately; no timeout risk unless a firewall DROPs loopback (then 60 s `REQUEST_TIMEOUT`).
- **journal fixture tests** (`every_pinned_fixture_replays_through_the_current_types` and the generator): <5 ms; 51 fixture files / 292 KB, `UPDATE_JOURNAL_FIXTURES` unset = verify-only, no writes.
- **figment::Jail** tests (12 in the endpoint crate): <3 ms each; the Jail's global mutex serialises them, which is invisible at this size.
- `upstream.rs` 0.47 s → 0.01 s between run 1 and 2: cold page cache on `fixtures/upstream/agent`; not a code issue.

---

## 3. Flakiness probe

**3 full runs** of `cargo test --workspace --all-features --locked`:

| run | wall | compiled (contamination) | passed / failed / ignored | outcome set vs run 1 |
|---|--:|--:|---|---|
| 1 | 38.1 s | 4 crates (27.2 s build) | 579 / 0 / 6 | — |
| 2 | 32.3 s | 4 crates (23.0 s build) | 579 / 0 / 6 | identical |
| 3 | 29.1 s | 4 crates (20.9 s build) | 579 / 0 / 6 | identical |
| 4 (clean) | **8.2 s** | 0 | 579 / 0 / 6 | identical |

Per-binary spread across runs ≤0.47 s (gateway 1.45/1.92/1.53 s: CPU contention from the concurrent agent; the rest ≤0.1 s). No failures, no differing outcomes. Additionally all 327 timed tests **passed in isolation** (`--exact`, own process), which rules out inter-test ordering dependence within a binary. `--shuffle` is nightly-only so it could not be used.

**gateway thread-count comparison** (direct binary, min of 3):

| `--test-threads` | 1 | 2 | 4 (=nproc) | 8 | 16 | 32 |
|---|--:|--:|--:|--:|--:|--:|
| wall | 4.27 s | 2.18 s | **1.35 s** | 1.43 s | 1.47 s | 1.43 s |

Via cargo: `--test-threads=16` → `finished in 1.49 s`; `--test-threads=1` → `4.45 s`. Parallelism scales linearly to core count and then flattens: the work is **CPU-bound** (X.509 parsing), not I/O-bound. No ordering sensitivity observed at either thread count.

**`cargo test -p restate-szamlazz-endpoint` ×2:** run 1: 9.7 s wall (3 crates recompiled; see feature note below), run 2: 0.71 s wall; 45/45 both times, identical outcome lines. Note that `-p restate-szamlazz-endpoint` *without* `--all-features` produced different artifact hashes (`check_config-5aecc…` vs `-cd5f8…`) and rebuilt 3 crates; `-p … --all-features` reuses the workspace artifacts (verified: 0.17 s no-op after a workspace build).

---

## 4. Static fragility scan

| pattern | location | verdict |
|---|---|---|
| `env::set_var`/`remove_var` | none in the workspace's own code. Hidden: `figment::Jail::set_env` in 11 `config.rs` tests (`config.rs:489-1057`) and `main.rs:410-424`, plus `Command::env_clear()` in `check_config.rs:355`, `stop.rs:112` (child env, harmless) | **Mitigated.** Jail holds a process-global mutex and restores env+cwd; every non-Jail test in those binaries uses `load()` (TOML string only, `config.rs:306`) not `load_with_env()`, and `missing_config_file_is_reported` uses an absolute path (`main.rs:344`). Under nextest (process per test) the Jail lock becomes moot. |
| `thread::sleep` / `tokio::time::sleep` literal | `check_config.rs:384` (20 ms poll), `stop.rs:210` (20 ms poll); e2e harness `mod.rs:139,213,271,478,567,621,687`, `prologue.rs:306,313` (ignored suite); production `prologue.rs:287` `FETCH_PAUSE` | **Acceptable.** Polls have 5–10 s deadlines (`stop.rs:31,34`, `check_config.rs:21`); sleeps are bounded-wait loops, not fixed waits. Only cost: 20 ms quantum × 14 tests. |
| `Instant::now()`/`elapsed()` assertions | `prologue.rs:456-462, 491-500` (unit) | **Mitigated:** `#[tokio::test(start_paused = true)]` → virtual time, `assert_eq!(elapsed, CALL_DEADLINE)` is deterministic. |
| same, e2e | `e2e/policies.rs:38-48,206,285`, `storno.rs:403-410`, `get.rs:143-146`, `faults.rs:264`, `prologue.rs:148,213,341`: e.g. `elapsed >= 1 s && < 60 s` | **Acceptable** (ignored suite). Lower bound is the real 1 s issue-policy delay; upper bounds 30–60 s are generous. Real risk only on a heavily loaded CI box (the 60 s upper bound). |
| hard-coded TCP ports | e2e `gate.rs:139-152`: restate-server on **18080/19070** (main) and **18081/19071** (canary), fixed | **Real risk (e2e only):** collides with any other restate-server or a second concurrent e2e run on the same host; no port-probe or retry. All non-ignored tests bind `:0` (wiremock `127.0.0.1:0`, `stop.rs --port 0`, `harness/mod.rs:180`). `check_config.rs:176,182` `19080` is only a string in a warning message. |
| `127.0.0.1:1` as "refuses connections" | `journal.rs:874`, `service/tests.rs:279`, `create.rs:749`, `build.rs:238`, `stop.rs:118`, `account.rs:622,674`, `static_resolver.rs:443-465`, `config.rs:775-783`, `main.rs:286-326` | **Acceptable.** Immediate ECONNREFUSED on loopback; only the journal leak guard actually connects. Would hang 60 s if loopback were firewalled. |
| `Zoned::now`/`Timestamp::now`/`today` | `live.rs:41,180` (ignored, real API); production `archive.rs:399` | **Mitigated:** `archive.rs:397` pairs the clock with a process-local `AtomicU64` sequence; the test `timestamped_redeliveries_always_get_distinct_versions` relies on the sequence, not clock resolution. |
| `/tmp` / `temp_dir` | `journal.rs:1093` (`scratch()`: pid-suffixed dir under `temp_dir()`), `gate.rs:250` (e2e, pid+port suffixed), `check_config.rs:337` (`CARGO_TARGET_TMPDIR`, per-test file names) | **Acceptable.** Unique per process/test; `scratch()` does `remove_dir_all` first. `check_config` file names are fixed per test but each test has its own name. |
| `#[serial]` / serial_test | none | n/a (Jail is the de-facto serialiser) |
| `static mut` / `OnceLock` / `LazyLock` / `lazy_static` | `run_names.rs:214` `LazyLock<Vec<…>>` (immutable table), `archive.rs:397` `AtomicU64` | **Acceptable:** read-only or monotonic. |
| wiremock `.expect(n)` verified on drop | gateway.rs: **89**, e2e scenarios: 121, harness: 2, client.rs: 1 | **Mitigated:** wiremock 0.6.5 `MockServer::drop` → `verify()` checks `std::thread::panicking()` and only `debug!`s while unwinding (`exposed_server.rs:364-367`), so no double-panic/abort. e2e `Harness::reset` calls `verify()` explicitly (`harness/mod.rs:631-633`), a clean panic. |
| `unwrap()` on network results | 0 `.await.unwrap()`; 19 `.await.expect("…")` in gateway/e2e harness/client | **Acceptable** (`unwrap_used = warn` workspace lint enforces `expect`). A wiremock/loopback failure surfaces with a message. |
| test-ordering dependence | non-ignored: none found (all 327 pass in isolation). e2e: **by design**: 60 scenarios in one `#[tokio::test]` (`e2e/main.rs:88-157`), 4 order keys shared across files (`E2E-1`: create_invoice/get/multi_account/storno; `E2E-29`, `E2E-30`, `E2E-34`), phase 2 depends on the flag-day switch, `pins::*` depend on the whole run's journal | **Real risk (accepted):** one failing scenario aborts the remaining ones; no per-scenario re-run. |
| host CA store dependency | `Gateway::open` / `Client::builder().build()` (see §2) | **Real risk:** tests fail on a host with no/empty `/etc/ssl/certs` (`rust:*-slim` ships ca-certificates, so CI passes today). |
| 64 MiB body tests memory | `protocol.rs:621-673` | **Acceptable:** ~600 MiB peak when the two run concurrently; fine at 7 GiB, watch on <2 GiB CI runners. |
| symlinked fixtures | `crates/*/tests/upstream → ../../../fixtures/upstream/*` | **Mitigated:** `upstream.rs:27-34` skips with a message when the dir is absent (crates.io package). |

---

## 5. Feature-flag matrix

`cargo test -p <crate> [--all-features] -- --list | grep -c ': test'` (doctests included; `--list` also counts ignored tests):

| crate | default | `--all-features` | only under all-features | gating feature |
|---|--:|--:|--:|---|
| szamlazz-agent | 187 | 201 | **14** | `client-reqwest`: `tests/client.rs` (6), `tests/live.rs` (4, ignored), 4 doctests (`client.rs:9`, `ReadmeDoctests` ×3) |
| szamlazz-ipn | 10 | 14 | **4** | `axum`: `axum::tests::*` (3); `serde`: `tests::deserializes_legacy_invoice_number_field` (1) |
| szamlazz-adatkapcsolat | 34 | 74 | **40** | `axum`: `tests/protocol.rs` (29) + 1 doctest; `opendal`: `tests/archive.rs` (8) + `archive::tests::timestamped_write_retries_…` (1) + 1 doctest |
| restate-szamlazz | 246 | 251 | **5** | `schemars`: `contract::tests::{contract_types_have_schemas, correction_id_schema_carries_the_pattern, invoice_number_schema_carries_the_bound, request_schemas_are_closed_and_response_schemas_are_open, schema_descriptions_carry_no_rustdoc_link_syntax}` |
| restate-szamlazz-endpoint | 45 | 45 | 0 | (no features) |
| szamlazz-cli | 0 | 0 | 0 | — |
| **workspace** | **575** | **585** | **10** | only **`opendal`**: feature unification via `szamlazz-cli` (`axum`, `serde`, `client-reqwest`) and the endpoint (`schemars`) already enables the other 53 |

So the `.dagger/modules/ci/main.dang:32-34` comment ("plain `cargo test` compiles … the `schemars` contract tests … to nothing") is stale: at workspace level only the 10 opendal tests are missing without `--all-features` (verified: `contract_types_have_schemas` is in the workspace default list).

---

## 6. Grouping: what exists, what does not, proposal

**Possible today:** `-p <crate>`; `--lib` / `--bins` / `--test <name>` / `--doc` / `--tests`; name substring filters (`cargo test -p restate-szamlazz journal`, `credential_codes`); `-- --ignored` / `--include-ignored`; `-- --exact <name>`; `-- --test-threads=N`; `UPDATE_JOURNAL_FIXTURES=1` mode switch; the e2e *server gate* via `RESTATE_ADMIN_URL`/`RESTATE_SERVER_BIN`/docker. No cargo aliases (`.cargo/config.toml` absent), no task runner.

**Not possible today:**
- one e2e **scenario** or one **handler family** (`create_invoice::*`, `storno::*`): all 60 scenarios are sequenced in `e2e_order_protocol` (`e2e/main.rs:88-157`); name filters only select the whole test. A scenario filter would need harness support *and* scenario self-containment (4 shared order keys, phase-2 flag day, run-wide `pins`).
- "everything that needs network/TLS" vs "pure" tests: no tagging; only inferable by binary.
- "fast inner loop" (skip 64 MiB tests, skip doctests, skip binary-spawning tests): only by hand-assembled `--skip` lists.
- running binaries in parallel with each other (cargo runs them sequentially: 2.9 s sum vs ≤0.7 s critical path).
- per-test timing/slow-test reporting on stable (`--report-time` is `-Z unstable-options`).

**nextest applicability:** yes, with nothing that breaks. Process-per-test *improves* figment::Jail (no global lock contention), is neutral for wiremock (loses in-process pooling, ~1–2 ms/test), neutral for `archive.rs` `SEQUENCE`, `journal.rs` `scratch()` (pid-suffixed) and `check_config` temp files. The two e2e tests would run concurrently under `--run-ignored all`; they already use disjoint fixed ports (18080/19070 vs 18081/19071), so that is by design. Caveat: nextest does not run doctests (keep `cargo test --doc`). Expected gain: cross-binary scheduling drops the 2.9 s sequential sum to ≈ the slowest test (0.66 s) + spawn overhead (~330 processes × ~5 ms / 4 cores ≈ 0.4 s) ≈ **1.1 s**, plus per-test timings and `--retries`/JUnit for free.

**Proposal** (a `.cargo/config.toml` for aliases, a `.config/nextest.toml` for profiles; a `justfile` is optional sugar over the same commands):

`.cargo/config.toml`
```toml
[alias]
# inner loop: no doctests, no live/e2e, same feature set as CI so artifacts stay warm
t        = "test --workspace --all-features --locked --tests"
t-doc    = "test --workspace --all-features --locked --doc"
t-all    = "test --workspace --all-features --locked"
# per layer
t-agent  = "test -p szamlazz-agent --all-features --locked"
t-in     = "test -p szamlazz-ipn -p szamlazz-adatkapcsolat --all-features --locked"
t-worker = "test -p restate-szamlazz --all-features --locked"
t-gw     = "test -p restate-szamlazz --all-features --locked --test gateway"
t-journal = "test -p restate-szamlazz --all-features --locked --lib journal"
t-endpoint = "test -p restate-szamlazz-endpoint --all-features --locked"
# fast: skip the two 64 MiB body tests and the binary-spawning endpoint tests
t-fast   = "test --workspace --all-features --locked --tests -- --skip body_limit --skip check_config --skip stop"
# slow/external
t-e2e    = "test -p restate-szamlazz --all-features --locked --test e2e -- --ignored"
t-e2e-canary = "test -p restate-szamlazz --all-features --locked --test e2e -- --ignored --exact e2e_check_account_without_protocol_v7"
t-live   = "test -p szamlazz-agent --all-features --locked --test live -- --ignored"
```

`.config/nextest.toml` (after `cargo install cargo-nextest --locked`)
```toml
[profile.default]
slow-timeout = { period = "10s", terminate-after = 3 }   # nothing un-ignored should take >10 s
status-level = "slow"
fail-fast = false

[profile.default.junit]
path = "junit.xml"

# fast inner loop: everything but the memory-heavy and process-spawning tests
[profile.fast]
default-filter = """
  not (test(/body_limit/) | binary(check_config) | binary(stop) | binary(live))
"""

# CI: run everything un-ignored, retry only the network-touching binaries once
[profile.ci]
retries = 0
[[profile.ci.overrides]]
filter = "binary(gateway) | binary(client) | test(/harness::szamlazz/)"
retries = 1

# e2e: only the two ignored tests, one at a time (fixed ports), long timeout
[profile.e2e]
default-filter = "binary(e2e) & test(/^e2e_/)"
test-threads = 1
slow-timeout = { period = "300s", terminate-after = 4 }
```
Commands:
```bash
cargo nextest run --workspace --all-features --locked                     # ~1 s of tests
cargo nextest run --workspace --all-features --locked -P fast
cargo nextest run --workspace --all-features --locked -E 'package(restate-szamlazz) & test(/credential_codes/)'
cargo nextest run -p restate-szamlazz --all-features --locked -P e2e --run-ignored ignored-only
cargo test --workspace --all-features --locked --doc                      # nextest never runs doctests
```

For **per-scenario e2e**, the minimal change is an env-driven filter in `e2e/main.rs` (e.g. `E2E_ONLY=storno` runs the listed families' scenarios *plus their prerequisites*), which requires first making each family self-contained (own order keys; `pins::*` stay run-wide). That is a harness design change, not a cargo change.

---

## 7. Compile-time cost

`cargo test --workspace --all-features --locked --no-run`, warm, 4 cores:

| change | wall | crates rebuilt | notes |
|---|--:|---|---|
| no-op | 0.20 s | — | |
| `touch crates/restate-szamlazz/tests/gateway.rs` (`-p … --test gateway`) | **1.2 s** | gateway test | lld link of a 153 MiB binary is <1 s |
| `touch crates/restate-szamlazz/src/lib.rs`, `-p … --lib` only | 2.9 s | lib test | |
| `touch crates/restate-szamlazz/tests/e2e/main.rs` (`--test e2e`) | 5.3 s | e2e test (11.7 k lines) | |
| **`touch crates/restate-szamlazz/src/lib.rs`** (workspace) | **14.1–16.3 s** (2 runs) | restate-szamlazz, restate-szamlazz-endpoint | `--timings`: lib(test) 8.4 s, e2e 8.6 s, endpoint bin 7.0 s + bin(test) 7.5 s, gateway 4.0 s, lib 2.3 s; critical path lib → endpoint bin → its tests |
| `touch crates/restate-szamlazz-endpoint/src/main.rs` | 12.1 s* | endpoint (+agent, restate-szamlazz*) | *contaminated by concurrent edits |
| **`touch crates/szamlazz-agent/src/lib.rs`** (leaf everyone uses) | **36.0 s** | szamlazz-agent, restate-szamlazz, szamlazz-cli, restate-szamlazz-endpoint | expected set; user 13.7 s / sys 8.4 s → the rest is serialisation on the dependency chain |
| `touch crates/szamlazz-adatkapcsolat/src/lib.rs` | 14.6 s* | adatkapcsolat, cli (+restate-szamlazz, endpoint*) | *contaminated |

Test binaries are large (full debuginfo, no `[profile.dev]/[profile.test]` overrides): endpoint bin 200 MiB, e2e 182 MiB, restate-szamlazz lib-test 176 MiB, gateway 153 MiB, readme 124 MiB, client 111 MiB; 1.7 GiB of test executables; `target/debug` = 15 GiB (8.7 GiB deps, 5.6 GiB incremental). Linking itself is fast (lld); the rebuild cost is rustc codegen of the big test crates (`restate-szamlazz` lib-test 8.4 s, e2e 8.6 s) and the endpoint bin compiled **twice** (bin + bin-test, 7.0 + 7.5 s) because the endpoint has unit tests in `src/main.rs`.

**Dev-dependencies (direct):** restate-szamlazz 5 (reqwest, static_assertions, tokio, tracing-subscriber, wiremock), szamlazz-adatkapcsolat 6 (axum, http-body-util, opendal, serde_json, tokio, tower), szamlazz-agent 5 (jiff, serde_json, tokio, ureq, wiremock), szamlazz-ipn 5, restate-szamlazz-endpoint 4 (+ `nix` on unix), szamlazz-cli 0. Dependency graph: 370 packages normal+build, **396 with dev** → 26 dev-only packages (wiremock, deadpool ×2, assert-json-diff, regex/aho-corasick/regex-syntax, tempfile/fastrand/rustix/linux-raw-sys, parking_lot ×2/lock_api/scopeguard, nix/cfg_aliases, ureq/ureq-proto/utf8-zero, static_assertions, num_cpus). The dev-only inflation is modest; the heavy hitters (reqwest/rustls/aws-lc-rs, restate-sdk, tokio, axum, figment, opendal) are all normal deps.

---

## 8. Prioritised recommendations

| # | action | effort | expected saving | evidence |
|--:|---|---|---|---|
| 1 | **Stop loading the system CA store in tests.** Add `Gateway::open_with_http(account, credentials, reqwest::Client)` (mirrors `szamlazz_agent::ClientBuilder::http_client`, useful to embedders for proxies anyway) and have `tests/gateway.rs::gateway()`, `client.rs`, the e2e harness (`harness/szamlazz.rs:442,585`, `harness/mod.rs:122`) and the lib tests that open gateways build the client with `reqwest::Client::builder().tls_certs_only(std::iter::empty()).cookie_store(true)…`: skips `rustls_platform_verifier::Verifier::new` (`reqwest-0.13.4/src/async_impl/client.rs:757`) while keeping the per-client cookie jar the isolation tests check. Do **not** use `SSL_CERT_FILE` via `.cargo/config.toml [env]`: it would break the `live` tests' real TLS. | S | **−1.85 s of 2.90 s test execution (−63 %)**; gateway 1.37→0.09 s | §2 table |
| 2 | Production twin of #1: build one `Arc<rustls::ClientConfig>` per process (or per `Accounts`) and hand it to `tls_backend_preconfigured`; keep a fresh cookie store per `Gateway::open`. Also removes the hard dependency on host CA certs for `http://` endpoints and the `No CA certificates were loaded` start-up failure mode. | S–M | −28 ms CPU per handler execution; removes a deployment fragility | §2 side finding |
| 3 | Adopt **cargo-nextest** with the profiles above; keep `cargo test --doc` for doctests. | S | cross-binary scheduling: 2.9 s → ~1.1 s today, ~0.5 s after #1; per-test timing, retries, JUnit, `slow-timeout` | §6 |
| 4 | **Doctests out of the inner loop** (`--tests` alias / nextest), run in CI only. | XS | −3.4 s per local run (41 % of the clean 8.2 s wall) | §1 |
| 5 | Shrink the 64 MiB tests: build the body once (`LazyLock<Bytes>`) and pass `Body::from(Bytes)` instead of `body.to_vec()` per request (`protocol.rs:341`); assert the limit boundary with 64 MiB+1 once and use e.g. 1 MiB with `router_with_body_limit(…, BodyLimit::Max(1 MiB))` for the ordering/auth cases (`protocol.rs:666-673`). | S | ~0.7 s → ~0.25 s; −450 MiB peak RSS | §2 #1–#2 |
| 6 | Replace the 20 ms `try_wait` polls in `check_config.rs:371-385` / `stop.rs:196-210` with a `wait()` on a thread + `recv_timeout` (the pattern `stop.rs:141-160` already uses for start-up). | XS | ~15 ms × 14 tests (~0.2 s sequential) | §2 |
| 7 | e2e port collision guard: probe `18080/19070/18081/19071` before spawning (or pick free ports and pass them through `RESTATE_INGRESS__BIND_ADDRESS` etc., `gate.rs:273-277`) and fail with a clear message. | S | robustness of e2e runs on shared hosts | §4 |
| 8 | Dev-loop compile: add `[profile.dev] debug = "line-tables-only"` (or `split-debuginfo = "unpacked"`) to trim the 150–200 MiB test binaries and codegen; consider moving the endpoint's 27 `src/main.rs` unit tests into a `lib.rs` so the 2 k-line binary is not compiled twice (7.0 + 7.5 s on the critical path). Unmeasured here (needs a cold rebuild; 15 GiB target already), typically 15–30 % on rebuilds. | S | est. −3–5 s of the 14–16 s single-crate rebuild | §7 timings |
| 9 | Always pass `--all-features` (or never) in local invocations; mixing `-p X` default-features with the workspace all-features build keeps two artifact sets and triggers 3-crate rebuilds (observed 9.7 s). The aliases in §6 encode this. Fix the stale Dagger comment (`main.dang:32-34`): only the 10 `opendal` tests need `--all-features` at workspace level. | XS | avoids ~10–25 s spurious rebuilds | §3, §5 |
| 10 | Longer term: make e2e families self-contained (own order keys for the 4 shared ones; `pins::*` stay run-wide) and add an `E2E_ONLY=<family,…>` filter to `e2e/main.rs`, so a failing scenario can be iterated on without the full ~minutes run. | M | developer time on e2e failures | §6 |

Combined effect of #1, #3, #4 on the un-ignored suite: from **8.2 s wall / 2.9 s tests** to an estimated **~2.5 s wall / ~0.5 s tests** with no change to what is tested.
