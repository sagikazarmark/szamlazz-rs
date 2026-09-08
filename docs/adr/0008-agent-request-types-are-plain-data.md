# `szamlazz-agent` request types are plain data; the wire request carries no transport fields

Status: accepted (#71).

## Context

`szamlazz-agent` marked 77 of its 95 public structs and enums `#[non_exhaustive]`, request types
included, so a caller could only build a request through `new()` and then assign field by field,
the workspace's own worker paid some thirty such lines in its request builder, and the CLI dodged
the cost by deserialising JSON. The sans-IO `WireRequest` also carried a `url` and a
`session_cookie`: the bundled reqwest client overwrote the first and never read the second, and
`to_wire` always set the cookie to `None`, so a non-reqwest caller had to know which fields to
honour and which to ignore (#71).

## Decision

We draw the line by who produces the value:

- **What the caller produces is plain data.** Every request/input struct in `ops/*` and `item.rs`
  (`CreateInvoice`, `InvoiceHeader`, `Buyer`, `Seller`, `LineItem`, `StornoInvoice`,
  `RegisterCreditEntry`, …) is an ordinary struct: build it as a literal, or extend a constructor
  with functional update (`CreateInvoice { external_id: Some(..), ..CreateInvoice::new(..) }`;
  `Seller { bank: Some(..), ..Seller::default() }`). The constructors stay (they are the required
  fields), and `Default` is derived only where every field has a sensible absence. Adding a field
  to one of these types is now a breaking change; that is the price of literal syntax, and it is the
  right one, because a new request field is something the caller must decide about anyway.
- **What szamlazz.hu produces stays `#[non_exhaustive]`.** Response and document types (and
  everything they nest), error enums, and the request-side *enums* (`InvoiceKind`,
  `InvoiceTemplate`, `ReceiptTemplate`, the selectors) keep the attribute: szamlazz.hu grows them,
  and on an enum the attribute never blocked construction, only exhaustive matching. The one
  request struct that keeps it is `ReceiptPayment`, because the same block is read back in
  `Receipt::payments`.
- **`WireRequest` is exactly what any HTTP client needs**: `content_type` and `body`, nothing about
  the transport. The URL is a property of the transport, the reqwest client owns it through its
  builder, a sans-IO integration POSTs to `wire::ENDPOINT` (or a mock server). Session reuse is
  the transport's too: reqwest keeps the `JSESSIONID` in its cookie store; a sans-IO integration
  replays `RawResponse::session_cookie()` as the `Cookie` header of its next request. The struct
  is `#[non_exhaustive]` like every other crate-produced type, so a header the wire may need later
  can be added without a break.

## Considered options

- **Keep `url` and `session_cookie` and document them as advisory.** Rejected. A field the crate
  fills but its own client discards is a trap, not an affordance: the only way to use it correctly
  was to know it is not used. Removing the two fields and `with_session_cookie` is a breaking
  change to a 0.x crate (the next release of `szamlazz-agent` needs a minor bump, which
  `cargo-release` applies workspace-wide) confirmed by `cargo semver-checks` as the *only*
  breaking change of #71: the 28 `#[non_exhaustive]` removals are a widening and produce no
  finding.
- **Drop `#[non_exhaustive]` from the request-side enums as well.** Rejected. It buys nothing (a
  variant is constructible either way) and turns a new document kind or template token into a
  breaking change.
- **Add `Default` to every request struct so `..Default::default()` always works.** Rejected. A
  default fulfillment date, buyer name or invoice number is a wrong document, not a convenience.

## Consequences

- `crates/szamlazz-agent/tests/literals.rs` builds the request types as literals from outside the
  crate (the compile-time proof of the widening), and `tests/sans_io.rs` drives the core with a
  client the crate knows nothing about (`ureq`, against wiremock), which is also the README's
  sans-IO example; the README is compiled as a doctest.
- The worker's request builder (`gateway/build.rs`), its seller and buyer projections, the storno
  and credit-entry sends, and the CLI's by-number commands are struct literals now.
- The `Client` module is where every transport concern is named: endpoint, cookie store,
  timeout, TLS, redirect policy.
