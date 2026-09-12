# Final-release code, history and review-evidence audit

Reviewed candidate: `1cbcf004bfa82b9d479424859a90c964589bedd2`.
Historical baseline: `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81` (`HEAD~20`).
Comparison: `git diff 2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81...HEAD`.
Date: 2026-09-12. Status: review complete; findings have not been implemented.

## Release assessment

**Conditional readiness, not unconditional release approval.** No new P0/P1 financial-safety
defect or architectural reversal was established in the protected Order protocol. Current
ordinary tests, monetary feature graphs, schemas, lints and feature checks pass. There are
confirmed medium-priority defects and test-portability problems, plus known integration
limitations that deserve an explicit release disposition.

The current candidate still needs fresh actual-Restate acceptance: this review's execution
was blocked before scenarios ran by Dagger disk exhaustion. The previous closure records
39 passing actual-Restate/mocked-vendor tests, but that is attributed historical evidence.
Candidate-specific vendor-live acceptance required by `docs/testing.md:190–196` was not run.

The answer to “did repairs turn anything wrong?” is **yes, at intermediate commits**.
The history contains reproducible feature-unification, recovery-identity, fault-attribution
and CI regressions. Their material fixes are present at HEAD. Passing an earlier review did
not establish that the next repair preserved every integration boundary.

## Scope and method

- Six primary subagent reviews: Standards/architecture; worker Spec/correctness;
  Agent/CLI behavior and money; receiver protocols; historical closure; release/test tooling.
- Two further independent adjudications: monetary/identity findings, and package/timezone
  reproductions. Findings were checked against current documented decisions before promotion.
- Exactly 20 commits, 279 changed files. Current production code was also reviewed outside
  changed hunks, so findings explicitly distinguish pre-existing defects from regressions.
- Historical reviewer inventoried 142 review documents, including the nine existing untracked
  files: eight `77d53c5` Agent reports and `worker-release-4394ed0.md`. Consolidated reviews,
  adjudications and closures were prioritized; older raw reports and detailed field inventories
  were sampled. This is not a claim that every line of every raw review was reread.
- Sources: `CONTEXT.md`, crate contracts, ADRs, design/runbooks, retained vendor schemas and
  research. Relevant GitHub issues were consulted, including #218, #216 and #196.
- No production source changes, vendor-live calls, credential reads, commits or releases.
  Local reproductions used scratch projects under `/tmp/opencode`.

P2 below means a concrete medium-priority defect, not an automatic financial release blocker.
P3 means a lower-priority improvement or explicitly limited supported behavior.

## Standards

**Zero documented-standard violations; one non-blocking possible duplication smell.**

`crates/restate-szamlazz/src/service/create.rs:1034–1043` independently constructs
`WriteOperation::Create { kind, expected_number, corrected_number }`, duplicating the
interpretation in `crates/restate-szamlazz/src/gateway.rs:403–414`
(`CreateStepRequest::operation()`). Both agree today. ADR 0014's shared request projection
and intent interpretation favor deriving the durable marker and immediate evidence intent
from the same method, so a future corrective/reissue field cannot diverge between them.

This is a heuristic maintenance concern, not a current contract breach. The accepted expert
Gateway consumer is explicit in ADR 0014; public orchestration is not speculative generality
under that decision. Separate XML models, plain request structs and immutable-deployment
journal rules are likewise intentional.

The `type_name` raw-JSON dispatch remains fragile integration machinery. Its present
limitations are documented; the integration cases below make their practical cost explicit.
No silent monetary corruption was found in the direct decoder.

**Axis total: 0 hard violations, 1 possible smell; worst issue is non-blocking intent duplication.**

## Spec

The independent worker Spec review found **one P2 operational contract violation**, F1 below.
No P0/P1 protected-write defect was established. Marker commitment precedes acknowledged
arming; completed-arm replay cannot regenerate send permission; interrupted writes reconcile
read-only; settlement is recorded before clearance. Issuance evidence excludes old reissue
targets and checks corrective bases. Protected reversal evidence checks both records, and
document queries cannot settle deletion.

Accepted boundaries were not reported as violations: resolver-owned account mapping without
an account pin, external writers racing queries, caller-owned Gateway durability, unmanaged
storno's distinct policy, and operator evidence being an assertion rather than vendor proof.

