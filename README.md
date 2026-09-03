# szamlazz

[![crates.io](https://img.shields.io/crates/v/szamlazz-agent?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-agent)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-agent?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-agent)

**Rust crates for integrating with [szamlazz.hu](https://www.szamlazz.hu), the Hungarian invoicing service.**

## Features

- **Complete integration surface.** Use the Számla Agent, receive IPN status snapshots, and accept Adatkapcsolat documents.
- **Portable library cores.** The three protocol libraries (`szamlazz-agent`, `szamlazz-ipn`, `szamlazz-adatkapcsolat`) target native Rust and `wasm32-unknown-unknown`, including Cloudflare Workers; the Restate crates are native-only.
- **Sans-IO Számla Agent.** Build complete wire requests and parse raw responses with any HTTP client, or enable the reqwest client.
- **Protocol-native models.** Typed operations, documents, Acks, errors, and Hungarian Rustdoc aliases preserve szamlazz.hu semantics.
- **Durable workers.** Issue, reverse, and reconcile documents exactly once per order through Restate services and a runnable endpoint.

## Workspace

This virtual workspace contains six packages intended for publication and independent use:

| Package | Purpose |
|---|---|
| [`szamlazz-agent`](crates/szamlazz-agent) | Sans-IO Számla Agent client for issuing and querying documents, registering credit entries, and looking up taxpayers. |
| [`szamlazz-ipn`](crates/szamlazz-ipn) | IPN receiver types for current payment-status snapshots, with an optional axum extractor. |
| [`szamlazz-adatkapcsolat`](crates/szamlazz-adatkapcsolat) | Adatkapcsolat receiver for outgoing and incoming invoices, bank transactions, and receipts. |
| [`szamlazz-cli`](crates/szamlazz-cli) | `szamlazz` command-line client and local development receiver for IPN and Adatkapcsolat. |
| [`restate-szamlazz`](crates/restate-szamlazz) | Restate `Order` Virtual Object and `SzamlaAgent` service issuing szamlazz.hu documents with durable, idempotent execution. |
| [`restate-szamlazz-endpoint`](crates/restate-szamlazz-endpoint) | Standalone `restate-szamlazz` endpoint hosting the services for a Restate server. |

The Hungarian-to-English vocabulary is documented in [CONTEXT.md](CONTEXT.md).

## Documentation

- [`docs/design/restate-szamlazz.md`](docs/design/restate-szamlazz.md): design and implementation spec for the Restate worker.
- [`docs/adr/`](docs/adr): architecture decision records behind it.
- [`docs/szamlazz-hu-behaviour.md`](docs/szamlazz-hu-behaviour.md): verified Számla Agent behavior the worker relies on, with a go-live checklist.

## Development

The workspace MSRV is Rust 1.92. Run the canonical Dagger check with:

```bash
dagger check
```

Run the individual host checks with the tracked lockfile:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
```

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
