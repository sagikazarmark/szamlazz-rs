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
  nothing. A `ServerSpec` names the shape: the experimental `Feature`s it needs on or off (set on the spawned server,
  checked against `/version` at launch for a spawned and a reused server alike; a feature not listed is neither set
  nor checked) and any other `NAME=value` environment.
- **Deployment**: serve a `restate_sdk` `Endpoint` in-process on a free port and register it (`force: true`,
  retried); repeatable, so a redeploy is a second call. `set_public`, `drain` (nothing in flight on
  `sys_invocation`).
- **The ingress**: a `Call` (Restate's URL grammar written once: a service or a Virtual Object key, under a scope,
  called or sent) and `invoke(&call, body, idempotency)` → a `Reply` (status, parsed body, `x-restate-id`,
  `x-restate-error-source`); `Reply::fault::<F>()` asserts Restate's error envelope (`code` = the HTTP status,
  `source` = `invocation`, the header) and decodes the JSON string in `message` into the caller's own fault type.
- **The admin API**: SQL introspection, journals (`raw` hex-decoded to bytes), `ctx.run` names, `sys_invocation`
  rows, the registered handlers (`GET /services`), kill / cancel / purge (waiting for the row to go),
  `await_status`, the in-flight invocations selected by service/key/scope, and a `Watch` that samples their run retries
  (`retry_count`, `last_failure`, the failing command: in-flight columns, gone once the invocation completes) while
  they run.
- **The step-name table**: a consumer tables, per handler, the ordered `ctx.run` names of every path it journals
  (`RunPath`), and `Table::check` verifies a whole run against it: every invocation's run sequence is a prefix of
  one of its handler's paths (a journaled name read as its pattern, `lookup-{sku}` by its prefix), every handler
  the deployments offer is tabled, and every path was walked in full. A renamed, inserted or reordered step can
  strand an invocation replayed on changed code. The table is a regression signal for exceptional resume or
  retained-prefix restart, not a compatibility proof: review the actual invocation prefix, branch logic, exact
  commands, serialization and inputs. Allowed patterns do not establish that old data takes the same branch.

The crate knows no particular endpoint: it deploys a `restate_sdk::prelude::Endpoint` and decodes a fault into the
caller's type. What is the consumer's stays with the consumer: its endpoint and mocks, its fault type, its table of
run names, its scenarios and the harness type that composes them.

## Getting a `restate-server`

Either reuse a running one (`RESTATE_ADMIN_URL=http://127.0.0.1:9070 RESTATE_INGRESS_URL=http://127.0.0.1:8080`; a
container of the Restate image with the features the suite expects), or point `RESTATE_SERVER_BIN` at the binary,
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
server, `host.docker.internal` for a reused one). The endpoint is bound to the loopback when the server reaches it
there (a spawned server, no override) and to every interface otherwise (a reused server may be a container reaching
back to the host).

### Process lifecycle

Before any child can be spawned, the launcher waits for both SIGINT and SIGTERM
to be registered on a dedicated thread/runtime. Initialization failure panics
and prevents this and later launches. Spawning and registering a process group
share a lock with shutdown: a launch already inside that boundary is included
in cleanup; once shutdown closes admission, further launches are refused.

A stop signal kills every registered server's process group (including descendants
that remain in it), then exits the test process with status **130** for SIGINT or
**143** for SIGTERM. This covers signals during first startup and concurrent
launches. Normal handle drop kills the group and reaps the child. Signal exit does
not unwind or remove the server's temporary directory; normal drop removes it
unless the test is panicking. SIGKILL cannot run cleanup.

## Example

The one copy, compiled as a doctest of the crate:

```rust,no_run
use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{Call, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::Endpoint;

const SERVER: ServerSpec = ServerSpec {
    name: "main",
    features: &[(VQUEUES, true), (PROTOCOL_V7, true), (SCOPED_VIRTUAL_OBJECTS, true)],
    env: &[],
};

# async fn run() {
let Some(launcher) = launcher_or_skip(ReusePolicy::Allowed) else { return };
let restate = launcher.launch(&SERVER).await;
let endpoint = Endpoint::builder() /* .bind(MyService) */ .build();
restate.deploy(endpoint).await;
let reply = restate.invoke(&Call::service("MyService", "handler"), None, None).await;
assert_eq!(reply.status, 200, "{}", reply.body);
let runs = restate.admin().runs(reply.invocation_id()).await;
# }
```

## Selecting objects to observe

`Target` always includes a service and key. Its public `scope: ScopeSelection` distinguishes three selections:

```rust
use restate_e2e_harness::{ScopeSelection, Target};

// Exactly the unscoped object, matching Call::object's default.
let unscoped = Target::object("Stock", "item-1");
assert_eq!(unscoped.scope, ScopeSelection::Unscoped);
// Exactly the object in one named scope.
let scoped = unscoped.scoped("warehouse_a");
assert_eq!(scoped.scope, ScopeSelection::Named("warehouse_a"));
// Deliberate aggregation: unscoped plus every named scope, same service/key.
let all = unscoped.all_scopes();
assert_eq!(all.scope, ScopeSelection::All);
```

`ScopeSelection::default()` is `Unscoped`. Selectors remain plain data and const-constructible.
**Migration:** `Target::object` previously included all scopes and `Target.scope` was an `Option<&str>`.
Use `.all_scopes()` to retain that aggregation; replace literal `None` with `ScopeSelection::All` for the old
behavior (or `Unscoped` for an exact unscoped selection), and `Some(name)` with `ScopeSelection::Named(name)`.
This is a breaking interface change of the independently versioned harness crate.

`Admin::in_flight_ids_on`, `in_flight_on`, `await_in_flight_on` and `Watch::start` all apply this selection.
`Watch` is **object-wide**: all matching invocations contribute retry counts and failures, and it stops when
the selection falls idle after being seen in flight. Queued or concurrent matching invocations keep it sampling;
`Retries::observed_completion` means the selection fell idle. To wait for **one invocation** to complete, use
`Admin::await_status(id, statuses)`. `Watch::finish` stops sampling even if matching invocations are still running.

## Tests

`cargo test -p restate-e2e-harness` runs the pure decisions (the gate, the sampler, the table check, the call
grammar, the envelope check), admin HTTP-boundary SQL escaping, and subprocess lifecycle regressions without a real
server. The lifecycle tests use a fake executable with a descendant, real SIGINT
and SIGTERM, and test-only barriers around registration, spawn and shutdown; they
check signal exit statuses and that no child or descendant remains alive, plus
refusal to launch when signal initialization fails.
`RESTATE_SERVER_BIN=… cargo test -p restate-e2e-harness -- --ignored` runs `e2e_smoke`, the crate's contract
against a server of its own (never a reused one: the test deploys a service and leaves its invocations retained,
which a suite sharing that server would meet as a stranger's) with a trivial service: the gate launches a server, the service is deployed (twice), invoked through the
ingress, its run read from the journal, a fault decoded out of the envelope, an invocation killed and purged, a
service made private and public, and the server stops on drop.
The same command runs `e2e_targets`: the same service/key unscoped and in two named scopes, exact and all-scope
in-flight/await selection, and isolated versus aggregate `Watch` sampling. Another service and another key are
excluded from every selection.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