### F1 — P2: unmanaged storno can lose the required credential warning

**Pre-existing in the baseline; high-confidence local HTTP reproduction.**

Locations:

- `crates/restate-szamlazz/src/gateway.rs:2026–2048`: `verify_storno_reply` converts the
  query error to text without emitting the credential warning.
- `gateway.rs:2061–2067`: successful external-id fallback discards that diagnostic.
- `src/service/storno.rs:657–662`: the service accepts the successful fallback outcome.

The design promises that every occurrence of credential codes 3/135/136/164 is logged at
`warn` (`docs/design/restate-szamlazz.md:816–824`; worker README:724–731).

Reproduced sequence for unmanaged storno of `SZ-1`:

1. Leading external-id query answers code 7.
2. Storno acknowledgement names `SS-1`, with gross omitted.
3. Verification of `SS-1` answers credential code 135.
4. External-id fallback finds `SS-1`, type SS, referencing `SZ-1`.

Observed `Ok(AlreadyReversed { storno_number: "SS-1" })`, with no credential warning.
The successful outcome contains neither the failure nor a signal for the promised monitoring.

Emit the shared sanitized warning when the typed credential answer is received, before
fallback/text wrapping can discard it. Preserve current settlement and uncertainty semantics.
The protected branch already follows this pattern at `gateway.rs:1952–1958`.

**Axis total: 1 finding; worst issue is the P2 missing operational warning.**

## Additional current-code findings

### F2 — P2: Adatkapcsolat accepts non-lexical monetary tokens as real numbers

**Pre-existing; reproduced through `Document::parse_strict`.**

`crates/szamlazz-adatkapcsolat/src/document.rs:1799–1810` delegates to `FromStr`.
The bank amount uses that helper at `document.rs:1434–1441`; Decimal accepts underscores,
although the vendor XSD's `double` does not.

```xml
<banktranz xmlns="http://www.szamlazz.hu/banktranz">
  <id>7</id><bankszamla>123</bankszamla><erteknap>2026-09-12</erteknap>
  <irany>BE</irany><technikai>false</technikai>
  <osszeg>1__2</osszeg><devizanem>HUF</devizanem>
</banktranz>
```

`parse_strict` succeeds with amount `Some(12)`; `1_` becomes `Some(1)`.
This conflicts with the stated lexical-type shape rule in CONTEXT and the receiver README:66.
The new complete-XML validator does not inspect domain numeric grammar.

Use an explicit wire-number lexical check before conversion, preserving the documented
shape/content distinction and raw XML. Do not replace this with general XSD-required-field
validation in the default parser.

### F3 — P2, conditional: a long IPN amount aborts an unoptimized receiver

**Pre-existing; debug reproduction confirmed, optimized reproduction did not fail.**

`crates/szamlazz-ipn/src/lib.rs:306–320` passes unrestricted amount text to Decimal's
recursive digit parser. On a standard 2 MiB thread stack, a 10,000-character zero amount
caused stack overflow and process abort in a debug build:

```rust
std::thread::Builder::new()
    .stack_size(2 * 1024 * 1024)
    .spawn(|| {
        let amount = "0".repeat(10_000);
        szamlazz_ipn::PaymentNotification::from_pairs([
            ("szlahu_szamlaszam", "E"),
            ("szlahu_bruttovegosszeg", amount.as_str()),
        ])
    })
    .unwrap()
    .join()
    .unwrap();
```

The same problem reproduces through the baseline parser and dependency. The input is below
the usual HTTP body cap. This is not an `IpnParseError` and cannot be recovered by handling
the returned `Result`. No optimized-production failure is claimed.

Bound or normalize numeric work without recursion, retaining raw text and treating unreadable
content as unknown. This finding concerns parser robustness, not payment authentication.

### F4 — P2: package-only Linux live tests lack their timezone backend

**Introduced with the live-test work in the reviewed range; independently reproduced.**

Both dev-dependencies enable only `tz-system` and `tzdb-bundle-platform`:

- `crates/szamlazz-agent/Cargo.toml:38`
- `crates/restate-szamlazz/Cargo.toml:42`

