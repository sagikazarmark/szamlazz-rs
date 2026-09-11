# Changelog

## 0.4.0 (unreleased)

Breaking changes for the upcoming minor release of `szamlazz-ipn` (#149, #75):

- `PaymentNotification::gross_total` and `paid_gross` are now `Option<Decimal>`;
  `payment_method` is now `Option<String>`. Handle unknown values explicitly
  rather than substituting zero or a default method.
- `payment_date` remains `Option<Date>`, but unreadable dates now produce `None`
  instead of a parse error. Empty, missing or unreadable amounts also produce
  `None`; missing or whitespace-only methods produce `None`.
- New `raw_gross_total`, `raw_paid_gross`, `raw_payment_date` and
  `raw_payment_method` fields retain decoded, untrimmed text, including empty
  strings. They are `None` for absent parameters. Serde includes these fields
  and represents unknown typed values as `null`.
- Amounts use exact decimal parsing (including the existing lone-comma
  tolerance). Excess precision or range is unknown content with raw text
  preserved, never a rounded amount.
- The `serde` feature owns its JSON precision mode. Optional monetary JSON
  fields accept exact numbers or numeric strings even when built together with
  `szamlazz-agent`; they serialize consistently as strings/null. Invalid or
  inexact typed JSON amounts are errors, unlike lenient form content. The JSON
  boundary and buffering limitations are documented in the README.
- `is_fully_paid()` now returns `Option<bool>`: `None` if either amount is
  unknown. It compares unverified reported amounts; confirm payment through
  the Számla Agent before financial actions, even for `Some(true)`.
- `IpnParseError::Invalid { field, message }` is removed: dates and amounts no
  longer cause errors. `Missing("szlahu_szamlaszam")` remains for absent or
  whitespace-only identity. `MalformedForm` rejects broken percent escapes;
  `InvalidUtf8` rejects non-UTF-8 decoded form text and retains its typed source.
  The previous decoder silently repaired these malformed bodies.
- `PaymentNotification::new` retains its arguments for known-content fixtures,
  wraps amounts and method in `Some`, and leaves raw fields `None`.

The README now runs as doctests and documents lenient content, query-before-action
trust, interleaved retries, account scoping and trusted-proxy allowlist caveats.
This entry records the intended release; workspace and package versions are not
bumped here. A captured real IPN payload remains a separate test-account probe.
