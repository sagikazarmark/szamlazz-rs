# Final hardening and release acceptance

Date: 2026-09-12.
Base: `1cbcf004bfa82b9d479424859a90c964589bedd2`.
Status: implemented, independently reviewed and accepted against the working-tree candidate;
not tagged or published.

This closes the focused pass approved after
[the final-release review](2026-09-12-final-release-1cbcf00.md). The owner explicitly
confirmed the repository `.env` key selects the intended test-mode account with EUR,
e-invoice and worker order-number settings, and authorized the five core live journeys.
The key was injected as a Dagger Secret, never printed or placed in command arguments.

## Candidate identity

Acceptance was run against the uncommitted changes on the base above, not the base alone.
The code/test/tooling content fingerprint after acceptance is:

```text
c014760437e0484305109ec0616fa69825f960ee0e382f790e0870e92b134041
```

Algorithm: SHA-256 over sorted, unique `git ls-files -co --exclude-standard` paths under
`crates`, `fixtures`, `.dagger`, `.config`, `.cargo`, `scripts`, and root `Cargo.toml`,
`Cargo.lock`, `dagger.toml`, `dagger.lock`; for each of 334 entries append path bytes, NUL,
file contents (the link target bytes for a symbolic link), NUL. Root review documents are
excluded so this evidence record does not change the fingerprint. Fixture link targets are
also represented by the retained fixture files in the selected inventory.

No implementation changes followed the live acceptance. Version preparation and tagging
remain release operations; the workspace version is still 0.3.0 with unreleased 0.4 migration
guidance. Future behavior changes require the relevant acceptance to be repeated.

## Finding-to-fix index

| Review item | Resolution | Evidence |
|---|---|---|
| F1: missing unmanaged-storno credential warning | Emit the shared sanitized warning before verification/fallback loses the typed answer; outcome classification is unchanged | Public Gateway test first failed for zero warnings, then passed for successful fallback and failed fallback, exactly one warning with namespace/code and no secret/source text |
| F2: malformed Adatkapcsolat numeric tokens | Full finite-number lexical check before Decimal conversion; reject underscores and malformed tails even after the rounding position | Public document and authenticated-router tests; valid exponent/point spellings, existing rounding and optional/empty behavior preserved |
| F3: recursive IPN amount parsing | Iterative exact significand parser shared by forms and JSON; JSON still validates its stricter grammar | Subprocess-isolated 100,000-character tests on small stacks; raw values, scale, exactness and form exponent exclusion preserved |
| F4: Linux timezone backend | Both live-test dev dependencies explicitly enable `jiff/tzdb-zoneinfo`; Dagger supplies tzdata before compilation | Package-only builds, actual Rust 1.92, Budapest winter/summer checks and live execution |
| F5: worker sibling-source inclusion | Package-local worker live support containing only the helpers it uses | All five crates packaged; extracted worker test target builds with explicit `test-util`; no workspace Agent source dependency |
| Empty configured Adatkapcsolat key | Fixed-key constructors reject empty configuration with a documented construction panic | Both constructors tested; an empty presented key follows ordinary KEY_ERR handling, without a request-time panic |
| Blanket test-marker cleanup | Scenario-owned expectation registry and exact scope/order/state-key inventory before cleanup; settled outcomes gain absence assertions | Full actual-Restate suite passes; helper tests reject unexpected, missing and wrong-scope/state markers |
| Linked live cleanup evidence | Exact storno candidate plus freshly queried exact original reporting reversed before dependent cleanup | Mock tests cover failure/blocked evidence/no resend and successful dependency sequence; live final/prepayment cleanup passed |
| Manual-only regression gates | `ci:money-features` and `ci:order-migration` are automatic Dagger checks | Both listed and executed successfully through Dagger |
| Consumer wrapper/identity limitations | Agent README/rustdoc explicitly recommend monetary strings through Axum/wrapped storno; consumers validate nonblank reported numbers | Documented supported boundary retained; no piecemeal raw-token allowlist extension |

The Adatkapcsolat parser also compacts redundant leading integer zeroes before delegating
to Decimal so that lexical-valid input does not reintroduce unbounded parser recursion.
Its historical rounding policy remains distinct from the Agent's exact-number contract.

Package-local cleanup code is deliberate test duplication, avoiding a new production or
test-support crate for release preparation. Future changes must keep the two evidence
checks aligned; the worker acceptance exercises its copy through real Restate and the vendor.

## Independent post-change review

**Spec:** no actionable findings. The reviewer compared 964,928 IPN cases with the previous
`Decimal::from_str_exact` behavior (acceptance, coefficient, scale and sign), plus 567
Adatkapcsolat public-boundary conversion cases. JSON/RON checks retained format behavior.
Marker expectations were traced to intentional unresolved scenarios rather than actual SQL
rows; no new expected marker concealed a wrongly settled scenario.

**Standards:** no documented violations or actionable smells. Separate parser policies and
package-local live helpers were judged justified by their contracts. No architectural rewrite
or new cross-crate dependency was introduced.

## Executed checks

