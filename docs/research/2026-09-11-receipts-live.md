# Receipt lifecycle, automatic MNB and email resend — executed 2026-09-11

## Scope and provenance

Executed against the operator-confirmed test account selected by the local
`SZAMLAZZ_AGENT_KEY`, using the working tree based on `10f00b3` with strict
success-verdict decoding and optional invoice indicators. The operator supplied
the email inbox; the address and agent key are not copied into this record.
All returned originals and verified reversals had `test=true`. No continuity
with the historical invoice-probe account is assumed.

The existing probes in `crates/szamlazz-agent/tests/probes/receipts.rs` were run
one at a time with `client-reqwest` and `jiff/tzdb-zoneinfo`. Console output and
subsequent CLI JSON were observed during execution; this record transcribes the
relevant facts. Raw HTTP bodies/headers and PDF files were not archived. PDF
checks in the passing probes establish the signature only, not rendered content.

## Runs and exact identities

| Run label | Order / creation call ID | Result |
|---|---|---|
| `agent-review-20260911-receipt-lifecycle` | `acab20f7-696e-4695-9c44-17d76baee994` | Prefix `RSPROBE` refused with 337; no successful create |
| `agent-review-20260911-receipt-lifecycle-5char` | `b166192e-4de5-4411-80f1-147956119015` | Passed: `RSPRB-2026-1`, id `87214636`; reversal `RSPRB-2026-2` |
| `agent-review-20260911-receipt-mnb` | `4894ec4b-fa77-46da-abee-33454e08179f` | Passed: `RSPRB-2026-3`, id `87214687`; reversal `RSPRB-2026-4` |
| `agent-review-20260911-receipt-email` | `9e99201e-65bf-4e80-8613-09e37d1e48b9` | First send acknowledged; immediate resend refused with 153. Original `RSPRB-2026-5`, id `87214712`; subsequently recovered and reversed as `RSPRB-2026-6`, id `87214810` |

### Prefix restriction and allocation

The seven-character `RSPROBE` request returned:

> 337: Az előtag csak nagybetűket és számokat tartalmazhat, és hossza legfeljebb 5 karakter lehet.

