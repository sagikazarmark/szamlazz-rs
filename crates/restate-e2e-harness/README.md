# restate-e2e-harness

[![Crates.io](https://img.shields.io/crates/v/restate-e2e-harness.svg)](https://crates.io/crates/restate-e2e-harness)
[![Documentation](https://docs.rs/restate-e2e-harness/badge.svg)](https://docs.rs/restate-e2e-harness)

An end-to-end test harness for [`restate_sdk`](https://docs.rs/restate-sdk) endpoints against a real
`restate-server`: the building blocks a test suite composes, not a suite. **Unix only.**

- **The server gate**: where the server comes from, decided once from the environment. `RESTATE_ADMIN_URL` /
  `RESTATE_INGRESS_URL` reuse a running server; `RESTATE_SERVER_BIN` names a `restate-server` binary the harness
  spawns on the loopback, on ports chosen free at launch, with its log and data under a temp directory kept when
  the test fails, in a process group of its own that is killed when the handle drops and on SIGINT/SIGTERM. With
  neither the suite **skips** with a message, and **fails** when `CI` is set: a run that passed by skipping proves
  nothing. A `ServerSpec` names the shape (`NAME=value` flags, the three experimental features checked against
  `/version` at launch).
- **Deployment**: serve a `restate_sdk` `Endpoint` in-process on a free port and register it (`force: true`,
  retried); repeatable, so a redeploy is a second call. `set_public`, `drain` (nothing in flight on
  `sys_invocation`).
- **The ingress**: a `Call` (Restate's URL grammar written once: a service or a Virtual Object key, under a scope,
  called or sent) and `invoke(&call, body, idempotency)` → a `Reply` (status, parsed body, `x-restate-id`,
  `x-restate-error-source`); `Reply::fault::<F>()` asserts Restate's error envelope (`code` = the HTTP status,
  `source` = `invocation`, the header) and decodes the JSON string in `message` into the caller's own fault type.
- **The admin API**: SQL introspection, journals (`raw` hex-decoded to bytes), `ctx.run` names, `sys_invocation`
  rows, the registered handlers (`GET /services`), kill / cancel / purge (waiting for the row to go),
  `await_status`, the in-flight invocations on a key, and a `Watch` that samples an invocation's run retries
  (`retry_count`, `last_failure`, the failing command: in-flight columns, gone once the invocation completes) while
  it runs.
- **The step-name table**: a consumer tables, per handler, the ordered `ctx.run` names of every path it journals
  (`RunPath`), and `Table::check` verifies a whole run against it: every invocation's run sequence is a prefix of
  one of its handler's paths (a journaled name read as its pattern, `verify-{number}` by its prefix), every handler
  the deployments offer is tabled, and every path was walked in full. Under in-place re-registration a renamed,
  inserted or reordered step strands every in-flight invocation on the next deploy; under immutable deployments the
  same sequence is what a pause-and-resume onto new code needs; either way the test fails first.

The crate knows no particular endpoint: it deploys a `restate_sdk::prelude::Endpoint` and decodes a fault into the
caller's type. What is the consumer's stays with the consumer: its endpoint and mocks, its fault type, its table of
run names, its scenarios and the harness type that composes them.

## Getting a `restate-server`

Either reuse a running one (`RESTATE_ADMIN_URL=http://127.0.0.1:9070 RESTATE_INGRESS_URL=http://127.0.0.1:8080`; a
container of the Restate image with the flags the suite expects), or point `RESTATE_SERVER_BIN` at the binary,
which the Restate image carries at `/usr/local/bin/restate-server`. In this workspace the Dagger `ci` module
exports it (`dagger call ci restate-server export --path ./restate-server`); anywhere else, copy it out of the
image:

```sh
id=$(docker create docker.restate.dev/restatedev/restate:1.7.8)
docker cp "$id:/usr/local/bin/restate-server" ./restate-server
docker rm "$id"
RESTATE_SERVER_BIN=$PWD/restate-server cargo test -p restate-e2e-harness -- --ignored
```

`RESTATE_ENDPOINT_HOST` overrides the host the server reaches the in-process endpoint at (`127.0.0.1` for a spawned
server, `host.docker.internal` for a reused one).

## Example

```rust,no_run
use restate_e2e_harness::gate::{FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS, FLAG_VQUEUES};
use restate_e2e_harness::{Call, Reuse, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::Endpoint;

const SERVER: ServerSpec = ServerSpec {
    name: "main",
    flags: &[FLAG_VQUEUES, FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS],
};

# async fn run() {
let Some(launcher) = launcher_or_skip(Reuse::Allowed) else { return };
let restate = launcher.launch(&SERVER).await;
let endpoint = Endpoint::builder() /* .bind(MyService) */ .build();
restate.deploy(endpoint).await;
let reply = restate.invoke(&Call::service("MyService", "handler"), None, None).await;
assert_eq!(reply.status, 200, "{}", reply.body);
let runs = restate.admin().runs(reply.invocation_id()).await;
# }
```

## Tests

`cargo test -p restate-e2e-harness` runs the pure decisions (the gate, the sampler, the table check, the call
grammar) without a server.
`RESTATE_SERVER_BIN=… cargo test -p restate-e2e-harness -- --ignored` runs `e2e_smoke`, the crate's contract
against a server of its own (never a reused one: the test deploys a service and leaves its invocations retained,
which a suite sharing that server would meet as a stranger's) with a trivial service: the gate launches a server, the service is deployed (twice), invoked through the
ingress, its run read from the journal, a fault decoded out of the envelope, an invocation killed and purged, a
service made private and public, and the server stops on drop.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
