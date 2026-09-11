# CI and historical regression review

**Implementation status:** the actionable findings below are fixed in the review-fixes
change based on `4394ed0`. Final Dagger checks passed: 785 ordinary tests plus
doctests, 39 actual-Restate tests, and the schema matrix. See the
[closure record](2026-09-11-review-fixes-closure.md) for the finding-by-finding
index, API migrations, independent follow-up review and verification evidence.
The original investigation below remains historical; its earlier disk-blocked
Dagger result is superseded by that execution.

Reviewed HEAD: `4394ed0977a0adf298a10e0acd9182d13cbf3c0c`.
Fixed point: `e41a9964e266088a4d22a4613108d97d81bab295` (`HEAD~20`).
Comparison: `git diff e41a9964e266088a4d22a4613108d97d81bab295...HEAD`.

Four parallel reviewers covered CI reproduction, worker write-safety/specification,
public API/standards, and historical review closure plus the other crates. The
parent review checked the reported source paths and reconciled attribution and
confidence. This is a recommendations-only review; retained changes are reports.

## Executive conclusion

The supplied `MARKER-KILL` 500s are **expected assertions**, showing that the
unresolved-write marker blocks queued mutations after kill. A subsequent test
dependency is missing from CI: **the test launches Python, but the e2e container
does not install it**. Removing Python locally reproduced the failing test and
the same server-log ending. This is a confirmed CI defect, but the original run's
panic/JUnit is needed to establish that it explains that particular failure and
its reported intermittency.

The prior review's two principal safety/API findings are confirmed. The reissue
finding now has an actual production Order/Restate reproduction, including marker
clearance and a second send. Both defects predate these 20 commits. Newly introduced
regressions include the missing Python dependency, build-dependent CLI monetary
input rejection, and operational date failures attributed to caller input.

## CI diagnosis

At `crates/restate-szamlazz/tests/e2e/unresolved.rs:520–535`, the test explicitly
requires 500 / `outcome_unknown`, no executed runs, and exactly one original send.
It then runs the migration inventory at line 543; the helper launches `python3`
at lines 608–617.

`.dagger/modules/ci/main.dang:83–91` adds the Restate binary to `self.compiled`
but no Python. Python at line 53 is installed in the separate schema-check branch.
The actual Dagger base-container probe returned:

```text
exec: "python3": executable file not found in $PATH
```

The focused reproduction with Python unavailable failed at `unresolved.rs:617`:

```text
migration inventory process: Os { code: 2, kind: NotFound, message: "No such file or directory" }
```

Restoring only Python's PATH made the test pass. With Python present, the test
passed **100/100** fresh-server executions and all **29** `unresolved::` tests
passed. Missing Python entered in **`28dcec1`**, which added the real migration
script invocation without provisioning its runtime in e2e.

**Recommendation:** install Python explicitly in the e2e container, validate the
prerequisite there, and retain the actual migration-script test. Do not weaken
marker assertions or add retries to hide a missing executable. Obtain the original
nextest FAIL/panic or JUnit failure text: the printed server-log tail is insufficient.

The full `dagger --progress plain check --no-generate ci:end-to-end` was attempted.
It selected 38 tests but failed in the first on Restate/RocksDB **No space left on
device**, leaving 37 unrun. That is a separate local environmental failure, not
proof of the original failure's cause. Rerun with adequate disk capacity after
the dependency fix.

Full reproduction commands, trace URLs and caveats:
[CI investigation](2026-09-11-ci-flake-investigation.md).

## Standards and public API

### S1 — High, direct Gateway consumers: unsafe retry contract

`crates/restate-szamlazz/src/gateway.rs:1200–1245` tells callers to re-execute
`Gateway::create` after `Unconfirmed`. Empty leading queries permit another send.
Two calls for one invisible corrective produced **two create POSTs and four
queries**. Correctives lack the ordinary order-number duplicate protection.

This public contract predates the review base. The supplied Order uses the
protected path and does not inherit this automatic retry behavior.

**Fix:** make the legacy path internal, or provide an explicit one-send boundary
with consumer-owned send permission and separate read-only reconciliation. Remove
retry-as-permission advice from the method, module and `Unconfirmed` documentation.

### S2 — P2, new: unified JSON features reject normal CLI amounts

