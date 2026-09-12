# Release-readiness review at 4394ed0

## Verdict and scope

**Hold the release for two focused safety/API corrections.** The supplied Order service has a sound protected-write architecture and substantial passing real-Restate coverage. The remaining concerns do not call for a redesign: contradictory reissue success can bypass settlement checks, and the public Gateway still recommends unsafe create retries.

Reviewed current source at `4394ed0977a0adf298a10e0acd9182d13cbf3c0c` on 2026-09-11. The review concentrates on `restate-szamlazz`: Rust interfaces, Restate boundaries, Számla Agent uncertainty, recovery, configuration, and caller/operator experience. The Számla Agent transport/parser boundary and inbound receiver surfaces received narrower inspection, plus workspace-wide tests. This is not a claim of an equally deep audit of every crate. Cargo manifests, changelogs and release machinery were excluded from findings.

Current implementation and design documents take precedence over historical descriptions in CONTEXT. Two independent review passes covered Rust/API standards and protocol/spec fidelity. Findings below were checked against source; the two principal Gateway behaviors were also reproduced using temporary local-wiremock tests. No vendor requests were made. The temporary test file was removed; this report is the only retained change.

## Standards: Rust interfaces and caller experience

### S1 — High: public Gateway create retries can send an unresolved corrective again

**Locations:** `crates/restate-szamlazz/src/gateway.rs:1173–1205`, `1221–1245`, `1341–1352`; the `Unconfirmed` instructions at `479–494` and module introduction at `1–16` repeat the contract.

`Gateway::create` remains public and explicitly instructs consumers to re-execute the step under a run retry policy when it returns `Unconfirmed`. On a lost create answer followed by absence, it returns that error. A subsequent execution with no expected reversed document queries again and sends again if the result is absent.

An external-id query returning code 7 does not establish that the first request did not execute or cannot execute later. Correctives are especially exposed because they are exempt from ordinary order-number duplicate protection. An unresolved corrective can therefore be sent twice by a direct consumer following the crate's documented usage.

**Verification:** a temporary test returned HTTP 500 for each create and code 7 for every external-id query. Two calls to the same corrective operation produced **two create POSTs and four queries**. This demonstrates the re-send mechanism, not two observed live vendor documents. The supplied Order service uses `protected_create` and is not subject to this retry path.

**Before release:** make the legacy create entry point internal, or give the public operation an explicit one-send/consumer-owned-permission contract with separate read-only reconciliation. Remove advice that treats `Unconfirmed` as permission to retry creation. Align the Gateway module and README descriptions with the actual protected/unprotected split.

### S2 — Low: invoice-number schema only approximates its decoder

**Locations:** `crates/restate-szamlazz/src/identity.rs:582–599`, `649–656`.

The decoder enforces 40 UTF-8 bytes and rejects all Rust Unicode control/whitespace characters. The schema's `maxLength: 40` counts characters, and its pattern excludes ASCII controls but not the full C1 range. For example, 21 `é` characters pass the length keyword but fail the 42-byte runtime check; `SZ\u00801` also passes the published exclusions but fails `is_control`. ECMAScript `\s` is not exactly Rust's Unicode whitespace predicate either.

The schema description does mention bytes, so this is not an undocumented runtime bound. It is a generated-client validation mismatch affecting all mutation-number inputs, not a worker safety bypass: the worker rejects the values before durable work.

**Improvement:** align expressible character exclusions; explicitly describe any remaining schema approximation/runtime-only constraint. Test Unicode boundary examples behaviorally rather than only asserting schema keywords.

### S3 — Low: an unusable vendor storno date is attributed to caller input

**Locations:** `crates/restate-szamlazz/src/service/storno.rs:75–92`; `crates/szamlazz-agent/src/xml.rs:19–34`.

