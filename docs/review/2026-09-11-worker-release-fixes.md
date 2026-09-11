# Worker release-review fixes

Implemented the seven findings from the release review against the current working tree. Existing
recovery hardening was retained; a concurrent Számla Agent commit advanced HEAD from `f83e5fd` to
`2ba5fb8` during verification. No live vendor writes were made.

## Changes

- Vendor credit-entry refusals retain `outcome_unknown` and `szamlazz_code`: a refusal of a repeated
  exchange cannot exclude an earlier execution of an interrupted open run. Local request refusals
  stay `invalid_input`. A real-runtime regression covers both additive and replacing requests.
- Unmanaged-storno exhaustion, cancellation and initialization faults use settlement-first guidance.
- Immediate protected-storno verification reuses the operator-recovery evidence predicate. Both the
  original and reversal must carry the intended order; wrong-order cases retain the marker.
- Marker decoding accepts resolver-owned opaque account ids and credential references, including
  empty values accepted by `Account`. A custom resolver test exercises send, kill and recovery.
- Hosting examples use fallible listener binding and explicit SIGINT/SIGTERM handling through
  `serve_with_cancel`. Three standalone runtime regressions now end with `finish()`.
- A loopback HTTP/2 relay withholds the actual protocol-v7 arm completion acknowledgement, proves
  zero sends before acknowledgement, drops the connection and verifies read-only replay.
- Journal checks cover marker guard/preparation/set, arm completion, write and reconciliation
  completion, and clearance. Successful reconciliation requires clearance, not only its ordering.
  Every create kind walks reconciliation; storno and deletion are interrupted after effect but before
  result recording. Recovery is interrupted after admission, document verification and receipt recording.
  Revocation denies a new invocation carrying the formerly admitted assertion while recorded admission replays.

## Verification

- `cargo test --workspace --all-features --quiet`: passed.
- `RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test --workspace --all-features --quiet -- --ignored e2e_`:
  **21 tests passed** on Restate 1.7.8 (three embedded tests and eighteen integration tests).
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`: passed.
- `cargo check -p restate-szamlazz --no-default-features`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.

The credit-refusal, wrong-order storno, opaque-account and cancellation-guidance regressions failed
before their fixes and passed afterward. Independent Standards and Spec reviews identified two weak
assertions (revocation without the original assertion header; optional clearance); both were strengthened,
their affected real-runtime tests rerun successfully, and the Spec recheck confirmed them addressed.
The credit interruption test additionally asserts that the run was still open when interrupted.

These checks establish worker behavior against scripted exchanges. They do not establish vendor
processing bounds, concurrent vendor deduplication, or a deployment's account/authorization mapping.