Workspace Jiff defaults are disabled (`Cargo.toml:28`). Native Linux gets no database from
the platform bundle, and `tz-system` does not enable the named-zone lookup backend.
`tests/live_support/mod.rs:18–21` calls `TimeZone::get("Europe/Budapest").expect(...)`.

An isolated Jiff 0.2.35 project using the exact declared features failed despite verifying an
installed `/usr/share/zoneinfo/Europe/Budapest` file. Adding only `jiff/tzdb-zoneinfo` passed.
Package-specific feature trees confirmed the omission. Workspace feature unification masks it.

The workaround is documented at `docs/testing.md:227–232`, but the advertised package-only
commands immediately above omit it. Declare the required test backend or correct every
package-local command. This does not affect production worker timezone-independent logic.

### F5 — P2: the packaged worker's live-test target includes a missing sibling file

**Introduced by `1b08368`; confirmed by Cargo's package inventory and path resolution.**

`crates/restate-szamlazz/tests/live.rs:2–3` unconditionally includes:

```rust
#[path = "../../szamlazz-agent/tests/live_support/mod.rs"]
mod live_support;
```

The worker package contains `tests/live.rs`, but cannot contain the sibling path outside its
root. Ignored tests still compile. An isolated package's `cargo test --test live --no-run`
therefore cannot compile this target. The full extracted-package build was not executed;
the inventory/path evidence establishes the absent module.

This is a packaged-test portability defect, not a claim that library consumers or normal
library-only package verification fail. Existing omitted upstream fixtures skip deliberately;
they are different from this unconditional compile-time dependency. Make support package-local
or explicitly exclude workspace-only targets from the published test surface.

## Compatibility and hardening decisions, not newly established blockers

The Agent reviewer initially proposed stronger classifications for the following cases.
Independent adjudication reproduced them but found current documentation or historical
decisions already limit the relevant contract. They are retained here rather than silently
discarded or counted as new hard violations.

| Case | Evidence and disposition |
|---|---|
| Axum JSON rejects numeric monetary input | `number/de.rs:27–35,93–109` excludes `serde_path_to_error`, used by Axum 0.8.9. `Json::<CreditEntry>::from_bytes` refuses `12.34`/`1e-2`; direct JSON accepts both. Integer `12` and string `"12.34"` work. This is a concrete integration limitation of the `1cbcf00` allowlist, but `src/lib.rs:61–65` expressly limits other wrappers. **P3 support/documentation follow-up**: name Axum explicitly or support its raw-token-preserving wrapper. |
| Wrapped storno depends on member order | Under `serde_ignored`, `{"response":{"net_total":12.34},"state":"unnumbered"}` fails while tag-first succeeds. Direct JSON succeeds both ways. `ops/storno/de.rs:38–45` deliberately uses derived decoding for wrappers, which buffers content-before-tag. The latest closure explicitly excludes a whole-envelope bypass. **P3 clarification**; strings avoid the limitation. |
| Blank concrete-document number | Receipt `ops/receipt.rs:780` and queried invoice `ops/query_xml.rs:739` accept empty/whitespace strings. The receipt CLI can report exit 0 with an empty number. Existing transparent wrappers are infallible strings, a numeric document id remains, and earlier reviews classify this as optional hardening. **P3 boundary decision**: recommend nonblank validation for concrete-document results, but do not confuse it with deliberate optional identity on credit/PDF acknowledgements. |
| Empty fixed Adatkapcsolat key | Historical B-06 remains: `axum.rs:253–257,346–352,397–403` permits `router("", handler)` plus an explicitly empty header. Missing headers are refused and nonempty configured keys are unaffected. **Pre-existing conditional configuration hardening**; reject empty fixed-key configuration. |
| IPN form notation narrowed | `lib.rs:309` now reads `1e2`, `1E-2` and excessive redundant scale as `None`, although the baseline could read them exactly. Raw text survives. The JSON adapter accepts representable exponents. The form contract explicitly selects `from_str_exact`, so this is a documented compatibility boundary, not established silent corruption. |
| Adatkapcsolat precision | Its longstanding Decimal `FromStr` path can round overprecision. Stable string serialization preserves the parsed value, not necessarily every original digit. Raw XML remains. The latest serialization closure does not establish new exact XML-input semantics. |

