# Review fixes: closure and verification

Implementation base: `4394ed0977a0adf298a10e0acd9182d13cbf3c0c`.
Status: implemented and verified; included with the review-fixes commit, not released.
Origin: [consolidated CI/regression review](2026-09-11-consolidated-ci-and-regression-review.md).

## Finding index

| Finding | Resolution | Regression evidence |
|---|---|---|
| CI Python prerequisite | Python installed on the Dagger e2e container branch | Real container MARKER-KILL and complete e2e selection pass |
| S1 unsafe public create retries | Replaced `Gateway::create` with `create_once(request, CreatePermission)`; `grant()` asserts caller-owned exclusive authorization and settlement; permission is consumed, non-Clone and non-serializable | Compile-fail ownership tests, Gateway read-only reconciliation/send-count tests |
| W1 old-number reissue settlement | Shared send boundary returns `Unconfirmed::ReissueEcho`; protected path retains marker and reconciles read-only | Production Order regression failed before the fix with absent marker; now ordinary and numbered-56 replies preserve state, block a fresh call after kill, and send once |
| W2 same-number protected storno | Protected verified-invoice reply retains uncertainty; unkeyed observed echo policy stays separate, recorded in ADR 0007 amendment | Same production Order regression checks marker retention, blocked later mutation and one storno send |
| S2 feature-dependent numeric rejection | Exact public Agent monetary adapter owns JSON precision features; CLI consumes it | Actual CLI numeric/string/exponent tests both CLI-only and worker-unified |
| S3 public money precision loss | Agent monetary fields and CLI amount flag use exact parsing; inexact tokens are refused before HTTP | Exact outgoing multipart amount and zero-send negative controls |
| S4 vendor date attribution | Unusable original fulfillment date yields invoice-specific `unavailable`; caller comment remains `invalid_input` | Storno intent matrix including invalid year, missing date and caller text; private input text excluded from faults |
| S5 discovery mismatch | Portable control/whitespace exclusions; byte limit explicitly identified as additional runtime validation | Schema/runtime Unicode and C1 boundary tests |
| IPN #149 | Optional amounts/method/date; decoded untrimmed raw content; tri-state full-payment comparison; malformed encoding/identity remains shape | Form/axum/raw/unknown-content tests; README doctests and 0.4 migration changelog |
| IPN #75 | Query-before-financial-action, account scoping, unordered deliveries, dated allowlist and trusted-proxy guidance | Current README and rustdoc, reviewed against issue requirements |
| Documentation debt | Replaced obsolete active glossary entries, aligned worker protocol/README, documented CLI acknowledgement and paid intent; sample preserves omission | Documentation review and doctests |

## Independent follow-up review

The standards reviewer found two composition defects in the initial repair:

1. IPN's derived JSON Decimal decoder still failed when built with Agent's precision
   feature. IPN now owns its optional JSON precision feature and exact monetary
   adapter; standalone and Agent-unified graphs pass.
2. The built-in adjacently tagged `StornoResponse` buffered content encountered
   before its tag, rejecting numeric totals depending on JSON member order. Its
   decoder now retains raw JSON content until the tag is known. Both variants,
   either member order, exact numeric/string totals, duplicates, lookalike objects,
   and reader/bytes/Value roundtrips are covered. `CreationOutcome` has a separate
   externally tagged control.

The spec reviewer found that the first old-number guard used the numberless
diagnostic category. Dedicated `ReissueEcho` now preserves the actual cause in
the public error and journaled uncertainty; a journal regression verifies it.

Both reviewers rechecked their fixes: **Standards PASS, Spec PASS**, with no
remaining findings within the reviewed changes.

## Final verification

```sh
dagger --progress plain check --no-generate ci:end-to-end ci:test ci:schemas
```

All three checks passed on the final code:

- **ci:test:** 785 ordinary tests passed; workspace doctests passed.
- **ci:end-to-end:** 39 tests passed, including the new contradictory-reply
  protected-handler regression (three reply shapes), MARKER-KILL, replay,
  recovery and account scenarios. Execution: 157.991 seconds.
- **ci:schemas:** 430 generated requests / 860 source-specific validations:
  790 valid, 70 exact expected vendor-source conflicts; seven negative controls
  and declared-element-path coverage passed. Eight schema-runner tests passed.

Trace: <https://dagger.cloud/sagikazarmark/traces/8cc8508b5d1988905545dde0a3f058f0>.

Also passed: workspace all-targets/all-features Clippy with warnings denied,
worker no-default-features check, formatting, diff checks, IPN no-default/serde
checks, and focused dependency-graph monetary tests. A local full Cargo run
reached the IPN doctests when its outer 120-second tool deadline terminated it;
workspace doctests were rerun successfully and the final Dagger ordinary test
and doctest check completed independently.

## Migration and remaining evidence boundary

- Direct Gateway callers migrate to `create_once` and explicitly own durable
  uncertainty, exclusion and settlement. A permission token is not external proof
  or crash protection; it cannot make an automatic retry safe.
- IPN monetary/method fields are optional, `is_fully_paid()` returns `Option<bool>`,
  and raw content is retained. Unknown is neither zero nor unpaid. Changes are
  documented for upcoming pre-1.0 minor 0.4; workspace versions are not bumped here.
- Direct Agent/CLI JSON monetary input is exact. Caller-added Serde buffering
  wrappers can erase the distinction between arbitrary-precision numeric tokens
  and lookalike objects; documented string input is required for ambiguous buffered
  cases. Prebuilt Values cannot restore information already lost upstream.
- No vendor-live operations were performed. Capturing a genuine IPN payload remains
  the separate live probe from #149. Synthetic contradiction tests prove local
  classification/protection, not that the vendor emits those replies.
- Prior dated reports are historical evidence. Their old line references and
  approval statements do not override this closure record or certify later changes.
