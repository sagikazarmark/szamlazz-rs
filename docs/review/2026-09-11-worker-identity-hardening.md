# Worker identity hardening

Implemented against `61c334f`, following the release-readiness review.

## Changes

- Every Gateway query requires a nonblank document number; number selectors must return the exact requested number.
  The check precedes ownership, date/form selection, mutation decisions and settlement. Vendor numbers keep their
  full spelling and are not subjected to caller length limits. Failed identity is an unanswered read.
- Storno candidates must differ from the original. Protected Order lookup, the armed write's leading query and
  recovery share candidate/order verification and a fresh check of the original's type, order and reversal.
- XML-invalid query selectors are `invalid_input` before the prologue, using the Agent writer's character rule.
  Local request failures retain a sanitized diagnostic instead of an unclassified exchange message.
- README now distinguishes protected Order mutations, unmanaged storno and credit-entry registration.

## Review and verification

Standards review found no remaining issues. Spec review identified an existing protected-storno lookup shortcut
and a masked distinct-number regression; both were corrected and the final review found no remaining issues.
Four added real-Restate tests cover nameless issuance, self-reference independently of original type, wrong-number
verification in both storno handlers, selector validation before durable work, and both protected storno lookup
boundaries. Assertions include zero/single sends, marker retention, recorded commands and later valid settlement.
Existing deletion expectations now treat wrong-number XML as an unanswered guard read, not target discovery.

- Workspace all-feature locked tests and doctests passed; worker tests were repeated after the final helper change.
- All **25** selected real-Restate tests passed on server **1.7.8**.
- Strict workspace Clippy, worker no-default-features check, formatting and diff checks passed.
- All **five vendor-live acceptance journeys** passed using the locally supplied test credentials, serially with
  zero test retries. Nextest run: `64fe3cb6-5770-458e-866a-00200693dcc4`; JUnit: `target/nextest/live/junit.xml`.
  The first direct financial runs stopped before sending because timezone support was unavailable; NAV passed.
  The successful run explicitly enabled `jiff/tzdb-zoneinfo` with system tzdata, as documented in `docs/testing.md`.

Live document evidence:

| Journey | Order | Documents / cleanup |
|---|---|---|
| Ordinary Order | `2b473b8f-7cfc-41f6-b047-019626ac6070` | `D-CTEST-10` consumed; `E-CTEST-2026-27` reversed by `28`; reissue `29` reversed by `30` (same prefix) |
| Prepayment/final | `f31e577f-165a-4c3c-a309-266571a6ed83` | `D-CTEST-11` consumed; `E-CTEST-2026-31` / `32` reversed by `34` / `33` |
| Direct invoice/credit entries | `65e51289-7ba2-4ce6-aad9-e7f4bd700c3c` | `CTEST-2026-9` reversed by `CTEST-2026-10`; repeat returned the same reversal |
| Direct proforma | `58d9cef2-9e03-4cf1-97ac-c66780d6b0b4` | `D-CTEST-12` deleted and absence verified |

No uncertain financial write or cleanup failure remained in this acceptance run.

## Remaining deployment gate

The actual deployed host's recovery authorization and credential wiring were not supplied. Its operator recovery
drill remains pending; local real-Restate recovery tests and vendor-live journeys do not substitute for that check.
Use a new immutable deployment: stricter evidence changes branch behavior even though step names remain unchanged.
