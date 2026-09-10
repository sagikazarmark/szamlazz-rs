# Deployments are immutable; the journal carries no cross-version compatibility contract

Status: accepted. Supersedes the #47, #125 and #127 amendments of
[ADR 0005](0005-stateless-order-szamlazz-hu-is-the-source-of-truth.md) (the *Journal compatibility*,
*Crate-owned projections* and *Completeness by mechanism, and the archive rule* sections) and design §10's
"a rolling update is safe" guidance.

## Context

ADR 0005's amendments built a compatibility contract for the journal: every type a `ctx.run` writes is
additive-only, pinned by one JSON fixture per variant under `tests/journal/`, replayed through the current types
by a compatibility test, registered through a sealed `Journaled` trait and a `journaled!` list, guarded against
leaking the agent key or the document body, archived on every reshape (`<variant>.<n>.json`), and, after go-live,
never deleted while the type is journaled. The rule exists so that an in-flight invocation started by one
deployment can replay its journal against the **next** deployment's code without being killed.

That premise is the deployment model the design assumed: a rolling update **in place**, the same endpoint URI
re-registered with new code, in-flight invocations resuming on whatever code answers at that URI (design §10,
*Shutdown*: "a rolling update is safe; a drain-first update is the quieter one").

That is not how Restate is meant to be operated. Restate's own model is **immutable deployments**: each release
is registered under a new URI (or Lambda version ARN, or operator-managed `RestateDeployment`), Restate routes
new invocations to the latest deployment and **pins every in-flight invocation to the deployment it started
on**, retries included, until it completes; the old deployment is removed once `restate deployment describe
<id> --extra` shows it drained. Restate's versioning docs state the consequence directly: "no version
compatibility logic is needed in your code". Re-registering the same URI with `--force` is documented as a
local-development convenience whose known effect is exactly the RT0016 journal-mismatch failure the contract was
built to prevent.

So the contract, and everything built to enforce it, substitutes in code for a guarantee the runtime gives
for free when it is operated as designed. Its cost is not small: `service::journal` (about 1,400 lines, a test
harness with tests of its own), 63 fixture files, the sealed trait and its macro, the registry test, the
`DELIBERATE_BREAKS` list, the archive rule, and the prose explaining all of it in the `gateway` module docs, the
README, CONTEXT.md and ADR 0005. It also shaped the code: `Journaled` is the bound of every run helper, and the
#70 *Widening* exception and the #127 projection rewrite were both driven by the fixtures rather than by a need
of the handlers.

## Decision

1. **A release is a new deployment.** The host registers each release under a URI of its own and keeps the
   previous release running until Restate reports it drained (`restate deployment describe <id> --extra` shows
   no invocations). Nothing is ever re-registered in place with `--force` outside local development. The drain
   has no fixed bound: run exhaustion thresholds can overshoot and do not interrupt a hung closure;
   same-key invocations queue serially behind the one holding the key, a crashed handler is
   re-dispatched under its invocation retry policy, and a paused invocation waits for an operator; the report is
   the check, not a clock. (Restate's guidance to keep handlers short so that old deployments drain quickly is
   met as far as the worker can meet it: no handler sleeps or awaits an awakeable.)

2. **The journal has no cross-version compatibility contract.** The additive-only rule, the per-variant JSON
   fixtures under `tests/journal/`, the generator and its `UPDATE_JOURNAL_FIXTURES` mode, the compatibility test
   (fixture → current type → superset), the `DELIBERATE_BREAKS` list and the archive rule are removed. A journaled
   type may be reshaped in any release: under normal routing no invocation started under the previous release
   decodes it with the new code. Operator-selected exceptional replay (deployment-changing resume or restart
   from a retained prefix, amendment below) requires its own review and is refused across incompatible changes.

