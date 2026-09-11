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
scenarios across the two `live` binaries; `probes` selects only the two
mismatching-appearance cases. A profile does not unignore a test: the external
commands must supply `--run-ignored only`. Nextest fails empty runs by default;
do not override that behavior.

## Normal development and CI

```sh
cargo t
cargo test --doc --workspace --all-features --locked
cargo hack check --workspace --feature-powerset --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
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

## Manual vendor-live acceptance

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

Missing/empty credentials or a missing required Restate source fail selected
tests. Credentials alone never enable them. Every lifecycle is a single test;
tests share no process state or execution-order assumptions. Live/probes run
serially, fail-fast, with **zero whole-test retries**. This is per runner, not a
cross-machine account lock. Their slow timer reports progress without killing a
possibly executing write; individual vendor/harness requests retain their own
timeouts. A run interrupted by infrastructure or an operator needs the same
reconciliation as any unanswered write.

Core scenarios:

1. Paper HUF invoice: half-forint rounding, create and queried PDF, persisted
   identity/type/order/currency/totals, replacement `[100]` → `[200]` then additive
   `[50]` credit entries (unordered comparison and returned outstanding amounts),
   previous-month fulfillment, matching storno appearance and relationships,
   own storno external id and repeated storno returning the existing reversal.
2. Proforma create/query/delete, then absence by number and external id.
3. Actual Restate ordinary e-invoice order: account probe, proforma consumption,
   same-key replay and fresh-invocation `already_issued`, observation, storno,
   ordinary `reversed`, exact-number reissue, newest external-id holder and stale
   expected-number `target_changed`.
4. Actual Restate EUR proforma/prepayment/final: explicit proforma reference and
   caller-supplied negative prepayment line at the same VAT rate, exchange rate
   400, fractional price, persisted references/totals/consumption and completed
   operation repetition. Full performance is 49.38 + 13.33; deduction is
   −24.69 − 6.67; final gross is 31.35. The vendor does not deduct automatically.
5. Read-only taxpayer lookup: valid and nonblank identity, no pinned company
   name/address. A NAV dependency failure fails this smoke explicitly.

`cargo probes` runs only the two #73 mismatching storno appearance experiments
(electronic→paper and paper→electronic). The established finding remains: storno
takes the request's appearance, with no server correction for a mismatch. The
matching cases now belong to the core lifecycles. Run probes for a specific
investigation, not before every release.

### Dagger secrets and execution freshness

`ci.live(agentKey: Secret, runId: String, probes: Boolean = false)` is manual and
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

### Evidence and cleanup

Budapest civil dates are used. Run/order/external ids are printed before sends;
known numbers and ingress invocation ids are printed immediately when returned.
Keep nextest output/JUnit or the Dagger trace with the release evidence.

Assertion failures are caught long enough for best-effort cleanup. Known live
documents are reversed in dependency order (final before prepayment); remaining
proformas are deleted. Reversals are verified by type and original reference.
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
probes and receipt expansion remain separately selected work.
