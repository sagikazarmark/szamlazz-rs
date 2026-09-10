# #216 implementation review and verification

Base: `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`. Reviewed as a working-tree change.

## Standards

Independent review identified privacy-registry coverage, recovery discovery schemas, replay-safe authorization,
open observation decoding, recovery read-policy/error mapping, explicit recovery handler settings, and execution
log attribution. All were addressed. Account attribution is populated only after the persisted marker has been
authorized, decoded and compared with the submitted marker.

## Spec

Independent protocol review found no remaining concrete release-blocking protocol bug after the fixes.
The review checked pre-prologue guarding, acknowledged arming, completed-arm versus completed-write replay,
original-send refusal classification, stronger storno evidence, and record-before-clear recovery ordering.
Host authentication, pinned credential mapping, audited operator assertions, and exclusion of old code and
external writers remain explicit trust boundaries.

## Executed checks

- `cargo check -p restate-szamlazz` and all-feature/test checks during implementation.
- `cargo clippy -p restate-szamlazz --all-features --all-targets -- -D warnings`.
- `cargo test --workspace --all-features` (unit, contract, Gateway, integration and doctests).
- `RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test --workspace --all-features -- --ignored e2e_`.
- Final `restate-szamlazz --test e2e -- --ignored` rerun after recovery changes: eight tests passed.

Real Restate 1.7.8 verification includes the three invisible-send interleavings; automatic pause and absent
resume; before-marker, after-marker, acknowledged-arm, open-write, reconciliation and recorded-settlement
interruptions; marker-before-arm command ordering; manual kill with queued same-kind/corrective/cross-kind
mutations; final-invoice code 73 and later credential/down answers; exact-marker recovery and spoofed operator
assertions; cancellation, credential rotation, privacy, and complete step-name paths in the main suite.
These are scripted vendor exchanges, not live duplicate-issuance evidence.

The complete protocol and migration procedure are in `docs/design/order-write-protocol.md`.
