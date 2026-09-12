# Testing

Ordinary `cargo test --workspace --all-features --locked` remains supported:
every externally dependent scenario is `#[ignore]`, even with credentials set.
Install `cargo-nextest` and `cargo-hack` for the named layers below. The shared
configuration is `.config/nextest.toml` (#132 and #218); Cargo aliases supply
`--all-features` so a missing transport cannot silently empty the live suite.
An explicitly selected agent target without `client-reqwest` fails with a
missing-transport diagnostic.

## Selection before execution

```sh
cargo nextest list --workspace --all-features --locked --profile default
cargo nextest list --workspace --all-features --locked --profile ci
cargo nextest list --workspace --all-features --locked --profile e2e --run-ignored only
cargo nextest list --workspace --all-features --locked --profile live --run-ignored only
cargo nextest list --workspace --all-features --locked --profile probes --run-ignored only
```

Default/CI exclude live, probes and ignored `e2e_` scenarios, including nested
names, but retain non-ignored helper tests in the worker's `e2e` binary (including
the two `only_tests::e2e_only_*` filter helpers). `e2e` includes the library's
three execution/cancellation scenarios as well as the integration binary, and
selects actual Restate with mocked szamlazz.hu; `live` selects exactly five
scenarios across the two `live` binaries; `probes` selects eleven investigative
cases (two appearance, two clearing, three receipt probes, plus four storno-email cases). A profile does not unignore a test: the external
commands must supply `--run-ignored only`. Nextest fails empty runs by default;
do not override that behavior.

## Normal development and CI

```sh
cargo t
cargo test --doc --workspace --all-features --locked
cargo hack check --workspace --feature-powerset --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test -p xtask --locked --test migration
bash scripts/check-money-features.sh
dagger check
```

The locked Rust Dagger module detects nextest and runs doctests separately. Its
`check` uses cargo-hack to enumerate offline feature combinations; no live
scenario is run per combination. Workspace-specific `ci.test` runs all-feature
nextest/CI and Cargo doctests. `ci.end-to-end` uses the same compiled targets and
the `e2e` profile (one scenario at a time, no retries, 20-minute scenario timeout).
JUnit is written to `target/nextest/<profile>/junit.xml`.
`ci.end-to-end` returns its report directory for export. Live/probe JUnit also
stores successful test output, including the run label and document numbers.
`dagger check` also runs `ci.money-features`, `ci.order-migration` and `ci.schemas`.
The migration check runs only the local Rust regression suite with synthetic
inputs and a loopback server, not the inventory against a deployment. Dagger
installs system tzdata before compiling shared test targets. The e2e container
builds the migration command and supplies its path through `XTASK_BIN`; local
e2e runs can supply that variable or let the suite invoke Cargo to build it.

```sh
# Export the same Restate 1.7.8 binary used by Dagger.
dagger -c 'ci | restate-server | export ./restate-server'
export RESTATE_SERVER_BIN="$PWD/restate-server"
cargo e2e
```

Alternatively `RESTATE_ADMIN_URL` and `RESTATE_INGRESS_URL` select an existing
server where supported. Dedicated-shape scenarios still need the binary.
`RESTATE_ENDPOINT_HOST` is the host that server uses to reach the local endpoint.
Do not share a server between concurrent runs or deployments: registration
changes which endpoint new invocations reach. Mocked-vendor tests retain their
controlled failure, concurrency, cancellation, recovery and journal/privacy
coverage in regular CI.

### Expert Gateway reconciliation regression

`cargo test -p restate-szamlazz --all-features --locked --test gateway` exercises the public
orchestration boundary against wiremock. `tests/gateway/recovery.rs` retains issuance intent across
a fresh Gateway and checks old reissue targets, corrective bases, collisions, blocked verification,
exact candidates and paired storno/original evidence. Reconciliation sends no mutations; deletion
remains inconclusive. `tests/gateway/duplicate.rs` checks that failed optional diagnostics do not
replace a conclusive 71/152 refusal. These tests establish evidence classification, not caller-owned
durability or vendor behavior. Actual-Restate/mocked-vendor Order tests cover the marker/arm protocol
separately; see [the protected protocol](design/order-write-protocol.md).

### Named-target deletion regression

