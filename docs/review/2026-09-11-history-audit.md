# Historical closure and regression audit: e41a9964..4394ed0

Reviewed 2026-09-11. Base `e41a9964e266088a4d22a4613108d97d81bab295`; HEAD
`4394ed0977a0adf298a10e0acd9182d13cbf3c0c`. Exactly 20 commits.

## Conclusion

**Yes, iterative fixes introduced incorrect behavior. Most identified intermediate regressions were repaired,
but one cross-crate JSON regression remains:** enabling arbitrary-precision serde_json for the worker makes
ordinary fractional numeric input fail in the CLI when built with that dependency graph. Even `12.34` fails.
The ordinary suites pass because the relevant fixtures use decimal strings.

Separately, public Agent/CLI JSON input still permits silent Decimal rounding; the worker's exact-input repair
does not cover that boundary. IPN's historical leniency work remains genuinely open. Neither is a regression
introduced by the optional-facts changes.

The Agent's headline historical findings about optional invoice identity, taxpayer validity/diagnostics,
explicit paid false, malformed success booleans, namespace aliases and code-56 fallback are **closed in current
code**. Do not turn their repeated appearance in old reports into today's outstanding count.

Ownership: this audit owns historical document inventory/closure and Agent, CLI and receiver regressions.
Worker production safety/API and e2e-flake analysis belong to the other reviewers. Worker closure records below
are mapped and qualified, not independently recertified. No production edits, secret-file reads, or vendor-live
calls. Two uniquely named temporary regression probes were removed. Existing untracked reviews and concurrent
worker test work were preserved.

## 1. Current actionable issues

### H1 — P2, high confidence: worker JSON feature breaks sibling Decimal decoding

**Introduced by `28dcec1`.** Current locations:

- `crates/restate-szamlazz/Cargo.toml:31`: enables `serde_json/arbitrary_precision` (and later `raw_value`).
- `Cargo.toml:31`: ordinary rust_decimal serde, without a compatible arbitrary-precision visitor.
- `crates/szamlazz-agent/src/ops/credit_entry.rs:20–31`: derived Deserialize of plain `Decimal`.
- `crates/szamlazz-agent/src/item.rs:89–110`: same boundary for quantities, prices and totals.
- `crates/szamlazz-cli/src/output.rs:174–180`: passes JSON directly through the public types' visitors.

The worker has its own exact-number adapter. Agent types and CLI file inputs do not. Cargo unifies the
serde_json feature when CLI and worker are selected together, changing a fractional numeric token into
serde_json's private number-map representation. rust_decimal's ordinary visitor rejects that map.

Executed the actual CLI against local wiremock with:

```json
[{"date":"2026-09-11","title":"átutalás","amount":12.34}]
```

| Build graph | Result |
|---|---|
| `-p szamlazz-cli` | Success; exactly `<osszeg>12.34</osszeg>` sent |
| `-p szamlazz-cli -p restate-szamlazz --all-features` | Nonzero exit, `invalid type: map, expected a Decimal type representing a fixed-point number`; no POST for this input |
| Either graph, tested quoted high-precision controls | Decimal strings remain supported |

`1e-2` and a representable high-precision decimal number fail similarly in the combined graph. The proof is
normal numeric input, not malformed JSON, an extreme domain value, or a live financial failure. XML response
adapters remain unaffected. Other consumers combining Agent DTOs with arbitrary-precision serde_json inherit
the same problem.

**Fix:** make supported JSON monetary input feature-composition-safe at the public boundary. Preserve the
worker's exactness; merely removing its feature would reintroduce its earlier rounding bug. Verify CLI-only and
workspace/unified graphs with actual number tokens, decimal strings, exponents, and representability controls.
Enabling a dependency visitor alone needs a separate exactness/closed-shape check; it does not solve H2.

### H2 — P2, high confidence: Agent/CLI JSON money still silently changes values

**Pre-existing and not closed by the worker repair.** Locations:

- `crates/szamlazz-agent/src/ops/credit_entry.rs:20–31` and `src/item.rs:89–110`.
- `crates/szamlazz-cli/src/commands/payment.rs:35–36,52–59` and `src/output.rs:174–180`.

Actual loopback CLI sends from a CLI-only build:

| Input `amount` | Emitted `osszeg` |
|---|---|
| `0.1234567890123456789012345678` | `0.12345678901234568` |
| `"0.1234567890123456789012345678"` | Exact original digits |
| `9007199254740993.5` | `9007199254740994` |
| `"9007199254740993.5"` | Exact original digits |
| `"0.49999999999999999999999999999"` | `0.5000000000000000000000000000` |

The last quoted case rounds in **both** build graphs. Numeric JSON passes through floating point without the
arbitrary-precision feature; string decoding calls Decimal's rounding FromStr. The first two numeric values
are exactly representable by Decimal, so finite-domain policy does not justify losing them. The last value
is unrepresentable and should be refused by an exact-input contract rather than silently moved across a midpoint.

This is not reopening fixed XML response underflow or checked arithmetic. Those repairs occur **after or
outside** this input conversion and cannot recover the original digits. The experiment used unusual precision
boundaries and proves local alteration, not a normal-amount vendor incident.

**Fix:** define exact monetary input handling for the CLI/public serde boundary, including amount flags;
preserve representable tokens and reject precision loss. A string-only CLI contract would need explicit
validation/documentation and still needs exact string parsing. Keep this separate from worker-specific DTOs.

### H3 — P2 existing backlog, high mechanism confidence: IPN content still causes deterministic refusal

`crates/szamlazz-ipn/src/lib.rs:194–208,218–222` still refuses invalid dates/amounts and missing payment method.
The axum extractor turns parse refusal into non-200. This is the original B-10 / #149 work, **not fixed** by
Adatkapcsolat's leniency and **not changed anywhere in the pinned range**.

