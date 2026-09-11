# CI end-to-end reliability investigation

Reviewed HEAD: `4394ed0977a0adf298a10e0acd9182d13cbf3c0c`.
Comparison base: `e41a9964e266088a4d22a4613108d97d81bab295`.
Scope: e2e reliability, `tests/e2e/unresolved.rs`, harness, nextest and Dagger. Recommendations only; production safety and broader history are separate audits. No subagent tool was available in this session. Existing untracked review documents were preserved. No production or test source was changed and no secrets or `.env` files were read.

## Conclusion

**Confirmed CI prerequisite defect: the e2e container does not contain `python3`, but the MARKER-KILL test now requires it.** This produces a panic immediately after the three intentionally refused mutations. A controlled execution reproduced both the panic and the server-log suffix described by the user. Restoring Python alone made the exact test pass.

**Not confirmed as the cause of the user's intermittent failure.** The original assertion/JUnit output was unavailable. Missing Python is deterministic for a fixed container, rather than a proven timing race. Differences in environment, source revision, selection or cached check results would need evidence to explain apparent intermittency.

The unmodified focused test passed a complete 100-iteration stress run, and all 29 `unresolved::` tests passed locally. The actual Dagger check also ran, but failed earlier on a separate, explicit disk-exhaustion error before reaching MARKER-KILL.

## 1. Expected 500 logs are not failed assertions

`crates/restate-szamlazz/tests/e2e/unresolved.rs`:

- Lines 476–499 start `MARKER-KILL`, wait for the first send and observe the marker.
- Lines 505–518 queue `create_invoice`, `create_prepayment` and `correct_invoice`.
- Line 519 kills the original invocation.
- Lines 520–533 **require** each queued request to return HTTP 500, a structured `TerminalCode::OutcomeUnknown`, and no executed runs.
- Line 535 requires exactly one send.
- Lines 539–549 then execute the migration inventory and require a completed owner, no unfinished invocations, and retained Order state.

The expected fault text is:

```text
an earlier Order write remains unresolved; retain its invocation identity and obtain operator reconciliation; no new mutation is authorized
```

This matches `docs/design/order-write-protocol.md:8–10,30–37,129–132`: a marker survives kill and blocks later mutations. Changing these 500 expectations or clearing the marker to make the logs disappear would undermine the behavior this test verifies.

The server log does not contain the Rust test process's panic. A tail of that server log can therefore show only expected application faults even when a subsequent test assertion or subprocess launch fails.

## 2. Confirmed finding: provision Python in the e2e container

**Priority: P1 (CI blocker). Confidence: high for the defect; medium for attribution to the user's excerpt.**

`unresolved.rs:605–620` uses `spawn_blocking` to launch `Command::new("python3")`, running the real `scripts/check-order-migration.py`. The spawn is unconditionally expected to succeed at line 617. The helper is called at lines 543, 594 and 601.

The Dagger e2e branch starts from `self.compiled` and adds only the Restate binary and environment variables (`.dagger/modules/ci/main.dang:83–91`). `compiled` adds nextest and compiles Cargo targets (lines 118–123). Python is installed in `schemas` (lines 51–56), which is a different container branch. Installing it there does not provision the e2e branch.

The configured base is `rust:1.98-slim-trixie` (`dagger.toml:3–14`); its pinned image digest is recorded in `dagger.lock:7`.

### Direct verification of the real container

Executed:

```sh
dagger --progress plain call rust container with-exec --args python3,--version combined-output
```

Result:

```text
exec: "python3": executable file not found in $PATH
Error: exit code: 1
```

Trace: <https://dagger.cloud/sagikazarmark/traces/95e016f4fd0d242a9e9d09a03a9609e9>.

### Exact test reproduction with Python unavailable

The test binary was first built from pinned HEAD using the focused Cargo command below. Executed:

```sh
CI=true RESTATE_SERVER_BIN=/tmp/opencode/review-restate-server PATH=/nonexistent \
  target/debug/deps/e2e-964fbcc2c47547ee \
  unresolved::e2e_unresolved_kill_preserves_marker_and_recovery_requires_evidence \
  --ignored --exact
```

Result: one test failed in 2.36 seconds, exit 101:

```text
thread 'tokio-rt-worker' panicked at crates/restate-szamlazz/tests/e2e/unresolved.rs:617:14:
migration inventory process: Os { code: 2, kind: NotFound, message: "No such file or directory" }

thread 'unresolved::e2e_unresolved_kill_preserves_marker_and_recovery_requires_evidence'
panicked at crates/restate-szamlazz/tests/e2e/unresolved.rs:620:6:
inventory task: JoinError::Panic(...)
```