`28dcec1` enables `serde_json/arbitrary_precision` in
`crates/restate-szamlazz/Cargo.toml:31`. Cargo feature unification affects sibling
Agent DTOs, whose Decimal fields use ordinary derived deserialization
(`crates/szamlazz-agent/src/ops/credit_entry.rs:20–31`, `src/item.rs:89–110`).

Actual CLI input containing numeric `amount: 12.34` sends `12.34` in a CLI-only
build but fails before HTTP when CLI and worker are built together with all
features: `invalid type: map, expected a Decimal type representing a fixed-point number`.
Numeric `1e-2` fails similarly. Existing string-valued fixtures miss this.

**Fix:** make public monetary input decoding feature-composition-safe and exact.
Keep the worker's exactness guarantee. Test runtime input under CLI-only and unified
workspace graphs; compile-only feature checks are insufficient.

### S3 — P2, existing: public Agent/CLI money can silently round

The same public input boundary loses exact numeric tokens through floating point
in a CLI-only graph, and ordinary Decimal string parsing can round too. Actual
loopback CLI output included:

| JSON amount | Amount sent |
|---|---|
| `0.1234567890123456789012345678` | `0.12345678901234568` |
| `9007199254740993.5` | `9007199254740994` |
| `"0.49999999999999999999999999999"` | `0.5000000000000000000000000000` |

The first two inputs fit Decimal exactly. The last does not and should be refused
by an exact-input contract. This predates the range; the worker-specific repair
does not cover the public Agent/CLI boundary. Address together with S2, including
string and numeric tokens and amount flags; enabling a compatible visitor alone
does not guarantee exactness.

### S4 — P2, new: vendor date defect becomes `invalid_input`

`crates/restate-szamlazz/src/service/storno.rs:75–92` copies the verified original's
fulfillment date and maps every outbound validation failure to caller input error.
A queried `0000-01-01` parses successfully but fails the outbound positive-year
rule. The caller cannot supply or repair that derived date.

Introduced by **`370ff2e`**, following date validation in `eec57fc`. A focused
parser/writer reproduction confirmed the invalid-year result; the HTTP 400
classification is source-traced, not separately reproduced through Restate.

**Fix:** report an unusable vendor fulfillment date as invoice-specific
`unavailable`, consistent with a missing date. Preserve `invalid_input` for
caller-owned defects such as invalid comment text. Test both attribution paths.

### S5 — Lower priority, existing: discovery schema is approximate

`crates/restate-szamlazz/src/identity.rs:649–655` emits character-count `maxLength`
while runtime validates UTF-8 bytes; its pattern also misses C1 controls. Twenty-one
`é` characters and `SZ\u00801` pass the emitted constraints but fail runtime.
This predates the range (`56e941e9`); `4394ed0` did not introduce it.

**Fix:** cover representable control restrictions and explicitly document the
additional runtime byte limit. Do not narrow legitimate Unicode numbers to ASCII
merely to simplify the schema. Add schema/runtime boundary examples.

## Spec and write safety

### W1 — High: old-number reissue reply clears the marker

`gateway.rs:1254–1258` accepts any numbered create success, even if its number is
the exact reversed document the caller asked to replace. `gateway/recovery.rs:193–196`
treats it as settled; `service/recovery.rs:490–507` bypasses reconciliation and
clears the marker. Other evidence paths correctly exclude the old number.

A real Restate 1.7.8 reproduction ran two production Order invocations expecting
reversed `SZ-OLD`, with the same old holder visible and each successful reply
naming `SZ-OLD`. Both returned `issued(SZ-OLD)`, both cleared state, neither
reconciled, and **two creates were sent**. This is contradictory synthetic vendor
evidence, not a claim that such replies have been observed live.

The defect already exists at the base. It violates the conclusive-settlement and
old-target-exclusion rules in `docs/design/order-write-protocol.md:33,73–75` and
ADR 0013.

**Fix:** treat an old-number reissue acknowledgement as unresolved at the shared
send boundary. Preserve the marker until matching replacement evidence or authorized
settlement arrives. The regression must exercise the production protected handler,
marker retention, subsequent blocked mutation, and send count; include ordinary
success and numbered notification-failure success.

