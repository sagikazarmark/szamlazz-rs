# Release-readiness review

Reviewed current tree at `e41a9964e266088a4d22a4613108d97d81bab295` on 2026-09-10.
Scope: implementation, architecture, Rust and Restate practices, public contracts and operator experience.
Primary focus: `restate-szamlazz`; selective supporting review of Számla Agent, CLI and inbound receivers.
Release manifests, changelogs and publishing machinery were excluded.

## Verdict

**Implementation follow-up:** the approved hardening is implemented in the subsequent working-tree changes.
See [the verification record](2026-09-10-release-hardening.md). Findings below describe the reviewed baseline.

**Close, but I would hold a general production release pending recovery and caller-guidance fixes.**
The protected Order protocol is substantially stronger than the historical query-and-resend implementation.
The current code has durable pre-send uncertainty, acknowledged arming, execution-local one-use permission,
read-only reconciliation, pause, cross-invocation mutation guards, expected-document intent and authorized recovery.
The historical missing-marker duplicate-send finding does not describe this revision.

The remaining issues chiefly concern getting safely unstuck and telling callers what they may do after an
uncertain operation. There is also a financial recovery-guidance issue on the unkeyed Agent and two narrower
findings outside the worker. These are concrete findings, not a recommendation for a broad rewrite.

## Verification

All completed successfully:

```sh
cargo test --workspace --all-features
cargo clippy --workspace --all-features --all-targets -- -D warnings
RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test --workspace --all-features -- --ignored e2e_
```

The server identified itself as **1.7.8**. The last command ran **11 real-Restate tests**: three embedded
runtime tests and eight integration tests, including the full Order protocol and unresolved-write interruption,
pause, kill, authorization and recovery scenarios. The full workspace test command also passed its doctests.
Live Számla Agent tests remained ignored; no live financial documents were issued by this review.

The individual edge cases below are source-traced findings, not newly executed reproductions. Existing tests
pass but do not establish these missing branches. The real-runtime interruption matrix uses `create_invoice`;
operation-specific recovery evidence still needs its own coverage.

Restate grounding includes the versioned [Rust SDK 0.12 ContextSideEffects documentation](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/trait.ContextSideEffects.html),
the repository's source-linked [Restate practices research](../research/2026-09-10-restate-practices.md), and the
executed runtime tests. Repository historical vendor probes are evidence of observed behavior, not vendor
guarantees about maximum processing time or concurrent deduplication.

## Standards — Rust, API and operator experience

### R1 — High: credit-entry fault guidance authorizes renewal on insufficient evidence

**Location:** `crates/restate-szamlazz/src/service/agent.rs:72–77`, used for lost/inconclusive answers at
`:150–152` and cancellation/initialization failures at `:254–257`.

The additive fault tells callers: “if entries are still missing, send only those entries with a new
Idempotency-Key.” The replacing fault likewise tells them to query and send the current intended snapshot.

A first registration can still be processing after its answer is lost. A subsequent query can show the old
entries. Following this advice sends again while the original remains unresolved: additive entries can land
twice, or the delayed original replacement can overwrite a newer replacement. Serializing client calls alone
does not settle an earlier external operation that outlived its HTTP request.

**Before release:** require settlement of the earlier send before deliberate renewal, explicitly stating that
missing entries or elapsed time alone do not settle it. Align fault strings, contract rustdoc and README examples.
Keep the unkeyed Agent's weaker execution guarantee explicit; Order's marker does not protect this operation.

### R2 — Medium: advertised retry controls no longer govern Order writes

**Locations:** `crates/restate-szamlazz/src/config.rs:66–83`;
`crates/restate-szamlazz/src/service/recovery.rs:399–430`;
`crates/restate-szamlazz/src/service/handlers.rs:83–94`;
`crates/restate-szamlazz/src/service/storno.rs:365–379`.

`WorkerConfig.issue` is documented as the create/storno retry policy, but protected Order writes hard-code
`max_attempts(1)`, and reconciliation has no explicit run policy. Its cadence and pause threshold come from
handler attributes. The remaining production use of `config.issue.run_retry_policy()` is unkeyed Agent storno.
Changing `[issue]` therefore does not tune Order reconciliation, despite the public configuration description.

