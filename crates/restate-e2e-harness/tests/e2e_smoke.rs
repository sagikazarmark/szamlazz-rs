//! The crate's own contract against a real `restate-server`, with no
//! consumer: the gate launches a server, a trivial service of this test is
//! deployed, invoked through the ingress, read back through the journal and
//! the fault envelope, killed and purged, and the server stops on drop.
//!
//! Ignored: `cargo test -p restate-e2e-harness -- --ignored` with
//! `RESTATE_SERVER_BIN` set. Never a reused server (`Reuse::Never`): the test
//! deploys a service of its own and leaves its invocations retained for a
//! day, which a suite sharing that server (one whose step-name table check scans every
//! invocation the server holds) would meet as an unpinned handler.

#![cfg(unix)]
#![allow(
    missing_docs,
    reason = "the macro-generated `SmokeClient` carries no documentation"
)]

use restate_e2e_harness::gate::{FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS, FLAG_VQUEUES};
use restate_e2e_harness::{Launcher, Reuse, ServerSpec, launcher_or_skip, plain_http};
use restate_sdk::prelude::*;
use serde::Deserialize;
use serde_json::json;

const SERVER: ServerSpec = ServerSpec {
    name: "smoke",
    flags: &[FLAG_VQUEUES, FLAG_PROTOCOL_V7, FLAG_SCOPED_VIRTUAL_OBJECTS],
};

struct Smoke;

#[restate_sdk::service(name = "Smoke")]
impl Smoke {
    /// One named `ctx.run`, echoing its input. Retained so the journal can be
    /// read after completion.
    #[handler(journal_retention = "1d")]
    async fn echo(&self, ctx: Context<'_>, input: String) -> HandlerResult<String> {
        let echoed = ctx
            .run(|| async move { Ok(input) })
            .name("echo-step")
            .await?;
        Ok(echoed)
    }

    /// A `TerminalError` whose message is a JSON body: what a consumer's
    /// structured fault travels as.
    #[handler(journal_retention = "1d")]
    #[allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "a handler is async"
    )]
    async fn refuse(&self, _ctx: Context<'_>) -> HandlerResult<String> {
        let body = json!({ "code": "refused", "message": "the smoke test's fault" });
        Err(TerminalError::new_with_code(422, body.to_string()).into())
    }

    /// Never completes on its own: what a kill acts on.
    #[handler(journal_retention = "1d")]
    async fn hang(&self, ctx: Context<'_>) -> HandlerResult<()> {
        ctx.sleep(std::time::Duration::from_secs(3600)).await?;
        Ok(())
    }
}

/// The consumer-side shape of the fault above.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Fault {
    code: String,
    message: String,
}

#[tokio::test]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN (a server of its own, never a reused one)"]
async fn e2e_smoke() {
    let Some(launcher) = launcher_or_skip(Reuse::Never) else {
        return;
    };
    assert!(
        matches!(launcher, Launcher::Binary(_)),
        "Reuse::Never yields a binary"
    );
    let restate = launcher.launch(&SERVER).await;
    let admin_url = restate.admin_url().to_owned();

    // Deploy, twice: a redeploy is a second call.
    let first = restate
        .deploy(Endpoint::builder().bind(Smoke).build())
        .await;
    let second = restate
        .deploy(Endpoint::builder().bind(Smoke).build())
        .await;
    assert_ne!(first.port, second.port, "two endpoints, two ports");

    // A call through the ingress, its run in the journal.
    let reply = restate
        .invoke(
            "/restate/call/Smoke/echo",
            Some(&json!("hello")),
            Some("smoke-1"),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body, json!("hello"));
    let id = reply.invocation_id();
    assert_eq!(restate.admin().runs(id).await, ["echo-step"]);
    let invocation = restate.admin().invocation(id).await;
    assert_eq!(
        (invocation.service.as_str(), invocation.handler.as_str()),
        ("Smoke", "echo")
    );
    assert_eq!(invocation.status, "completed");
    assert!(restate.admin().all_journals().await.contains_key(id));

    // A fault, decoded out of the envelope into the caller's type.
    let reply = restate
        .invoke("/restate/call/Smoke/refuse", None, None)
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(
        reply.fault::<Fault>(),
        Fault {
            code: "refused".to_owned(),
            message: "the smoke test's fault".to_owned(),
        }
    );

    // Kill and purge an invocation that would never finish.
    let reply = restate.invoke("/restate/send/Smoke/hang", None, None).await;
    assert_eq!(reply.status, 202, "{}", reply.body);
    let id = reply.invocation_id().to_owned();
    restate
        .admin()
        .await_status(&id, &["running", "suspended"])
        .await;
    restate.admin().kill(&id).await;
    let status = restate
        .admin()
        .await_status(&id, &["completed", "killed"])
        .await;
    let invocation = restate.admin().invocation(&id).await;
    assert!(
        invocation.completion_failure.is_some(),
        "a killed invocation completes with a failure: {status} {invocation:?}"
    );
    restate.drain().await;
    restate.admin().purge(&id).await;
    assert!(
        restate
            .admin()
            .all_invocations()
            .await
            .iter()
            .all(|(held, _)| *held != id)
    );

    // A private service is refused at the ingress; public again, it answers.
    restate.set_public("Smoke", false).await;
    let reply = restate
        .invoke("/restate/call/Smoke/refuse", None, None)
        .await;
    assert_ne!(
        reply.status, 422,
        "a private service is not invoked: {}",
        reply.body
    );
    restate.set_public("Smoke", true).await;
    let reply = restate
        .invoke("/restate/call/Smoke/refuse", None, None)
        .await;
    assert_eq!(reply.status, 422, "{}", reply.body);

    // The spawned server stops with the handle.
    drop(restate);
    let health = plain_http()
        .build()
        .expect("client")
        .get(format!("{admin_url}/health"))
        .send()
        .await;
    assert!(
        health.is_err(),
        "the spawned server is gone with the handle"
    );
}
