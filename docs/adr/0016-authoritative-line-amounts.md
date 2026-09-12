# Authoritative line amounts beside net-calculated input

Accepted 2026-09-12 for #224. Keep `unit_price` as net unit price and add optional
explicit `amounts: {net, vat, gross}` rather than reinterpret old requests or invent
a gross-price convention implicitly. Validate explicit HUF/Ft and EUR lines against
net-first or line-gross-first rounding, preserving assertions exactly; refuse an
inconsistent combination. A shared monetary preflight and optional document
`expected_totals` guard approved money before execution.

The trade-off is deliberate: the downstream 3 × €10 example migrates its split from
23.61/6.39 to 23.62/6.38 while retaining €30 gross. We do not encode undocumented
provider tolerances to keep arbitrary externally calculated splits. Nor do we claim
those splits are impossible at the vendor. Explicit input leaves the caller in control
of its approved convention and net-unit precision without adding a second calculator.

Exact, bounded arithmetic refuses overflow and precision loss. Explicit currencies and
zero-VAT codes are a conservative subset; legacy rate/currency semantics remain open.
Whole-document sums must also be exact. Query observations after a send cannot undo a
financial mismatch, hence assertions are checked through the same construction path
before Order arms a write. This adds no send permission, durable state or recovery path.

See the [public monetary contract](../../crates/restate-szamlazz/README.md#approved-monetary-amounts-and-gross-price-migration)
and [primary-source/evidence record](../research/2026-09-12-authoritative-line-amounts.md).
