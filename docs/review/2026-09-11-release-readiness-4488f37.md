# Release-readiness review — 4488f37

Reviewed 2026-09-11 at `4488f37c807255b31bf0402e7a1a5c617814cf3e`.
Scope: current implementation, especially `restate-szamlazz`, its public Rust/ingress contracts,
durable execution, operator recovery and the underlying Számla Agent boundary. Cargo manifests,
changelogs and release machinery were excluded. Worker source received the deepest inspection;
workspace-wide checks are broader evidence, not an equally deep audit of every receiver and CLI path.

## Verdict

**Close to release-ready. The core Order protection is credible; finish the bounded fixes below
before giving the release an unqualified approval.** No critical/high-severity duplicate-send defect
was established within the documented protection boundary. This is not evidence of unconditional
exactly-once behavior across arbitrary vendor behavior, external writers or administrative state changes.

The two main pre-release recommendations are consistent corrective-base checking after resolving
intent, and correct attribution of invalid account configuration. The exported identity constructors
should also be tightened before consumers depend on their signatures. Padded persisted marker
identity is a smaller fail-closed defect.

## Standards: Rust, API and caller experience

### S1 — Medium: configuration failures are reported as caller `invalid_input`

- `crates/restate-szamlazz/src/account/static_resolver.rs:358–365` copies defaults and seller configuration
  into a usable resolver without validating their business/XML representations.
- `crates/restate-szamlazz/src/gateway/build.rs:140–142` rejects an unknown language selected from either
  a caller override or the account default.
- `crates/restate-szamlazz/src/service/create.rs:811–816` maps both projection and complete request
  validation failures unconditionally to `invalid_input`.
- `crates/restate-szamlazz/src/service/storno.rs:85–92` similarly maps validation of the derived storno request.

A static account with `defaults.language = "hhu"` loads successfully, but a valid caller document fails
with `unknown document language "hhu"`. An XML-forbidden character in account-only fields such as
`defaults.aggregator` cannot be fixed through the caller's request at all. The documented 400 contract
attributes the fault to the caller and tells it to correct its input, turning a deployment problem into
a permanent business-request refusal.

**Recommendation:** validate configuration-owned values during static resolver construction; validate
dynamically resolved account values at an appropriate operational boundary too. Preserve the origin
of merged-field failures: caller overrides remain `invalid_input`, configuration defects become an
operational fault with safe diagnostics. Do not require a default exchange rate when callers are
explicitly allowed to provide one per document.

**Evidence:** a temporary executable constructed the invalid static resolver, resolved its account,
and projected an otherwise valid document, reproducing the error. Handler fault attribution was
confirmed from the source. The temporary executable was removed.

### S2 — Medium: structured storno external-id constructors bypass their component bounds

`crates/restate-szamlazz/src/identity.rs:784–799` accepts unrestricted `&str` invoice numbers in
`ExternalId::for_storno` and `for_unmanaged_storno`, despite the bounded-composition guarantee at
`:696–699` and the hypothetical component-bound assertions at `:823–846`.

Passing a 120-byte number to `for_unmanaged_storno` with namespace `acct` produces a 142-byte id,
exceeding `MAX_LEN = 110`. Separators and other invalid mutation-number characters are also accepted.
The supplied handlers validate numbers first, so this is an exported Rust/Gateway-consumer defect,
not a demonstrated handler bypass. Behavior beyond the verified vendor length is unknown.

**Recommendation:** take the validated worker `InvoiceNumber` or make these structured constructors
fallible. Distinguish their guarantees from the intentionally unrestricted `ExternalId::new` wrapper.
The oversized-id behavior was reproduced with a temporary executable.

### S3 — Low: migration's replacement request example is incomplete

`crates/restate-szamlazz/README.md:978–986` omits the required `document.payment_method`
(`src/contract/document.rs:37`). A caller copying it gets a malformed-body fault before the prologue.
Add the field and include this example in the documented-request checks.

## Spec: durable protocol and recovery

