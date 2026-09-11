# szamlazz-cli

[![crates.io](https://img.shields.io/crates/v/szamlazz-cli?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-cli)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-cli?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-cli)

**Command-line client for the Számla Agent and local development receiver for IPN and Adatkapcsolat.**

## Usage

Set an Agent key, then issue or query documents from the shell. Ready-to-edit request files are available as [examples/invoice.json](examples/invoice.json) and [examples/receipt.json](examples/receipt.json).

```text
export SZAMLAZZ_AGENT_KEY=your-agent-key

szamlazz invoice create -f examples/invoice.json --pdf invoice.pdf
szamlazz invoice get E-2026-123
szamlazz invoice download E-2026-123 -o invoice.pdf
szamlazz invoice storno E-2026-123 --comment "Hibás vevő"

szamlazz payment register E-2026-123 --date 2026-07-04 --method átutalás --amount 12700
szamlazz proforma delete D-2026-42
szamlazz receipt create -f examples/receipt.json
szamlazz taxpayer 13421739

szamlazz listen --adatkapcsolat-key KEY
```

Számla Agent commands support `--json` for machine-readable output. The `listen` command does not: it pretty-prints received messages for interactive development.

`szamlazz listen` serves IPN at `POST /ipn`. When an Adatkapcsolat key is configured, it also serves `POST /adatkapcsolat`. Point a tunnel such as `cloudflared` at the listener to inspect real deliveries during integration work. The Adatkapcsolat endpoint answers a wrong key with `KEY_ERR`, after which szamlazz.hu never resends that bank transaction or receipt; configure the key of the registration you point at it. Bodies over 64 MiB are answered `413` (the `szamlazz-adatkapcsolat` router's default cap).

The command remains named `payment register` for shell ergonomics; it registers a credit entry against an invoice through the Számla Agent.

## JSON input

`invoice create` and `receipt create` read their request from `--file` (`-` for
stdin). Unknown fields are rejected recursively before any request is sent,
including fields inside enum variants, line items, carrier blocks and email
attachments. The error names the ignored field's path. Wrong types, missing
required fields and trailing JSON are also rejected. Optional fields may be
omitted; open wire tokens such as payment methods and VAT codes remain accepted.

Invoice `header.paid` is tri-state: omitted or `null` leaves `fizetve` out of
the request (the former default), `true` sends an explicit paid instruction,
and `false` sends an explicit unpaid instruction. An old JSON file containing
`"paid": false` now explicitly sends false; omit it to retain the former
omission behavior. The invoice example does so.

## Document results and PDF output

`invoice storno` first queries the original and derives its paper/electronic appearance and fulfillment date.
An unknown appearance, missing fulfillment date or mismatched queried number stops before the storno send;
inspect the original in szamlazz.hu. The command does not silently default an electronic original to paper.

`invoice create`, `invoice storno`, and `receipt create`, `storno`, and `get`
report the remote result even if writing the requested PDF fails. A local write
failure exits nonzero; it does **not** undo issuance. Use the reported document
number to fetch the PDF rather than issuing the document again.

Their `--json` output separates the two results:

```json
{
  "remote": { "issued": { "invoice_number": "E-2026-123" } },
  "pdf_output": {
    "status": "failed",
    "target": "invoice.pdf",
    "error": "writing PDF to invoice.pdf: Permission denied (os error 13)"
  }
}
```

The `remote` value above is abbreviated: invoice creation returns either
`{"issued": {...}}` or `{"preview": {...}}` (a preview issues nothing).
Receipt commands return the receipt, including its number and type. Invoice
storno returns `{outcome, message, original_number, document}`, with the returned
document and one of these outcomes. An unnumbered acknowledgement instead has
`document: null` and an `acknowledgement` object preserving the reported optional
metadata and PDF; it is `unconfirmed`, exits nonzero, and requires reconciliation
of the original before another write.

- `reversed`: a different number and a non-positive gross total confirm a
  reversal. A repeated storno returns the existing reversal and the same outcome;
  the reply cannot establish whether it was issued just now. Zero-total reversals
  also qualify.
- `noop`: the reply echoes the requested number; nothing was reversed (as happens
  for a proforma or delivery note). Exits nonzero.
- `unconfirmed`: an unnumbered acknowledgement, or a different number with a
  missing or positive gross total, does not confirm a reversal. Exits nonzero;
  reconcile the original and any returned document before another write.

Credit-entry registration and PDF download can also succeed without a reported
invoice number. Their JSON preserves that absence as `null`; human registration
output prints only a reported number when present. A missing
number is not filled with the request's target. Registration totals, balance and
payment method are optional reported facts too.

`pdf_output.status` is `not_requested`, `written`, `missing`, or `failed`.
`written`, `missing`, and `failed` include `target`; `failed` also includes the
local `error`. `target` is a display string: non-UTF-8 path bytes are rendered
lossily with replacement characters in JSON; the filesystem write uses the
original path bytes. A missing PDF remains a warning, not a write failure. Human output
likewise retains the remote result and labels a local PDF failure separately.
With `--pdf -`, stdout contains only PDF bytes; the human or JSON report goes to
stderr, followed by any exit-error diagnostic. This separation also applies when
the stdout PDF write fails.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
