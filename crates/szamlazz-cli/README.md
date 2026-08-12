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

`szamlazz listen` serves IPN at `POST /ipn`. When an Adatkapcsolat key is configured, it also serves `POST /adatkapcsolat`. Point a tunnel such as `cloudflared` at the listener to inspect real deliveries during integration work.

The command remains named `payment register` for shell ergonomics; it registers a credit entry against an invoice through the Számla Agent.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