### W2 — Policy decision required: same-number storno settlement

The protected protocol requires a distinct reversal and says contradictory
identity retains uncertainty (`order-write-protocol.md:89–91`). The broader
design explicitly maps a same-number echo to `NotStornoable`, based on observed
proforma/delivery-note behavior (`restate-szamlazz.md:551–561`).

`gateway.rs:1819–1821` and `gateway/recovery.rs:224–230` extend that settled result
to protected storno, which has already verified a stornoable invoice. A synthetic
same-number negative-total reply caused two fresh Order calls to return
`rejected/not_stornoable`, clear markers, and send twice.

**Recommendation:** resolve the conflicting rules explicitly; prefer retaining
uncertainty for a contradictory reply to a verified invoice. This is inherited
policy, not a newly introduced implementation deviation or demonstrated duplicate
storno-document bug. Vendor storno idempotence remains relevant.

Full requirements, production traces, origin attribution and actual-Restate
reproductions: [independent spec review](2026-09-11-independent-spec-e41a996-4394ed0.md).

## Historical conclusions and documentation

The [history audit](2026-09-11-history-audit.md) maps review families from September
6–11 and lists all 20 commits chronologically. Important conclusions:

- Most repeated Agent findings are genuinely closed: optional identity/facts,
  explicit paid false, taxpayer diagnostics, malformed verdicts, namespace row
  handling, header retention and code-56 identity precedence.
- Intermediate fixes did create regressions: namespace normalization needed quote
  preservation; stronger mutation identities over-constrained recovery evidence;
  numberless storno support initially crossed a retry boundary. The records and
  current checks show these particular intermediate problems were repaired.
- **Current fix-induced regressions:** missing e2e Python and JSON feature effects
  in `28dcec1`; storno date attribution in `370ff2e` after `eec57fc`.
- IPN leniency [#149](https://github.com/sagikazarmark/szamlazz-rs/issues/149) and
  delivery/trust guidance [#75](https://github.com/sagikazarmark/szamlazz-rs/issues/75)
  remain open backlog. IPN was unchanged in this range; do not call these new
  regressions or infer they were closed by Adatkapcsolat repairs.
- `CONTEXT.md` still contains standalone claims that Order has no state, marker
  implementation is pending, and queries/renewed keys authorize resend. Amendments
  supersede some of them, but current instructions should be rewritten consistently.
- CLI guidance should describe numberless storno acknowledgements and the changed
  meaning of the existing `"paid": false` sample.

Add a commit-linked current finding/closure index. Keep historical reports as
historical evidence, with supersession links; a previous green or approved verdict
does not certify subsequent edits. Avoid counting the same issue in each report
as a separate outstanding defect.

## Verification and recommended execution plan

Executed by the reviewers: 334 ordinary worker checks; 29 focused API checks;
423 Agent/CLI/receiver tests and doctests; 29 unresolved real-Restate tests plus
additional overlapping selected scenarios; 100 focused fresh-server iterations;
temporary public-Gateway, protected-Order and actual-CLI reproductions. Counts
overlap and are not a summed unique-test total. Offline schema checks covered
430 requests / 860 validations (790 valid, 70 specifically expected source conflicts),
seven negative controls and eight runner tests. No vendor-live execution was made.
Temporary probes were removed. `git diff --check` passed. The full Dagger check
did not pass because of the separate storage failure described above.

Recommended work packages:

1. **Restore CI prerequisites:** provision Python; obtain the original panic;
   rerun focused and full container checks with sufficient disk.
2. **Correct settlement/API boundaries before release:** W1 and S1, with protected
   regressions; adjudicate W2's contradictory identity policy.
3. **Repair public money inputs:** S2/S3 together, verified under both dependency
   graphs and with exactness/refusal controls.
4. **Correct attribution and discovery:** S4 and S5, retaining caller/vendor
   ownership distinctions.
5. **Close documentation accurately:** current glossary, CLI migration notes and
   a finding index; track IPN backlog separately.

**Axis summary:** Standards/API has five concrete findings, led by the unsafe
public retry contract; Spec has one confirmed settlement defect and one policy
conflict, led by old-number reissue marker clearance. CI has a separately confirmed
missing-runtime dependency; original-run attribution remains conditional on its
failure report.