### P1 — Medium: second corrective lookup accepts evidence the armed lookup rejects

`crates/restate-szamlazz/src/service/create.rs:940–950` passes the full lookup through `decide_lookup`
without the intended corrective base. `lookup_step` at `:973–989` asks only the order/type ownership
question. A live holder becomes `already_issued`; a reversed holder becomes `reversed`.

Scenario: initial corrective ownership lookup says absent; base `SZ-A` is verified; the second
external-id lookup returns `HS-FOUND` for the same order/correction id but with no base reference
or with reference `SZ-B`. The handler reports an existing correction rather than a collision.
If precisely the same holder appears on the third, armed lookup, the stronger check at
`src/gateway/recovery.rs:171–178` returns `external_id_collision`.

The protocol specification (`docs/design/order-write-protocol.md:102–105`) explicitly requires the
armed check and explicitly preserves the *initial* existing-target short circuit before prerequisites.
The intervening full lookup is an implementation/spec-completeness gap: the literal armed requirement
is implemented, but the resolved intent is not checked consistently.

**Recommendation:** apply the intended-base predicate to the full lookup after prerequisites, while
retaining the explicitly approved initial-target behavior. Cover a wrong-base holder first visible
on the second query, not only the third. This finding does not establish a duplicate send or an
incorrect clearance of an already-present unresolved marker.

**Runtime reproduction:** temporarily changed `tests/e2e/release_hardening.rs:407` to return absence
once instead of twice, and its expected query count to two. On real Restate 1.7.8 the existing collision
assertion failed with `outcome: already_issued`, `invoice_number: HS-FOUND`. The first case has no
base reference. The source test was restored exactly afterward.

### P2 — Low: marker decoding silently repairs padded Order identity

`crates/restate-szamlazz/src/contract/recovery.rs:165–167` uses the general `OrderKey` deserializer,
which trims input (`src/identity.rs:237–240`). Therefore `service/recovery.rs:316–325` compares an
already-normalized marker order with the actual object key.

Persisted `"order":" ORD-1 "` under object `ORD-1` is accepted when the remaining fields match;
it compares equal to a marker containing `"ORD-1"` at recovery's exact-marker comparison (`:189`).
The spec requires invalid persisted identity to fail closed (`docs/design/order-write-protocol.md:132–134`).

**Recommendation:** use strict, non-normalizing deserialization for the marker's Order field, keeping
the intentionally lenient general `OrderKey` unchanged. Add padded identity to the unreadable-state
matrix. Ordinary mutation guards still block the marker, and authorization/evidence remain required;
this is not an unauthenticated recovery bypass.

**Evidence:** a temporary executable decoded padded and exact marker bodies and confirmed they compare
equal. The remaining decoder predicates and clearance path were checked in source.

## Architecture and resilience assessment

- **Sound boundaries:** account resolution, credential acquisition, Gateway I/O, domain decisions and
  Restate adapters have distinct responsibilities. No redesign is warranted by this review.
- **Appropriate Rust contracts:** object-safe resolver/store traits, explicit validated configuration,
  exact/fallible money arithmetic, closed request objects, open response tokens, typed faults and
  dependency re-exports. No actionable smell-only architecture finding was established.
- **Meaningful Order protection:** persisted marker, awaited arm acknowledgement, execution-local
  one-use permission, and read-only reconciliation address the effect-before-journal crash window.
  Resume does not regrant permission; cancellation/kill do not erase uncertainty.
- **Operator control:** shared observation, pause/resume, default-deny recovery authorization, pinned
  account/credential reference, exact-marker evidence and distinct audited positive/non-execution
  attestations provide ways to resolve vendor incidents without automatic resends.
- **Restate discipline:** nondeterministic decisions are journaled, completed work replays without
  credentials, run/invocation policies and per-call deadlines are distinguished, and immutable
  deployment/exceptional-replay guidance acknowledges their actual limits. Replay logging is exercised.
