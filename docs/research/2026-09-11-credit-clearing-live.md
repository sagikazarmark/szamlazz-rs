# Explicit credit-entry clearing: live observations

**Date:** 2026-09-11. **Result:** both independently selected probes passed on
the locally configured account that the operator confirmed was intended for
testing. Each probe asserted queried `teszt=true` before changing credit entries.
Continuity with the historical September 3/6/7 account was not established.

Code: `bed1d24` plus the uncommitted Agent review follow-up (outbound positive-year
validation, documentation and offline schema matrix). Scenario source:
[`tests/probes.rs`](../../crates/szamlazz-agent/tests/probes.rs).

## Executions and observed facts

| | Populated | Already empty |
|---|---|---|
| Run label | `agent-review-20260911-clear-populated` | `agent-review-20260911-clear-empty` |
| Order | `4988518e-b53a-499e-83a8-a3c3a91a6e33` | `281f8449-2076-4e1e-a4a5-8754326d40ac` |
| External id | `live:4988518e-b53a-499e-83a8-a3c3a91a6e33:clear` | `live:281f8449-2076-4e1e-a4a5-8754326d40ac:clear` |
| Invoice | `CTEST-2026-13` | `CTEST-2026-15` |
| Before clear | Queried empty initially; registered one 100 HUF credit entry and queried it back | Queried empty |
| Clear | `additiv=false`, zero `kifizetes` elements | Same |
| Parsed acknowledgement number | `CTEST-2026-13` | `CTEST-2026-15` |
| Returned outstanding | `3136` HUF, equal to original gross | `3136` HUF, equal to original gross |
| Queried entries after clear | `[]` | `[]` |
| Cleanup storno | `CTEST-2026-14` | `CTEST-2026-16` |
| Test result | 1 passed, 6 filtered, 5.20 s execution | 1 passed, 6 filtered, 4.70 s execution |

Test support queried each original before cleanup, then queried the returned
storno and verified its distinct number, storno type and original-number
reference. It did not query the original's reversal marker again after cleanup.
Both probes completed without an unresolved write or cleanup failure. No
whole-test retry or repeat of an unanswered write occurred.

Relevant captured output:

```text
LIVE run=agent-review-20260911-clear-populated order=4988518e-b53a-499e-83a8-a3c3a91a6e33
LIVE sending create live:4988518e-b53a-499e-83a8-a3c3a91a6e33:clear; an unanswered write must be reconciled, never rerun blindly
LIVE order=4988518e-b53a-499e-83a8-a3c3a91a6e33 document=CTEST-2026-13
LIVE sending register credit entry on CTEST-2026-13; an unanswered write must be reconciled, never rerun blindly
LIVE sending clear credit entries CTEST-2026-13 state=populated; an unanswered write must be reconciled, never rerun blindly
PROBE clear number=CTEST-2026-13 state=populated echoed=CTEST-2026-13 outstanding=Some(3136) entries=[]
LIVE cleanup reversal=CTEST-2026-14 original=CTEST-2026-13

LIVE run=agent-review-20260911-clear-empty order=281f8449-2076-4e1e-a4a5-8754326d40ac
LIVE sending create live:281f8449-2076-4e1e-a4a5-8754326d40ac:clear; an unanswered write must be reconciled, never rerun blindly
LIVE order=281f8449-2076-4e1e-a4a5-8754326d40ac document=CTEST-2026-15
LIVE sending clear credit entries CTEST-2026-15 state=already-empty; an unanswered write must be reconciled, never rerun blindly
PROBE clear number=CTEST-2026-15 state=already-empty echoed=CTEST-2026-15 outstanding=Some(3136) entries=[]
LIVE cleanup reversal=CTEST-2026-16 original=CTEST-2026-15
```

## Commands and evidence boundary

Credentials were loaded locally from `.env` without displaying their values.
These commands ran separately, serially, with the populated probe's successful
completion inspected before starting the already-empty probe:

```sh
SZAMLAZZ_LIVE_RUN_ID=agent-review-20260911-clear-populated cargo test -p szamlazz-agent --locked --offline --all-features --features jiff/tzdb-zoneinfo --test probes clear_credit_entries_populated -- --ignored --exact --test-threads=1 --nocapture
SZAMLAZZ_LIVE_RUN_ID=agent-review-20260911-clear-empty cargo test -p szamlazz-agent --locked --offline --all-features --features jiff/tzdb-zoneinfo --test probes clear_credit_entries_already_empty -- --ignored --exact --test-threads=1 --nocapture
```

`--offline` prevents Cargo dependency downloads, **not** the explicitly selected
tests' vendor requests. Output above is transcribed from the execution tool's
captured stdout/stderr. Raw HTTP response bodies/headers were not archived, so
the parsed number does not identify which response channel supplied it.

**Conclusion:** explicit empty replacement cleared one populated invoice and
succeeded on one already-empty invoice on this test account. Both acknowledged
the expected number and full outstanding balance. This does not establish
success-specific number guarantees for all accounts/versions, concurrent-write
behavior, or permission to repeat a clear after an uncertain answer. The
[vendor clarification](2026-09-11-agent-vendor-clarification.md) remains relevant.
Receipt and combined-preview probes were not executed.