The README simultaneously describes the correct one-use protocol (`:734–760`) and the old issue-policy
create/re-execution protocol (`:808–824`). Its earlier credential guidance (`:199–205`) still mentions renewal
with a new key. This makes the operator's retry model unnecessarily difficult to determine.

**Before release:** document precisely which operations each setting controls, consolidate the issuing and
recovery descriptions, and show how a host configures effective Order invocation policies if customization is
supported. Preserve one-use send permission regardless of retry settings. The issue-delay validation error's
claim that a delay prevents overlap (`config.rs:208–212`) also needs qualification.

### R3 — Medium: protected uncertainty discards useful, already-sanitized evidence

**Locations:** `crates/restate-szamlazz/src/gateway/recovery.rs:59–67,90–97,103–110`;
`crates/restate-szamlazz/src/service/recovery.rs:403–425`.

Protected writes collapse their failure to a unit `WriteResult::Unresolved`. Reconciliation similarly collapses
answered vendor/credential codes and transport failures. The retained read emits one generic error. An
unconfirmed storno's returned candidate number can also disappear at this boundary.

Operators consequently cannot reliably distinguish HTTP failure, malformed reply, open vendor code, absent
document or identity mismatch from the retained result. This is particularly costly for operations that now
deliberately stop and require human evidence. Safe diagnostic categories already exist in
`gateway/diagnostic.rs`; preserving these does not require retaining raw XML, credentials or source errors.

**Recommended before broad production use:** journal the sanitized original-send cause and available candidate
number; retain or expose the latest safe reconciliation reason. Unknown remains unknown, but explain why.

### R4 — Medium, Adatkapcsolat: receipt artifact names can silently collide

**Locations:** `crates/szamlazz-adatkapcsolat/src/archive.rs:240–243,397–409`.

Receipt JSON uses a sanitized business number in preference to the required record id. Distinct numbers such as
`NY/1` and `NY-1` become the same path in the same month; a numeric number can also collide with another receipt's
id fallback. Default Overwrite replaces the first artifact, and successful archiving allows both deliveries to
be acknowledged. With source XML disabled there may be no retained copy of the first record.

**Correction:** key receipt artifacts by `info.id`, optionally including the business number for readability.
Verify distinct ids remain distinct across sanitization and fallback cases.

### R5 — Medium, CLI: storno silently defaults an electronic original's reversal to paper

**Location:** `crates/szamlazz-cli/src/commands/invoice.rs:251–257`.

The CLI uses `StornoInvoice::new` without deriving or exposing `e_invoice`; the constructor defaults to false.
The repository's vendor evidence establishes that szamlazz.hu accepts this mismatch and the reversal takes the
request's form (`docs/szamlazz-hu-behaviour.md:98`). The worker already derives appearance from its verified original.

**Correction:** have the CLI query and derive the original's appearance, with an explicit policy for unknown
appearance, or expose a clearly documented choice. Do not silently select paper merely because the operator
used the CLI.

## Spec — protected Order protocol

### R6 — Medium: duplicate-order refusal can be turned into indefinite uncertainty

**Requirement:** `docs/design/order-write-protocol.md:22` says “Settled original-send evidence is data”;
`:67–69` allows the owning invocation's recorded refusal to settle its sole send.

**Locations:** `crates/restate-szamlazz/src/gateway.rs:1287–1289,1333–1351`;
`crates/restate-szamlazz/src/gateway/recovery.rs:59–67`.

Sequence: the sole permitted create receives code 71/152; the diagnostic external-id re-query fails;
`after_duplicate` turns the result into `Unconfirmed`; protected create journals only `Unresolved`.
If subsequent external-id queries are empty, the invocation pauses and blocks the order despite having received
a conclusive refusal to its only send. Ordinary duplicate refusals should not require manual non-execution
attestation because an optional diagnostic read failed.

