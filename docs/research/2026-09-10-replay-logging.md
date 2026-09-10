# Replay-aware logging after fresh work (#202)

## Result and versions

Reproduced on 2026-09-10 with `restate-sdk` **0.12.0**, shared core **7.0.3**,
`tracing-subscriber` **0.3.23**, and Restate server **1.7.8**. The documented
`fmt + EnvFilter + ReplayAwareFilter` subscriber suppresses both the duplicate
replay event and fresh application events after a run retry under a child span.

No fixed SDK release is available as of this investigation: 0.12.0 is the latest
release, and upstream `main` still has the affected code. The candidate patch
below was tested on SDK tag `v0.12.0` (`33c57d2212b1fc0cbb99e6ed038124fac01bd73d`).
It is an experimental upstream fix, not a released dependency of this workspace.

**Upstream submission blocked:** `gh issue create --repo restatedev/sdk-rust`
returned `GraphQL: Resource not accessible by personal access token (createIssue)`.
No upstream issue or patch URL was created. The minimal reproduction and tested
patch below are the ready-to-submit report; publishing it with a credential that
can create upstream issues remains the outstanding acceptance item of #202.

## Observable regression

`crates/restate-szamlazz/src/service/prologue/logging_tests.rs` runs the worker's
real `execute` wrapper, prologue, external-operation run helper and Gateway under
a scoped SDK endpoint. Its first probe completes, the next operation fails once,
and Restate re-dispatches the handler. The completed probe replays; the failed
operation executes, queries the mock Számla Agent endpoint and logs. A subsequent
query receives credential code 135 and emits the worker's paging warning.

The test asserts two handler executions, two executions of the failed operation,
one HTTP request per completed external operation, one pre-probe event (the
duplicate is filtered), and one each of the fresh closure event, Gateway event
and credential warning. Every fresh event must carry `scope=acme`,
`account.id=logging-account` and the invocation id returned by the ingress.
It observes formatted subscriber output, not the replay flag or span internals.

```sh
RESTATE_SERVER_BIN=/path/to/restate-server cargo test -p restate-szamlazz \
  --lib e2e_replay_filter -- --ignored --nocapture
```

Experiments before the local mitigation:

| SDK | Duplicate replay event | First fresh closure event | Later credential warning |
| --- | --- | --- | --- |
| Released 0.12.0 | suppressed | missing | missing |
| Retain the endpoint span only | suppressed | missing | visible |
| Retain span **and** update before `ExecuteRun` | suppressed | visible | visible |

With the local mitigation and released 0.12.0, all assertions pass. The test is
an ignored `e2e_` test picked up by the workspace's existing Dagger `endToEnd`
check. Its process-wide subscriber matches an endpoint host and avoids tracing
callsite-interest races with the other ignored replay test running concurrently.

## Root causes and proposed upstream patch

