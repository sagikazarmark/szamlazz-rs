# restate-e2e-harness

[![Crates.io](https://img.shields.io/crates/v/restate-e2e-harness.svg)](https://crates.io/crates/restate-e2e-harness)
[![Documentation](https://docs.rs/restate-e2e-harness/badge.svg)](https://docs.rs/restate-e2e-harness)

An end-to-end test harness for [`restate_sdk`](https://docs.rs/restate-sdk) endpoints against a real
`restate-server`: the building blocks a test suite composes, not a suite. **Unix only.**

- **The server gate**: where the server comes from, decided once from the environment. `RESTATE_ADMIN_URL` /
  `RESTATE_INGRESS_URL` reuse a running server; `RESTATE_SERVER_BIN` names a `restate-server` binary the harness
  spawns on the loopback, on ports chosen free at launch, with its log and data under an exclusively allocated
  temp directory (`restate-e2e-{pid}-{name}-{sequence}`). Existing candidates are skipped, so port reuse cannot
  reuse storage or overwrite earlier failure evidence. Failed launches and panic teardown retain their own
  directory; normal teardown removes only that launch's directory. Its process group is killed when the handle
  drops and on SIGINT/SIGTERM. With
  neither the suite **skips** with a message, and **fails** when `CI` is set: a run that passed by skipping proves
  nothing. A `ServerSpec` names the shape: the experimental `Feature`s it needs on or off (set on the spawned server,
  checked against `/version` at launch for a spawned and a reused server alike; a feature not listed is neither set
  nor checked) and any other `NAME=value` environment.
- **Deployment**: serve a `restate_sdk` `Endpoint` in-process on a free port and register it (`force: true`,
  retried); repeatable, so a redeploy is a second call. Local endpoints belong to the `Restate` handle:
  dropping it requests shutdown of all of them, including on a reused server. Discarding a `Deployment`
  descriptor leaves its endpoint running. `set_public`, `drain` (nothing in flight on
  `sys_invocation`).
- **The ingress**: a `Call` (Restate's URL grammar written once: a service or a Virtual Object key, under a scope,
  called or sent; logical segments percent-encoded by the harness) and `invoke(&call, body, idempotency)` → a `Reply` (status, parsed body, `x-restate-id`,
  `x-restate-error-source`); `Reply::fault::<F>()` asserts Restate's error envelope (`code` = the HTTP status,
  `source` = `invocation`, the header) and decodes the JSON string in `message` into the caller's own fault type.
  `Restate::ingress_url()` exposes the base URL for consumer-owned HTTP clients sending raw bodies or custom
  headers, or using a different timeout.
- **The admin API**: SQL introspection, journals (`raw` hex-decoded to bytes), `ctx.run` names, `sys_invocation`
  rows, the registered handlers (`GET /services`), kill / cancel / purge (waiting for the row to go),
  `await_status`, the in-flight invocations selected by service/key/scope, and a `Watch` that samples their run retries
  (`retry_count`, `last_failure`, the failing command: in-flight columns, gone once the invocation completes) while
  they run.
- **The step-name table**: a consumer tables, per handler, the ordered `ctx.run` names of every path it journals
  (`RunPath`), and `Table::check` checks observed named-run sequences and path coverage against the current
  table. See [Checking step-name sequences](#checking-step-name-sequences) for the matching rules and what
  the result contributes to deployment review.

The crate knows no particular endpoint: it deploys a `restate_sdk::prelude::Endpoint` and decodes a fault into the
caller's type. What is the consumer's stays with the consumer: its endpoint and mocks, its fault type, its table of
run names, its scenarios and the harness type that composes them.

## Getting a `restate-server`

Either reuse a running one (`RESTATE_ADMIN_URL=http://127.0.0.1:9070 RESTATE_INGRESS_URL=http://127.0.0.1:8080`; a
container of the Restate image with the features the suite expects), or point `RESTATE_SERVER_BIN` at the binary,
which the Restate image carries at `/usr/local/bin/restate-server`. In this workspace the Dagger `ci` module
exports it (`dagger call ci restate-server export --path ./restate-server`); anywhere else on Linux, copy it out of
the image (Docker selects the host architecture):

```sh
id=$(docker create docker.restate.dev/restatedev/restate:1.7.8)
docker cp "$id:/usr/local/bin/restate-server" ./restate-server
docker rm "$id"
```

The image's binary is Linux-only. On macOS, use a native `restate-server` from the
[Restate installation instructions](https://docs.restate.dev/installation).
The quick start below is tested with **Rust SDK 0.12.0 and Restate server 1.7.8**,
with vqueues, protocol v7 and scoped Virtual Objects enabled. This crate requires
**Unix** and **Rust 1.92 or later**.

Admin and ingress base URLs accept trailing slashes; the harness removes those
separators while preserving any path prefix, including in the exported base URLs.
With reuse allowed, setting only one of `RESTATE_ADMIN_URL` and `RESTATE_INGRESS_URL`
fails immediately and names the missing variable. An incomplete pair never falls
back to a spawned server or a skip. `ReusePolicy::Never` ignores both reuse URLs.
Spawned servers use TCP for the node, admin and ingress listeners, overriding
listener modes inherited from the environment or supplied through `ServerSpec.env`.

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

Exit inspection leaves the child unreaped, reserving its process ID until group
signaling, reaping and registry removal happen under the lifecycle lock. On Unix
targets without `waitid` (OpenBSD, Redox, Cygwin, Horizon), early exit inspection is
unavailable; readiness still fails at its deadline.

Readiness uses one 90-second deadline for health, SQL introspection and version
checks, including response bodies. The owned child is inspected throughout those
stages and after successful responses; another server answering on a selected
admin port cannot hide an observed child exit.

Dropping the handle also requests shutdown of every local endpoint it deployed.
Keep the Tokio runtime running to execute that shutdown; Drop does not wait for
completion. Connections have up to ten seconds to drain; remaining connection and
SDK handler tasks are then cancelled and joined. The private HTTP/2 server owns
both kinds of tasks over the SDK's `HyperEndpoint` adapter, since SDK 0.12's
`HttpServer` detaches them and its timeout only stops waiting. This terminates
handler futures, not external effects or independently spawned application tasks.
Earlier endpoints stay available while the handle lives, and a reused Restate
server is itself left running.

## Quick start

In a Rust crate using edition `2024`, add these to `Cargo.toml`:

```toml
[dev-dependencies]
restate-e2e-harness = "0.1"
restate-sdk = "=0.12.0"
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

Save the complete test below as `tests/e2e.rs`. `macros` provides `#[tokio::test]`;
`rt-multi-thread` enables the runtime selected by the test. The service retains its
journal explicitly so its named run is still available after the call completes.

```rust,no_run,test_harness
use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{Call, ReusePolicy, ServerSpec, launcher_or_skip, run_result};
use restate_sdk::prelude::*;
use serde_json::json;

const SERVER: ServerSpec = ServerSpec {
    name: "quick-start",
    features: &[(VQUEUES, true), (PROTOCOL_V7, true), (SCOPED_VIRTUAL_OBJECTS, true)],
    env: &[],
};

struct Greeting;

#[restate_sdk::service(name = "Greeting")]
impl Greeting {
    #[handler(journal_retention = "1d")]
    async fn greet(&self, ctx: Context<'_>, name: String) -> HandlerResult<String> {
        let greeting = ctx.run(|| async move { Ok(format!("Hello, {name}!")) })
            .name("greet-person")
            .await?;
        Ok(greeting)
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs RESTATE_SERVER_BIN"]
async fn e2e_greeting() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else { return };
    let restate = launcher.launch(&SERVER).await;
    restate.deploy(Endpoint::builder().bind(Greeting).build()).await;

    let reply = restate
        .invoke(&Call::service("Greeting", "greet"), Some(&json!("Ada")), None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body, json!("Hello, Ada!"));

    let id = reply.invocation_id();
    assert_eq!(restate.admin().runs(id).await, ["greet-person"]);
    let journal = restate.admin().journal(id).await;
    assert!(run_result(&journal, "greet-person")
        .expect("retained greet-person result")
        .raw_contains("Hello, Ada!"));
}
```

Select the binary obtained above and run the ignored test:

```sh
RESTATE_SERVER_BIN="$PWD/restate-server" cargo test --test e2e -- --ignored --nocapture
```

`ReusePolicy::Never` gives this test a server of its own, so its retained invocations
cannot interfere with another suite. The gate ignores reuse URLs for this policy.
Without `RESTATE_SERVER_BIN`, an explicitly run test prints a skip message on a
developer machine; with `CI` set, it fails instead. Without `--ignored`, Cargo
does not run this test at all, including in CI. Server ports are selected at launch;
the handle stops the server on drop.

This is the one maintained Rust example: the README is compiled by the crate's
doctests using `test_harness` (so the async test body is typechecked). The ignored
`e2e_quick_start` regression copies both blocks into a temporary standalone crate,
patches only the harness dependency to the local source, and runs this exact test
against a real server. Its fresh dependency resolution checks these features
without workspace dev-dependency unification.

## Selecting objects to observe

`Call::object` and `Target::object` take the **same logical key**. Pass `invoice/2026` to both: `Call::path()`
renders the key as `invoice%2F2026`, while the SQL selector matches `invoice/2026`. A literal `%2F` is encoded
as `%252F`; callers must not pre-encode keys, services, handlers or scopes. Standalone `.` and `..` segments
are refused because HTTP URL parsers normalize them even when encoded. This is a breaking change from the
previous caller-encoded `Call` convention; remove caller-side percent encoding when migrating.

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

### Retry-sampling limits

**A missing observed retry is not evidence that no retry occurred.** The public
[`Watch` / `Retries` module guidance](src/watch.rs)
explains invoker counts versus durable-step executions, scheduler-yield resets,
completion clearing, sampling gaps and key-wide aggregation, with evidence for
Restate **1.7.8 / vqueues / protocol v7 / scoped Virtual Objects**.
Its **Configuring a retry-observation test** section
covers the one-second test delay below that configuration's observed two-second
yield threshold and how sample/query-error counts help diagnose incomplete
observation. Neither that delay nor the nominal 100 ms poll interval guarantees
visibility; a two-second delay can yield instead of widening the window.

## Reading run results

`run_result(&journal, name)` returns the matching `Notification: Run` row, or
`None` if the command or its completion is absent. It correlates the command and
notification by **completion id**, never by adjacency or by journal index.
An incomplete run cannot borrow a later run's result.

Supply one invocation's unfiltered journal (an unfinished prefix is fine), in
strictly increasing index order. `Admin::journal` and `Admin::all_journals` select
the necessary `version` and `entry_json` columns. Supported: **journal v2**, with
the `entry_json` representation exposed by **Restate server 1.7.8**, verified with
**Rust SDK 0.12.0 / protocol v7**. Journal v1 and unknown versions are explicitly
unsupported. The server's
[SQL projection](https://github.com/restatedev/restate/blob/v1.7.8/crates/storage-query-datafusion/src/journal/row.rs)
serializes its decoded entry; the
[run command](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/journal_v2/command.rs)
and [run completion](https://github.com/restatedev/restate/blob/v1.7.8/crates/types/src/journal_v2/notification.rs)
both carry a `u32` `completion_id`:

```json
{"Command":{"Run":{"completion_id":0,"name":"lookup"}}}
{"Notification":{"Completion":{"Run":{"completion_id":0,"result":{"Success":[123,125]}}}}}
```

Notifications must follow their commands; unrelated entries and completion
reordering are supported. The
[SDK 0.12.0 rule](https://docs.rs/restate-sdk/0.12.0/restate_sdk/context/trait.ContextSideEffects.html#tymethod.run)
is to **immediately await each run before other context operations**. The helper's
A/B → A/B and A/B → B/A cases are synthetic robustness tests, not live claims
that interleaving runs is supported by this SDK. The real-server smoke test uses
immediately awaited runs, repeated names, and an intervening sleep, and verifies
each result's distinct content and matching identity.

**Ambiguity fails explicitly:** following the harness's assertion convention,
the helpers panic on unsupported versions, missing/malformed run identities,
unordered or duplicate indices, duplicate run command/completion identities,
and orphan run notifications. They validate the whole supplied journal before
answering, including a lookup of an absent name. There is no proximity fallback.
`run_result` also panics on repeated names. Select a **zero-based occurrence in
journal index order** explicitly with `run_result_at`:

```rust,no_run
use restate_e2e_harness::{JournalEntry, run_result, run_result_at};
# fn inspect(journal: &[JournalEntry]) {
let account = run_result(journal, "account").expect("unique completed account run");
let ownership = run_result_at(journal, "lookup-invoice", 0).expect("ownership lookup");
let full = run_result_at(journal, "lookup-invoice", 1).expect("full lookup");
# }
```

**Migration:** name-only selection previously silently chose the first occurrence.
Use `run_result_at` where names repeat. Custom SQL must select `version` and
`entry_json` along with `index`, `entry_type`, `name`, and `raw` when using
`JournalEntry::from_row`; literals must supply the new `version` and
`run_completion_id` fields. This is a breaking interface change of the
independently versioned harness crate. `raw` remains the hex-decoded entry bytes
for content and leak assertions; it is not the correlation source.

`JournalEntry::from_row` requires a non-empty string `entry_type` and a valid hex string `raw`, even for
content-only inspection. Missing, null or malformed values panic at decoding, rather than becoming unrelated
rows or empty bytes that could falsely pass an absence or leak assertion. An explicitly supplied empty hex
string is valid empty evidence. Custom queries must select both columns; sources that cannot provide the
raw bytes are not usable for content assertions through this decoder.

For journal v2, unknown `entry_type` classifications and contradictions with a
decoded `entry_json` classification are refused. Run commands require a string
SQL `name`; an unnamed run is the empty string, not null or a missing column.
When `entry_json` also supplies the run name, it must agree. The same classification
and name-evidence rules apply to hand-built rows during semantic inspection, so an
unreadable run cannot disappear from `Admin::runs`, result lookup or the table check.

Query every column in `Invocation::COLUMNS` when using `Invocation::from_row`.
Nullable `completion_failure` and `scope` accept a string, explicit null or omission:
Restate 1.7.8's JSON writer omits SQL-null values. Other types panic instead of looking
like no failure or an unscoped invocation. A row alone cannot distinguish an omitted
SQL null from an unselected nullable column; custom queries must select them themselves.
The status, service and handler must be present strings; unknown status strings are preserved.

## Checking step-name sequences

`Table::check` takes the deployed handlers, invocations and journals supplied by the suite. It checks that:

- Each invocation's **named-run sequence**, read as patterns in journal order, is a prefix of at least one
  tabled path for its service and handler. Shorter sequences are allowed, including early answers and
  unfinished invocations. A missing journal fails with its invocation id in
  `Violations::missing_journals`; it cannot establish conformance or coverage.
- Every supplied deployed or invoked handler has a row, and every tabled handler is deployed.
- Every tabled path is **walked in full**: at least one invocation's observed pattern sequence equals the
  entire row. This is coverage of the declared named-run paths, not all possible branches or proof that
  those invocations completed. An explicitly supplied empty journal walks only an empty row, if one is declared.

Named-run inspection (`JournalEntry::is_run`, `Admin::runs` and `Table::check`)
requires journal v2. Missing or unsupported versions panic rather than establishing
that no runs occurred or accepting an empty sequence.

Patterns are scoped to `(service, handler)`: adding another handler cannot change
how an existing handler's names are read. `Table::pattern(service, handler, name)`
uses the same matching as the check. Standalone `RunPatterns` matches the set of
rows the caller deliberately supplies.

Fixed names match exactly. A parameter pattern such as `lookup-{sku}` matches any name starting with
`lookup-` and a non-empty remainder; the longest matching prefix wins. Matching uses only the text before
the first `{`, without validating the remainder or comparing parameter values or operation inputs.
`Table::new` rejects empty parameter prefixes, different patterns sharing a prefix, and fixed names shadowed
by a parameter prefix within the same handler.

**Migration:** pass service and handler to `Table::pattern`. A missing map entry
is now missing evidence, not an empty sequence; retain and collect every supplied
invocation's journal. Do not fill missing entries with empty vectors unless the
test has independently established an empty journal. `Admin::all_journals` only
returns invocations with retained entries.

The check compares **current observations with current rows**. A renamed, inserted or reordered step can fail
against an unchanged table, but changing the implementation and table together can pass. It does not compare
historical deployments, the full journal command sequence, result serialization/decoding, inputs or historical
branch decisions. A passing check and the table's diff are supporting evidence for deployment review, not
proof of replay compatibility.

**Immutable deployments are the normal execution model:** register each release separately and keep the
original code available for invocations pinned to it. Exceptional cross-deployment resume or restart from a
retained journal prefix requires reviewing that invocation's **actual retained prefix, exact command sequence
(including names and parameters), result serialization and decoding, operation inputs, and branch behavior**
against the candidate code. Allowed paths do not establish that an old result takes the same branch. For a
restart, also reconcile external effects of operations outside the copied prefix before allowing them to
execute again. The table imposes no general cross-release journal-compatibility contract.

## Tests

`cargo test -p restate-e2e-harness` runs the pure decisions (the gate, the sampler, the table check, the call
grammar, the envelope check), admin HTTP-boundary SQL escaping, and subprocess lifecycle regressions without a real
server. The lifecycle tests use a fake executable with a descendant, real SIGINT
and SIGTERM, and test-only barriers around registration, spawn and shutdown; they
check signal exit statuses and that no child or descendant remains alive, plus
refusal to launch when signal initialization fails.
Readiness regressions cover child exit during health, SQL and version probes and
a version probe inheriting time spent in earlier stages. Endpoint regressions
exercise successful handler completion during draining and forced cancellation
of a pending handler, including termination of its existing HTTP/2 connection.
`RESTATE_SERVER_BIN=… cargo test -p restate-e2e-harness -- --ignored` runs `e2e_smoke`, the crate's contract
against a server of its own (never a reused one: the test deploys a service and leaves its invocations retained,
which a suite sharing that server would meet as a stranger's) with a trivial service: the gate launches a server, the service is deployed (twice), invoked through the
ingress, its run read from the journal, a fault decoded out of the envelope, an invocation killed and purged, a
service made private and public, and the server stops on drop.
`e2e_listener_modes` repeats the smoke test in isolated processes with conflicting
admin and ingress listener modes, supplied by both the environment and `ServerSpec.env`.
The same command runs `e2e_targets`: the same service/key unscoped and in two named scopes, exact and all-scope
in-flight/await selection, and isolated versus aggregate `Watch` sampling. Another service and another key are
excluded from every selection.
It also runs `e2e_quick_start`, compiling and executing the README in a separate
temporary crate. That check needs access to the Cargo registry (or a populated
cache), retains the crate on failure, and removes it on success.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.