**Correction:** retain the original refusal in the protected result independently of the best-effort duplicate
lookup. Add a protected-path test for 71/152 followed by an unanswered diagnostic query and subsequent absence.

### R7 — Medium: storno recovery rejects valid by-number reversal evidence

**Requirement:** `docs/design/order-write-protocol.md:60–61`: “Storno evidence verifies the reversal's original
reference and the original's reversal.”

**Location:** `crates/restate-szamlazz/src/gateway/recovery.rs:119–137,159–181`.

All evidence first requires a holder under the marker's external id, including an operator-supplied document
number. Sequence: verification sees a live original; another writer reverses it; the protected storno is an
idempotent repeat, whose reply is lost. The new request's external id is not attached on repeat storno, as
observed in `docs/szamlazz-hu-behaviour.md:69,95`. The matching reversal and reversed original can both be available
by number, but recovery returns unresolved at code 7 before checking either.

**Correction:** add storno-specific by-number positive evidence, verifying candidate identity, original reference,
original reversal and order on the pinned account. Supplying a number must mean verifying that document, rather
than only comparing it to an external-id holder. Test the missing-external-id repeat-storno scenario.

## Recovery design decision required

### R8 — Medium, release decision: successful deletion has no truthful positive recovery path

**Locations:** `crates/restate-szamlazz/src/gateway/recovery.rs:184–185`;
`crates/restate-szamlazz/src/contract/recovery.rs:132–149`;
`crates/restate-szamlazz/src/service/recovery.rs:228–245`.

Deletion reconciliation always returns unresolved. Operator recovery accepts document evidence (which cannot
settle a Delete marker), or an assertion that the exact request **did not execute and cannot execute later**.

If deletion succeeded and its reply or journal completion was lost, even independent confirmation from vendor
support that the exact deletion completed cannot be represented by the supported recovery contract. A
non-execution assertion would be false. The marker therefore blocks later Order mutations indefinitely through
the normal interface. Consumed proforma issuance has an analogous limitation once its external-id query returns 7.

This follows the currently specified evidence menu: it is a design/control gap, not an accidental violation of
the deletion-absence rule. Refusing to infer success from absence alone is appropriate.

**Before release:** decide on a narrowly scoped audited positive-settlement mechanism, with operation-specific
evidence obligations, or explicitly accept and document the need for exceptional administrative intervention.
Do not repurpose non-execution attestation or make elapsed time/empty queries sufficient evidence.

## What is already strong

- Clear separation between wire client, Gateway projections, deterministic decisions and Restate handler code.
- Bounded identity types, checked decimal arithmetic, closed request decoding, open response tokens and structured
  ingress-fault decoding are good Rust/API choices.
- Non-deterministic external results are durably recorded; credentials are fetched only inside executing work,
  with fresh execution-local clients and cookie jars. Completed replay does not require credential access.
- Order's marker/acknowledged-arm/send ordering directly addresses unfinished-run re-execution; HTTP retries and
  redirects are explicitly disabled for the production Gateway.
- Cancellation preserves write uncertainty; pause retains the owner; kill leaves the marker; later mutations guard
  before prerequisites; unknown marker schemas fail closed.
- Expected-document reissue/deletion prevents stale permission from automatically targeting a replacement.
- Runtime identity, scope authorization, operator authorization and account/credential mapping have distinct,
  documented responsibilities. Immutable routing and exceptional replay are treated separately.
- Real Restate tests cover actual replay and interruption behavior, rather than relying solely on mocked contexts.

## Release recommendation

Fix R1 and close the normal recovery dead ends R6/R7. Resolve R8 explicitly before presenting the operator
surface as sufficient for production recovery. Align R2's public control descriptions and retain actionable
diagnostics (R3). Address R4/R5 when releasing those surfaces.

No broad architectural rewrite is warranted. A focused recovery-evidence pass, accurate control documentation
and targeted regression cases would make this a much stronger release candidate.

**Axis summary:** Standards/API: five findings, highest severity R1 (unsafe credit-entry renewal guidance).
Spec: two findings, both medium (refusal preservation and storno recovery). Separate design decision: R8.
