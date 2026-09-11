# Independent SPEC review: e41a996…4394ed0

## Spec summary

**The two earlier claims are confirmed, with an important attribution correction: both predate this diff.** The reissue defect reaches the production Order handler; the unsafe retry contract affects direct public Gateway consumers. No additional newly introduced send-permission or marker-clearance safety regression was established in the 20 reviewed commits.

- **High — contradictory reissue success clears the marker.** The protocol requires “a journaled conclusive result” before clearance and says to “exclude the expected old reversed number for reissue.” With expected `SZ-OLD`, owned reversed `SZ-OLD` in all three reads, and a numbered success also naming `SZ-OLD`, the actual Order returns `issued(SZ-OLD)` and clears state. A second fresh invocation sends again. This is now reproduced through real Restate, extending the earlier review's Gateway-only reproduction.
- **High, direct-consumer boundary — public Gateway recommends unsafe create retries.** Its documented “caller's run retry policy re-executes the step” conflicts with the recovery decision: “An empty query … never authorizes another send.” Two calls for the same invisible corrective produced two create POSTs and four queries. Production Order uses `protected_create`, not this public method.
- **Additional specification inconsistency — same-number storno success is treated as a settled no-op even for a verified live invoice.** The protected protocol says “Missing or contradictory identity retains uncertainty,” while the broader design explicitly says “echo of the requested number → NotStornoable.” The implemented echo policy clears the marker; two fresh invocations sent twice in a real-Restate reproduction. This is an inherited policy gap, not a newly introduced implementation deviation. Resolve the conflicting requirements explicitly.

The reviewed replay machinery and operator recovery satisfy their specified structure: acknowledged arming, volatile one-use permission, read-only replay/reconciliation, exact-marker matching, pinned-account verification, admitted-operator replay, and evidence recorded before clearance. Selected existing checks passed: **334 ordinary tests and 19 real-Restate tests**, plus **three temporary reproduction tests** asserting the behaviors above. No vendor requests were made.

## Scope, source precedence and method