1. [`ContextInternalInner::maybe_flip_span_replaying_field`](https://github.com/restatedev/sdk-rust/blob/v0.12.0/src/endpoint/context.rs#L69-L76)
   records on `Span::current()`. Input handling sets the flag on the SDK endpoint
   span, but subsequent syscalls under a user child span record there instead.
   [`ReplayAwareFilter`](https://github.com/restatedev/sdk-rust/blob/v0.12.0/src/filter.rs#L53-L65)
   reads only the `restate_sdk_endpoint_handle` ancestor. Adding a replay field
   to the child cannot repair that mismatch.
2. The `RunFutureImpl` `ExecuteRun` arm starts the closure without refreshing the
   flag after the VM transitions. Retaining the endpoint span alone therefore
   still hides the first fresh operation's events until result polling updates it.

The SDK creates `ContextInternalInner` while its own endpoint span is current.
Retain that span and update it before invoking the closure:

```diff
diff --git a/src/endpoint/context.rs b/src/endpoint/context.rs
--- a/src/endpoint/context.rs
+++ b/src/endpoint/context.rs
@@ -40,1 +40,2 @@
     pub(super) span_replaying_field_state: bool,
+    replay_span: tracing::Span,
@@ -55,1 +56,2 @@
             span_replaying_field_state: false,
+            replay_span: tracing::Span::current(),
@@ -71,1 +73,1 @@
-            tracing::Span::current().record("restate.sdk.is_replaying", true);
+            self.replay_span.record("restate.sdk.is_replaying", true);
@@ -74,1 +76,1 @@
-            tracing::Span::current().record("restate.sdk.is_replaying", false);
+            self.replay_span.record("restate.sdk.is_replaying", false);
@@ -1043,1 +1045,2 @@
                             assert_eq!(handle, handle_to_run);
+                            inner_ctx.maybe_flip_span_replaying_field();
```

## Minimal upstream reproduction

This requires only the SDK and tracing, not szamlazz.hu or this workspace. Put
this in `src/main.rs` of a new Cargo package with these dependencies:

```toml
[dependencies]
restate-sdk = "=0.12.0"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

```rust
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use restate_sdk::prelude::*;
use tracing::Instrument as _;
use tracing_subscriber::{layer::SubscriberExt as _, util::SubscriberInitExt as _, Layer as _};

#[derive(Default)]
struct ReplayLog(AtomicUsize);

#[restate_sdk::service]
impl ReplayLog {
    #[handler]
    async fn run(&self, ctx: Context<'_>) -> Result<(), HandlerError> {
        async {
            tracing::info!("before completed run");
            ctx.run(|| async { Ok(()) }).name("completed").await?;
            ctx.run(|| async {
                if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(HandlerError::from(std::io::Error::other("retry once")));
                }
                tracing::info!("fresh closure");
                Ok(())
            })
            .name("retry")
            .retry_policy(RunRetryPolicy::new().max_attempts(2)
                .initial_delay(Duration::from_secs(1)))
            .await?;
            tracing::warn!("fresh after run");
            Ok(())
        }
        .instrument(tracing::info_span!("execution", account = "example"))
        .await
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_filter(tracing_subscriber::EnvFilter::new("info"))
            .with_filter(restate_sdk::filter::ReplayAwareFilter)
    ).init();
    HttpServer::new(Endpoint::builder().bind(ReplayLog::default()).build())
        .listen_and_serve("127.0.0.1:9080".parse().unwrap()).await;
}
```

Run against a local Restate server, register `http://127.0.0.1:9080`, then POST
once to `http://127.0.0.1:8080/ReplayLog/run`. Expect one `before completed run`,
one `fresh closure` and one `fresh after run`. Released 0.12.0 logs only the first.
Restart the example between trials to reset its retry counter. The failure is a
run retry: reusing an ingress idempotency key for an already completed invocation
does not reproduce it.

## Local mitigation and removal condition

`prologue::execute` retains the current SDK endpoint span before entering the
worker's child execution span, scoped to that execution with a Tokio task-local.
The shared `RunCtx::run` adapter clears `restate.sdk.is_replaying` on that retained
span at the first line of an **actually executed** closure. This includes account
resolution and credential acquisition. Completed runs never enter that closure,
so replayed events remain suppressed. The task-local crosses async polls without
sharing the span between concurrent invocations. The execution span still owns
scope, order, account and invocation correlation.

This is deliberately worker-local: it does not fix another service bound to the
same endpoint. No custom subscriber or SDK fork is required by the worker. Keep
the SDK endpoint INFO span enabled (`RUST_LOG=info,restate_szamlazz=debug`, for
example); the SDK's filter needs that span to track replay at all.

Remove `SDK_SPAN`, its scope in `execute`, and `mark_fresh_work` plus its run-adapter
call when the minimum supported SDK version includes **both** corrections above.
Run the regression against that version **without** the mitigation before removal.
No fixed release number is claimed until upstream ships one.