The independent monetary probes exercised thousands of signed exponent spellings, equivalent
representations, representability boundaries, RON and binary round trips. No direct-decoder
silent rounding or incorrect acceptance was found in those probes.

## Test and release-enforcement gaps

These are coverage/process findings, not claims of a reproduced production mutation defect.

1. **Unrestricted test-marker cleanup can hide an unexpected retained marker.**
   `crates/restate-szamlazz/tests/e2e/harness/mod.rs:312–346` inventories every marker and
   simulates audited positive settlement before migration. It asserts marker type/scope, not
   an expected scenario inventory. A settled scenario accidentally retaining state can be
   cleaned up rather than fail that phase. Assert the expected unresolved-key set before
   cleanup and absent state for settled scenarios. Existing targeted clearance tests remain valid.
2. **Live cleanup verifies the reversal document but not the fresh original.**
   `crates/szamlazz-agent/tests/live_support/mod.rs:249–265,289–315` accepts an SS referencing
   the original and continues linked cleanup. Pair it with a fresh original-reversed observation
   before proceeding from final to prepayment cleanup. No actual vendor inconsistency was observed.
3. **Required regression commands remain outside automatic checks.**
   `.dagger/modules/ci/main.dang:39–58,83–93` does not run
   `scripts/check-money-features.sh` or `scripts/test-order-migration.py`. The latter's real
   inventory path is exercised in e2e, but its standalone malformed/redirect controls are separate.
   The former tests dependency graphs that cargo-hack's package features do not cover.
   Both commands passed in this review; wiring is the remaining enforcement issue.
4. **Tag publication is not gated by acceptance in checked-in workflows.**
   `.github/workflows/release.yml:215–221,269–279` depends on artifact jobs, while
   `.github/workflows/dagger.yaml:16–20` triggers on PR/main, not release tags. External rules
   or the operator may gate publication; the repository workflow itself does not establish that.
5. **A local e2e success can mean a skip.** Without a server and without `CI`, the explicitly
   documented developer server gate returns normally. Dagger sets `CI=true`; worker live fails
   on missing prerequisites. Use the fail-closed path for acceptance and do not reinterpret
   this intentional convenience as a new live-suite regression.
6. **Remaining release evidence:** fresh candidate live journeys, actual-Restate completion,
   actual Rust 1.92 compilation, advertised target checks and isolated packaging. Version 0.3.0
   with unreleased 0.4 migration prose is staging, not independently a bug; prepare versions at release.

## Historical regression and closure analysis

### Repairs that temporarily regressed behavior

| Repair sequence | What went wrong | HEAD disposition |
|---|---|---|
| `28dcec1` → `e5fc606` → `1cbcf00` | Worker exact money changed sibling CLI behavior through feature unification; JSON-specific fixes then broke alternate-format/feature compositions. | Material defects repaired. Executable CLI, RON, SDK byte replay and eight dependency graphs now cover the exposed boundaries. Wrapper limitations above remain explicit. |
| `eec57fc` + `370ff2e` → `e5fc606` | Derived unusable vendor fulfillment dates were attributed to invalid caller input. | Shared storno intent now returns `unavailable`; caller comment errors remain `invalid_input`. |
| `5c6d5ea` → `bed1d24` | Stronger identity validation over-restricted vendor recovery evidence to mutation-number rules. | Separate evidence type preserves wide/non-normalized vendor numbers; mutation targets stay bounded. |
| `835a370` implementation | An intermediate optional-storno change could re-send on an unnumbered acknowledgement. | Caught before that commit's closure; completed unnumbered outcomes preserve uncertainty. Unmanaged interrupted execution retains its separately documented boundary. |
| `e5fc606` → `1cbcf00` | Consumed public create permission alone did not make collision, blocked verification or old-target observations valid post-send settlement. | Shared intent-aware reconciliation fixes those classifications; caller durability remains required. |
| `28dcec1` → `e5fc606` | New migration inventory script needed Python missing from the e2e container branch. | Python installation is present. This review hit disk exhaustion during that installation, a different execution failure. |

### Material finding-to-current-evidence matrix

Historical paths below are under `docs/review/`; source/test paths are within the named crate.

