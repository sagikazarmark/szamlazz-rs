//! The crate's own contract against a real `restate-server`, with no
//! consumer: the gate launches a server, a trivial service of this test is
//! deployed, invoked through the ingress, read back through the journal and
//! the fault envelope, killed and purged, and the server stops on drop.
//!
//! Ignored: `cargo test -p restate-e2e-harness -- --ignored` with
//! `RESTATE_SERVER_BIN` set. Never a reused server (`ReusePolicy::Never`): the test
//! deploys a service of its own and leaves its invocations retained for a
//! day, which a suite sharing that server (one whose step-name table check
//! scans every invocation the server holds) would meet as an untabled
//! handler.

#![cfg(unix)]
#![allow(
    missing_docs,
    reason = "the macro-generated `SmokeClient` carries no documentation"
)]

use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{
    Call, Launcher, ReusePolicy, ServerSpec, launcher_or_skip, run_result, run_result_at,
};
use restate_sdk::prelude::*;
use serde::Deserialize;
use serde_json::json;

const SERVER: ServerSpec = ServerSpec {
    name: "smoke",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, true),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

const LISTENER_MODE_SOURCE: &str = "HARNESS_TEST_LISTENER_MODE_SOURCE";

fn server_spec() -> ServerSpec {
    ServerSpec {
        env: if std::env::var(LISTENER_MODE_SOURCE).as_deref() == Ok("spec") {
            &[
                "RESTATE_ADMIN__LISTEN_MODE=unix",
                "RESTATE_INGRESS__LISTEN_MODE=unix",
            ]
        } else {
            SERVER.env
        },
        ..SERVER
    }
}

/// Run the actual launcher in isolated processes so inherited settings do not
/// require mutating this test process's environment while Tokio is running.
#[test]
#[ignore = "needs RESTATE_SERVER_BIN; verifies listener-mode override precedence"]
fn e2e_listener_modes() {
    if launcher_or_skip(ReusePolicy::Never).is_none() {
        return;
    }
    for source in ["inherited", "spec"] {
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("test executable"));
        command
            .args(["--exact", "e2e_smoke", "--ignored", "--nocapture"])
            .env(LISTENER_MODE_SOURCE, source);
        for name in ["RESTATE_ADMIN__LISTEN_MODE", "RESTATE_INGRESS__LISTEN_MODE"] {
            if source == "inherited" {
                command.env(name, "unix");
            } else {
                command.env_remove(name);
            }
        }
        let output = command.output().expect("isolated listener-mode smoke test");
        assert!(
            output.status.success(),
            "{source} listener settings must not disable the advertised TCP ports:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }
}

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

    /// SDK-supported ordering: immediately await each run, with a sleep
    /// between repeated names. Distinct results prove the selected occurrence.
    #[handler(journal_retention = "1d")]
    async fn correlation(&self, ctx: Context<'_>) -> HandlerResult<()> {
        ctx.run(|| async { Ok("first-result".to_owned()) })
            .name("repeated")
            .await?;
        ctx.sleep(std::time::Duration::from_millis(1)).await?;
        ctx.run(|| async { Ok("second-result".to_owned()) })
            .name("repeated")
            .await?;
        ctx.run(|| async { Ok("unique-result".to_owned()) })
            .name("unique")
            .await?;
        Ok(())
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

/// The refusing handler, called three times below.
const REFUSE: Call<'static> = Call::service("Smoke", "refuse");

/// The consumer-side shape of the fault above.
#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Fault {
    code: String,
    message: String,
}

#[tokio::test]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN (a server of its own, never a reused one)"]
async fn e2e_smoke() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    assert!(
        matches!(launcher, Launcher::Binary { .. }),
        "ReusePolicy::Never yields a binary"
    );
    let restate = launcher.launch(&server_spec()).await;
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
            &Call::service("Smoke", "echo"),
            Some(&json!("hello")),
            Some("smoke-1"),
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body, json!("hello"));
    let id = reply.invocation_id();
    assert_eq!(restate.admin().runs(id).await, ["echo-step"]);
    let journal = restate.admin().journal(id).await;
    assert!(
        run_result(&journal, "echo-step")
            .expect("echo result")
            .raw_contains("hello")
    );
    let invocation = restate.admin().invocation(id).await;
    assert_eq!(
        (invocation.service.as_str(), invocation.handler.as_str()),
        ("Smoke", "echo")
    );
    assert_eq!(invocation.status, "completed");
    assert!(restate.admin().all_journals().await.contains_key(id));

    check_run_correlation(&restate).await;
    check_raw_ingress(&restate).await;

    // A fault, decoded out of the envelope into the caller's type.
    let reply = restate.invoke(&REFUSE, None, None).await;
    assert_eq!(reply.status, 422, "{}", reply.body);
    assert_eq!(
        reply.fault::<Fault>(),
        Fault {
            code: "refused".to_owned(),
            message: "the smoke test's fault".to_owned(),
        }
    );

    // Kill and purge an invocation that would never finish.
    let reply = restate
        .invoke(&Call::service("Smoke", "hang").send(), None, None)
        .await;
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
    let reply = restate.invoke(&REFUSE, None, None).await;
    assert_ne!(
        reply.status, 422,
        "a private service is not invoked: {}",
        reply.body
    );
    restate.set_public("Smoke", true).await;
    let reply = restate.invoke(&REFUSE, None, None).await;
    assert_eq!(reply.status, 422, "{}", reply.body);

    // The spawned server stops with the handle: nothing listens on its admin
    // port any more.
    drop(restate);
    check_teardown(&admin_url, [first, second]).await;
}