Named-target deletion (#225) uses the same public seams: `tests/expected_document.rs` checks closed mode
selection, discovery schemas and the migration example; Gateway `agent_writes.rs` checks number-only
verification/deletion and fresh identity/credit-entry guards. `tests/e2e/delete_proforma.rs` checks named
selection, coexistence, unavailable/mismatched reads and stale requests after purge. Recovery evidence
scenarios cover named deletion after a lost reply or interrupted result, pause/resume, cancel/kill,
blocked subsequent writes and exact audited settlement on real Restate 1.7.8. The provider is mocked;
these checks assert worker behavior, not vendor atomicity (ADR 0015).

### Monetary dependency-graph regression

Run `bash scripts/check-money-features.sh` from the workspace root when changing monetary Serde
boundaries or dependencies. It runs the focused Agent `decimal_serde`/`numeric_fidelity`, executable CLI
`boundary` tests (including its direct `serde_ignored` JSON wrapper), IPN Serde,
Adatkapcsolat `monetary_serialization`, and worker `decimal_input` and journal codec tests under eight
locked graphs: default, `rust_decimal/serde-float`, `rust_decimal/serde-arbitrary-precision`, both
features together, and `serde-str` or `serde-bincode` combined with `serde-float`, each with and
without arbitrary precision. Journal tests decode serialized bytes through the actual SDK codec;
a prebuilt JSON `Value` round trip alone can hide feature-dependent text-decoding failures.
It also prints the Agent's reverse Decimal feature tree for each graph. Cargo and the locked
dependencies are required; a cold cache may fetch dependencies, but these tests contact no vendor.

The regression is that downstream feature unification must not change public monetary strings into
floating-point or numeric serialization, or silently lose exact digits on round trip. The focused
tests also exercise supported non-JSON formats and representable exponent inputs. A default workspace
run or cargo-hack's package feature powerset alone does not establish these downstream graph cases.
The script is wired automatically as `ci.money-features` in `dagger check`; the
local command remains useful for focused verification. Run either tool's named
Dagger check independently with:

```sh
dagger -c 'ci | money-features'
dagger -c 'ci | order-migration'
```

## Offline request XSD validation

Run the **required standalone schema check**, in addition to normal Rust tests:

```sh
cargo xtask check-agent-schemas
# Targeted runner regression checks (also required in ci.schemas):
cargo test -p xtask --locked --test schema_runner -- --include-ignored
# Outside devenv, with Nix:
nix shell nixpkgs#libxml2 --command cargo xtask check-agent-schemas
# The dedicated CI check (also part of dagger check):
dagger -c 'ci | schemas'
```

Requires Rust and `xmllint` with XML Schemas support; `devenv.nix` supplies both.
The unpublished `xtask` workspace crate implements the checks and their regressions.
Dagger generates the request matrix and builds the checker and regression executable
in its Rust container, then transfers those artifacts and the cached schemas to a
separate `debian:trixie-slim` container with `libxml2-utils`. That runtime base matches
the Rust builder's Debian release. The validation container needs no Cargo, compiler
or Python; the Rust container needs neither Python nor `xmllint`.
Dagger explicitly provisions locked Cargo dependencies with `cargo fetch --locked`
before running these offline checks, so a cold cache is supported. Local offline
runs likewise require dependencies already provisioned.
`xmllint --schema` uses libxml2's **XSD 1.0 validator**, covering sequences,
required/empty containers, cardinality, lexical types and facets. The Rust XML
parser only inventories coverage. No production dependency is added. Real-validator
regression tests are ignored in ordinary Rust tests and required by `ci.schemas`.

For artifact-based execution, `cargo xtask prepare-agent-schemas --output <directory>`
exports `requests.json`, `xtask` and `schema-runner-tests`. Run the exported checker as
`xtask check-agent-schemas --requests <requests.json> --corpus <schema-directory>`;
it consumes these files without invoking Cargo. The exported regression executable
takes `--include-ignored`, with `XTASK_BIN` and `SZAMLAZZ_SCHEMA_CORPUS` pointing to
the transferred checker and corpus. Schema validation and regression logic is the
same locally and in Dagger.

The runner invokes `cargo test -p szamlazz-agent --locked --offline --test
schema_requests -- --ignored --exact emit_request_matrix` to export freshly
generated **`to_wire` multipart XML**, then validates each source independently.
The ignored Rust test is an input exporter, **not a successful schema check**;
ordinary Cargo runs do not claim this coverage. Missing tools, corpus, checksums,
schema compilation, empty/missing operation coverage and unexpected results fail
the standalone invocation. No schema or Cargo dependency is fetched during it.
`--nonet`, disabled catalogs, and a self-contained-schema/entity-declaration
check prevent external XML resolution. Dummy credentials are literal test data;
no client is constructed or live request made.

The matrix includes:

| Operation | Variations |
|---|---|
| Invoice create | Minimal; every XML option populated; independently populated settings and blocks; all six kinds and proforma/prepayment references; all four carrier blocks; empty optional containers; false/true options; all nine absent/false/true preview × simple-items combinations; group id and erasure counts separately; all 15 languages, six templates plus an open token, five taxpayer statuses; EUR explicit/automatic MNB; multiple signed rows and five attachments (XML part only) |
| Invoice storno | Minimal, every optional field, each top-level option independently, false guardian and empty seller email |
| Credit entries / explicit clear | One/five entries × additive/replacing; optional issuer/aggregator/descriptions; empty additive and both clearing forms |
| Invoice PDF / XML query | All three selectors; XML PDF false/true |
| Proforma deletion | Both number and order selectors |
| Receipt create | Minimal/full/common options; individual header options; item ledger/erasure, empty ledger, multiple rows/tenders; all four templates; EUR explicit/automatic MNB |
| Receipt storno / query | Template absent/all four values × PDF false/true × call id absent/present; both query selectors |
| Receipt send | Default resend; all 16 email-child presence combinations, with populated and empty strings |
| Taxpayer | Eight-digit stem including a leading-zero control; no optional business fields |

**Every case uses both credential forms.** Every declared element path of each
source must appear in at least one **fully valid** generated request for that
source; known-invalid rows earn no coverage. This prevents the full invoice's
first source conflict from hiding untested later blocks. Negative controls also
require actual rejection of wrong booleans/money, a negative erasure count,
missing required seller, a bad taxpayer pattern, a duplicate PDF flag and
reordered PDF-query fields. This is a representative optional-field matrix,
not the Cartesian product of all possible business values or account rules.

Sources are the separately retained 2026-09-11 **EN inline** definitions and
**downloads**, with precise extraction/checksums in
[`fixtures/SOURCES.md`](../fixtures/SOURCES.md#2026-09-11--separate-request-xsd-validation-sources).
Each verdict names its source, case and credentials. `EXPECTED-SOURCE-CONFLICT`
is reported with the validator diagnostic, distinct from `VALID`:

| Source | Required conflicting result |
|---|---|
| EN inline invoice | Combined preview/simple-items fails at `simpleItems`: inline requires simple-items before preview; the writer follows download/PHP's inverse order |
| Download invoice | Buyer `csoportazonosito` and item `torloKod` fail because those declarations are absent |
| Download receipt create | `torloKod` fails because its declaration is absent |
| Download receipt query | Order selector `rendelesSzam` fails because its declaration is absent |

Expectations come from the requested options, not the emitted XML. Expected
invalidity must be exit code 3 with exactly the named unexpected-element
diagnostics; unrelated validation errors, schema-load failures and unexpected
success fail the check. Originals are never patched or combined to obtain a
green result.

Both sources cover all 11 operations, including the working deletion download at
[`dijbekerodel/xmlszamladbkdel.xsd`](https://www.szamlazz.hu/szamla/docs/xsds/dijbekerodel/xmlszamladbkdel.xsd).
**Source gaps:** the HU PDF inline source is outside the
executable matrix: its untouched definition has a missing attribute separator,
requires `szamlaszam`, and moves `rendelesSzam` after `valaszVerzio`, conflicting
with EN/download and the documented alternative selectors. See
[Q-S1](review/2026-09-11-agent-api-61c334f-queries.md#q-s1--p3--hungarian-pdf-request-schema-cannot-be-a-common-conformance-target).
No synthetic repair is substituted. HU sources generally, response schemas,
rendered artifacts, business rules and actual server acceptance remain outside
this check. Ordinary contemporary dates exercise `xs:date`; outbound date-domain
boundary tests are maintained separately.

## Manual vendor-live acceptance

Keep the five core scenarios as a rarely executed acceptance suite. Run them
manually before releases that materially change transport, XML, arithmetic,
document conversion or the worker write protocol, and after relevant vendor
changes. Documentation-only and unrelated changes do not need a fresh live run.
After a long quiet period, a quarterly manual drift check is reasonable if no
relevant release has already supplied fresh evidence. Ordinary tests, schemas
and actual-Restate/mocked-vendor e2e remain regular CI checks.

Select the receipt lifecycle and populated credit-entry clearing as optional
acceptance when those capabilities change or are relevant to consumers.
Already-empty clearing, appearance mismatches, automatic MNB and email resend
remain targeted investigations; clearing stays separate so the core still tests
storno of a credited invoice. A corrective lifecycle is future optional work if
corrections become a main use case, not part of the five-case selection.

Use an intended **test-mode** account with e-invoice and EUR capabilities and
the worker's documented order-number uniqueness setting. Configure it before
running: the response `teszt` assertion detects a wrong account only after the
first document. The separate deployed resolver/seller checks remain the
deployment/rotation checks; this suite does not replace them.

```sh
# Load locally held credentials without putting a key in command arguments.
set -a
source .env
set +a
export RESTATE_SERVER_BIN="$PWD/restate-server"
cargo live

# Independently selectable read-only NAV dependency smoke.
cargo live -E 'package(szamlazz-agent) & test(taxpayer_query)'

# Ordinary Cargo, without nextest (keep serial execution).
cargo test -p szamlazz-agent --all-features --test live -- --ignored --test-threads=1 --nocapture
cargo test -p restate-szamlazz --all-features --test live -- --ignored --test-threads=1 --nocapture
```

Both live suites explicitly enable Jiff's `tzdb-zoneinfo` dev feature for Linux
and retain the platform bundle for Windows/macOS. Install system `tzdata` on
Linux; Dagger provisions it before compilation. The normal live/probe commands
need no extra Jiff feature flag.

Each package owns its `tests/live_support` module. The worker carries only its
observation, cleanup and assertion helpers, without including sibling Agent test
source or adding a support crate. Its packaged live target can compile using the
declared Agent dependency without that dependency's test sources. Compile the
targets without contacting szamlazz.hu:

```sh
cargo test -p szamlazz-agent --all-features --locked --test live --test probes --no-run
cargo test -p restate-szamlazz --all-features --locked --test live --no-run
# Local mock cleanup and timezone regressions, also selected by default/CI:
cargo test -p szamlazz-agent --all-features --locked --test live_cleanup
```

Missing/empty credentials or a missing required Restate source fail selected
tests. Credentials alone never enable them. Every lifecycle is a single test;
tests share no process state or execution-order assumptions. Live/probes run
serially, fail-fast, with **zero whole-test retries**. This is per runner, not a
cross-machine account lock. Their slow timer reports progress without killing a
possibly executing write; individual vendor/harness requests retain their own
timeouts. A run interrupted by infrastructure or an operator needs the same
reconciliation as any unanswered write.

Core scenarios:

1. Paper HUF invoice: half-forint rounding, create, XML-query and standalone-query
   PDFs (signature checks, not rendering), persisted
   identity/type/order/currency/totals, replacement `[100]` → `[200]` then additive
   `[50]` credit entries (unordered comparison and returned outstanding amounts),
   previous-month fulfillment, matching storno appearance and relationships,
   removal of the original's credit entries on storno, own storno external id
   and repeated storno returning the existing reversal.
2. Proforma create/query/delete, then absence by number and external id.
3. Actual Restate ordinary e-invoice order: account probe, proforma consumption,
   same-key replay and fresh-invocation `already_issued`, live invoice observation,
   one by-number `Szamlazz.Agent.query` checking public identity, totals and reference,
   storno retaining the original's explicit previous-month fulfillment and
   electronic appearance despite the account's paper default,
   ordinary `reversed`, exact-number reissue, newest external-id holder and stale
   expected-number `target_changed`.
4. Actual Restate EUR proforma/prepayment/final: explicit proforma reference and
   caller-supplied negative prepayment line at the same VAT rate, exchange rate
   400, fractional price, persisted references/line totals/consumption, live
   prepayment/final observations and completed
   operation repetition. Full performance is 49.38 + 13.33; deduction is
   −24.69 − 6.67; final gross is 31.35. The vendor does not deduct automatically.
5. Read-only taxpayer lookup: valid, matching tax-number stem and nonblank name, no pinned company
    name/address. A NAV dependency failure fails this smoke explicitly.
6. `authoritative_gross_eur`: approved 3 × €10 gross through protected Order, public
   preflight, exact queried line/document 23.62 / 6.38 / 30.00 and cleanup. Select it with
   `cargo live -E 'package(restate-szamlazz) & test(authoritative_gross_eur)'`.
   Added for #224; [live acceptance passed on 2026-09-12](research/2026-09-12-authoritative-line-amounts.md#live-acceptance--passed-2026-09-12),
   with exact persisted totals and verified cleanup reversal.

PDF and credit-entry acknowledgements may omit the invoice number: a reported
number must match, and an omitted echo is logged. PDF signatures, credit-entry
read-backs and financial expectations remain strict. Electronic storno appearance
is checked as electronic rather than pinned to the original's numeric code;
both codes are logged. Expected fulfillment dates are retained from the requests.

### Separately selected investigative probes

Select a specific experiment rather than running the entire probe set by default:

```sh
# Optional clearing acceptance; already-empty clearing is a separate investigation.
cargo probes -E 'test(clear_credit_entries_populated)'
cargo probes -E 'test(electronic_original_paper_storno) | test(paper_original_electronic_storno)'

# Use a configured receipt-only prefix on the intended test account.
export SZAMLAZZ_RECEIPT_PREFIX="NYGTA"
cargo probes -E 'test(receipt_lifecycle)'
cargo probes -E 'test(receipt_automatic_mnb)'

# The email probe deliberately requests two emails to an operator-controlled inbox.
export SZAMLAZZ_RECEIPT_EMAIL="operator@example.com"
cargo probes -E 'test(receipt_email_resend)'

# Four fresh originals/reversals; independently observe the correlated inbox messages.
export SZAMLAZZ_STORNO_EMAIL="operator@example.com"
cargo probes -E 'test(storno_email::)'
```

- **Clearing:** independent populated/already-empty tests each create a test
  invoice and verify its initial empty entries. The populated case registers
  and reads back one credit entry first. Each sends `ClearCreditEntries` and
  checks any echoed number, outstanding gross and empty queried entries. Filter
  `clear_credit_entries_already_empty` to run that case even if populated clearing fails.
  A refusal or unchanged entries fails the hypothesis. Both cases passed on an
  operator-confirmed test account on 2026-09-11; see the
  [dated execution record](research/2026-09-11-credit-clearing-live.md). A probe's
  source alone is not execution evidence or a universal server guarantee.
- **Receipt lifecycle:** creates one fractional-net HUF receipt with a stable
  logged call id and unique order, queries by number and order, checks PDF,
  identity, totals and tenders. Deliberately repeats only the completed verified
  create, expecting 338; then reverses and queries both original and SN. Checks
  `%PDF-` signatures for create, query and storno replies, not full PDF rendering.
- **Automatic MNB:** creates a EUR receipt with bank MNB and no numeric rate;
  verifies a positive stored rate, currency and total, then reverses it.
- **Email:** supplies all four email details, then requests empty-block resend
  after the first acknowledgement and a 16-second delay. A September 11 run
  received code 153 for immediate resend, requiring at least 15 seconds;
  the delay spaces intended sends and never retries an unanswered one.
  Both acknowledgements are checked;
  inspect the inbox for two messages with the printed unique subject. A passing
  protocol check alone does not prove delivery or inherited contents.
- **Appearance:** the two #73 electronic→paper and paper→electronic storno
  experiments retain their existing assertions. Matching cases are core journeys.
- **Storno email (#223):** four individually selectable paper/electronic ×
  omitted/explicit cases, each with a fresh original containing the controlled
  recipient and `sendEmail=false`. The storno repeats verified appearance/date
  and sets a unique subject. Acknowledgement and queried reversal are checked;
  actual mailbox receipt and provider send-history need independent observation.
  Current docs redirect test-account notifications to the account email. The
  [dated record](research/2026-09-12-storno-email-delivery.md#controlled-live-execution-2026-09-12)
  reports mailbox receipt for both explicit cases only, with configuration limits.

Receipt prefix and inbox settings are checked before creation when needed.
Unanswered writes defer receipt cleanup; known receipts are otherwise reversed
by number, with SN type/original reference and original reversal verified.
Receipt call ids, order and numbers are printed before/after writes. Keep the
output as the recovery record; rerunning generates a new logical operation.
These probes were added from documentation hypotheses. Record actual dated
results separately before promoting them into verified behavior or core tests.
The [September 11 receipt record](research/2026-09-11-receipts-live.md) captures
passing lifecycle/MNB probes, the immediate-email-resend refusal and exact-number
recovery with verified cleanup. Use a receipt-only prefix of at most five
uppercase letters/digits (the observed code-337 limit); `RSPRB` was accepted
without prior UI registration. The fixed delayed email probe has not yet been
run as a complete scenario; its recovery resend was acknowledged separately.

### Dagger secrets and execution freshness

`ci.live(agentKey: Secret, runId: String, probes: Boolean = false,
receiptPrefix: String = "", receiptEmail: String = "", stornoEmail: String = "", filter: String = "")` is manual and
has no `@check`. It injects the key with `withSecretVariable`, never a command
literal. Give **each deliberate execution a fresh non-secret run id**:

```sh
dagger -c 'ci | live env://SZAMLAZZ_AGENT_KEY release-check-20260911-1 | export ./live-report-1'
# A second deliberate execution needs a different id, invalidating its exec cache.
dagger -c 'ci | live env://SZAMLAZZ_AGENT_KEY release-check-20260911-2 | export ./live-report-2'
```

The run id is injected after compilation, busting only the external execution
layer. Each scenario also generates a fresh UUID order key/external ids and
prints its run label and UUID before sending. Compare those records and the
JUnit timestamps to establish a second run executed. Reusing a run id with the
same inputs may return cached evidence. Never rerun an uncertain write merely
to obtain a new report. Reports are returned on success; failures and kept
server logs remain in the Dagger trace.

Use `--filter` with a nextest expression to select individual scenarios within
the chosen profile. It cannot expand the selection beyond that profile, and an
invalid expression or empty selection fails. The expression is passed as one
argument, not evaluated as shell code. For example:

```sh
# Only clearing; no receipt prefix or email needed.
dagger -c 'ci | live env://SZAMLAZZ_AGENT_KEY clearing-20260911-1 --probes --filter "test(clear_credit_entries_populated)" | export ./clearing-report'
# Optional receipt acceptance, without sending email.
dagger -c 'ci | live env://SZAMLAZZ_AGENT_KEY receipt-20260911-1 --probes --filter "test(receipt_lifecycle)" --receipt-prefix NYGTA | export ./receipt-report'
# A read-only core selection also works.
dagger -c 'ci | live env://SZAMLAZZ_AGENT_KEY taxpayer-20260911-1 --filter "test(taxpayer_query)" | export ./taxpayer-report'
```

Selected receipt scenarios require `receiptPrefix`; only email resend needs
`receiptEmail`. Those tests check their settings before creating a receipt.
Unfiltered `--probes` selects all eleven cases and requires receipt prefix,
receipt email and storno email settings; it deliberately requests notifications.
The storno email filter requires `stornoEmail`; each selected test checks it
before creating anything. Prefer a filtered investigation.

### Evidence and cleanup

Budapest civil dates are used. Run/order/external ids are printed before sends;
known numbers and ingress invocation ids are printed immediately when returned.
Keep nextest output/JUnit or the Dagger trace with the release evidence.

Assertion failures are caught long enough for best-effort cleanup. Known live
documents are reversed in dependency order (final before prepayment); remaining
proformas are deleted. Reversals require the exact reported storno candidate,
storno type and original reference, followed by a fresh query of the exact
original reporting `reversed = true` before dependent cleanup may proceed.
Local mock tests exercise incomplete/mismatched evidence, blocked reads, the
successful dependency sequence and retained uncertainty without another send.
Cleanup failures are separate diagnostics and stop dependent cleanup. An
unanswered write retains its exact intent diagnostic and defers mutation-based
cleanup: absence, timeout or elapsed time cannot settle it. Reconcile the
reported invocation/external id and exact request before further mutations.
Definitive direct-call refusals permit cleanup of earlier known documents;
worker faults stay conservative because a retained invocation may have sent.
Successful worker tests call `Restate::finish`; failing tests unwind with the
handle still owned so the harness keeps its server diagnostics. Cleanup is
best-effort, not rollback, and cannot run after a process abort.

This small live suite is release evidence about persisted business facts, not
proof of exactly-once effects under arbitrary vendor delays. Historical go-live
probes, including receipt expansion, remain separately selected work.