The retained server log ended with the expected `outcome_unknown` responses for:

```text
/restate/call/Szamlazz.Order/MARKER-KILL/create_invoice
/restate/call/Szamlazz.Order/MARKER-KILL/create_prepayment
/restate/call/Szamlazz.Order/MARKER-KILL/correct_invoice
```

Those were at log lines 886–911 for this reproduction. The actual failure was launching Python, after all those checks had succeeded. After capturing this evidence, this investigation's three retained local MARKER-KILL diagnostic directories (the interrupted stress iteration and two deliberate missing-Python failures) were removed. Other runs' diagnostic directories and the pre-existing Restate binaries were preserved.

A second paired experiment ran the same binary twice, changing only `PATH`: `/nonexistent` failed identically (1.78 s); `os.path.dirname(sys.executable)` from the invoking Python process passed (2.82 s). This removes Cargo/toolchain setup and unrelated command lookup from the minimized failure.

### Origin and recommendation

`git blame` assigns the inventory additions and helper to **`28dcec1456cc08d50089ed8f9c9d15f877c7c3d2`**, `fix(restate-szamlazz)!: enforce exact inputs and safe scope migration`. That commit adds 39 lines to this test and the executable inventory script, with no corresponding e2e runtime installation. The original MARKER-KILL scenario and its expected faults already exist at base **`e41a9964`**.

Recommended fix: install `python3` explicitly on the e2e container branch, or its shared runtime dependency layer. Keep the test running the actual operational script. Validate `python3 --version` in that branch before starting the expensive suite, then rerun the focused test and full Dagger check. Test retries or longer timeouts cannot fix a missing executable.

## 3. Commands and stress results

Dedicated glob searches found existing Restate binaries under `/tmp/opencode`; `/tmp/opencode/review-restate-server --version` returned `restate-server 1.7.8`. These pre-existing binaries were not modified.

### Correct focused command

```sh
CI=true RESTATE_SERVER_BIN=/tmp/opencode/review-restate-server \
  cargo test -p restate-szamlazz --all-features --locked --test e2e \
  unresolved::e2e_unresolved_kill_preserves_marker_and_recovery_requires_evidence \
  -- --ignored --exact
```

Result: **1 passed**, 43 filtered out, 2.66 seconds. An initial invocation omitted the `unresolved::` prefix while using `--exact` and selected zero tests; that run is explicitly not verification.

The exact test checks the fault envelope, marker protection, send count, migration inventory and successful evidence recovery, so it is red-capable for failures in this scenario.

### Sequential stress

Executed this Python driver (no temporary driver file):

```python
import os, subprocess, sys, time
env = dict(os.environ, CI="true", RESTATE_SERVER_BIN="/tmp/opencode/review-restate-server")
cmd = ["target/debug/deps/e2e-964fbcc2c47547ee",
       "unresolved::e2e_unresolved_kill_preserves_marker_and_recovery_requires_evidence",
       "--ignored", "--exact"]
start = time.monotonic()
for i in range(100):
    t = time.monotonic()
    p = subprocess.run(cmd, env=env, stdout=subprocess.PIPE,
                       stderr=subprocess.STDOUT, text=True, timeout=100)
    print(f"iteration {i+1}: exit={p.returncode} duration={time.monotonic()-t:.2f}s", flush=True)
    if p.returncode:
        print(p.stdout)
        sys.exit(p.returncode)
print(f"100/100 passed in {time.monotonic()-start:.2f}s")
```

Result: **100/100 passed in 262.08 seconds**, individual executions 2.02–4.50 seconds. Each execution launches fresh Restate storage.

An earlier 30-iteration Cargo driver completed 23 successful iterations before its outer tool timeout of 120 seconds interrupted the loop. One completed iteration took approximately 58 seconds; it still passed. This incomplete run is not reported as 30 passes or as an application failure. No specific cause of that latency was established.

### Wider unresolved suite

```sh
CI=true RESTATE_SERVER_BIN=/tmp/opencode/review-restate-server \
  cargo test -p restate-szamlazz --all-features --locked --test e2e \
  unresolved:: -- --ignored --test-threads=1
```

Result: **29 passed**, 15 filtered out, 81.52 seconds. Includes interruption, exhaustion, recovery, document-identity and release-hardening scenarios.