3. **What stays**, because it costs almost nothing or serves a different purpose:
   - the crate-owned projections `FoundDocument` / `IssuedDocument` stay, for the reason #127 also gave that
     has nothing to do with replay: what the handlers never read (the buyer, the seller, the line items, the
     PDF) does not belong in an entry the Restate UI shows for the retention period, and the agent key must
     never be in one. `service::journal` keeps that as its purpose: a sample of every variant of every journaled
     type round-trips through serde (a unit test of the derives, not a contract), carries no sentinel agent key
     and carries no document-body key;
   - the sealed `Journaled` marker and the `journaled!` list stay as the run helpers' bound, **re-purposed**: not
     a compatibility marker (nothing about replay) but the registry of the privacy scan, so that "every entry is
     scanned" is a mechanism and not a convention: only a listed type can be a `ctx.run` result, the scan's
     samples are held to the list by type name, and each enum's samples to an exhaustive `variants!` match, so a
     type or variant journaled without a sample fails by name. It is the one part of #125's "completeness by
     mechanism" whose reason survives the contract;
   - the *Step-name table* (`RUN_NAMES`, checked against `sys_journal` in the e2e) stays as a regression signal
     for exceptional replay. It checks allowed path patterns, not whether a particular old invocation takes the
     same branch. Its diff helps identify changed run sequences; compatibility still requires reviewing the actual
     invocation prefix, branch logic, exact context commands, serialization and inputs (amendment below).
     The target-first cleanup inserts an ownership `lookup-{kind}` before prerequisites and adds
     `hint-storno-{number}` paths for reversed non-corrective targets. Its sequence differs from the previous
     deployment's: **do not resume an invocation from that sequence onto this release**, or restart with that
     incompatible prefix. Keep it on its original deployment; reconcile effects before choosing a new invocation.

