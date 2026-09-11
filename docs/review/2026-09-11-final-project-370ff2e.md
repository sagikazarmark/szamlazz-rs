# Final project release review

Reviewed: `370ff2e5398e9ff4a3aff3b6008ab82e38b9d058`, 2026-09-11.

## Implementation closure — working changes after review

All five findings below are addressed in the working changes based on the reviewed commit:

- S1: the migration inventory checks unfinished invocations and **every Order state key in every scope**,
  without decoding values. Unknown/unreadable state blocks. Redirects are refused so an admin bearer credential
  cannot follow another origin. Recovery precedes service privacy; fallback instructions explicitly restore
  Order's public eligibility while business calls remain blocked at the ingress gateway.
- S2: exact decimal decoding shares the Számla Agent numeric parser. JSON `arbitrary_precision` retains numeric
  tokens; strings, `Value` conversion and buffered Serde wrappers are covered. Unrepresentable money, quantities
  and exchange rates are `invalid_input` before the prologue. No currency rounding occurs during decoding.
- S3: fan-out diagnostics include every member's complete error chain, verified through router tracing.
- P1: supported corrective bases are ordinary/prepayment/final invoices; other types are refused before arming.
  Existing-target precedence remains. The vendor document-types page did not establish corrective-on-corrective
  support; further corrections name the original base as an explicit worker contract.
- P2: discarded duplicate diagnostic and immediate storno verification results emit credential warnings before
  fallback. Tests retain the original refusal/storno uncertainty and check credential-safe logging.

Closure review caught and corrected two decimal-composition regressions and the recovery-access ordering detail.
The migration state gate also exposed 11 markers in the old e2e flag-day journey: it previously switched despite
them. That journey now explicitly settles its simulated incidents with **test-only audited operator evidence**
before privacy/drain/state checks. Those attestations are not presented as vendor verification. The independent
killed-owner regression runs the actual Python inventory before and after recovery, then checks private recovery
ingress refusal and the final empty inventory.

Verification passed:

- Workspace all-feature tests and doctests; after the final numeric compatibility change, all four targeted
  decimal tests passed again.
- Final real-Restate suite: **32 scenarios** (3 library, 29 integration), server 1.7.8, mocked vendor.
- Clippy with `-D warnings`, formatting, `git diff --check`.
- Cargo feature powerset: **37 combinations**.
- Migration script suite: **5 tests**, including cross-origin redirect refusal; actual admin SQL exercised by e2e.

The initial e2e run failed at the newly added state gate, correctly detecting the old unsafe test migration;
the corrected final full run passed. An initial workspace build timed out while concurrent Cargo jobs held locks;
the subsequent workspace test run completed successfully. Production XML writers were not changed; the standalone
XSD evidence recorded below belongs to the preceding review, not a repeated execution in this implementation pass.

**Local code-review blockers are closed.** Before production go-live, execute the recovery drill through the actual
deployed host's authorization boundary and seller/scope verification through its deployed resolver/store. No host
URL or operator credentials were supplied, so those checks and fresh vendor-live calls were not performed. Changes
are left uncommitted.

## Verdict

The following verdict and findings describe the original reviewed commit; see implementation closure above.

**Hold final release sign-off.** The protected Order architecture is credible and well tested, but the scope
migration procedure can bypass retained uncertainty, and JSON money decoding silently changes unrepresentable
amounts. Resolve those before release. Also define and enforce corrective-base eligibility before signing off
that handler. Two diagnostic findings are lower-risk follow-ups.

Scope: current implementation, particularly `restate-szamlazz`, its Számla Agent boundary, Rust public contracts,
Restate execution and recovery, and caller/operator guidance. Selected receiver public surfaces were also reviewed.
This was not an exhaustive re-audit of every vendor operation/parser. Cargo metadata, changelogs and release
machinery were excluded from findings. No production source was changed and no vendor-live request was made.

## Standards: Rust, public API and operational contract

### S1 — High: scope migration can leave unresolved-write protection behind

References: `docs/design/restate-szamlazz.md:952–963`, `crates/restate-szamlazz/README.md:152–155,1309–1313`,
`crates/restate-szamlazz/src/service/recovery.rs:393–402,419–453`.

