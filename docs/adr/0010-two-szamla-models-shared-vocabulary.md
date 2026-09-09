# The two `<szamla>` models stay two; the crates share vocabulary, not types

Status: accepted (#129).

## Context

`szamlazz-agent` (`ops/query_xml.rs`) and `szamlazz-adatkapcsolat` (`document.rs`) both model the
`<szamla>` invoice document from the same `szamla.xsd`. Field for field they overlap almost
completely (`InvoiceInfo` in both, `BuyerInfo` / `Party`, `DocumentItem` / `InvoiceItem`, the totals,
the financial items, the credit entries, the bank and address blocks), in two styles: the agent maps
private `*Xml` wire structs onto public types through `From`, the receiver deserialises one struct
with `rename(deserialize = …)` doing XML in and JSON out. The 2026-09-06 and 2026-09-08 reviews
(#121) found two concrete divergences a consumer of both crates hits: the same XSD integer elements
typed at different widths across and within the crates (`alap/id` `u64` vs `i32`, `gazdEsemAzon`
`u64` vs `i64`, `forras` `u32` vs `i64`, `lokacio` `i64` vs `i32`, `afalevon` a required `i32` vs an
`Option<i64>`), and `InvoiceAppearance` twice with a different width (`i32` vs `i64`) and a different
JSON shape (a string `"2"` vs an integer `2`). The `tipus` code was a `String` in both until #139 gave
the agent an open `DocumentType`.

The review's estimate for a shared core crate (`szamlazz-xml`, holding the `<szamla>` model and the
`de` helpers, depended on by both) was M–L and about 1,000 lines removed.

## Decision

1. **Two models, kept deliberately.** Neither crate depends on the other and no core crate is
   extracted. What the two crates share is **vocabulary**: the integer-width policy below, the
   open-set rule of CONTEXT.md's *Document type* (a wire token the crate does not know is an
   `Other(String)`, a numeric code an `Unknown(n)`), one JSON shape for `InvoiceAppearance` (the
   integer code), and the same English name for the same Hungarian element wherever both read it
   (`document_type` for `tipus`, `appearance` for `eszamla`, `registration_number` for
   `iktatoszam`, `economic_event_id` for `gazdEsemAzon`, `is_e_invoice` as the `Electronic`
   variant). The receiver keeps `tipus` as the wire token (`Option<String>`) rather than adopting
   the agent's `DocumentType`: the type would be the one shared item and would pull a dependency
   for it.
2. **One integer-width policy.** Every integer element of a queried or pushed document (`xs:int`,
   `xs:integer`, `xs:long`) is an `i64` in both crates, signed and wide whatever the schema declares:
   the schema is szamlazz.hu's description of its own output and has been wrong about presence
   before, so neither reader bets on a width, and one width lets a consumer of both compare ids
   without a cast. *Presence* stays each crate's own rule (the agent requires what the schema
   requires, the receiver reads everything but the identity as `Option`; the *Shape vs content*
   rule). Request-side counts (`download_copies`, a waybill's `parcel_count`) are the caller's
   numbers and stay narrow, range-checked at the writer. `InvoiceAppearance` is `i64` in both and
   serialises as the integer code in both (the agent's `"2"` was the odd one). Each crate records the
   policy in its docs (`szamlazz_agent::ops::query_xml` *Integer widths*, the `szamlazz_adatkapcsolat` crate root's *Integer widths*).

## Considered options

- **A shared `szamlazz-xml` core crate.** Rejected, for now. The two parsers are different products
  of the same schema: the agent's is **strict** (a value not of its type is a `ParseError`, a `<pdf>`
  that does not decode fails the document), because a query is a request the caller made and can
  repeat; the receiver's is **lenient by design** (content never fails the parse, a bad date or PDF
  is `None` with the text in `raw_xml()`, only the identity is shape), because a push is
  at-most-N-times delivery and a refusal loses the record (#95, #122, #176). A shared struct would
  have to be the lenient one, with `Option` on every field, and the agent would then re-validate
  what its schema requires on top; or carry a leniency parameter through every `de` helper. That
  is the M–L estimate spent on making one type serve two contracts. The receiver also stays free of
  the agent's dependency tree (`reqwest` behind a feature, `rust_decimal` macros, the request side).
  The option stays open: if a third reader of `<szamla>` appears, or the two models drift again
  after this ADR's alignment, the extraction is the next step and this record is where it starts.
- **The receiver depends on the agent crate** (for `DocumentType`, `InvoiceAppearance`, the
  helpers). Rejected: the receiver's every consumer would build the agent's request side and its
  transport feature surface for two enums, and the lenient/strict split above would still leave the
  document types separate.
- **`i64` for values and a shared `DocumentId(i64)` newtype for the ids.** Rejected with the core
  crate: a newtype needs a home both depend on, and an id compared without a cast is what the
  policy buys already.
- **`i32`, the XSD's `int`.** Rejected: `banktranzid` is `xs:integer` (unbounded) in `szamlabe.xsd`,
  and a reader that trusts the declared width refuses a record over a bound szamlazz.hu may not
  honour itself; a shape refusal on the receiver side drops the record after 72 hours.

## Consequences

- Breaking 0.4 changes in both crates, listed in each README's *Breaking Changes in 0.4*: the agent's
  `InvoiceInfo::id`, `Supplier::id`, `BuyerInfo::id`, `economic_event_id`, `source`, `ordering`,
  `deductible_vat`, `bank_transaction_id`, `CreatedInvoice::document_id`, `Receipt`'s `id` and the
  `InvoiceAppearance` code are `i64`; the agent's `InvoiceAppearance` serialises as an integer (the
  CLI's `--json` output of a queried invoice changes with it); the receiver's `InvoiceInfo::id`,
  `Party::location` and the `InvoiceAck` id are `i64`; the receiver's `InvoiceInfo::kind` /
  `e_invoice`, `BankTransaction::kind` and `ReceiptInfo::kind` are `document_type` / `appearance` /
  `transaction_type` / `document_type`, in the archived JSON too.
- The worker's `FoundDocument::document_id` / `appearance` and `IssuedDocument::document_id` follow
  the agent's width (a journaled reshape, which ADR 0009 allows between releases).
- CONTEXT.md's *Document type* and *Invoice appearance* entries name the shared vocabulary; no new
  crate and no new term.
