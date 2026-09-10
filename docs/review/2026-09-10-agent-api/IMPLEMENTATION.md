# Review follow-up implementation

Implemented on 2026-09-10, following `REPORT.md` and `JUDGMENT.md`. The original
reports describe their reviewed baseline; this file records subsequent closure.

## Completed

- P05-02: README recovery recognizes an absent reversal marker as live, retaining
  order/type checks and unresolved-query handling.
- IO-01: selector, operation and recovery docs state the all-matching-proformas
  scope of order-number deletion.
- IR-01/R-01: raw numeric VAT tokens calculate as percentages; response helpers
  interpret numeric XML whitespace while preserving raw text and special-code
  precedence. Unrepresentable numeric tokens refuse derived arithmetic.
- QX-02: one exact finite-number converter serves response Decimal adapters,
  invoice body/header amounts and numeric VAT interpretation. Ordinary exponent
  and comma-header support remain. Representable source scale is retained;
  nonzero precision loss is refused. Numeric underscore leniency is removed,
  documented in the 0.4 migration notes.
- P05-01: multipart collision candidates remain bounded below MIME's 70-character
  limit and are checked against XML and attachment content.
- IO-02/QX-01: foreign subtrees cannot supply protocol fields. Unknown-child
  placeholders preserve scalar structure rather than joining adjacent text.
  Undeclared element/attribute prefixes are refused. Namespace character
  references compare by decoded URI, including the versioned taxpayer parser.
- IR-03/IR-04/IO-03/P05-03: VAT definitions, K.AFA processing guidance,
  credit-entry matching terminology and transport retry ownership corrected.

Vendor questions are drafted in
[`2026-09-10-agent-vendor-questions.md`](../../research/2026-09-10-agent-vendor-questions.md),
**not sent**. Combined preview/simpleItems ordering and layout mappings remain
pending clarification. NAV envelope metadata remains a deferred capability choice.
No live account calls were made.

## Independent review

### Standards

The reviewer identified scalar text joining, source-scale compatibility and
duplicate numeric grammar. All three were addressed and rechecked; no remaining
actionable standards findings were reported.

### Spec

The reviewer identified escaped namespace URI identity and missing committed
controls for financial-item/receipt-subtotal VAT helpers. Both were addressed
and rechecked; no remaining actionable spec findings were reported.

The spec reviewer additionally ran an external scratch probe with 174,000
representable-number equivalence checks, plus header, namespace and numbered-56
controls. Those are independent review checks, not part of the crate test count.

## Final verification

```text
cargo fmt -p szamlazz-agent
cargo clippy --locked --offline -p szamlazz-agent --all-features --all-targets -- -D warnings
cargo test --locked --offline -p szamlazz-agent --all-features
cargo check --locked --offline --workspace --all-features
```

All passed. **256 crate tests passed** (184 unit, 64 integration, 8 doctests);
four live tests remained ignored. Targeted failing-then-passing regressions
cover the numeric and namespace counterexamples. The workspace check includes
the CLI and Restate worker consumers of the changed crate.
