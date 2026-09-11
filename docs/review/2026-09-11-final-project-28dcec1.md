# Final project release review

Reviewed HEAD: `28dcec1`, 2026-09-11. Scope: current implementation, primarily
`restate-szamlazz`, Rust public contracts, Restate durability and recovery,
Számla Agent integration, and caller/operator experience. Cargo metadata,
changelogs and release machinery are outside the assessment.

## Verdict

### Implementation closure — working changes after this review

The findings below describe the original `28dcec1` baseline. Both are addressed
in the current working changes:

- **S1:** execution initialization validates fetched credentials through a fixed
  valid request's wire serializer, without sending it or exposing its diagnostic.
  Unrepresentable credentials produce sanitized `unavailable`. A real-Restate
  regression first reproduced the false `invalid_input`, then passed for both
  additive and replacing credit entries: one accepted POST, interruption,
  malformed rotation, uncertain fault, no second POST, secret-free journal/fault,
  and retained completion replay without another fetch.
- **S2:** all four decimal input fields share the corrected discovery grammar.
  The handler's raw JSON boundary rejects number-lookalike objects before Serde
  introduces its private arbitrary-precision representation. Exact numeric
  tokens, buffered Serde composition, duplicate-field refusal, escaped object
  keys and bounded nesting are covered. Generic direct Serde decoding retains
  its internal number-map support; the actual-JSON shape check belongs to `Body`.
- **Recovery coverage:** five malformed persisted-state forms are injected via
  Restate's admin API and remain unreadable, block all seven mutation handlers
  and recovery, issue no external requests and emit no clearance. A separate
  scenario changes the resolver endpoint/id/credential reference after creating
  uncertainty; recovery queries only the pinned account, then replays completed
  verification successfully while both credential references are unavailable.

Final verification passed: workspace all-feature tests and doctests; **35 actual
Restate scenarios** (3 library, 32 integration); Clippy with warnings denied;
formatting and diff checks; **37 feature combinations**. Standards and Spec
reviews of the implementation reported no findings. No fresh vendor calls were
made; request XML writers were unchanged, so XSD evidence below remains the
preceding review's execution rather than a repeated check.

One initial test run exhausted disk space; generated worker build artifacts were
cleaned and testing resumed. The pinned-account test initially supplied a
by-number mock where create recovery intentionally requires external-id
ownership evidence; correcting that fixture made the intended assertion pass.
The initial full workspace build exceeded its command timeout; the subsequent
full invocation completed successfully.

**The code-review release blockers are closed. I approve the library code for
release within its documented scope.** Deployed seller/scope verification,
authorization-boundary recovery drill and operational monitoring remain the
go-live checks described below.

### Original review verdict

**Hold release sign-off for S1.** The protected Order architecture is credible,
and all 32 existing real-Restate scenarios passed. An additional real-Restate
control demonstrated a false settled fault after interrupted credit-entry
registration and credential rotation. Fix that classification before release.
S2 is a smaller public-contract inconsistency worth closing alongside it.

No confirmed defect was found in the protected Order send/replay/recovery
state machine. Its guarantees are materially stronger than the unkeyed Agent
write surface; they should continue to be communicated separately.

## Standards: Rust API and operational contract

### S1 — High: changed credentials can falsely settle an interrupted credit-entry write

References:

- `crates/restate-szamlazz/src/gateway.rs:2040–2047`
- `crates/restate-szamlazz/src/service/agent.rs:136–140`
- `crates/restate-szamlazz/src/service/prologue.rs:412–416,462–471`
- `crates/szamlazz-agent/src/client.rs:226–238,374–375`

`ClientError::Request` is always converted to `RejectionCode::Request`, then
to `400 invalid_input` with “nothing was sent.” Some such errors come from
the freshly fetched credentials, rather than the invocation's immutable
request: the complete outgoing XML is validated at `to_wire`, while client
construction accepts XML-unrepresentable credentials.

Verified sequence against local Restate 1.7.8 and a mocked vendor:

1. Submit a valid additive credit-entry request with a valid dummy key.
2. The mock accepts the POST and delays its reply.
3. Pause the invocation; confirm the registration run has no recorded result.
4. Change the dynamic credential store to a dummy key containing U+0000.
5. Resume and attach using the original Idempotency-Key.

Observed: **one accepted POST, two credential fetches**, followed by:

```text
InvalidInput: the credit entries cannot be sent: request XML contains character
U+0000, which XML 1.0 forbids; nothing was sent
```

The second execution sent nothing, but the logical invocation's first exchange
may have registered the entries. The settled-fault contract tells the caller a
financial effect did not occur when it may have. This is the same uncertainty
the existing vendor-refusal-after-interruption test correctly preserves.

**Fix:** validate credential XML representability at execution initialization
and return a sanitized `unavailable` fault, preserving earlier-write
uncertainty. Reserve settled `invalid_input` for failures determined by the
immutable request/account. Cover interrupted registration followed by malformed
credential rotation, for additive and replacing modes; assert that no secret
material appears in the fault or journal. The fix should not introduce a new
send or imply that correcting credentials permits automatic renewal.

Reproducer source for this review session:
`/tmp/opencode/final-review-28dcec1-rotation.rs`. It uses the public services,
a dynamic store, the published harness and wiremock. The mock establishes the
worker interleaving, not a live vendor registration. The successful run ended
with `Restate::finish()`.

### S2 — Medium: decimal input and discovery schema disagree

References:

- `crates/restate-szamlazz/src/contract/document.rs:295–301`
- `crates/restate-szamlazz/src/contract/decimal.rs:18–53`

The discovered quantity schema uses:

```text
^-?\d+(\.\d+)?([eE]\d+)?$
```

The runtime accepts the exactly representable string `"1e-2"`, which this
pattern rejects. Schema-driven callers therefore reject valid worker input.

Conversely, the custom visitor accepts this actual object as a quantity:

```json
{"$serde_json::private::Number":"1"}
```

The visitor consumes serde_json's arbitrary-precision implementation format
from ordinary caller JSON. The documented input is a string or number, not an
object. This does not demonstrate numeric precision loss, but weakens the
closed input contract.

Both behaviors were reproduced against the built worker and its discovery
schema. **Fix:** give input decimals a shared schema matching the supported
grammar, including signed exponents; reject actual object-valued numeric inputs
at the JSON boundary while preserving exact token parsing and supported Serde
composition. Add schema/runtime agreement examples and malformed-shape cases.

## Spec: durable writes and recovery

**No confirmed implementation finding.** Current code satisfies the reviewed
protected-write protocol at the principal failure boundaries:

- Every exclusive mutation checks unresolved state before prerequisites.
- Marker → acknowledged arm → execution-local one-use permit → recorded write
  result → read-only reconciliation preserves send permission across replay.
  SDK 0.12.0/shared-core acknowledgement semantics were independently checked;
  the actual acknowledgement-loss regression passed.
- Cancellation and kill preserve uncertainty. Absent queries, elapsed time and
  retry-policy exhaustion do not authorize another protected send.
- Positive evidence checks order, kind and operation-specific identity,
  including corrective base and exclusion of the old reissue target. Storno
  settlement requires appropriate original/reversal evidence; deletion absence
  alone remains unresolved.
- Operator recovery is default-deny, compares the exact marker, uses its pinned
  account, records authorization/evidence before clearance, and distinguishes
  operator assertions from worker-verified evidence.
- Expected document numbers constrain stale reissue/deletion commands. Scope
  migration inventories unfinished invocations and all Order state before a
  switch, including unreadable/unknown state.

Two useful **coverage follow-ups**, not demonstrated release defects:

1. Exercise malformed persisted marker bytes, unknown versions and inconsistent
   operation/external identities through observation, recovery and all mutation
   adapters. Existing decoder tests do not establish that complete stored-state
   behavior. Expected: unreadable/refused, zero sends, no clearance.
2. Create a marker against mock A, change the resolver mapping to B, then recover
   with document evidence. Assert queries still use A and the pinned credential
   reference; replay completed verification with credential fetch unavailable.
   Current code reconstructs the pinned account correctly, but existing custom
   account coverage does not exercise this adversarial mapping change.

## Architecture, resilience and user experience

The domain/Gateway/service split is useful and idiomatic. Validated identities
and configuration, checked decimal arithmetic, open response tokens, structured
fault decoding and lazy execution-local credentials provide strong boundaries.
External observations belong to durable runs; completed runs replay without
fetching credentials. Privacy projections keep rich upstream transport content
out of journals while preserving operation/failure categories.

The Order API offers substantial control over szamlazz.hu ambiguity: retained
invocations, pause/resume, shared observation, exact-marker recovery and audited
settlement. This deliberately trades availability for duplicate prevention:
an invisible effect without adequate evidence can leave an order blocked.

Unmanaged Agent storno relies on vendor storno idempotence. Agent credit entries
remain concurrent, unkeyed writes whose interrupted open runs may repeat; caller
serialization cannot fence vendor work still processing. That documented scope
is acceptable as an explicit lower-level surface, but S1 violates even its
conservative fault contract.

The provided services expose no custom transport factory; `Gateway::open_with_http`
is for direct Gateway consumers. This limitation is accurately documented and
was not treated as a new release defect. Selected receiver checks likewise found
appropriate body/authentication/retry boundaries and retained handler failures.

This was not a fresh exhaustive vendor conformance audit of every Agent parser.
The existing Agent adjudication records numberless-success and customer-URL
encoding questions, limited taxpayer diagnostic exposure and small documentation
defects. Those are not erased by passing mocked tests or request XSD checks:
see `2026-09-11-agent-api-77d53c5-adjudication.md`.

## Verification

Passed during this review:

- `cargo test --workspace --all-features --locked`, including doctests.
- `RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1`:
  **32 passed** (3 library, 29 integration), actual Restate, mocked vendor.
- Additional actual-Restate credential-rotation control: reproduced S1.
- Local Gateway/decimal/discovery controls: reproduced S1's request mapping and S2.
- `cargo fmt --all --check`; `git diff --check`.
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`.
- Cargo feature powerset: **37 combinations**.
- Migration script tests: **5 passed**.
- Standalone request XSD validation: **430 generated requests**, **790 valid**
  source/request verdicts, **70 expected source conflicts**, **7 negative controls**.
- Schema-runner regression tests: **8 passed**.

`xmllint` was absent initially; the documented Nix invocation passed both schema
checks. Initial throwaway reproducer builds encountered mismatched cached
dependency artifacts; after linking consistent artifacts the real-Restate
control completed successfully. No production source was edited.

No fresh vendor-live requests or deployed-host recovery drill were performed.
The five prior live journeys are separately recorded in
`2026-09-11-worker-release-acceptance.md`; they are not fresh HEAD evidence.
Go-live still needs seller/scope verification using the deployed resolver/store
and recovery through the host's actual authorization boundary. Unresolved-age,
paused-owner and queued-mutation monitoring remains an operator deliverable.

**Summary:** Standards axis: **2 findings**, highest **High** (S1). Spec axis:
**0 confirmed defects**, with **2 coverage follow-ups**.