The single-to-multi flag-day procedure waits for no non-completed invocations, then switches scopes with “no
data migration.” A cancelled or killed write can be completed in Restate while its unresolved marker persists.
That marker belongs to the old scope. The same order under the new scope has empty state and passes `guard`,
while retaining the same vendor account and external ids. An absent external-id query then permits another write
although the old request may still execute.

This contradicts `docs/operations/order-recovery.md:13–30`: completion/kill does not settle uncertainty. The newer
protected-write document acknowledges scope remapping as outside its protection, but the concrete migration
procedure still needs safe preconditions.

Fix: require independent settlement of external uncertainty and an inventory proving no unresolved **or unreadable**
markers under the old identity before switching. Recover under the original scope. Keep the existing producer
quiescence and drain requirements. Make “no data migration” conditional on clean uncertainty state. Add a migration
regression with a killed owner and retained marker; the existing successful flag-day journey does not cover it.

### S2 — Medium: JSON money decoding rounds before the explicit rounding policy

References: `crates/restate-szamlazz/src/contract/document.rs:288–302,344–355`,
`crates/restate-szamlazz/src/contract/agent.rs:454–465`, `crates/restate-szamlazz/src/service/body.rs:51–59`.

`Decimal`'s ordinary deserializer accepts a string it cannot represent exactly by rounding it. Reproduced against
the built worker through SDK `Json<LineItemInput>` decoding and `to_line_item`:

```json
{"name":"boundary","quantity":"1","unit":"db","unit_price":"0.49999999999999999999999999999","vat_rate":"AAM"}
```

The decoded price is `0.5000000000000000000000000000`; HUF calculation produces net/gross **1**, although the
submitted value lies below the half-forint boundary. The arithmetic checks cannot detect the earlier alteration.
Other input Decimal fields, including credit-entry amounts, share this boundary.

Fix: exact input parsing with `invalid_input` for values that cannot be represented exactly. Apply consistently to
quantity, money and exchange rates; preserve numeric JSON token precision too. Add decoding-to-calculation tests
for rounding boundaries and underflow. This is a correctness finding, not a preference about type names.

### S3 — Medium: Fanout member source chains disappear from receiver logs

References: `crates/szamlazz-adatkapcsolat/src/fanout.rs:140–159`,
`crates/szamlazz-adatkapcsolat/src/axum.rs:520–543`.

`FanoutError` retains member errors, but its Display prints only their outer messages and its Error implementation
exposes no source. The router's source-chain formatter therefore cannot reach a member's root cause. A contextual
“probe failed” wrapping “database down” logs only the former. This conflicts with the router's stated diagnostic
contract, although delivery still correctly returns retryable 500.

Fix: render each member's full chain with its identity in the aggregate diagnostic; assert router tracing with
multiple contextual member errors. This is an observability follow-up rather than a data-integrity blocker.

## Spec: durable writes and resilience

### P1 — Medium: corrective-base validation permits non-invoice documents

References: `crates/restate-szamlazz/src/service/create.rs:437–455,680–707`.

`decide_base` checks order ownership and reversal, but not document type. A fresh correction id naming this order's
live proforma, delivery note, storno or unknown type proceeds to creation. The contract calls the target a “live
invoice”; the test with that name covers ownership/reversal only. The design at
`docs/design/restate-szamlazz.md:475–479` likewise omits the eligible type set.

This sends an unsuitable target rather than rejecting it locally. Even if the vendor refuses, loss of that answer
or interruption of the open write leaves an avoidable order-wide recovery incident. Vendor acceptance of such
correctives was not established and is not claimed here.

Fix: define supported corrective-base types and enforce them before marker preparation, retaining existing-target
precedence. Test non-invoice and unknown types. This needs a small domain-contract decision, not a broad redesign.

### P2 — Medium: diagnostic credential failures can be discarded without alerting

References: `crates/restate-szamlazz/src/gateway.rs:1304–1313,1364–1367,1404–1405,1781–1793`,
`crates/restate-szamlazz/src/gateway/recovery.rs:238–269,374–386`.