| Historical report/findings | Current disposition and evidence |
|---|---|
| `2026-09-10-release-readiness.md` R1–R3/R6–R8 | Repaired, chiefly `028dfcd`: worker `service/recovery.rs:453–508`, paired reversal/deletion evidence in `gateway/recovery.rs`, and `tests/e2e/recovery_evidence.rs`. |
| Same report R4/R5 | Repaired in `837dad0`: receipt archives use record id (`archive.rs:240`, `tests/archive.rs:339`); CLI derives original appearance (`tests/boundary.rs:269`). |
| `worker-identity-hardening`, `worker-release-acceptance` (September 11) | `5c6d5ea` repaired identity trust, `bed1d24` repaired evidence over-restriction; `tests/recovery_contract.rs:18–60` and `tests/gateway/recovery.rs:229–283`. |
| `worker-release-hardening` / `worker-release-readiness` | `10f00b3`/`370ff2e` repaired corrective evidence, read-fault ordering, pre-arm checks and recovery controls. `tests/e2e/release_hardening.rs` retains ingress, stopped-read and corrective-stage cases. |
| `final-project-370ff2e` S1/S3/P1/P2 | `28dcec1` repaired migration inventory, corrective-base validation and tracing; `scripts/check-order-migration.py`, worker e2e and receiver tracing controls remain. Money S2 needed subsequent sibling-boundary fixes. |
| `final-project-28dcec1` S1/S2 and recovery follow-ups | `99762c1`: execution-local credential validation, interrupted registration plus malformed rotation, unreadable state and pinned-account recovery. See `service/prologue.rs:469–493`, `e2e/agent_writes.rs`, `e2e/recovery_boundaries.rs`. |
| `release-readiness-4488f37` S1–S3/P1/P2 | `4394ed0`: Account validation, typed mutation ids, executable README intent and marker-order constraints. See `account_validation.rs`, `expected_document.rs`, `recovery_contract.rs`. |
| Untracked `worker-release-4394ed0` S1/P1; independent spec F1/F2/C1; consolidated W1/W2 | `e5fc606`, completed by `1cbcf00`: public permission and shared reconciliation; `tests/e2e/contradictory_replies.rs` preserves markers/one-send behavior for old-number replies. |
| `2026-09-11-agent-api.md` F1–F4 | `9b78546`: legacy credential redaction, response identity/refusal precedence; `credentials.rs`, `ops/envelope.rs`, response-header/namespace tests. |
| Agent `837dad0` F1/F2 | `61c334f`/`835a370`: explicit credit clearing and paid tri-state intent; accidental empty registration remains separately refused. |
| Agent `61c334f` F1 and schema gap | `eec57fc`: request-date refusal and source-labelled schema validation; required runner checks successful coverage and explicit conflicts. |
| Agent `eec57fc` H-01/M-01 | `77d53c5`: optional invoice facts and malformed required verdicts; `tests/response_booleans.rs`. |
| Untracked Agent `77d53c5` F1–F3/Q1; Agent `28dcec1` F1–F6 | `835a370`: numberless acknowledgements, optional validity/metadata, code provenance; `numberless_responses.rs`, `taxpayer_diagnostics.rs`. Blank concrete numbers remain the older permissive boundary, not one of those fixes. |
| September 6 B-09–B-12 / September 11 history H3 | `e5fc606` repaired IPN optional/raw content and guidance. Genuine captured IPNs remain separate evidence; debug parser robustness is F3 here. |
| September 11 history H4/H5; newest closure D1/D2 | Active resend/settlement prose repaired; dated historical reports retain superseded advice and must not be used as current instructions. |

### Rechecking the newest closure

`2026-09-12-regression-and-gateway-closure.md` is supported within its claimed scope:

- **C1/C2:** intent-aware public reconciliation and conclusive 71/152 diagnostic behavior have
  real one-send/public-boundary tests (`gateway/create.rs`, `duplicate.rs`, `recovery.rs`).
- **C3/C4:** direct alternate-format round trips and representable IPN JSON exponents have
  coverage. Neither establishes arbitrary wrapper support or broader IPN form syntax.
- **C5:** journal tests really decode bytes through the SDK; they are not only JSON `Value`
  round trips. The eight-graph command passed again here.
