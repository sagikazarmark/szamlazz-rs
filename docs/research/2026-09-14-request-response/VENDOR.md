# Vendor overlap prototype — 2026-09-14

Archived findings; the one-off runner was retired after #247. Its source remains
in commit `55d3c3d` at `crates/restate-szamlazz/tests/prototype_request_response/vendor.rs`.
Archiving does not settle the inconclusive corrective cleanup below.

## Result

**Two overlapping identical ordinary invoice requests returned the same number
and provider id. Two overlapping identical corrective requests created two
different corrective invoices.** Both correctives referenced the same original;
the shared external-id query returned only the newer corrective.

This is actual szamlazz.hu test-account evidence, distinct from the adjacent
fake-provider RequestResponse experiment. The user confirmed the configured
test-account key and enabled duplicate-order checking before execution. Every
queried document reported `test: true`.

## Method and scope

- Native reqwest 0.13.4, Számla Agent 0.8.0 source in this workspace.
- Direct wire exchange, no Restate: precisely two create sends per pair, started
  behind a two-party barrier. Separate HTTP clients, connections and fresh cookie
  jars. Credentials were supplied from the ignored `.env` and are absent from evidence.
- Both members of each pair serialize the same immutable request: same order,
  external id, issue/fulfillment/due dates, buyer, line and totals. Paper HUF
  document, net 1000, VAT 270, gross 1270, no requested email or PDF.
- Client retries and redirects disabled; 60-second request deadline. No manual
  timeout, request abort or provider response suppression. Both answers were
  collected before further mutation.
- Exactly **five create sends**: ordinary pair, separate corrective base,
  corrective pair. Two cleanup storno sends followed; no write was repeated.
- Both client exchanges overlapped, but we cannot control or establish simultaneous
  vendor transaction execution. This tests overlapping submissions, not an atomicity
  proof or an estimate of accidental duplicate frequency.

Run: `61e35a2ff49343629c06e98ae112e7bb`, started
`2026-09-14T15:43:36.674111531Z`.

## Observed documents and timing

| Operation | Client start (ms from run start) | Answer (ms) | Number | Provider id |
|---|---:|---:|---|---:|
| Ordinary A | 90 | 2449 | CTEST-2026-28 | 930038478 |
| Ordinary B | 90 | 2537 | CTEST-2026-28 | 930038478 |
| Corrective base | 5937 | 7577 | CTEST-2026-30 | 930038520 |
| Corrective B | 7721 | 10238 | CTEST-2026-31 | 930038529 |
| Corrective A | 7722 | 12450 | CTEST-2026-32 | 930038532 |

All five create replies were HTTP 200 with numbered successful results. By-number
queries verified the ordinary document as `SZ`, both corrective documents as `HS`,
and both corrective base references as `CTEST-2026-30`.

Ordinary order: `vp-61e35a2ff49343629c06e98ae112e7bb`.
Corrective base and corrective order: `vb-61e35a2ff49343629c06e98ae112e7bb`.

The shared corrective external id was
`vendor-prototype:vb-61e35a2ff49343629c06e98ae112e7bb:corrective:c1`.
Its query returned `CTEST-2026-32` / `930038532`. It did not reveal the other
corrective, which was independently queried by its reported number.

## Cleanup and retained work

- Ordinary `CTEST-2026-28` was reversed by `CTEST-2026-29`. Read-back verified
  the `SS` type, original reference, negative totals and refreshed original's
  `reversed: true`.
- One reversal request against corrective `CTEST-2026-32` received HTTP 200 with
  vendor code **13**, no reported document number. The library classified it
  `OutcomeClass::Unknown`. No vendor-message interpretation was invented and no
  additional reversal was sent.
- The probe therefore **failed at cleanup**, after completing both overlap
  observations. It must not be reported as a fully passing lifecycle.
- A separate read-only follow-up successfully queried `CTEST-2026-30`,
  `CTEST-2026-31`, `CTEST-2026-32`. All still existed, reported `test: true`, and
   none reported `reversed: true` (`reversed` parsed as `None`). This is the recorded
  observation, not proof that the earlier cleanup request cannot act later.
- Those three documents remain on the test account for operator review; the
  corrective cleanup request remains inconclusive by the library's classification.

## Implications for the policy discussion

1. Ordinary-invoice duplicate protection has fresh, directly relevant positive
   evidence under independent-session overlap, consistent with vendor documentation.
   One pair does not prove every concurrent/lifecycle case or configuration.
2. **Corrective deduplication cannot be assumed.** Here two identical submissions
   really created two documents—not merely two HTTP sends to a nondeduplicating mock.
3. Query-first reconciliation may prevent a second corrective if the first is
   already visible. It does not establish exclusion while the first is pending,
   nor does finding the newest holder establish that only one was issued.
4. The RequestResponse narrow policy therefore needs an explicit corrective-risk
   decision. Do not generalize ordinary-invoice evidence into a uniform retry
   guarantee, and do not equate successful completion with uniqueness.
5. No failure was injected into the live exchanges. This does not measure how
   frequently a real Restate interruption would lead to overlapping submissions.

## Retained artifacts

- [vendor-observed-2026-09-14.json](vendor-observed-2026-09-14.json): incrementally
  persisted exchange metadata, parsed identities and cleanup outcome.
- [vendor-followup-2026-09-14.json](vendor-followup-2026-09-14.json): exact-number
  read-only follow-up. Its elapsed times start at that separate process's start.

The evidence contains allowlisted metadata, not full response XML, PDF, session
cookies, credentials or buyer records. Reports were written to separate files
without automatic test-level retry. Repeating overlap would create new documents;
it would not recover or settle this recorded run.
