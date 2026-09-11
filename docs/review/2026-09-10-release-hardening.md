# Release hardening: implementation and verification

Implemented against `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9` following the release-readiness review.

- Added audited positive settlement (`completed` evidence) distinct from vendor-verified document evidence and
  non-execution attestation, with exact-marker, operation and expected-document checks; ADR 0013 records the trust
  boundary. Marker schema stays version 1. New diagnostic journal shapes require a new immutable deployment.
- Preserved original 71/152 refusals on protected creates when diagnostic queries fail or return credential codes.
  Diagnostic documents are not adopted as proof of the requested corrective intent.
- Added direct by-number storno verification and automatic candidate/external-id/order-hint fallback, always
  validating original reference, order and original reversal. Operator-supplied numbers remain strict.
- Retained safe original-send diagnostics and candidate numbers; open reconciliation failures report the original
  and latest cause without copying response bodies or vendor free-text messages.
- Corrected credit-entry renewal guidance and effective retry-control descriptions; added the operator runbook.
- Keyed receipt JSON by record id and made CLI storno derive original appearance and fulfillment date.

## Standards review

Independent review found diagnostic credential codes replacing original refusal, automatic candidate recovery
failing to fall back, and inconsistent retry/deletion documentation. Addressed all three; added corresponding
runtime cases and corrected the public descriptions. Strict Clippy and formatting checks pass.

## Spec review

Independent review found adoption of diagnostic corrective documents without the full intended-base check and
missing positive-attestation/automatic-storno coverage. The protected duplicate path now preserves its original
refusal instead of adopting diagnostic document outcomes. Runtime tests cover wrong corrective base, credential
failure at both diagnostic positions, old reissue/storno exclusions, positive issuance/reversal/deletion
attestations, and automatic hint/external-id fallback. Follow-up review found no remaining blockers.

## Executed checks

- `cargo test --workspace --all-features --quiet`: unit, Gateway, contract, integration and doctests passed.
- `RESTATE_SERVER_BIN=/tmp/opencode/restate-server cargo test --workspace --all-features --quiet -- --ignored e2e_`:
  **15 real-Restate tests passed** on server 1.7.8 (three embedded runtime tests and twelve integration tests).
- `cargo clippy --workspace --all-features --all-targets -- -D warnings`: passed.
- `cargo check -p restate-szamlazz --no-default-features`: passed.
- `cargo fmt --all --check` and `git diff --check`: passed.

New contract, archive, CLI and runtime regressions were run failing before their corresponding fixes. The first
full build hit disk exhaustion; cleaning generated Cargo package artifacts restored capacity. Subsequent full
checks passed. Live vendor tests were not run; all financial writes used scripted loopback exchanges.