- **Számla Agent boundary:** production Gateway disables transport retries and redirects, uses fresh
  cookie jars and a request timeout, validates identity, preserves ambiguous outcomes, and sanitizes
  transport/parse diagnostics. Unknown vendor answers do not become permission to reissue.

Important accepted limits remain operationally significant:

1. Unkeyed Agent credit entries can repeat an additive entry or an older replacement after interruption.
   They do not provide Order's send-permission protection. Caller serialization cannot fence a delayed
   vendor request. This surface is not suitable for unattended exactly-once registration as currently designed.
2. Unmanaged Agent storno relies on vendor idempotence; it has no Order marker.
3. An unanswered deletion can require independent operator/vendor evidence; absence alone cannot recover it.
4. The actual host must wire recovery authorization and validate its seller/scope mapping. Static credential
   rotation, experimental scope flags, shutdown task ownership and compatible retained deployments need the
   documented deployment treatment. These are explicit integration responsibilities, not newly found bugs.
5. `get` is a mixed-time observation, excludes correctives, and cannot establish invocation completion.

## Verification

Passed during this review:

```text
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo fmt --all --check
python3 scripts/check-agent-schemas.py
python3 scripts/test-agent-schema-runner.py
python3 scripts/test-order-migration.py
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
```

The ordinary run included workspace doctests. Real Restate: **36 passed**, comprising three library
scenarios and 33 integration scenarios (including the aggregate Order protocol suite), with no skips.
Server 1.7.8 was downloaded from the upstream release and its published SHA-256 checked.
Schema validation: 430 generated requests, 790 valid source verdicts, 70 expected source conflicts,
seven negative controls; runner regression tests 8 passed, migration tests 5 passed.

The deliberately varied corrective test failed as described in P1; that is reproduction evidence,
not a failure of the unchanged baseline suite. All temporary source changes were removed.

No fresh vendor-live writes were run. The repository records five successful live acceptance journeys
in `2026-09-11-worker-release-acceptance.md`; those are earlier evidence, not executions from this review.
No actual deployed host was supplied for a recovery-authorization or seller-mapping drill.

**Summary:** Standards: 3 findings (2 medium, 1 low), led by configuration fault attribution.
Spec: 2 findings (1 medium, 1 low), led by corrective lookup consistency.

## Implementation and acceptance follow-up

Implemented against `46fab05468495a01ad0ef501ac23df72a0b00e66` after approval of all five fixes:

- S1: `Account::validate` checks configuration-owned language and XML text, reporting only the field
  and rule. Static resolver construction refuses invalid accounts. Dynamic accounts are validated from
  the journaled resolution before document reads/arming and produce `unavailable`; bad caller overrides
  remain `invalid_input`. Non-MNB defaults remain valid when callers provide explicit rates.
- S2: structured storno constructors now require the validated worker `InvoiceNumber`. The arbitrary
  `ExternalId::new` wrapper is explicitly documented separately.
- S3: the migration request includes `payment_method`; a test decodes the actual README example.
- P1: the full corrective lookup checks the resolved base through the same document predicate as
  armed checking and reconciliation. Initial existing-target behavior remains unchanged.
- P2: marker-specific deserialization rejects padded Order identities; general `OrderKey` parsing
  remains lenient. The real-Restate unreadable-state matrix includes padded identity.

The corrective second-lookup, static configuration, dynamic fault-attribution and padded-marker
regressions were observed failing before their fixes, then passing afterward. Final acceptance passed:

```text
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
python3 scripts/check-agent-schemas.py
python3 scripts/test-agent-schema-runner.py
python3 scripts/test-order-migration.py
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
```

The real-Restate suite passed **38 scenarios** (3 library + 35 integration, no skips), including
the two new scenarios. Workspace tests included doctests. Independent final Standards and Spec
reviews of the hardening diff reported no findings.

The production recovery/seller-mapping drill remains unperformed: it needs the actual host's authenticated
ingress and Restate admin URLs, operator authentication method, designated scope/order and the deployed
resolver/store wiring with independent seller expectations. No production marker has been cleared and
no fresh vendor-live writes were executed as part of this hardening pass.