Storno's fulfillment date comes exclusively from the verified original. The final request validation maps every error to `invalid_input`, although response parsing accepts a broader civil-date domain than outbound validation. An original carrying `telj = 0000-01-01` can therefore produce caller `invalid_input` for a date the caller cannot supply or correct. Missing fulfillment already receives the operational `unavailable` treatment.

This is a malformed-vendor-content edge case, not an observed production date or a send-safety defect. No write is armed.

**Improvement:** classify an unusable original date as `unavailable`, preserving `invalid_input` for caller-owned fields such as the comment. Cover both storno shells.

## Spec: protected-write settlement

### P1 — Release blocker: a reissue reply naming the old document clears uncertainty

**Locations:** `crates/restate-szamlazz/src/gateway.rs:1254–1258`; `gateway/recovery.rs:193–196`; `service/recovery.rs:490–507`.

Every numbered create success is converted directly to `CreateOutcome::Issued`. There is no check against `request.reversed`. The protected adapter accepts that result as settled; `protected_write` skips read-only reconciliation and clears the unresolved-write marker.

Scenario:

1. The caller requests reissue with `expected_number = SZ-OLD`.
2. The target/full/leading queries report owned, reversed `SZ-OLD`.
3. The create succeeds syntactically but names **SZ-OLD**, rather than a replacement.
4. The operation returns `issued` for the old document and clears its marker.
5. A fresh invocation with the same expected-document intent may send again while the original remains the queried holder.

The contradictory number cannot establish replacement issuance or non-execution of the send. The same exclusion is already enforced for automatic reconciliation (`gateway/recovery.rs:328–334`) and audited completion (`service/recovery.rs:294–298`). The direct reply is the inconsistent path.

**Design basis:** `docs/design/order-write-protocol.md:27–37` requires conclusive recorded settlement before clearance; `73–84` excludes the old number from reissue evidence. ADR 0012 binds the replacement intent to the expected document.

**Verification:** a temporary wiremock test confirmed that `Gateway::create(Some(SZ-OLD))` returns `Issued(SZ-OLD)` for this response. The protected adapter and marker-clearance consequence were source-traced through the shared `create_send` method; the complete handler scenario was not newly reproduced. No evidence establishes that szamlazz.hu emits this contradiction live.

**Before release:** treat a returned old reissue number as unresolved. Keep the marker and reconcile until matching replacement evidence or authorized settlement exists. Add a protected-handler regression asserting both the retained marker and zero additional sends from a later mutation. Apply the check consistently to ordinary numbered success and numbered notification-failure success.

## Architecture and resilience assessment

### Strong foundations

- **Write permission has a real durability boundary.** State is written before acknowledged arming; permission exists only in the executing closure, is consumed once, and is not reconstructed from replayed results. The arm-ack-loss test exercises the actual protocol-v7 boundary.
- **Uncertainty survives invocation lifetime.** Cancellation and kill do not erase the marker. Every ordinary Order mutation blocks on any present state, including unreadable state. An empty query never clears an unresolved write.
- **Reconciliation is read-only and pausable.** Order mutation exhaustion retains the invocation and exclusive lock; resume cannot grant another send. Queries, credential repairs and deliberate operator recovery provide workable incident controls.
- **Recovery is operation-specific.** Exact-marker comparison, pinned endpoint/credential reference, corrective-base checks, storno relationship verification, and separate positive/non-execution attestations give operators meaningful control without pretending absence is proof.
- **Authorization defaults to denial.** Operator identity is admitted and journaled before marker access. Recovery records evidence before clearance. The host's trusted-metadata and runtime-identity responsibilities are explicit.
- **Rust contracts are generally strong.** Validated identity/config types, exact decimal decoding and checked arithmetic, closed requests, open response tokens, typed faults, preserved native ingress errors, and crate-owned journal projections all improve correctness and upgrade behavior.
- **Execution isolation is appropriate.** Credentials and the Gateway are acquired inside executing operations, completed runs replay without them, and the production worker transport uses fresh cookies, deadlines and explicit retry/redirect suppression.
- **Diagnostics preserve useful facts.** Transport/parse categories remain actionable without copying arbitrary response bodies or credential messages into durable records. The documented business-message exception is explicit.
- **The inbound surfaces keep protocol meanings distinct.** Adatkapcsolat distinguishes unknown keys from unavailable resolution and uses lenient content handling; IPN models absolute paid-amount observations and documents its authentication limitations.