After a protected create's conclusive 71/152 refusal, `after_duplicate` can return a credential rejection from its
diagnostic query. The protected branch replaces that result with the original refusal without the credential
warning. Similarly, immediate verification of a numbered storno reply calls `reconcile_write_checked` directly
and collapses every non-success result into generic verification uncertainty. Its credential result bypasses the
warning emitted by the later `reconcile_write` wrapper.

The original refusal should remain settled and the ambiguous storno should remain uncertain. The defect is loss
of the actionable credential signal, especially if later observations only encounter transport failures.

Fix: warn at these result-consumption sites before fallback/reduction, preserving existing settlement decisions.
Add log assertions to duplicate diagnostic cases and immediate storno verification. This follows
`docs/design/restate-szamlazz.md:700–703` and the alert-before-fallback rule in
`docs/design/order-write-protocol.md:121–123`.

## Architecture and resilience assessment

- The Gateway/domain/service split provides useful boundaries. Validated identities and configuration, closed
  requests, open response tokens and structured fault decoding make the Rust-facing contract coherent.
- External observations are journaled; SDK context commands are kept outside run closures and runs immediately
  awaited. Credentials are acquired only for executing external operations and excluded from journal results.
- Protected Order writes use marker → acknowledged arm → one-use execution-local permission → write → read-only
  reconciliation. The local permit deliberately distinguishes executing an arm from replaying its recorded unit;
  it does not determine a different context-command prefix outside a run. Real protocol acknowledgement-loss tests
  substantiate this SDK-version-sensitive mechanism.
- Kill/cancellation do not erase markers, uncertain owners pause, and later mutations fail closed. Recovery checks
  the exact marker and pinned account, journals authorization/evidence, and supports both verified document evidence
  and explicitly audited operator settlement. There is no timeout-based clearance or blind recovery resend.
- Expected document numbers protect reissue/deletion intent. Storno derives fulfillment and appearance from the
  original. External-id collisions and contradictory evidence do not become absence.
- The unkeyed Agent credit-entry handler remains a materially weaker surface: interruption can repeat an open
  write, additive entries can duplicate, and replacement requires caller serialization plus independent settlement
  of delayed execution. This is documented, not equivalent to protected Order guarantees.
- Deployment readiness still includes the actual host's operator-authorization drill and seller/scope verification.
  The existing acceptance record explicitly leaves that host-specific verification outstanding.

Restate grounding included the repository's version-pinned research and fresh reads of SDK 0.12.0
[`ContextSideEffects`](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/trait.ContextSideEffects.html) and
[official versioning guidance](https://docs.restate.dev/services/versioning), particularly run ordering and persistent
Virtual Object state versus immutable invocation deployments.

## Verification performed

Passed on the reviewed tree:

- `cargo test --workspace --all-features --locked`, including doctests; externally dependent tests remain ignored.
- `RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1`:
  **30 passed** (3 library and 27 integration scenarios), actual Restate with mocked vendor.
- `cargo fmt --all --check`.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- `cargo hack check --workspace --feature-powerset --locked` (via installed cargo-hack binary): **37 combinations**.
- Standalone request XSD validation, using `nix shell nixpkgs#libxml2`: **430 generated requests**, 790 valid
  source/request verdicts, 70 explicitly expected source conflicts, 7 negative controls, complete declared-path
  coverage for included sources.
- Schema-runner regression suite: **8 passed**.
- Isolated money-decoding reproducer in `/tmp/opencode/final-release-money-370ff2e.rs`.

The initial standalone schema invocation lacked `xmllint`; the documented Nix invocation then passed. No fresh
vendor-live acceptance or deployed-host authorization test was performed. Prior live evidence is recorded in
`docs/review/2026-09-11-worker-release-acceptance.md`, and is distinct from this run.

Summary: Standards axis **3 findings**, highest **High** (scope migration); Spec axis **2 findings**, highest
**Medium** (corrective eligibility and lost credential diagnostics). No confirmed high-severity defect was found
inside the protected Order send/replay state machine itself.