- **C6/C7:** complete XML/DTD grammar and namespace normalization have protocol-level controls.
  No new grammar defect was confirmed. F2 concerns a longstanding numeric grammar below XML.
- The interrupted-deletion fixture change retains zero sends, reconciliation, cancellation
  and exact retained intent. Removing an unnecessary query did not remove those protections.
- ADR 0014 explicitly approves expert Gateway orchestration as of September 12; it does not
  retrospectively claim that old public visibility proved the requirement.

Older broad statements that “every gap” was closed were not reliable all-history certificates:
some reports themselves retained tails, and later independent reviews found further defects.
Historical B-06 empty configuration, B-08 late Ack text rejection and B-17 transient PDF-in-JSON
allocation are examples of older follow-ups outside the newest closure's narrow findings.

## Checks executed in this review

| Check | Observed result |
|---|---|
| `cargo test --workspace --all-features --locked` | **823 ordinary tests and 25 doctests passed**; 52 ignored tests/exporters were not executed. |
| `bash scripts/check-money-features.sh` | **65 tests × 8 graphs = 520 test executions passed.** |
| `python3 scripts/test-order-migration.py` | **5 passed.** Python emitted a ResourceWarning for an HTTPError cleanup; tests succeeded. |
| `python3 scripts/test-agent-schema-runner.py` | **8 passed.** |
| `python3 scripts/check-agent-schemas.py` | **430 requests, 860 source validations: 790 valid, 70 expected source conflicts; 7 negative controls passed.** The documented malformed HU inline source gap remains. |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed. |
| `cargo fmt --all --check`; `git diff --check` before this report | Passed. These were working-tree checks, not a claim that the full historical diff has no whitespace warnings. |
| `dagger --progress plain check --no-generate ci:end-to-end rust:check` | The **37 feature-powerset checks completed successfully**. Outer 240-second timeout interrupted the e2e compilation path. |
| `dagger --progress plain check --no-generate ci:end-to-end` with longer timeout | Test targets compiled; **blocked before scenarios**: apt exit 100, `You don't have enough free space in /var/cache/apt/archives/`. Not a failing application assertion or an e2e pass. |
| Focused reviewer reproductions | Worker missing warning, receiver numeric/stack behavior, monetary adapters, blank identity and isolated Jiff lookup reproduced locally. Package file-list inspection confirmed the missing sibling module. |

Historical totals from the previous closure are not added to these counts. No vendor-live
result, Rust 1.92 result, or advertised-target result is claimed by this session.

Local execution logs:

- Workspace: `/home/laborant/.local/share/opencode/tool-output/tool_09450cbdf0010oI07UwKdmrwRB`
- Scripts/schemas: `/home/laborant/.local/share/opencode/tool-output/tool_09450bb290017zDXuTFAGF3OuM`
- Money matrix: `/home/laborant/.local/share/opencode/tool-output/tool_09455d853001LiBWPidoQMzM4C`
- Feature/e2e initial run: `/home/laborant/.local/share/opencode/tool-output/tool_09456cb12001qQYHK27KENTT5S`
- E2e blocked rerun: `/home/laborant/.local/share/opencode/tool-output/tool_0945adff3001UpE41Df2hj4nDZ`

## Recommended release disposition

1. Fix or explicitly accept F1–F5 with their stated scope, rather than treating previous
   closure language as a blanket waiver. Prioritize the simple timezone declaration and
   missing warning; decide receiver lexical/robustness behavior before adding narrow tests.
2. Make the common Axum/string requirement and wrapped-storno limitation visible to consumers,
   or deliberately extend support with public-boundary regression tests.
3. Strengthen the expected-marker inventory and automate the currently manual money/migration
   controls. Preserve actual HTTP/SDK/CLI boundary tests; direct helper tests missed past regressions.
4. Complete actual-Restate acceptance on a runner with sufficient space, then the documented
   candidate-specific five-core vendor-live acceptance and relevant portability/package checks.

**No architectural rewrite is indicated.** Shared reconciliation is directionally sound;
the remaining architecture work is concentrated at serializer integration boundaries and
test isolation, with explicit release evidence more valuable than another broad abstraction.