- Fixed point: `e41a9964e266088a4d22a4613108d97d81bab295`.
- HEAD: `4394ed0977a0adf298a10e0acd9182d13cbf3c0c`, verified before and after review.
- Comparison: `git diff e41a9964e266088a4d22a4613108d97d81bab295...HEAD`.
- Reviewed production worker Order creation/correction, storno, deletion integration, Gateway exchange/settlement, automatic and operator recovery, marker contract, handlers and execution initialization. Read ADRs 0004, 0012 and 0013, `docs/design/order-write-protocol.md`, `docs/design/unresolved-order-writes.md`, the relevant sections of `docs/design/restate-szamlazz.md`, and `docs/operations/order-recovery.md`.
- Applied the latest CONTEXT/ADR amendments over historical kill/query-first/stateless wording. Retrieved originating issues [#216](https://github.com/sagikazarmark/szamlazz-rs/issues/216) and [#206](https://github.com/sagikazarmark/szamlazz-rs/issues/206) with `gh issue view … --json title,body,comments,state`.
- Reviewed the implementations before consulting the existing release report. That report was then used to distinguish the earlier evidence from the fresh protected-handler reproduction here.
- No production edits, vendor-live tests, full-workspace release checks, or e2e flake investigation. Existing untracked review documents were preserved. A uniquely named temporary integration test was added, run and removed.

### Commit list

From `git log e41a9964e266088a4d22a4613108d97d81bab295..HEAD --oneline`:

```text
4394ed0 fix(restate-szamlazz)!: validate account configuration and mutation identities
46fab05 test: refine live assertions and cover worker query
4488f37 fix(restate-szamlazz): validate credit reply identity and exact balances
835a370 fix(szamlazz-agent)!: preserve optional response facts and paid intent
d85cdf2 test: strengthen live acceptance and enable targeted probes
99762c1 fix(restate-szamlazz)!: preserve rotation uncertainty and tighten decimal inputs
28dcec1 fix(restate-szamlazz)!: enforce exact inputs and safe scope migration
370ff2e fix(restate-szamlazz)!: validate before arming and harden recovery controls
77d53c5 fix(szamlazz-agent)!: preserve boolean facts and validate success verdicts
10f00b3 fix(restate-szamlazz)!: validate corrective evidence and preserve read faults
eec57fc fix(szamlazz-agent)!: validate request dates and check vendor schemas
bed1d24 fix(restate-szamlazz)!: preserve vendor numbers in recovery evidence
5c6d5ea fix(restate-szamlazz)!: validate document identity before settlement
61c334f feat(szamlazz-agent): add explicit credit clearing and receipt probes
837dad0 fix!: harden receipt archive paths and CLI storno
1b08368 test: add opt-in vendor-live journeys and nextest profiles
9b78546 fix(szamlazz-agent)!: redact legacy keys and enforce response identity
028dfcd fix(restate-szamlazz)!: harden write settlement and recovery
2ba5fb8 fix(szamlazz-agent)!: normalize namespaces and preserve response facts
f83e5fd fix(szamlazz-agent)!: retain response evidence and tighten XML parsing
```

## F1 — High: an old-number reissue acknowledgement becomes settled issuance

### Requirements

`docs/design/order-write-protocol.md:33`: “A journaled conclusive result precedes clearing `unresolved-write` and returning the operation result.”

`docs/design/order-write-protocol.md:73–75`: document evidence must “validate external-id ownership, kind, order and corrective base, and exclude the expected old reversed number for reissue.”

`docs/adr/0013-audited-positive-write-settlement.md:9–12`: “issuance excludes the old reissue target”; the effect must include “corrective base and reissue identity.” ADR 0012:21–33 binds the operation to replacement of the named reversed holder.

### Production path

1. `crates/restate-szamlazz/src/service/create.rs:942–955,1032–1040` carries the full lookup's expected reversed number into both the write request and marker.
2. `crates/restate-szamlazz/src/gateway.rs:1254–1258` accepts every `CreationOutcome::Issued(created)` as `CreateOutcome::Issued`, without comparing the reported number with `request.reversed`.
3. `crates/restate-szamlazz/src/gateway/recovery.rs:193–196` classifies that as a settled `WriteResult::Create`.
4. `crates/restate-szamlazz/src/service/recovery.rs:490–508` reconciles only `WriteResult::Unresolved`; the issued result bypasses reconciliation and reaches `ctx.clear(STATE)`.
5. `crates/restate-szamlazz/src/service/create.rs:117–128` returns `issued` with that old number.

The same number is correctly excluded in automatic/operator document verification at `gateway/recovery.rs:328–334`, and in audited completion at `service/recovery.rs:294–298`. The direct reply is the inconsistent evidence channel. A numbered notification-failure acknowledgement reaches the same issued branch.

### Conditions and impact

The caller requests `options.reissue = {"expected_number":"SZ-OLD"}`. Target ownership, full lookup and armed leading query all report this Order's reversed `SZ-OLD`. The single send returns a syntactically successful numbered envelope naming `SZ-OLD` itself. No replacement or non-execution is established. Nevertheless, the response says `issued` and uncertainty state disappears. If a subsequent invocation still sees the same reversed holder, it acquires another permit and sends again; any unresolved external effect from the first exchange is no longer guarded.

This requires contradictory upstream response evidence; no observation here establishes that szamlazz.hu produces it live. It is an actual fail-open classification under that input, not a hypothetical route inferred only from a helper.

### Fresh reproduction

Temporary test `protected_handlers_clear_markers_on_contradictory_numbered_replies` used the production `Order`, normal validated worker configuration, a static resolver with a loopback endpoint, the actual Restate 1.7.8 server, and wiremock:

- `acct:REVIEW-REISSUE:invoice`: reversed `SZ-OLD` throughout; **six** reads across two invocations.
- Other relevant external ids and order hint: code 7.
- Create replies: `created("SZ-OLD", "1000", "1270")`; **two** POSTs.
- Both calls: HTTP 200, `outcome: issued`, `invoice_number: SZ-OLD`.
- Both journals: `arm-write` and `ClearState`; no `reconcile-write`.
- Fresh `observe_unresolved` after each call: `state: absent`.

Separate test `public_reissue_accepts_old_number` confirmed the shared public Gateway path too.

### Origin and recommendation

This is **present at the base**, not introduced by `028dfcd`'s change from an explicit settled-outcome allowlist to `Ok(outcome)`: that original allowlist already accepted `Issued`. The direct conversion dates to `f9e91ce`; expected-document intent landed in `1764f7a`; `e41a996` attached marker clearance to this result. Verified with blame and the base versions of both recovery modules.

Reject the old-number reply as evidence of replacement: retain a safe diagnostic, enter `Unresolved`, and require matching new-document evidence or authorized independent settlement. Apply the guard in the shared send boundary so public and protected paths agree. Retain an end-to-end regression covering the marker, a blocked subsequent mutation and numbered code-56 success as well as an ordinary success.

## F2 — High for direct consumers: unsafe public create retry contract remains

### Requirements

Issue #216: “An empty query, elapsed time, retry exhaustion, cancellation, kill or a new Idempotency-Key never authorizes another send.”

ADR 0004:329–335 requires guarding the owner's open send closure, and `docs/design/unresolved-order-writes.md:56–60` states that visibility/timing observations and ordinary duplicate-order refusals do not establish deduplication for every kind.

### Path, conditions and impact

`crates/restate-szamlazz/src/gateway.rs:1198–1205` publicly advises that the “caller's run retry policy re-executes the step.” The module introduction at 5–16 and `Unconfirmed` at 479–490 repeat that description.

`create_inner` at 1221–1245 queries and sends on absence. An unanswered send passes through `create_send(..., false)` at 1302–1308 into `settle_or` at 1341–1352; an empty re-query returns `Err(Unconfirmed)`. Calling the method again repeats the send if still absent. The method holds no permit or uncertainty record. Correctives have no ordinary duplicate-order-number guard, so a direct consumer following the documented run-retry usage can issue the same logical correction twice while the first exchange is unresolved.

Temporary test `public_corrective_retries_send_twice_while_invisible` called `Gateway::create` twice through the existing Gateway test helper for `acct:ORD-1:corrective:c1`, kind `Corrective`, base `SZ-1`. Each create returned HTTP 500; every external-id query returned code 7. Both calls returned `Unconfirmed::Transport`; wiremock verified **two creates and four queries**. This demonstrates repeat exchanges, not observed duplicate vendor documents.

### Attribution and scope qualification

The wording and query-first retry behavior originate in `f64e36a` and predate the selected base. The base commit `e41a996` split protected Order writes from this surviving public path. Current production Order calls crate-private `protected_create` at `service/create.rs:1050–1061`; it does **not** call public `Gateway::create`. The order-wide protection acceptance criteria do not require arbitrary direct consumers to gain Order state automatically. The defect is the public instructions implying that an unsettled create is safe to re-execute without an independent permission boundary.

Make the legacy operation internal, or explicitly expose a one-send/consumer-owned-permission contract plus read-only reconciliation. Remove automatic-create-retry advice from the method, module and outcome documentation. Do not describe this as a new duplicate-send regression in the supplied Order service.

## C1 — Additional safety-related specification inconsistency: same-number storno echoes

### Conflicting requirements

`docs/design/order-write-protocol.md:89–91`: “Storno evidence must name a reversal distinct from its original … Missing or contradictory identity retains uncertainty.” The marker-clearance rule remains line 33.

But `docs/design/restate-szamlazz.md:551–561` explicitly specifies “echo of the requested number → `NotStornoable`” and identifies same-number **proforma/delivery-note** echoes as the live evidence. The production Order only sends after verifying an invoice type (`SZ`/`ES`/`VS`/`HS`), at `service/storno.rs:126–142`.

### Reproduced behavior

`gateway.rs:813–819,1819–1821` labels any same-number reply a no-op, regardless of gross or the known original type. `gateway/recovery.rs:224–230` accepts `NotStornoable` as settled and `service/recovery.rs:507` clears the marker.

The temporary protected-handler test used a live `SZ-LIVE` invoice of `REVIEW-STORNO-ECHO`, absent storno external id, and a successful reply naming `SZ-LIVE` with **negative** gross (`-1270`). Two fresh calls each returned `rejected / not_stornoable`, left `observe_unresolved = absent`, never reconciled, and produced **two storno POSTs**. The by-number original was read only once per invocation, before sending.

### Interpretation, origin and recommendation

This is not a newly introduced implementation departure: `14b80f2` introduced the explicit echo classification; `e41a996` accepted it for marker clearance. `835a370` moved it inside the new numbered/unnumbered response match without changing it. The code follows the broader design's stated echo rule, and storno repeats rely on vendor idempotence. Consequently, this review does **not** count C1 as an additional proven duplicate-document regression or silently override the deliberate reply heuristic.

There is nevertheless no stated vendor evidence that a same-number, negative-total reply to a verified live invoice proves a no-op. Resolve the specification conflict: preferably retain protected Order uncertainty on such a contradiction, while keeping the observed D/SL no-op behavior confined to the unkeyed path that can actually send those types. Otherwise explicitly document that this broader same-number classification is accepted policy and qualify the blanket contradictory-identity guarantee.

## Other examined paths and why they are not additional findings

- **Open-write replay:** `service/recovery.rs:453–488` sets the marker before awaiting arming; the atomic permit begins false and only the executing arm closure sets it. A completed arm replay cannot regenerate permission. The write run consumes it before opening the Gateway. Replay enters unresolved data and read-only reconciliation. A conservative marker for a never-sent request is an explicitly accepted availability cost.
- **Clearance after recorded settlement:** both `protected_write` and operator recovery journal their conclusive result/receipt before `clear`. The exclusive lock prevents another cooperative mutation replacing the marker during ordinary replay. Restarting an old retained prefix after unrelated later work is exceptional replay, explicitly requiring operator review; it is not ordinary resume.
- **Before-send settled failures:** armed leading-query credential/vendor/unavailable answers, collisions, expected-target disappearance, and deletion guard failures can clear the freshly armed marker because that executing closure had the one permit and did not send. Open-write replay cannot reach these send-capable branches without permission. Treating every post-arm non-success as uncertainty would ignore the specified original-execution evidence distinction.
- **71/152 after the one permitted create:** `gateway.rs:1310–1330` preserves the original refusal even if diagnostic queries fail. The permissive `Ok(outcome)` in protected creation does not currently expose a post-uncertainty query result: protected uncertain sends return `Err`, and duplicate diagnostics are collapsed back to the conclusive refusal. This diff closes a false-pause path, rather than introducing another send.
- **Corrective identity:** initial existing-target lookup deliberately answers the document previously issued under the correction id before checking a newly supplied base. After prerequisites, both full lookup and armed query enforce the resolved base; reconciliation retains uncertainty on mismatch. This asymmetry is explicitly specified at `order-write-protocol.md:102–107`.
- **Storno query evidence:** by-number identity, distinct reversal number, candidate/original order, original type and reversal are checked at the shared Gateway boundary. Candidate fallback is read-only, and credential warnings precede fallback. The reply-only negative/zero-gross fast path is separately explicit policy, not an accidentally missing query.
- **Operator attestations:** full-marker equality precedes evidence, the default authorizer denies, and admitted identity replays. Positive attestation validates operation/target shape, not the truth of the operator's independent evidence; ADR 0013 explicitly assigns that trust to the operator. Empty queries cannot clear deletion uncertainty. Opaque empty account ids/credential references are legal Account values and are intentionally accepted in their markers.
- **Migration and unkeyed writes:** old code ignoring state, scope remapping, vendor UI changes and unkeyed Agent writes are documented exclusions. No absence of an Order guard on those surfaces is counted as an in-scope regression.

## Verification executed in this review

All commands below completed successfully. The reproduction tests intentionally assert the current undesirable behavior; passing them confirms the findings, not safety.

```sh
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
  cargo test -p restate-szamlazz --all-features --locked \
  --test review_spec_4394ed0_independent -- --nocapture --test-threads=1
```

**3 passed**, 0 ignored (3.33 s test execution): the two public Gateway reproductions and the real-Order reissue/storno compound reproduction. The local server used the three required experimental features. `Restate::finish().await` completed. Temporary source: `crates/restate-szamlazz/tests/review_spec_4394ed0_independent.rs`, removed after verification. The fixture sequence and observed counts are recorded above to reconstruct each test.

```sh
cargo test -p restate-szamlazz --all-features --locked \
  --lib --test gateway --test recovery_contract --test expected_document
```

**334 passed**: 242 library, 79 Gateway, 9 recovery contract, 4 expected-document contract. Three externally dependent library tests remained ignored.

```sh
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
  cargo test -p restate-szamlazz --all-features --locked --test e2e \
  e2e_unresolved_ -- --ignored --test-threads=1
```

**15 passed**, 0 skipped (58.10 s): actual arm-ack loss, retained one-send interleavings, interrupted arm/open send, final refusal after uncertainty, kill/marker/recovery evidence, authorization refusal, recovery admission/evidence replay, old-number attestation exclusions, positive settlement, opaque account values, storno/delete interruption, storno fallback and original/order verification, and duplicate diagnostic failure.

```sh
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
  cargo test -p restate-szamlazz --all-features --locked --test e2e \
  e2e_recovery_ -- --ignored --test-threads=1
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
  cargo test -p restate-szamlazz --all-features --locked --test e2e \
  e2e_protected_storno_ -- --ignored --test-threads=1
```

**2 + 2 passed** (5.57 s + 4.89 s): pinned-account recovery and credential-free verification replay; unreadable-state mutation blocking; complete evidence at both pre-send storno boundaries.

`git diff --check` passed. These selected checks support the reviewed paths; they do not claim coverage of every exceptional replay prefix or establish vendor behavior for the synthetic contradictory replies.
