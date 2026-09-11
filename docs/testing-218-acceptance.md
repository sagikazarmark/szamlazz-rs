# #218 acceptance — 2026-09-11

Implemented against starting commit `9b78546ad2c1c024d3cf14992285f02a19aa5dc6`.
Credentials came from the local `.env` through environment/Secret injection;
no credential value is recorded here. Queried issued documents reported test
mode. Restate server: 1.7.8; Rust: 1.98; local nextest: 0.9.144; Dagger nextest:
0.9.143; cargo-hack: 0.6.45.

## Vendor evidence

All five core scenarios passed locally, then in two deliberate complete Dagger
executions. Every known live invoice was reversed; proformas were consumed or
deleted. No unresolved sends or cleanup failures were reported.

| Dagger run label | Nextest run id | Start (UTC) | Result |
|---|---|---|---|
| `acceptance-218-20260911-1` | `11a738e2-da8f-40c0-96e5-d73c459838c7` | 11:37:12.604 | 5 passed, 53.368 s |
| `acceptance-218-20260911-2` | `1405cc6e-519a-4eba-90e6-21cbc6e0f6d2` | 11:38:34.054 | 5 passed, 52.315 s |

The second Dagger trace shows cached container preparation but a newly executed
`live` function. Distinct JUnit timestamps/run ids establish execution freshness:

- [Run 1](https://dagger.cloud/sagikazarmark/traces/cf4e855b7c79f7737bfc3f9e1257764b)
- [Run 2](https://dagger.cloud/sagikazarmark/traces/f1ee7222485e061ce499be3039d7dac6)

Initial local document evidence:

- Paper/credit entries: order `6cf5ef75-9d5e-4c84-a90c-5e36a88a866e`,
  `CTEST-2026-1` → storno `CTEST-2026-2`. Stored HUF totals 2469/667/3136;
  credit-entry outstanding 3036 → 2936 → 2886. Repeated storno returned the
  same reversal.
- Direct proforma: order `0c3c932c-eeb3-4dd8-afed-71707bbb9e2d`, `D-CTEST-1`,
  deleted and absent by both selectors.
- Ordinary worker: order `f890aa85-4393-4db4-9df6-def89d9f6833`, `D-CTEST-2`
  consumed by `E-CTEST-2026-1`, reversed by `E-CTEST-2026-2`, reissued as
  `E-CTEST-2026-3`, cleaned up by `E-CTEST-2026-4`.
- EUR chain: order `896f2506-a14a-4daf-9412-5f2fb019d23a`, `D-CTEST-3`
  consumed by prepayment `E-CTEST-2026-5`; final `E-CTEST-2026-6`, then
  cleanup stornos `E-CTEST-2026-7` (final) and `E-CTEST-2026-8` (prepayment).
  Stored final totals 24.69/6.66/31.35, including the explicit deduction.

After the review tightened storno verification, both separated appearance probes
passed through that shared send/verification path in Dagger:
[probe run](https://dagger.cloud/sagikazarmark/traces/7e487637f5bfdd0ff0b7cf02484c5d95).
They retained the #73 result: the storno uses its request's appearance.

## Offline, selection and execution policy

- Full `cargo test --workspace --all-features --locked` passed with the agent
  key present: 709 ordinary tests and 21 doctests; all external tests ignored.
- All 37 `cargo hack check --workspace --feature-powerset --locked`
  combinations passed.
- All 21 actual-Restate/mocked-vendor scenarios passed: 18 integration-target
  cases, plus the three library execution/cancellation cases. The Dagger
  integration-target run exported its JUnit directory successfully:
  [e2e trace](https://dagger.cloud/sagikazarmark/traces/cdcae3bbd7adbd6dbccaed861456740c).
- Programmatic assertions over nextest listing verified: default/CI select 709
  ordinary tests (including all nine e2e helper tests); live selects five;
  probes selects two; e2e selects 21. Live/probes are disjoint from ordinary CI.
- Ordinary Cargo on `--test live --test probes` with credentials set ignored
  all seven vendor-dependent scenarios.
- Selected missing-key, missing-Restate and missing-transport runs failed
  explicitly, rather than passing by skipping or selecting no tests.
- A Dagger empty-key run failed before any vendor operation, stopped after the
  first scenario, and printed failure JUnit while preserving exit code 100:
  [failure trace](https://dagger.cloud/sagikazarmark/traces/762993ae72549ac7f9868fc72d398b23).
- Workspace typechecking, formatting and diff-whitespace checks passed.
  Clippy completed with existing warnings in `src/xml.rs` (`doc_markdown`) and
  `tests/response_headers.rs` (`single_match_else`); strict `-D warnings` is
  blocked by those existing findings. No new-code warnings remain.

## Review

Independent Standards and Spec reviews covered this change against #218.
Standards initially reported two violations and one duplication smell; Spec
reported three findings. Fixes retain storno uncertainty until verification,
share reversal verification, permit cleanup after definitive direct refusals,
print worker fault details, and include failed JUnit/kept server logs in Dagger
traces. Both reviewers rechecked and reported no unresolved findings.

Shared profiles/tooling form #218's integration with #132; its remaining
fast-profile/MSRV/alias-performance work is not claimed complete here.