4. **The journaled value types are closed themselves** (*amendment, 2026-09-09, #175*). `Defaults`,
   `SellerConfig` and `SellerEmailConfig` carry `#[serde(deny_unknown_fields)]` and the static resolver reads its
   `[account.defaults]` and `[account.seller]` tables as them directly; the `StaticDefaults` / `StaticSeller` /
   `StaticSellerEmail` mirror types, their two field-by-field `From` impls and the round-trip test that held the
   copies equal are removed. They existed so that the value types could "stay permissive so that an `account`
   entry of an earlier deployment replays"; under item 2 no entry is decoded by a later release, so nothing asks
   them to be permissive, and a closed type refuses a misspelt key at the one point it can be caught. The
   consequence an embedder sees: a resolver of its own that deserialises `Defaults` from its own storage is
   held to the same rule (an unknown key is a parse error), which is the intended shape.

## Replay-safe initialization (#200, 2026-09-10)

Credential acquisition and Gateway initialization now occur inside an executing operation Run. Completed
operations replay without either. The step-name sequences, success-result types and operation inputs are
unchanged; initialization failure is now a terminal failure of the operation Run rather than an early
handler Output. The old early Output could diverge from recorded operation commands (RT0016, reproduced
on server 1.7.8 / SDK 0.12.0 / shared core 7.0.3).

Normal immutable-deployment routing still applies. Exceptional replay onto this release requires reviewing
all intervening changes, including the target-first sequence change above. Matching step names alone does
not authorize a cross-deployment resume or prefix restart. Retained deployments using `StaticResolver` retain startup keys;
a new deployment does not update their credential store.

## Exceptional replay (#204, 2026-09-10)

Immutable deployments remain the normal routing recommendation, and justify having no general cross-release
journal compatibility contract. There are two deliberate exceptions to review:

- **Resume with a changed deployment.** In server **1.7.8**, `resolve_pinned_deployment` keeps the existing pin
  when no patch is supplied (or `KeepPinned` is selected). An explicit deployment or latest-deployment patch can
  replace it, subject to a protocol-version check.
- **Restart from a retained journal prefix.** Server **1.7.8** `restart_as_new` creates a new invocation from a
  completed invocation's retained prefix, copies the associated completions, inherits the deployment pin unless
  patched, and clears the original idempotency key. Commands beyond the copied prefix may execute again.

These facts are source-verified at the pinned tag, not a runtime repair probe:
[`manual_resume.rs`](https://github.com/restatedev/restate/blob/v1.7.8/crates/worker/src/partition/state_machine/lifecycle/manual_resume.rs)
(`resolve_pinned_deployment`) and
[`restart_as_new.rs`](https://github.com/restatedev/restate/blob/v1.7.8/crates/worker/src/partition/state_machine/lifecycle/restart_as_new.rs)
(`OnRestartAsNewInvocationCommand::apply`). Verify the deployed server version, the CLI/API request and the
selected deployment before acting; current unversioned [invocation management](https://docs.restate.dev/services/invocation/managing-invocations)
and [versioning](https://docs.restate.dev/services/versioning) guidance must not override these version-specific defaults.

For either path review the actual invocation's retained prefix against the candidate code: **branch logic,
exact context commands (including names and parameters), serialization and operation inputs**, not only the
`ctx.run` names. The step-name table is a regression signal, not proof of replay compatibility; an old result may
choose a different branch even when both paths remain allowed by the table. The server's protocol check is not
this application review. For restart, also reconcile effects of operations outside the copied prefix before
deliberately allowing them to run again, even if the deployment is unchanged. Kill does not undo those effects.

Keep the original deployment if compatibility cannot be established. Completion/idempotency retention preserves
answers for deduplication; journal retention makes prefix restart possible while history is retained. Neither
keeps the old code available or provides a deadline for draining it. After normal drain, remove a deployment only
when it is no longer needed for an intended repair. Unresolved-write exhaustion/kill policy belongs to
[#205](https://github.com/sagikazarmark/szamlazz-rs/issues/205), not to a claim that a fresh invocation or prefix
restart is automatically safe.

## Considered options

- **Keep the contract as a belt beside the braces.** Rejected. Belt and braces is a fair argument for a rule that
  costs a line; this one costs a module, a fixture tree and an archive discipline whose false positives (a
  fixture regenerated without its archive) are as expensive to argue about as the failure they prevent, and it
  guards a path (`--force` re-registration in production) the deployment procedure now forbids.
- **Keep the fixtures, drop the mechanism** (the sealed trait, the registry, the archive rule). Considered as
  the middle ground. Rejected because the fixtures' only reader is the compatibility test, and a compatibility
  test without a compatibility requirement asserts nothing. The opposite cut is what was taken: the fixtures go,
  the mechanism stays for the privacy scan (item 3), since that scan is the one reason left for wanting "every
  type, every variant" to be enforced rather than remembered.
- **In-place updates with the contract, as before.** Rejected: it is the model Restate documents as unsafe, and
  the contract covered only the *shape* of an entry, not the sequence or inputs of the steps, which Restate lists
  as the other three unsafe in-place changes; the *Run-name pin* covered the sequence, and nothing covered the
  inputs. The contract was never complete protection for the path it was built for.

## Consequences

- Design §10's *Shutdown* paragraph is rewritten: a release is a new deployment, in-place re-registration is
  local development only, and the go-live and rolling-update guidance describe register → drain → remove.
- ADR 0005's three journal amendments are marked superseded by this ADR; the rest of ADR 0005 stands.
- CONTEXT.md's *Journaled type* entry is replaced by a short one (a `ctx.run` result type; crate-owned; carries
  neither the agent key nor the document body; no compatibility rule), and *Run-name pin* is reworded to name
  exceptional replay as its reason. The word "pin" is retired from the journal fixtures' vocabulary.
- The e2e's flag day (`Harness::switch_to_multi_account`, the second `deploy` into one server) keeps proving that a second deployment can
  be registered beside a first; it no longer proves anything about replay across them, and says so.
- The host's deployment procedure carries the one operational obligation this ADR creates: do not remove, stop or
  `--force`-replace a deployment Restate still reports invocations on. `restate deployment describe <id>
  --extra` is the check.