Translation: the prefix may contain only uppercase letters and digits, with a
maximum length of five characters. The current [error table](https://docs.szamlazz.hu/agent/generating_receipt/response)
states the alphabet but omits the length. The refusal was returned directly to
the only send for that call ID; the next run was a new operation with corrected
prefix, not a retry of an unresolved write.

`RSPRB` was chosen for these probes and accepted without prior UI registration.
This distinguishes receipt prefix allocation from the invoice prefix's
pre-registration requirement. The docs separately forbid using an invoice
prefix for receipts (336); that collision case was not executed.

### Lifecycle — passed

HUF item: quantity 1, unit net 787.40, net 787.40, VAT 212.60, gross 1000;
one cash tender of 1000. Creation requested a PDF and persisted order/call ID.

- Number and order queries returned the same number/id, order/call ID, NY type,
  unreversed state, test marker, HUF currency, totals and tender sum.
- Create and query PDFs had `%PDF-` signatures.
- Deliberately repeating the **completed, verified** create returned 338.
  A further order query still returned the original number/id.
- Storno call ID `21ffb8ad-e447-4d28-abed-432d02a9aa5c` returned `RSPRB-2026-2`.
  The SN was queried and checked for distinct number, type, original reference,
  test marker and PDF signature. The original was queried with `reversed=true`.

This establishes duplicate-call behavior for a completed request, not concurrent
deduplication, retention duration or unknown-answer recovery.

### Automatic MNB — passed

The same item/tender amounts were sent in EUR with `ExchangeRate::automatic_mnb()`:
bank `MNB`, omitted numeric rate. The stored receipt returned:

```text
receipt=RSPRB-2026-3 currency=EUR bank=Some("MNB") rate=Some(363.9)
```

Gross remained 1000. Cleanup used storno call ID
`63f0c3b2-9eb9-4c9a-b2be-f884d8863854`, producing `RSPRB-2026-4`; the same
SN-reference and original-reversal verification passed. This is receipt-specific
execution evidence supplementing PHP comments, not a rule for every currency/date.

### Email — initial probe failed; exact-number recovery succeeded

First send included recipient, reply-to, subject and body. Subject:

```text
Receipt probe 9e99201e-65bf-4e80-8613-09e37d1e48b9
```

The first send was acknowledged. The immediately following present-empty
`emailKuldes` resend returned:

> 153: Hoppá... e-mail küldés sikertelen. Az ok: Rövid időn belül túl sok értesítőt küldél ki. Kérjük, várj legalább 15 másodpercet a következő küldés előtt!.

The message explicitly refused sending because notifications were too close and
requested at least 15 seconds. Code 153 remains an open `ErrorCode::Unknown` in
the library: this one receipt observation does not establish its meaning on
every operation. The probe conservatively deferred cleanup and failed.

Recovery did **not** rerun creation or the whole probe:

1. CLI query of `RSPRB-2026-5` confirmed exact id/order/call ID, NY, test=true,
   unreversed, and the expected amounts. Receipt existence is not delivery proof.
2. After more than the requested interval, a deliberate `receipt send RSPRB-2026-5`
   with no overrides (present-empty email block) returned `{"sent": true}`.
   The repeat was based on the explicit rate-limit refusal, not elapsed time as
   settlement of an unanswered exchange.
3. CLI storno of that exact original returned `RSPRB-2026-6`, SN, original
   reference `RSPRB-2026-5`, test=true. This cleanup request omitted call ID/PDF.
4. Separate CLI queries verified both the original's `reversed=true` and the
   SN's identity, original reference, order and test marker. The SN showed
   `reversed=true`, quantity −1, negative net/VAT/gross and a −1000 cash tender;
   the original retained its positive tender and totals.

**All three issued originals are verified reversed.** No unresolved create,
storno or email send remains from these runs. On 2026-09-11 the operator confirmed
that both emails arrived, establishing inbox delivery for the first send and
delayed empty-block resend. Exact body/attachment equality and individual-field
inheritance were not separately checked.

The probe now spaces its two intended email sends by 16 seconds. This is a
test-only pacing adjustment, not automatic client retry or proof a fixed delay
always avoids account-wide throttling. The adjusted full probe was not rerun:
the successful exact-number recovery supplies the resend evidence without
creating another receipt or sending another pair of messages.

## Commands

Each probe invocation loaded the key from local `.env` without putting it in
arguments. The first used `RSPROBE`; subsequent invocations used `RSPRB`.
The email invocation additionally set the operator-supplied inbox in its
environment. Run labels are listed above.

```sh
cargo test -p szamlazz-agent --all-features --features jiff/tzdb-zoneinfo --locked --test probes receipts::receipt_lifecycle -- --ignored --exact --test-threads=1 --nocapture
cargo test -p szamlazz-agent --all-features --features jiff/tzdb-zoneinfo --locked --test probes receipts::receipt_automatic_mnb -- --ignored --exact --test-threads=1 --nocapture
cargo test -p szamlazz-agent --all-features --features jiff/tzdb-zoneinfo --locked --test probes receipts::receipt_email_resend -- --ignored --exact --test-threads=1 --nocapture

cargo run -p szamlazz-cli --locked -- --json receipt get RSPRB-2026-5
cargo run -p szamlazz-cli --locked -- --json receipt send RSPRB-2026-5
cargo run -p szamlazz-cli --locked -- --json receipt storno RSPRB-2026-5
cargo run -p szamlazz-cli --locked -- --json receipt get RSPRB-2026-5
cargo run -p szamlazz-cli --locked -- --json receipt get RSPRB-2026-6
```

## Still open

NAV reporting, exact email body/attachment equality, partial email overrides, call-ID lifetime/scope,
post-storno order-query selection, concurrent deduplication, template rendering,
the universal success-number guarantee and authoritative XSD versions. No
combined-preview experiment was performed.
