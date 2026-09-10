# szamlazz

[![crates.io](https://img.shields.io/crates/v/szamlazz-agent?style=flat-square&label=crates.io)](https://crates.io/crates/szamlazz-agent)
[![docs.rs](https://img.shields.io/docsrs/szamlazz-agent?style=flat-square&label=docs.rs)](https://docs.rs/szamlazz-agent)

**Rust crates for integrating with [szamlazz.hu](https://www.szamlazz.hu), the Hungarian invoicing service.**

Three protocol libraries (the Számla Agent client, the IPN receiver, the Adatkapcsolat receiver) and a **durable
invoicing worker** on [Restate](https://restate.dev/): the `Szamlazz.Order` service issues, corrects and reverses
szamlazz.hu documents with **one unresolved write per order** under retries, crashes and concurrent callers.
It retains uncertainty for read-only reconciliation and guards later mutations with a durable marker;
szamlazz.hu remains the document source of truth. You call it over HTTP with a JSON body from any language (no Rust, no
Restate SDK), and branch on the outcome it returns as data; the request and response contract, the fault envelope
and the guidance for calling from a webhook handler are in the [`restate-szamlazz` README](crates/restate-szamlazz/README.md).
Issuing from Rust directly starts at [`szamlazz-agent`](crates/szamlazz-agent).

## Features

- **Complete integration surface.** Use the Számla Agent, receive IPN status snapshots, and accept Adatkapcsolat documents.
- **Portable library cores.** The three protocol libraries (`szamlazz-agent`, `szamlazz-ipn`, `szamlazz-adatkapcsolat`) target native Rust and `wasm32-unknown-unknown`, including Cloudflare Workers; the Restate crates are native-only.
- **Bring your own HTTP client.** The Számla Agent core performs no I/O: build complete wire requests and parse raw responses with any HTTP client, or enable the reqwest client.
- **Protocol-native models.** Typed operations, documents, Acks, errors, and Hungarian Rustdoc aliases preserve szamlazz.hu semantics.
- **Durable workers.** Serialize Order mutations and retain unresolved writes for evidence-based recovery; szamlazz.hu stays the document source of truth.

## Workspace

This virtual workspace contains five packages intended for publication and independent use:

| Package | Purpose |
|---|---|
| [`szamlazz-agent`](crates/szamlazz-agent) | Számla Agent client for issuing and querying documents, registering credit entries, and looking up taxpayers. |
| [`szamlazz-ipn`](crates/szamlazz-ipn) | IPN receiver types for current payment-status snapshots, with an optional axum extractor. |
| [`szamlazz-adatkapcsolat`](crates/szamlazz-adatkapcsolat) | Adatkapcsolat receiver for outgoing and incoming invoices, bank transactions, and receipts. |
| [`szamlazz-cli`](crates/szamlazz-cli) | `szamlazz` command-line client and local development receiver for IPN and Adatkapcsolat. |
| [`restate-szamlazz`](crates/restate-szamlazz) | Restate `Szamlazz.Order` with unresolved-write protection and the stateless `Szamlazz.Agent` service; document status comes from szamlazz.hu. |

The worker's end-to-end tests use [`restate-e2e-harness`](https://crates.io/crates/restate-e2e-harness)
from crates.io.

The Hungarian-to-English vocabulary is documented in [CONTEXT.md](CONTEXT.md).

## Documentation

- [`docs/szamlazz-hu-behaviour.md`](docs/szamlazz-hu-behaviour.md): verified Számla Agent behavior the worker relies on, with a go-live checklist.
- [`docs/adr/`](docs/adr): architecture decision records for the Restate worker.
- [`docs/design/restate-szamlazz.md`](docs/design/restate-szamlazz.md): the Restate worker's architecture in one document.
- [`docs/review/`](docs/review): dated whole-workspace reviews with verified findings and the live probes still open.

## Development

The workspace MSRV is Rust 1.92. Run the canonical Dagger check with:

```bash
dagger check
```

It runs the `rust` module's build, test, clippy, doc, audit and fmt checks and the workspace's own `ci` module
(`.dagger/modules/ci`): `ci:test` (the workspace tests with every feature) and `ci:end-to-end` (the
`restate-szamlazz` handler layer against `restate-server` processes started
inside the container). Run one with `dagger check ci:end-to-end`.

Run the individual host checks with the tracked lockfile:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo test --doc --workspace --all-features --locked
cargo doc --workspace --all-features --no-deps --locked
# the end-to-end suites, against a restate-server binary (or a running server:
# RESTATE_ADMIN_URL / RESTATE_INGRESS_URL, e.g. `docker compose up -d`)
dagger call ci restate-server export --path ./restate-server
RESTATE_SERVER_BIN=$PWD/restate-server cargo test -p restate-szamlazz --test e2e --all-features --locked -- --ignored
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