| Command / boundary | Result |
|---|---|
| `cargo test --workspace --all-features --locked` | **847 ordinary test executions and 25 doctests passed**, 52 ignored scenarios/exporters. Cargo executes the four shared cleanup tests in three targets; eight executions are duplicates of the dedicated `live_cleanup` target. |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed |
| `cargo fmt --all --check`; `git diff --check` | Passed |
| `dagger --progress plain check --no-generate ci:end-to-end ci:money-features ci:order-migration ci:schemas rust:check` | All five checks passed |
| Actual-Restate/mocked-vendor e2e | **39 passed**, including the unfiltered main scenario suite, exact marker inventory, migration and run-wide checks |
| Automatic monetary feature check | **65 × 8 = 520 executions passed** |
| Automatic migration-runner tests | **5 passed** |
| Schema-runner tests | **8 passed** |
| Generated request XSD validation | **430 requests × 2 retained sources: 790 valid, 70 expected source conflicts; 7 negative controls passed** |
| Feature powerset | **37 checks passed** |
| Actual Rust 1.92 workspace check | `cargo check --workspace --all-features --all-targets --locked --offline` passed |
| Extracted package/native/wasm matrix | **22 acceptance commands passed**, detailed below |
| Five vendor-live core journeys | **5 passed**, fresh external execution, no whole-test retries |

The first combined local test/Clippy shell reached its outer timeout during Clippy compilation,
after all tests/doctests had passed. Clippy was subsequently run to successful completion.
The earlier review's Dagger disk blocker did not recur: provisioning now precedes compilation,
and the fresh full e2e suite completed. No incomplete run is counted as a pass.

Offline/Dagger trace:
<https://dagger.cloud/sagikazarmark/traces/f5436353f63edd39b1863b9e707c42ac>

Local logs:

- Tests: `/home/laborant/.local/share/opencode/tool-output/tool_094aed83e001f7P5kJMvZJnotk`
- Automatic gates: `/home/laborant/.local/share/opencode/tool-output/tool_094ac73e6001fPL2zVnW7ddrm4`

### Package and target acceptance

Used official `rustc 1.92.0` / Cargo 1.92.0 with `wasm32-unknown-unknown`, isolated under
`/tmp/opencode/release-portability`. Workspace checks used the locked graph. A standalone
consumer used extracted current `.crate` artifacts, not workspace source paths, selecting
one package/feature set at a time without dependency dev-feature unification.

- Agent core and `client-reqwest`: native builds and wasm checks.
- Adatkapcsolat core and axum/tracing: native builds and wasm checks; OpenDAL native build.
- IPN core and axum/serde: native builds and wasm checks.
- Extracted Agent, Adatkapcsolat and IPN test executables: all-feature `cargo test --no-run`.
- Extracted worker: default and `schemars` library builds, plus tests with `schemars,test-util`.
- Extracted CLI: test executable build.

Same-release dependencies of worker/CLI were patched to the corresponding extracted packages.
The published worker's path-only self dev-dependency is correctly absent, so `test-util` was
explicitly enabled for its test build. Initial scratch resolution permits allowed cached
dependencies; subsequent checks use its lock. These establish compilation/linking, not wasm
runtime or execution of all packaged tests. One existing Rust 1.92 unused-assignment warning
appeared in the receipt probe; it did not prevent compilation.

Exact commands, package hashes and toolchain setup are retained locally in
`/tmp/opencode/release-portability/REPORT.md` with per-command logs.

## Vendor-live acceptance

Run label: `final-hardening-20260912-1cbcf00-1`.
Nextest run UUID: `bf095896-809e-417c-836e-0d6811851ab0`.
JUnit start: `2026-09-12T08:29:20.037+00:00`; duration 61.234 seconds.

Executed with the owner-confirmed `.env` credentials exported silently, then:

```sh
dagger --progress plain -c 'ci | live env://SZAMLAZZ_AGENT_KEY final-hardening-20260912-1cbcf00-1 | export /tmp/opencode/final-hardening-live-report'
```

Compilation was reused; the live execution itself was fresh and logged all five scenarios.
No failure required a retry or operator settlement.

| Journey | Recorded documents / result |
|---|---|
| Worker ordinary Order | Proforma `D-CTEST-16`; invoice `E-CTEST-2026-43`; storno `E-CTEST-2026-44`; reissue `E-CTEST-2026-45`; cleanup reversal `E-CTEST-2026-46`. Same-key replay, fresh already-issued, consumption, Agent query, reversed result and stale expected-target conflict passed. |
| Worker EUR prepayment/final | Proforma `D-CTEST-17`; prepayment `E-CTEST-2026-47`; final `E-CTEST-2026-48`; cleanup `E-CTEST-2026-49` then `E-CTEST-2026-50`. References, caller-supplied deduction, rounding, observation, replay and dependency-ordered cleanup passed. |
| Direct paper invoice | `CTEST-2026-17`, reversed by `CTEST-2026-18`. Replacement `[100]` → `[200]`, additive `[50]`, read-back/balance, PDF, original fulfillment date and repeated storno identity passed. |
| Direct proforma | `D-CTEST-18` created, verified, deleted and verified absent. |
| Taxpayer | Read-only NAV smoke passed. |

Known created documents were consumed, deleted or reversed as their scenarios prescribe;
paired cleanup checks reported no outstanding uncertainty.

Trace: <https://dagger.cloud/sagikazarmark/traces/0e183242d9e6b14d4e53271a8d84fab3>

JUnit: `/tmp/opencode/final-hardening-live-report/junit.xml` (5 tests, 0 failures/errors).
Optional receipt, clearing and email investigations were not selected: the owner authorized
the five core journeys, and this pass did not alter their operation semantics.

## Release disposition

The focused stopping criterion is met: F1–F5 and the selected hardening gaps are closed,
documented integration limits have explicit dispositions, independent review is clear, and
offline, actual-Restate, portability and five-core live acceptance passed.

**Ready for version preparation and release of this candidate.** This is not a deployed
resolver/seller mapping check and does not replace per-deployment scope/account verification.
No tags, publication or account configuration changes were performed.
