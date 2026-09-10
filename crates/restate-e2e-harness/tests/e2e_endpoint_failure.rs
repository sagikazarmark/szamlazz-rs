//! A successful Restate retry must not erase a failed in-process assertion.

#![cfg(unix)]
#![allow(missing_docs, reason = "test-only generated client")]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{Call, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::*;
use serde_json::json;

const SERVER: ServerSpec = ServerSpec {
    name: "endpoint-failure",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, true),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

struct PanicOnce(Arc<AtomicUsize>);

#[restate_sdk::service(name = "PanicOnce")]
impl PanicOnce {
    #[handler(invocation_retry_policy(initial_interval = "100ms", max_interval = "100ms"))]
    async fn run(&self, _ctx: Context<'_>) -> HandlerResult<String> {
        tokio::task::yield_now().await;
        assert_ne!(
            self.0.fetch_add(1, Ordering::SeqCst),
            0,
            "first execution assertion failed"
        );
        Ok("retried successfully".into())
    }
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN: a server of its own"]
async fn e2e_successful_retry_does_not_erase_an_endpoint_panic() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher.launch(&SERVER).await;
    let calls = Arc::new(AtomicUsize::new(0));
    let first = restate
        .deploy(Endpoint::builder().bind(PanicOnce(calls.clone())).build())
        .await;
    let reply = restate
        .invoke(&Call::service("PanicOnce", "run"), None, None)
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body, json!("retried successfully"));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(
        restate
            .admin()
            .invocation(reply.invocation_id())
            .await
            .completion_failure
            .is_none()
    );

    // Earlier deployments' failures remain visible after a clean redeployment.
    restate
        .deploy(Endpoint::builder().bind(PanicOnce(calls)).build())
        .await;
    let error = tokio::spawn(restate.finish())
        .await
        .expect_err("the test must still fail");
    let message = error.to_string();
    assert!(
        message.contains("first execution assertion failed"),
        "{message}"
    );
    assert!(message.contains(&first.uri), "{message}");
}