async fn check_teardown(admin_url: &str, deployments: [restate_e2e_harness::Deployment; 2]) {
    let admin_addr = admin_url
        .strip_prefix("http://")
        .expect("the admin URL of a spawned server is plain http");
    assert!(
        std::net::TcpStream::connect(admin_addr).is_err(),
        "the spawned server is gone with the handle: {admin_addr} still accepts connections"
    );
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        for deployment in deployments {
            while tokio::net::TcpStream::connect(("127.0.0.1", deployment.port))
                .await
                .is_ok()
            {
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("local endpoints stop with their owner after SDK connection draining");
}

async fn check_raw_ingress(restate: &restate_e2e_harness::Restate) {
    // Send malformed bytes rather than a valid JSON string. The SDK must
    // refuse this before running the handler.
    let http = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("consumer HTTP client");
    let response = http
        .post(format!(
            "{}{}",
            restate.ingress_url(),
            Call::service("Smoke", "echo").path()
        ))
        .header("content-type", "application/json")
        .body("\"unfinished")
        .send()
        .await
        .expect("raw ingress request");
    assert_eq!(response.status().as_u16(), 400);
    let id = response
        .headers()
        .get("x-restate-id")
        .expect("invocation id")
        .to_str()
        .expect("text invocation id");
    assert!(restate.admin().runs(id).await.is_empty());
}

async fn check_run_correlation(restate: &restate_e2e_harness::Restate) {
    let reply = restate
        .invoke(&Call::service("Smoke", "correlation"), None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    let journal = restate.admin().journal(reply.invocation_id()).await;
    assert!(
        journal
            .iter()
            .any(|entry| entry.entry_type == "Notification: Sleep")
    );
    for (occurrence, expected, other) in [
        (0, "first-result", "second-result"),
        (1, "second-result", "first-result"),
    ] {
        let result = run_result_at(&journal, "repeated", occurrence).expect("completed occurrence");
        assert!(result.raw_contains(expected));
        assert!(!result.raw_contains(other));
        let command = journal
            .iter()
            .filter(|entry| entry.is_run() && entry.name.as_deref() == Some("repeated"))
            .nth(occurrence)
            .expect("command");
        assert_eq!(command.run_completion_id, result.run_completion_id);
        assert!(command.run_completion_id.is_some());
    }
    assert!(std::panic::catch_unwind(|| run_result(&journal, "repeated")).is_err());
    assert!(
        run_result(&journal, "unique")
            .expect("unique result")
            .raw_contains("unique-result")
    );
    assert!(run_result(&journal, "absent").is_none());
    let all = restate.admin().all_journals().await;
    assert_eq!(all[reply.invocation_id()], journal);
}
