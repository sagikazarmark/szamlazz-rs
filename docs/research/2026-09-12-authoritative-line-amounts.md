# Authoritative line amounts: provider basis and evidence boundary

Issue: #224. Primary source retrieved 2026-09-12:
[Számla Agent invoice rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding),
page footer v202608271632.

The vendor defines net unit price separately from net, VAT and gross line values.
Its HUF gross-first algorithm rounds gross unit price × quantity, calculates and
rounds `line gross × rate / (100 + rate)`, then subtracts VAT from gross. Its actual
XML example sends quantity 3, net unit 393.66, net 1181, VAT 319 and gross 1500.
The paragraph labels `(1500 − 319) / 3` as total net, but the XML distinguishes
the unit price and line net correctly. The worker regression uses that XML example.
The guide requires integral HUF line amounts and allows fractional foreign amounts;
it also describes server repair of fractional HUF input. This contract sends already
rounded assertions and does not rely on repair.

For EUR the chosen cent-scale extension gives 30.00 gross, 6.38 VAT and 23.62 net;
net unit 7.873333 × 3 rounds to 23.62. The old 23.61/6.39 split is outside the chosen
contract, not established as a vendor refusal. Existing [EUR precision observations](../szamlazz-hu-behaviour.md)
showed independent two-decimal storage rounding; they did not execute this convention.

## Offline evidence

`tests/monetary_input.rs` exercises public worker JSON, shared preflight and
`Account::build_create` through generated monetary XML, with independent literal
totals for the EUR mismatch and vendor HUF example. It covers mixed rates, deduction
lines, fractional quantities, midpoint boundaries, exact sums, invalid combinations
and schema/runtime refusal. Metamorphic cases test sign symmetry and preservation of
legacy-calculated amounts when asserted explicitly. The protected ingress suite checks
monetary refusals produce `invalid_input` without a vendor operation or marker.

## Live acceptance — passed 2026-09-12

The initial implementation session could not run this case because
`SZAMLAZZ_AGENT_KEY` was unset. In a follow-up, the operator authorized issuance
and reversal using the existing `.env` test-account credential. The focused case
ran once through actual Restate 1.7.8 and protected Order against szamlazz.hu,
with no whole-test retry:

```sh
set -a
source .env
set +a
RESTATE_SERVER_BIN=/tmp/opencode/restate-server-x86_64-unknown-linux-musl/restate-server \
SZAMLAZZ_LIVE_RUN_ID=issue224-20260912 \
cargo test -p restate-szamlazz --all-features --locked --test live \
  authoritative_gross_eur -- --ignored --exact --nocapture
```

Result: **1 passed, 0 failed**, 8.51 seconds. Code under test: `3b8a347`
(monetary implementation introduced in `99480de`).

| Evidence | Observed value |
|---|---|
| Order | `45decc5a-512e-477e-a4d2-e047f7e2b67b` |
| Ingress idempotency key | `45decc5a-512e-477e-a4d2-e047f7e2b67b:gross-invoice` |
| Invocation | `inv_1d2fgdU4exkF5cqxw1VAvCVnJIiaL65HWy` |
| Worker outcome | HTTP 200, `issued` |
| Invoice | `E-CTEST-2026-63` |
| Queried identity | Exact invoice number, matching order, `SZ`, `teszt=true` |
| Currency / exchange rate | EUR / 400 |
| Line quantity / VAT rate | 3 / 27% |
| Persisted line and document totals | Net 23.62, VAT 6.38, gross 30.00 |
| Cleanup reversal | `E-CTEST-2026-64` |

Public monetary preflight accepted the explicit amounts before issuance. The
test then queried the exact issued number and asserted all three line and
document amounts, quantity, VAT rate, currency and exchange rate. Cleanup sent
one direct Számla Agent storno with the original's queried date and appearance,
queried its exact returned number as `SS` referencing the original, and freshly
queried the original as reversed. No unresolved write or cleanup failure remained;
`Restate::finish` completed successfully.

This establishes provider acceptance and queried persistence for the selected
EUR gross-first convention. It does not inspect PDF rendering, test the old
23.61/6.39 split, or establish all currencies, VAT regimes or provider tolerances.
