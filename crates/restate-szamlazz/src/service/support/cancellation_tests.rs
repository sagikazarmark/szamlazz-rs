//! Hold the otherwise immediate namespace run to observe its ingress boundary.

use std::sync::Arc;
use std::time::Duration;

use restate_sdk::errors::HandlerError;
use restate_sdk::prelude::{Context, Endpoint};
use tokio::sync::Notify;

use super::run_once;
use crate::contract::{Fault, TerminalCode};
use crate::identity::Namespace;

struct Pin {
    reached: Arc<Notify>,
}

#[allow(missing_docs, reason = "test-only SDK-generated clients")]
#[restate_sdk::service]
impl Pin {
    #[handler]
    async fn pin(&self, ctx: Context<'_>) -> Result<(), HandlerError> {
        let reached = Arc::clone(&self.reached);
        run_once(&ctx, "namespace", move || async move {
            reached.notify_one();
            tokio::time::sleep(Duration::from_secs(4)).await;
            "acct".parse::<Namespace>().expect("namespace")
        })
        .await?;
        Ok(())
    }
}

#[tokio::test]
#[ignore = "needs a Restate server: RESTATE_SERVER_BIN"]
async fn e2e_cancelled_namespace_is_a_structured_read_fault() {
    use restate_e2e_harness::{
        Call, ServerSpec,
        gate::{ReusePolicy, launcher_or_skip},
    };
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let server = launcher
        .launch(&ServerSpec {
            name: "cancel-namespace",
            features: &[],
            env: &[],
        })
        .await;
    let reached = Arc::new(Notify::new());
    server
        .deploy(
            Endpoint::builder()
                .bind(Pin {
                    reached: Arc::clone(&reached),
                })
                .build(),
        )
        .await;
    let call = Call::service("Pin", "pin");
    let submitted = server.invoke(&call.send(), None, Some("cancel-pin")).await;
    assert_eq!(submitted.status, 202);
    tokio::time::timeout(Duration::from_secs(30), reached.notified())
        .await
        .expect("pin reached");
    server.admin().cancel(submitted.invocation_id()).await;
    let reply = server.invoke(&call, None, Some("cancel-pin")).await;
    assert_eq!(reply.status, 409, "{}", reply.body);
    let fault: Fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::Cancelled);
    assert_eq!(fault.is_cancelled(), Some(true));
}