This host Cargo execution is narrower than the full CI selection and does not reproduce the CI container's missing dependencies. Passing stress does not rule out a low-frequency race elsewhere.

## 4. Actual CI command and independent environmental failure

The check's effective command is:

```sh
CI=true RESTATE_SERVER_BIN=/usr/local/bin/restate-server NEXTEST_PROFILE=e2e \
  cargo nextest run --workspace --all-features --locked --run-ignored only
```

`.config/nextest.toml:13–19` selects the worker's ignored e2e tests in both the library and e2e binary, uses **one test thread**, **zero retries**, and a 60-second slow interval with termination after 20 intervals. Inter-test nextest concurrency is therefore not a supported explanation for MARKER-KILL. Individual tests can still run concurrent operations internally.

Executed the actual check:

```sh
dagger --progress plain check --no-generate ci:end-to-end
```

It compiled all workspace/all-feature test targets, selected **38 tests across 2 binaries**, then failed the first test:

```text
service::prologue::logging_tests::e2e_replay_filter_keeps_fresh_operation_logs_and_correlation
restate-server exited ... terminating_signal: 6
rocksdb error: ... No space left on device ... OPTIONS-000018.dbtmp
Summary ... 1/38 tests run: 0 passed, 1 failed
warning: 37/38 tests were not run due to test failure
Error: exit code: 141
```

This is a **different observed failure**, not reproduction of the user's MARKER-KILL failure. Do not interpret outer exit 141 as the missing-Python test's exit code. The output showed the server storage failure explicitly; the reason for the outer 141 was not diagnosed.

Trace: <https://dagger.cloud/sagikazarmark/traces/b7da3fc6fac72279f338f980cdfbdd19>.
The tool preserved output under `/home/laborant/.local/share/opencode/tool-output/tool_0924da97a001r8GwKyr7wr07MV`.

After this run, `df -h / /tmp /var/lib/docker` showed a 79-GiB root filesystem at 96% usage with 3.6 GiB available; a subsequent container probe showed 97% usage and 2.3 GiB available. Inodes were only 20% used. These are post-failure observations, not a measurement of free space at the instant RocksDB failed. Existing caches and other agents' diagnostics were not deleted to force a full rerun.

Recommendation: rerun the full check on a runner with adequate disk capacity, recording capacity before compilation and before launching Restate. Preserve the test panic and server diagnostics separately. Do not conflate runner storage failure with unresolved-write safety.

## 5. CI log access and attribution boundary

Executed:

```sh
gh run list --commit 4394ed0977a0adf298a10e0acd9182d13cbf3c0c --limit 20 \
  --json databaseId,name,status,conclusion,url,headSha
gh api repos/sagikazarmark/szamlazz-rs/commits/4394ed0977a0adf298a10e0acd9182d13cbf3c0c/check-runs \
  --jq '.check_runs[] | {name,status,conclusion,details_url}'
gh run list --workflow dagger.yaml --limit 10 \
  --json databaseId,name,status,conclusion,url,headSha
gh run view 31589392494 --log-failed
```

Pinned HEAD exposed only successful Dependabot and Scorecard checks. The Dagger workflow query returned four older August runs. The newest of those, `31589392494`, failed downloading Dagger with HTTP 403 on 2026-08-12; it never ran this suite and is unrelated. No original September CI assertion or JUnit artifact was obtained through GitHub.

The Dagger failure wrapper prints JUnit when found, then the last 300 lines of every retained server log (`.dagger/modules/ci/main.dang:99–111`). The user excerpt may therefore be the final server-log tail rather than the failure report. Obtain the **first nextest FAIL section, its stderr/panic, JUnit failure text, actual commit and runner/container details** before declaring the original cause confirmed.

## Recommended next actions

1. Provision Python explicitly for `ci:end-to-end`; keep the real migration-script acceptance test.
2. Run the corrected focused command in that exact container, then the full 38-test nextest selection on a runner with sufficient storage.
3. Compare the original failed-run panic with `unresolved.rs:617/620`. If it matches, missing Python explains that run; investigate environment/revision differences separately for apparent intermittency.
4. If it instead fails the inventory assertions (`544–549`, `595`, `601`), capture subprocess status/stdout/stderr and actual unfinished-invocation rows before changing synchronization. No such inventory race was reproduced here.
5. Preserve the expected 500 protection assertions and the one-send invariant. No production behavior change is recommended from this reliability evidence.