Fresh tracker read: [#149](https://github.com/sagikazarmark/szamlazz-rs/issues/149) is OPEN and explicitly asks
for optional content with retained raw text. [#75](https://github.com/sagikazarmark/szamlazz-rs/issues/75) remains
OPEN for IPN trust/delivery guidance too. Current tests deliberately assert strict behavior and passed.

The deterministic loss mechanism is established; no new captured real payload establishes its incidence.
Complete #149 rather than counting it as a newly discovered release regression. The README at lines 35–39
still emphasizes upserting delivered values and does not explicitly require authoritative Agent confirmation
before a financial action or explain ordering limitations. It does correctly label IPN unauthenticated and
the IP allowlist as defense in depth; do not claim those warnings are absent.

### H4 — P3, high confidence: CLI docs/sample did not follow the optional-facts migration

- `crates/szamlazz-cli/README.md:70–81` describes only a returned document and a different-number
  `unconfirmed` case. Current `src/commands/invoice.rs:134–165` additionally returns
  `document: null`, an `acknowledgement` object and unconfirmed for a numberless response.
- `crates/szamlazz-cli/examples/invoice.json:15` still contains `"paid": false`. Before `835a370` that
  meant omit `fizetve`; now it explicitly sends false. Agent rustdoc explains this at
  `src/ops/invoice.rs:178–189`, but the CLI's own JSON guidance does not.

**Fix:** describe the actual unnumbered report and the tri-state paid migration in CLI documentation. If the
ready-to-edit sample is intended to retain its previous/default emission, remove `paid` or make it null;
otherwise label its explicit unpaid intent. The behavior change is certain; no omission/false financial
difference has been established. It would be wrong to report the deliberate tri-state API itself as defective.

### H5 — P2 documentation priority, high confidence: current glossary still describes obsolete write behavior

Current `CONTEXT.md` remains a mixed historical/current source:

- line 130 says the Order keeps no state; unresolved-write markers now exist.
- line 183 says marker implementation is pending, despite the marker pre-dating this range.
- the Create step/Issue policy descriptions retain query-and-resend language for protected Order writes.
- line 279 still advises fresh-key deletion/credit renewal after observation, despite the stronger
  settlement-first rule elsewhere in the same file.
- line 332's `_Avoid_` says Order keeps no state, reinforcing the obsolete model.

Several entries contain explicit supersession paragraphs; those are important and must be followed. They do
not make contradictory standalone paragraphs reliable current instructions. This is document-closure debt,
not proof current Order handlers lack a marker or automatically resend. Replace the obsolete descriptions
with the current protected/unmanaged split and link historical decisions rather than continuing to append
overrides. The dedicated current protocol/runbook and the worker review owner govern production behavior.

## 2. Review-family coverage map

This is a family-level closure audit, not a claim to have independently repeated every old raw experiment.
Read the consolidated reports, closure records and the relevant adjudications; followed raw details for
namespace/identity/numeric disputes and receiver backlog. Old line numbers describe old trees.

| Family | Authoritative historical record | Current disposition |
|---|---|---|
| September 6 workspace | `2026-09-06/FINAL-REVIEW.md`, judges/raw provenance | Major Agent/Adatkapcsolat repairs precede this range. Timing-only exactly-once reasoning is superseded. IPN B-09/B-10 and lower-priority tails are not all closed. |
| September 8 architecture/tests | `2026-09-08/REVIEW.md`, four raw slices | Pure worker decisions, custom loopback TLS, split harness/step tables, CLI tests and receiver tests evolved subsequently. Its blanket “every gap ... is closed” cannot mean the still-listed IPN/receiver backlog. Harness runtime/flake assessment owned elsewhere. |
| September 9 architecture/naming/tracker | `2026-09-09/REVIEW.md`, especially raw receiver/Agent slices | RootKind/public identify, KEY_ERR rendering, owned fallible resolver, Error bounds, naming and shared envelope changes implemented. #130 is CLOSED. #74 remains an umbrella with stale and open tail rows; issue openness is not proof its top three bugs persist. |
| September 9 Agent API | `2026-09-09-agent-api/FINAL.md` and `round-2/JUDGE.md` supersede REVIEW/first ADJUDICATION; five raw + five round-two slices | Six original Agent mechanisms and four field capabilities largely closed before base. Sixteen documentation corrections consolidated; monetary/namespace residuals drove later rounds. Judge's 44-row ledger prevents duplicate counting. Optional validity/diagnostic/paid policy decisions were later deliberately revised. |
| September 10 Agent directory | `2026-09-10-agent-api/REPORT.md`, `JUDGMENT.md`, `IMPLEMENTATION.md`; five raw slices | VAT interpretation, exact XML numbers, bounded multipart candidates, namespace filtering, recovery example and deletion scope fixed before base. TP metadata subsequently implemented in `835a370`. |
| September 10 Agent flat | `2026-09-10-agent-api.md`; six slices, consolidated adjudication in summary | Arithmetic, refusal preservation, userinfo redaction, bounds and principal docs fixed before base; incomplete transfer completed in `f83e5fd`. |
| September 10 Agent current | `...-current.md` + adjudication/six slices, pinned `fbda137` | Four P3 mechanisms fixed by `f83e5fd`; URL trimming remains explicit policy. “current” is a historical filename, not HEAD. |
| September 10 Agent f83e5fd | `...-f83e5fd.md` + adjudication/six slices | Repeated-row prefix grouping, normalized namespace binding/attribute checks and diagnostic isolation fixed by `2ba5fb8`. Summary has an implementation addendum. Quoting regression caught during that implementation and repaired before commit. |
| September 11 Agent unqualified suffix | `2026-09-11-agent-api.md` + adjudication/six slices, pinned `2ba5fb8` | Legacy key/key Debug leak, malformed identity fallback, reserved xmlns/PI gaps and protocol overclaims fixed by `9b78546`. |
| September 11 Agent 837dad0 | `...-837dad0.md` + adjudication/six slices | Explicit clearing added by `61c334f`; explicit paid false added by `835a370`. No remaining headline capability exclusion. |
| September 11 Agent 61c334f | `...-61c334f.md`, six slices, adjudication incorporated in summary | Outbound nonpositive date hole and missing full request-XSD matrix fixed by `eec57fc`. Receipt/clearing evidence subsequently expanded. |
| September 11 Agent eec57fc | `...-eec57fc.md` + adjudication/six slices | Required success boolean and five optional indicators fixed by `77d53c5`; partial corrupt-PDF recovery remains a deliberate limitation. |
| September 11 Agent 77d53c5 | Existing untracked `...-77d53c5.md` + adjudication/six slices | Code-56/evidence prose and NAV diagnostics now fixed; numberless success supported. Earlier PHP URL single-decoder premise corrected by later adjudication. Reports preserved untouched. |
| September 11 Agent 28dcec1 | `...-28dcec1.md` + adjudication/six slices | All six headline findings implemented in `835a370`; closure in `agent-contract-follow-up.md`. Vendor success guarantees remain unanswered despite local capability completion. |
| September 10 Restate practices | `2026-09-10-restate-practices.md` + research | Led to lazy credentials, cancellation, expected-document intent and unresolved-write protection. Historical stateless/query-first endorsement is superseded for protected writes. |
| Order write protection | `2026-09-10-order-write-protection.md` | Records pre-base #216 implementation, actual acknowledgement boundary and interruption coverage. Does not certify every later recovery branch. |
| Release at e41a9964 | `release-readiness.md` + `release-hardening.md` | R1–R3/R6–R8 attributed to `028dfcd`; archive/CLI R4–R5 landed separately in `837dad0`. Closure prose calls these one working-tree effort, not one commit. |
| Worker first release fixes | `worker-release-fixes.md` | `028dfcd`: credit-refusal uncertainty, wrong-order storno, opaque account marker, arm-ack/command/authorization coverage. Worker owner verifies present safety. |
| Worker identity/acceptance | `worker-identity-hardening.md`, `worker-release-acceptance.md` | `5c6d5ea` strengthened query identity; `bed1d24` then repaired overly restrictive evidence-number types. Dated live runs are historical account evidence. |
| Worker hardening/readiness | `worker-release-hardening.md`, `worker-release-readiness.md` | `10f00b3`: corrective evidence/XML identities/read-fault preservation. `370ff2e`: pre-arm full validation, retention and warnings. |
| Final project 370ff2e | `final-project-370ff2e.md`, closure addendum | `28dcec1`: scope-state migration gate, exact input, corrective base eligibility, fanout chains and diagnostics. This is the source of H1's cross-crate feature regression. |
| Final project 28dcec1 | `final-project-28dcec1.md`, closure addendum | `99762c1`: malformed rotated credentials, exact numeric schema/shape, unreadable-state/pinned-account coverage. Does not fix sibling Agent/CLI serde. |
| Agent contract follow-up | `agent-contract-follow-up.md` | `835a370`: optional facts and paid intent; its intermediate unnumbered unmanaged-storno retry regression was caught and fixed within the patch. |
| Release 4488f37 | `release-readiness-4488f37.md`, follow-up | `4394ed0`: account-config attribution, typed external-id constructors, README body, second corrective lookup, exact marker order. |
| Worker 4394ed0 | Existing untracked `worker-release-4394ed0.md` | Current worker findings remain with the worker owner: public Gateway retry contract, contradictory reissue settlement, schema approximation, vendor-derived date attribution. Earlier approvals do not override this later review. |
| Live assessment/strategy | `agent-live-test-assessment.md`, `live-test-strategy.md`; later research `live-suite-current-assessment.md` | `d85cdf2` and `46fab05` implement missing standalone PDF, stronger business read-backs, targeted filters, optional-echo alignment, public worker query and stable date/form assertions. Remaining rendering/concurrency/host-evidence limits are not missing local tests. |

The review tree has no root-level current-status index. Several closure records describe working changes or
say “uncommitted” after those changes were committed. Keep historical conclusions, but add commit-linked closure
or supersession pointers; do not edit them to pretend their original review saw today's code. Likewise do not
sum overlapping test totals from six specialists or confuse 118/125 declarations with 134 descendant paths.

## 3. Chronology of the pinned 20 commits

| Commit, oldest first | Important delta / historical meaning |
|---|---|
| `f83e5fd` | Retains incomplete HTTP status/headers as uncertainty; isolates body identity; adds XML lexical checking; strict receipt reversal and blank-PDF normalization. |
| `2ba5fb8` | Canonical protocol row names + overlapped lists, normalized reserved bindings/expanded attributes, independent optional diagnostics. Legal-quote regression fixed inside implementation. |
| `028dfcd` | Worker settlement/recovery hardening and real acknowledgement/command coverage; reference closure documents above. |
| `9b78546` | Redacts both legacy credential fields; prevents header 56 from erasing malformed body identity; rejects reserved xmlns element prefix and colon PI target; qualifies protocol claims. |
| `1b08368` | Opt-in live journeys and nextest profiles; tests are execution definitions, not universal vendor guarantees. |
| `837dad0` | Receipt JSON path switches from sanitized business number to required record id; CLI queries original and derives storno appearance/date. |
| `61c334f` | Explicit checked clearing operation alongside accidental-empty guard; receipt probes. |
| `5c6d5ea` | Worker number identity and storno evidence checks; exports XML text validation for shared use. |
| `bed1d24` | Recovery evidence preserves wider vendor numbers rather than applying mutation-input alphabet/length. |
| `eec57fc` | Outbound years 1–9999 checked at to_wire/client; separately retained EN-inline/download schemas and full required checker. |
| `10f00b3` | Corrective base evidence, XML-safe worker identities and immediate get fault classification. |
| `77d53c5` | Required envelope success no longer coerces empty to false; five invoice booleans retain absence. |
| `370ff2e` | Complete request validation before arming; recovery retention and correlated credential warnings. |
| `28dcec1` | Exact worker JSON + migration state gate + fanout cause chains; introduces H1 through serde_json feature unification. |
| `99762c1` | Credential-rotation uncertainty and raw-JSON/number-schema refinements; restores worker composition cases. |
| `d85cdf2` | Stronger live assertions and selectable Dagger probes. |
| `835a370` | Tri-state paid; optional credit/PDF number; distinct storno acknowledgement; optional NAV validity/diagnostics; downstream gates and docs. |
| `4488f37` | Worker credit reply identity and exact balance accumulation; does not extend exactness to public CLI inputs. |
| `46fab05` | Optional-echo live assertions, stable intended dates, semantic appearance, public worker query. |
| `4394ed0` | Account validation and mutation identity/second corrective lookup/marker strictness. |

### What actually went wrong during fixing

1. **Evidence salvage grew too broad.** Optional payload failure initially erased a known refusal, then a body-only
   number; a later fallback could erase the identity decoder's failure and trust a header. These are distinct
   stages, not the same unclosed issue. Current `envelope.rs:220–239,330–348` preserves verdict precedence and
   propagates identity failure while allowing optional-metadata salvage under numbered 56.
2. **Namespace projection became a semantic boundary.** Filtering fixed foreign-field adoption but exposed
   raw-QName grouping/adjacency; canonicalizing names fixed rows but needed quote preservation. Current
   `xml.rs:303–377` and namespace tests cover aliases, interleaving, scalar children, singletons and quoting.
3. **Stricter worker identity over-constrained recovery.** Wider reported vendor numbers needed a separate
   EvidenceNumber. That repair should not be undone by importing the caller's 40-byte alphabet everywhere.
4. **Optional response support crossed a retry boundary.** The `835a370` closure records an unnumbered unmanaged
   storno acknowledgement initially re-entering the issue policy; it was changed to journaled uncertainty after
   read-only reconciliation. The current worker owner is responsible for its full safety assessment.
5. **Exact JSON handling had a dependency-graph effect.** Worker-only tests and successful builds missed H1.
   Testing quoted decimals alone cannot expose numeric-token visitor incompatibility.

## 4. Optional facts, response identity and namespace verdict

- **Paid intent:** current None omits, Some(false) emits false, Some(true) emits true. Constructor/missing/null
  defaults preserve omission; old serialized false deliberately changes meaning and is documented in Agent.
  Worker retains its former boolean emission policy. No vendor omission/false equivalence is claimed.
- **Credit/PDF:** absent reported number no longer destroys acknowledged balance/artifact. A requested number
  is never substituted as vendor evidence. Duplicate/nested identity still fails; PDF query still needs a PDF.
- **Storno:** Unnumbered is neither preview nor proven reversal. Numberless 56 remains Unknown. Numbered
  `CreatedInvoice::reverses` remains a reply heuristic, not a vendor query or general proof.
- **Taxpayer:** absent validity is None; false is a reported negative; blank/malformed validity fails. NAV 2 API
  and NAV 3 Common/API/Base paths remain separate. Header/software/ordered successful notifications are exposed;
  error-result extra metadata remains a documented projection limit.
- **Namespace/shape:** current checks reject incomplete/multiple roots, foreign identity/verdict, undeclared
  prefixes, duplicate expanded attributes, invalid reserved bindings and lexical violations. Correct aliases,
  valid ignored extensions and repeated rows remain accepted. This is not full response XSD validation.
- **Receiver boundary:** Agent lexical/present-content strictness was not copied into Adatkapcsolat. Its
  malformed optional date/PDF handling remains lenient; identity is still shape by deliberate #176 decision.
- **Archive:** record-id JSON naming fixes the reported business-number collision. Existing directories are
  not migrated; monthly placement and configured connection/storage isolation still matter. Batch XML uses
  min/max ids under the selected redelivery policy; this was not changed by the JSON-name repair.

## 5. False positives and unresolved questions to keep separate

Suppress as current defects:

- “All numberless credit/PDF/storno responses fail”, “validity is always required”, “paid false is impossible”,
  “taxpayer diagnostics are unavailable”, “code-56 guidance is universal”, and “all receipt additions unobserved”.
- “Incomplete HTTP transfer loses all headers”, “alternate row prefixes cause duplicate-field errors”,
  “namespace references are compared raw”, and “header 56 accepts duplicate/nested identity”. Current controls pass.
- “CLI always creates paper stornos”, “receipt JSON is keyed by sanitized business number”, “transient resolver
  failure gets KEY_ERR”, and “Adatkapcsolat strictly rejects every missing XSD content element”.
- “No standalone PDF/live public-worker query exists”, “Dagger cannot select probes”, or “receipts/clearing have
  never been exercised”. These confuse older source inventories with later definitions or dated executions.
- Applying Agent's date/optional-PDF strictness to the inbound receiver, or applying the worker's mutation-number
  restrictions to every reported vendor number.

Keep open or intentionally limited:

- **Customer URL encoding:** earlier PHP comparison stopped at rawurldecode. The public getter decodes again.
  Later `28dcec1` adjudication and vendor draft correct that premise. This does not prove Rust is right or wrong
  for emitted headers; do not blindly copy either percent-only or PHP double decoding.
- **Source conflicts:** preview/simpleItems order, fields absent from downloads, HU PDF schema, template labels,
  receipt settings/reporting. The checker expects specific conflicts; 70 expected failures are not 70 live accepts.
- **Success guarantees:** numberless/validity-absent support is now a local capability; vendor guarantees remain
  unanswered. Current clarification draft explicitly says not sent/no answer. Its “upcoming” section is stale
  completion language, not evidence that `835a370` is absent.
- **Artifact recovery:** corrupt nonblank optional PDF still loses the typed result outside numbered-56 leniency.
  Previously adjudicated explicit boundary/possible enhancement, not a new valid-PDF regression.
- **Codes 7/338:** 7 can be missing email subject on receipt send; 338 refuses this duplicate exchange without
  recovering the earlier receipt. Neither establishes absence of an earlier effect.
- **Evidence provenance:** 337 observed among the thirteen new mappings; 338 predates them; 55/56 not triggered.
  Receipt MNB/lifecycle have bounded executions. Delayed email recovery/inbox confirmation is not a rerun of the
  later paced full scenario, proof of attachment equality, or a concurrency/retention guarantee.
- **Worker accepted limits and current findings:** unmanaged writes, host authorization/seller mapping, vendor
  visibility and current public Gateway/reissue findings remain with the worker owner. No historic “approved”
  paragraph is an approval of every subsequent commit.

## 6. Verification performed in this audit

```sh
git status --short
git rev-parse HEAD e41a9964e266088a4d22a4613108d97d81bab295
git log e41a9964e266088a4d22a4613108d97d81bab295..HEAD --oneline
git diff --stat e41a9964e266088a4d22a4613108d97d81bab295...HEAD
git diff e41a9964...HEAD -- <reviewed paths>
git show 28dcec1 -- crates/restate-szamlazz/Cargo.toml
cargo test -p szamlazz-agent -p szamlazz-cli -p szamlazz-adatkapcsolat -p szamlazz-ipn --all-features --locked --offline
```

**423 ordinary tests/doctests passed:** Agent 298, Adatkapcsolat 99, CLI 12, IPN 14. Ten Agent vendor tests and
the schema exporter remained ignored. This is one nonoverlapping runner total; it excludes temporary probes.

Temporary actual-CLI probe commands (files subsequently removed):

```sh
cargo test -p szamlazz-cli --locked --offline --test history_audit_4394_numeric -- --nocapture
cargo test -p szamlazz-cli -p restate-szamlazz --all-features --locked --offline --test history_audit_4394_numeric -- --nocapture
cargo test -p szamlazz-agent --locked --offline --test history_audit_4394_xml -- --nocapture
```

The numeric probe spawned the real CLI with dummy credentials and a loopback endpoint, supplied raw JSON on
stdin, and inspected actual multipart amounts. H1/H2 tables record observed results; the probes printed behavior
and are not passed desired-behavior regression tests. The expanded numeric matrix was run under both graphs.
The XML probe rejected NUL in CDATA/comment/PI, multi-colon element/attribute/binding names, and accepted a
single-quoted attribute containing a literal double quote. No new XML defect reproduced. Temporary probes
emitted missing-crate-doc warnings; they were not retained as production-quality tests.

```sh
env PATH="/nix/store/6xp8y3aclw6m89sy7r12sf6l98s2di0m-libxml2-2.15.3-bin/bin:$PATH" TMPDIR="/tmp/opencode" python3 scripts/check-agent-schemas.py
env PATH="/nix/store/6xp8y3aclw6m89sy7r12sf6l98s2di0m-libxml2-2.15.3-bin/bin:$PATH" TMPDIR="/tmp/opencode" python3 scripts/test-agent-schema-runner.py
git diff --check
```

Schema result: **430 generated requests, 860 validations: 790 valid, 70 exact expected source conflicts;
all seven negative controls and every declared-element-path coverage check passed.** Runner tests: **8 passed**.
No schema acquisition or authenticated HTTP occurred. Source semantics were grounded in retained schema/research
and adjudication evidence; this audit did not claim to freshly refetch the vendor website. Read-only GitHub
queries verified #74/#75/#149 remain open and #130 closed.

No new full-workspace Clippy, feature powerset, wasm/browser, real-Restate or vendor-live run is claimed.
The targeted combined-graph test is materially different from a compile-only feature powerset: its runtime
numeric token is what exposes the regression.
