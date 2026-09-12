# Worker hardening acceptance — 2026-09-12

Reviewed and implemented over `a45479e1215008ca245d1fa59f9b61c490e0e221`.
This record covers the four release-review fixes, not release machinery.

## Changes

- A missing expected reissue holder at the permitted leading query returns
  `conflict{target_changed}`. No create is sent; its recorded result precedes
  marker clearance. Post-send absence still retains uncertainty.
- Credit-entry caller content is validated through shared request construction
  and Számla Agent `to_wire` before account resolution or credential fetching.
- Issue-floor rustdoc describes unmanaged storno and does not infer settlement
  from delay or empty queries.
- `DeleteReason::Szamlazz(String)` is replaced by `Other(String)`. This is a
  Rust source API change; JSON strings stay unchanged. Numeric strings also
  remain unclassified because the wire does not identify their origin.

Validation ordering changes an invalid credit request's journal prefix. Keep
immutable deployment routing; exceptional replay needs the usual actual-prefix
review. These fixes do not change the stored unresolved-write marker schema.

## Regression evidence

Both new Restate regressions failed on the original behavior before their fixes:

- `e2e_missing_target_before_send_is_a_settled_conflict`: observed the old HTTP
  500; now asserts HTTP 200 conflict, arm/write steps, no reconciliation or send,
  absent marker and retained-response replay.
- `e2e_invalid_credit_entries_need_no_account_or_credentials`: observed the old
  credential-store HTTP 503; now rejects empty replacement, six entries, year
  zero and XML-forbidden title/comment before any account or credential call.
  Valid input still reaches the unavailable store as a positive control.

Public JSON coverage preserves unknown deletion reasons exactly without assigning
vendor provenance. Independent standards and specification reviews found no issues.

Passed on the implemented changes:

- `cargo test -p restate-szamlazz --all-features --locked`:
  **374 ordinary tests and 6 doctests**.
- `RESTATE_SERVER_BIN=… cargo test -p restate-szamlazz --all-features --locked e2e_ -- --ignored --test-threads=1`:
  **41 actual-Restate tests**, server **1.7.8** (official musl binary, SHA-256 verified).
  Includes recovery admission/spoofing, exact-marker recovery, interrupted arming,
  cancellation, delayed visibility, credential rotation and retained uncertainty.
- `cargo clippy -p restate-szamlazz --all-targets --all-features --locked -- -D warnings`.
- `cargo fmt --all --check` and `git diff --check`.

## Live acceptance

Run label: `worker-hardening-a45479e`. Both worker live tests were selected
individually, serially, with no whole-test retries, using locally configured
credentials and a freshly launched Restate 1.7.8 per journey. Their test-mode,
document relationships, totals and cleanup assertions passed.

| Journey | Order | Documents and cleanup |
|---|---|---|
| Ordinary e-invoice | `344b5064-f986-479f-9e60-bf47b3d3b5f6` | `D-CTEST-19` consumed by `E-CTEST-2026-51`; reversal `E-CTEST-2026-52`; replacement `E-CTEST-2026-53`, cleaned up by reversal `E-CTEST-2026-54` |
| EUR prepayment/final | `e0b432b7-48a2-4dd1-85f2-1bed60d1d53c` | `D-CTEST-20` consumed by prepayment `E-CTEST-2026-55`; final `E-CTEST-2026-56`; cleanup reversed final as `E-CTEST-2026-57`, then prepayment as `E-CTEST-2026-58` |

No unresolved cleanup was reported. Ordinary acceptance verified original and
storno electronic appearance (`3`), previous-month fulfillment, retained-key
replay, fresh-invocation lookup, reissue and stale expected-target refusal.
EUR acceptance verified caller-supplied prepayment deduction, references and totals.

## Deployment-specific check still required

No deployed Restate admin/ingress URLs or production host recovery authorizer
were supplied. Local real-Restate recovery/admission tests passed, but they do
not establish that an operator can reach recovery through the actual deployment's
ingress and authorization configuration. Complete that check using
`docs/operations/order-recovery.md`, and verify the deployed resolver/store mapping
with `examples/verify_seller.rs` before go-live.
