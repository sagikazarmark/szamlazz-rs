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

## Live evidence still required

**Not executed in this implementation session:** `SZAMLAZZ_AGENT_KEY` was unset.
No new provider acceptance, persisted-total or printed-PDF evidence is claimed.
The focused opt-in worker case creates one actual EUR e-invoice through protected
Order, queries its exact number, checks identity, currency, rate, all three persisted
line/document totals, and uses the existing uncertainty-aware live cleanup. It reuses
the #218 live infrastructure, with no automatic whole-test retries:

```sh
# Load the intended test-account credential and actual Restate source as in docs/testing.md.
cargo live -E 'package(restate-szamlazz) & test(authoritative_gross_eur)'
# Without nextest:
cargo test -p restate-szamlazz --all-features --test live authoritative_gross_eur -- --ignored --exact --nocapture
```

Record the dated outcome and retained run diagnostics here after deliberate execution.
The test proves queried persistence if it passes; it does not inspect PDF rendering,
all currencies, VAT regimes or every provider tolerance.
