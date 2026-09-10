//! Object selection against a real server, including the unscoped object
//! beside the same service/key under two named scopes.

#![cfg(unix)]
#![allow(missing_docs, reason = "test-only generated clients")]

use std::time::Duration;

use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{
    Admin, Call, Retries, ReusePolicy, ServerSpec, Target, Watch, launcher_or_skip,
};
use restate_sdk::prelude::*;
use serde_json::json;

const SERVER: ServerSpec = ServerSpec {
    name: "targets",
    features: &[
        (VQUEUES, true),
        (PROTOCOL_V7, true),
        (SCOPED_VIRTUAL_OBJECTS, true),
    ],
    env: &[],
};

struct ScopedObject;

#[restate_sdk::object(name = "ScopedObject")]
impl ScopedObject {
    #[handler(
        journal_retention = "1d",
        invocation_retry_policy(initial_interval = "1s")
    )]
    async fn retry(&self, ctx: ObjectContext<'_>, label: String) -> HandlerResult<()> {
        ctx.run(|| async { Err::<(), _>(std::io::Error::other(label.clone()).into()) })
            .name(&label)
            .await?;
        Ok(())
    }
}

struct OtherObject;

#[restate_sdk::object(name = "OtherObject")]
impl OtherObject {
    #[handler]
    async fn hang(&self, ctx: ObjectContext<'_>) -> HandlerResult<()> {
        ctx.sleep(Duration::from_secs(3600)).await?;
        Ok(())
    }
}

/// The invocations remain in flight throughout sampling. A busy runner may
/// not process a sample before `finish` cancels it; try a new watch if so,
/// within one deadline. Once a sample answers, its selection must be correct.
async fn sample(admin: &Admin, target: &Target<'_>) -> Retries {
    tokio::time::timeout(Duration::from_secs(30), async {
        let mut window = Duration::from_millis(200);
        loop {
            let watch = Watch::start(admin.clone(), target);
            tokio::time::sleep(window).await;
            let retries = watch.finish().await;
            assert_eq!(retries.query_errors, 0, "{retries:?}");
            if retries.samples > 0 {
                return retries;
            }
            window *= 2;
        }
    })
    .await
    .expect("a processed watch sample")
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN: a server of its own"]
async fn e2e_targets_isolate_the_same_service_and_key_in_each_scope() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher.launch(&SERVER).await;
    restate
        .deploy(
            Endpoint::builder()
                .bind(ScopedObject)
                .bind(OtherObject)
                .build(),
        )
        .await;

    let object = Target::object("ScopedObject", "O'Brien");
    let call = Call::object("ScopedObject", "O'Brien", "retry").send();
    let cases = [
        (object, call, "unscoped"),
        (object.scoped("alpha"), call.scoped("alpha"), "alpha"),
        (object.scoped("beta"), call.scoped("beta"), "beta"),
    ];
    let mut ids = Vec::new();
    for (_, call, label) in &cases {
        let reply = restate.invoke(call, Some(&json!(label)), None).await;
        assert_eq!(reply.status, 202, "{}", reply.body);
        let id = reply.invocation_id().to_owned();
        restate.admin().await_status(&id, &["backing-off"]).await;
        ids.push(id);
    }

    // Neither another key nor another service with this key belongs to any
    // selection below, including the explicit all-scope selection.
    let mut decoys = Vec::new();
    for (call, body) in [
        (
            Call::object("ScopedObject", "another-key", "retry").send(),
            Some(json!("decoy")),
        ),
        (Call::object("OtherObject", "O'Brien", "hang").send(), None),
    ] {
        let reply = restate.invoke(&call, body.as_ref(), None).await;
        assert_eq!(reply.status, 202, "{}", reply.body);
        let id = reply.invocation_id().to_owned();
        restate
            .admin()
            .await_status(&id, &["running", "suspended", "backing-off"])
            .await;
        decoys.push(id);
    }

    for ((target, _, _), id) in cases.iter().zip(&ids) {
        assert_eq!(
            restate.admin().in_flight_ids_on(target).await,
            [id.as_str()]
        );
        assert_eq!(restate.admin().in_flight_on(target).await, *id);
        assert_eq!(
            restate.admin().await_in_flight_on(target, 1).await,
            [id.as_str()]
        );
    }

    let all = object.all_scopes();
    let mut sorted_ids = ids.clone();
    sorted_ids.sort();
    assert_eq!(restate.admin().in_flight_ids_on(&all).await, sorted_ids);
    assert_eq!(
        restate.admin().await_in_flight_on(&all, 3).await,
        sorted_ids
    );

    for (target, _, label) in &cases {
        let retries = sample(restate.admin(), target).await;
        assert_eq!(retries.failing_commands, [*label]);
    }
    let mut retries = sample(restate.admin(), &all).await;
    retries.failing_commands.sort();
    assert_eq!(retries.failing_commands, ["alpha", "beta", "unscoped"]);
    assert!(
        !retries.observed_completion,
        "named scopes are still in flight"
    );

    for id in ids.iter().chain(&decoys) {
        restate.admin().kill(id).await;
    }
    restate.drain().await;
}
