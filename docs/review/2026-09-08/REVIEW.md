# szamlazz-rs: architecture-and-tests review, 2026-09-08

Workspace at `4950e1f` (the split e2e tree) plus the uncommitted working-tree edits of that evening (a prose punctuation sweep and test renames in the endpoint crate, being made while the review ran). Four reviewers in parallel, one lead. Inputs: the code, `CONTEXT.md`, the design document and ADRs, the 2026-09-06 review (to know what was already found and since fixed: #60–#120). Tracking issue: #135.

The questions asked: is it well architected; is it well tested; is the suite stable, where is it fragile, can it be optimised and grouped; what could be tested in more isolation so that higher-level tests can shrink.

## 1. Verdict

**Architecture: good, with one structural gap that is also the biggest test gap.** The worker's layering is deep where it matters: `Gateway` hides every szamlazz.hu quirk behind eleven async fns whose only `Err`s are the two retry classes (*Unanswered*, *Unconfirmed*); every `ctx.run` closure is a one-line gateway call, so no decision lives inside a durable step; the handler file is attributes plus delegation; invariants live in types with a compile-time length proof for the *External id*. The gap: in the create, storno, delete and get paths of `Szamlazz.Order`, the *decision* (reissue vs conflict vs reversed, exclusivity, the proforma link, the storno verify arms, the delete guard) is interleaved with the `ctx`-bound read in the same async fn, so it can be exercised only under a real Restate server. `Szamlazz.Agent` and the *Prologue* extract their decisions into pure fns and unit-test them; the `Order` handlers do not.

**Well tested: yes.** Every gap the 2026-09-06 review found is closed. The gateway wiremock layer matrixes every szamlazz.hu answer with exact send counts; the *Journaled type* fixtures pin every variant; the *Run-name pin* cannot pass vacuously; the e2e reads Restate's own `sys_journal` instead of trusting responses. The protocol crates are pinned against the upstream corpus and, for the Adatkapcsolat receiver, element by element against the XSDs.

**Stable: the fast suite, yes; the e2e, conditionally.** 579 tests, four identical runs, all 327 non-trivial tests pass in isolation, no `env::set_var`, no network, no wall-clock assertion outside virtual time. The e2e passed (61 scenarios, 82.9 s) but has fixed host ports, ~40 s of fixed wait windows, one sequential `#[tokio::test]` where the first failure hides the rest, and one scenario whose premise silently expired with #114.

**Optimisable: substantially.** 63 % of test execution is reqwest parsing the system CA store on every `Gateway::open` (1.85 s of 2.9 s). Cargo runs the 24 test binaries one after another (critical path 0.66 s). Doctests are 41 % of local wall time.

## 2. Baseline

| Suite | Result | Time |
|---|---|---|
| `cargo test --workspace --all-features --locked` | 579 passed, 0 failed, 6 ignored; identical over 4 runs | 8.2 s wall, 2.9 s test execution, 3.4 s doctests |
| `-p restate-szamlazz --test e2e -- --ignored` | 2 tests, 61 scenarios, green; 2 Restate containers, cleaned up | 82.9 s |
| `-p szamlazz-agent --test live -- --ignored` | not run (real szamlazz.hu) | |

4 vCPU, 7 GiB, Rust 1.98 stable, docker 29.7.

## 3. The test pyramid

| Layer | Location | Count | Needs | Time |
|---|---|---|---|---|
| Unit | `src/**` of every crate | ~400 | nothing | ~0.3 s |
| Integration, in-process | `restate-szamlazz/tests/gateway.rs` (72), the adatkapcsolat `protocol`/`document`/`archive`/`fanout` tests (60), the agent crate's `tests/` (18), ipn | ~170 | wiremock loopback, tower `oneshot` | ~2.4 s |
| Binary-spawning | the endpoint's `check_config`, `stop`, `readme` | 18 | the compiled binary, port 0, signals | 0.1 s |
| E2E | `restate-szamlazz/tests/e2e/` | 2 tests / 61 scenarios | Restate 1.7.8 via docker, `RESTATE_SERVER_BIN` or reuse | 83 s |
| Live | `szamlazz-agent/tests/live.rs` | 4 | the szamlazz.hu test account | manual |

By count the pyramid is right-shaped. For the `Szamlazz.Order` handler layer it is inverted: the create module has four unit tests, the storno module none, and about 25 decision branches are reachable only through the e2e. The full per-branch table is in `raw/02-worker-tests.md` (T-01).

## 4. Findings

Severity is the review's; every item is a ticket (§8).

### P1 · `Szamlazz.Order`'s decisions have no seam and no unit tests (high)

Reached only by the e2e: the decision after the *Lookup step* (live × `reissue`, reversed × `reissue`, collision, foreign), the exclusivity check, prepayment-for-final, the proforma link in its three shapes, the corrective's base, verify-for-storno (including `not_stornoable` by `tipus`), the delete guard, the consumed-proforma derivation, and `Szamlazz.Agent.storno`'s unmanaged verdict. The `StornoLookupOutcome` match is duplicated verbatim in the two services. `respond_to`'s *Issued* arms (a number-less success → `outcome_unknown`; `notification_delivery_failed` → the warning) have zero coverage at any layer. `Identity::respond_to` itself shows the fix: a pure fn beside the handler, unit-tested. Extracting the rest turns ~25 e2e-only branches into millisecond tests and lets ~10 e2e scenarios that prove nothing Restate-specific be dropped. → #137, #138, #152.

### P2 · 63 % of test execution is TLS root-store parsing; a production cost and a deployment fragility too (high for the suite, medium for production)

`Gateway::open` builds a fresh reqwest client; reqwest 0.13's platform verifier PEM-parses all 121 system certificates on every build (~28 ms). Verified with a one-certificate store: gateway suite 1.37 → 0.09 s, agent `client` tests 0.20 → 0.01 s, e2e harness unit tests 0.24 → 0.01 s. With an *empty* store `Gateway::open` fails ("No CA certificates were loaded from the system") even for an `http://` endpoint; the Dockerfile installs `ca-certificates`, so production is fine today. The fresh client is a boundary (the cookie jar isolates sessions between accounts); the TLS configuration need not be part of it. → #136 (tests), #151 (production; a human decision first).

### P3 · E2E harness fragilities (medium)

Fixed 4 s `watch` windows at ten sites (~40 s of the run); hard-coded host ports 18080/19070 and 18081/19071 with no collision guard; one sequential `#[tokio::test]` where the first failing scenario aborts the rest and progress is `eprintln!`; four order keys shared across families; the killed-invocation scenario's "will not finish" premise expired when #114 bounded the resolver call at 10 s (it passes because submit → poll → kill takes ~1.5 s); *Reuse* mode replays the literal `Idempotency-Key`s; `RUN_NAMES` is not tied to the discovered handler set; no journal-fixture archive exists yet, so the compatibility guard is process-only. Untested: two creates on one key, same scope, different `Idempotency-Key`s (the headline exactly-once claim; every existing `join!` is cross-scope) and a cancellation mid-write. → #140, #141, #155, #143.

### P4 · Repetition that hides regressions (medium)

The `{code, message}` → fault mapping is written ~23 times over ten outcome enums; a forgotten arm is a missing e2e scenario, not a compile error (J-05-04 of #68, still open). `tipus` is a bare string matched against literals in six places with two different sets. Three `Doc` fixture renderers all default `eszamla` to a code szamlazz.hu was never observed to emit. → #153, #139, #156.

### P5 · Smaller items (low)

`szamlazz-cli` has zero tests and its shipped example JSON is never checked to deserialize (#150). `szamlazz-ipn`'s tests lock in a strict 400 on an unparsable date, empty amount or missing payment method, a deterministic loss after ten deliveries, with no captured real payload (B-10 of #75; #149). Eight pure adatkapcsolat parse tests hide behind `cfg(axum)`; lexical-shape refusal is pinned for boolean and date only; `keys_match` has no direct test; fixture literals are copied four times (#148); the 64 MiB body tests copy the body up to six times, 0.7 s and ~600 MiB (#147). The agent crate has no operation × failure-shape table and its rounding overflow is pinned under `Exact` only (#146); the response envelope parser lives inside the invoice operation and is imported by four siblings (#145). `build_create` validates by building a throwaway request with placeholder references; the storno external-id constructors take `&str` although the length proof assumes the bounded type (#154). Rustdoc still says every `TerminalError` means "outcome unknown" and still speaks of the account pins ADR 0006's amendment removed (#153). The stale Dagger comment: only the ten `opendal` tests need `--all-features` at workspace level (#143).

## 5. Move down the pyramid

What can be proven without Restate, and the seam that makes it possible (details and signatures in `raw/01-architecture.md` S-1 to S-5 and `raw/02-worker-tests.md`):

| Today | Should be | Seam |
|---|---|---|
| e2e: `prepayment_missing`, `prepayment_reversed`, the exclusivity conflicts, `proforma_live`, the proforma-by-number refusals, `conflict{live}`, `already_issued`, `reversed`, the corrective base refusals | unit | a `decide_*` fn per read in the create path, taking the journaled outcome |
| e2e: `not_managed` / `not_stornoable` / already reversed on storno, the delete guard, the consumed derivation, `managed_by_order` | unit | the same, in the storno/delete/get path; one shared after-storno-lookup |
| wiremock: credential codes × 4 per operation (~40 gateways) | unit | `classify_failure` and `outcome` are pure already |
| wiremock: the create send/no-send matrix, the duplicate-order-number naming, foreignness, the `Unconfirmed` display | unit | pure private fns of the gateway module already |
| wiremock/unit: input validation needing a `Gateway` | unit without a `Gateway` | `validate_input` split from `assemble` |

Rightly e2e, keep: the *Create step*'s leading query on a re-executed closure; run retries against the handlers' invocation retry policy; exhaustion stored under the caller's key and replayed; the per-key lock; scope namespacing of key and `Idempotency-Key`; purge; kill; the flag day; the ingress envelope; the journal scan; the *Run-name pin*; the protocol-v7 canary.

## 6. Grouping and selective running

Possible today: `-p`, `--lib`, `--test <name>`, `--doc`, name filters, `-- --ignored`, `--exact`. Not possible: one e2e scenario or family; a "pure only" or "fast" set; running test binaries in parallel. No cargo aliases, no nextest configuration. nextest applies with nothing that breaks (`figment::Jail`'s lock becomes moot; the rest is already per-process safe); it never runs doctests, so `cargo test --doc` stays. Proposed aliases and nextest profiles, with commands, are in `raw/04-stability-performance.md` §6 (#143); per-family e2e filtering needs the families to own their keys first (#141 → #155).

## 7. Compile cost

Warm incremental after touching `restate-szamlazz`'s lib: 14–16 s (its lib-test 8.4 s, the e2e binary 8.6 s, the endpoint binary twice at 7.0 + 7.5 s because its unit tests live in the binary); after touching `szamlazz-agent`: 36 s. Test binaries 110–200 MiB; `target/debug` 15 GiB. → #144.

## 8. Tickets

| # | Ticket | Blocked by |
|---|---|---|
| #136 | the test harnesses stop loading the system CA store (`Gateway::open_with_http`) | |
| #151 | one TLS configuration per process behind `Gateway::open` (human decision first) | #136 |
| #137 | the create-side decisions of `Szamlazz.Order` are pure functions with unit tests | |
| #138 | the storno, delete and get decisions are pure functions with unit tests; one after-storno-lookup | |
| #152 | prune the e2e scenarios the unit tests now cover | #137, #138 |
| #153 | one answered-code → fault mapping, an exhaustive table test; the rustdoc drift | #137, #138 |
| #139 | `tipus` as an open `DocumentType` | |
| #154 | `build_create` splits into validate and assemble; typed storno external-id constructors | #137 |
| #140 | e2e: `watch` returns when the invocation settles; free ports | |
| #141 | e2e: every scenario reports, families own their keys, the pins cover discovery | |
| #155 | e2e: `E2E_ONLY=<family,…>` | #141 |
| #142 | e2e: the one-key race and a cancellation mid-write | |
| #156 | `tests/gateway.rs` → tree; classification tests to unit; renderers default to paper | #136 |
| #143 | a fast inner loop: aliases, nextest profiles, doctests out of the loop | |
| #144 | cheaper rebuilds: debuginfo profile, the endpoint binary compiled once | |
| #145 | agent: the envelope parser leaves the invoice operation | |
| #146 | agent: operation × failure-shape table; rounding edge cases | |
| #147 | adatkapcsolat: the body-limit tests stop copying 64 MiB | |
| #148 | adatkapcsolat: feature-free parse tests, numeric shape cases, `keys_match`, one fixture module, README doctest | |
| #149 | ipn: content is lenient | |
| #150 | cli: first tests | |

Suggested order: #136 first (smallest change, largest saving, no test changes); #137 and #138 next (the correctness gain); then the e2e quick fixes #140, #141, #142; #143 for the day-to-day; the rest as they come. #151 waits for a human.

## 9. Method and trust

Four reviewers read the code and wrote a report each (`raw/`): architecture; the worker's test pyramid; the protocol crates', CLI's and endpoint's tests; stability and performance (this one ran the suites, timed 327 tests in isolation, and verified the TLS cause experimentally). The lead spot-checked the load-bearing claims (the fixed `watch` window, the absence of a storno test module, the fresh client in `Gateway::open`), ran the un-ignored suite and the e2e, and wrote this synthesis and the tickets. The raw reports carry the `file:line` citations the tickets deliberately omit; they were written against a working tree that was being edited at the time, so a citation may be off by a few lines.