### Important operating boundaries

These are present, documented design limits rather than additional findings:

- **Agent credit entries are weaker than Order writes.** They are unkeyed, can race replacements, and an interrupted open run can repeat an additive entry. Neither a caller lock nor `max_attempts(1)` fences delayed vendor processing. A deployment requiring protected credit registration needs a separate serialization/uncertainty design; the current interface should not be presented as universally exactly-once.
- **Unmanaged Agent storno relies on vendor idempotence.** It has no Order marker or per-invoice lock.
- **Recovery can deliberately block indefinitely.** Without positive evidence or a defensible exact-request attestation, there is no automatic safe clearance. The authorizer and incident procedure must be wired before operators need them.
- **Scope and credentials select the real account.** The worker has no seller/account pin. The deployed resolver/store seller check and scope canary remain necessary operating controls; a copied test key checks a different thing.
- **Multi-account mode is version-sensitive.** The documented server/SDK combination and experimental scope flags are important. Actual discovery settings, ingress trust boundaries and deployment routing must match the documented setup.
- **Transport customization is limited for embedders.** `Gateway::open_with_http` supports direct Gateway consumers, but supplied Order/Agent services do not expose a transport factory. This is clearly documented; deployments needing custom TLS/proxy behavior should establish compatibility before adopting the supplied services.
- **A numbered successful create clears protection immediately.** Subsequent discovery relies on the vendor exposing that completed document. The local suite proves behavior under its scripted visibility model; it does not establish a universal vendor read-after-success guarantee. Keep this as an explicit evidence limit, particularly for correctives and cross-kind decisions.

## Verification executed

All completed checks below passed:

| Check | Result |
|---|---|
| `cargo test --workspace --all-features --locked` | Ordinary workspace tests and doctests passed; externally dependent tests remained ignored |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo check -p restate-szamlazz --no-default-features --locked` | Passed |
| `python3 scripts/check-agent-schemas.py` | 430 generated requests; 790 valid source/request combinations, 70 specifically expected source conflicts, seven negative controls |
| `python3 scripts/test-agent-schema-runner.py` | Eight passed |
| `python3 scripts/test-order-migration.py` | Five passed; Python emitted a ResourceWarning while cleaning up a redirect-test HTTPError |
| Actual Restate 1.7.8, all ignored worker `e2e_` tests, serial | **38 passed**: three library scenarios and 35 integration scenarios |
| Temporary local-wiremock review probes | Both reported behaviors reproduced; temporary file removed |

Real-Restate command:

```sh
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
  cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1
```

The first e2e invocation exceeded the review shell's 120-second command timeout and received SIGTERM. It was not counted as a successful suite. A complete rerun with a 600-second command timeout passed (about 155 seconds for library plus integration scenarios).

No fresh vendor-live acceptance, exhaustive feature powerset, cross-platform build or deployed-host authorization check was performed. Existing dated live records provide historical evidence, not execution of every assertion at this revision.

## Release exit criteria

1. Resolve S1's public create retry contract and P1's contradictory reissue settlement path.
2. Run focused regressions demonstrating one send under unresolved direct-consumer behavior and retained protection after an old-number reissue reply; rerun affected real-Restate scenarios.
3. For the final candidate, follow the existing manual live-acceptance policy for material worker/transport/parser changes and retain revision-specific results. No need to expand every optional vendor probe into a release requirement.

**Axis summary:** Standards: three findings, most serious the public Gateway retry contract. Spec: one finding, premature reissue settlement and marker clearance. The overall architecture is release-shaped; these focused corrections and candidate-specific acceptance evidence should precede approval.
