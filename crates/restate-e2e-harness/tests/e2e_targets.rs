//! Object selection against a real server, including the unscoped object
//! beside the same service/key under two named scopes.

#![cfg(unix)]
#![allow(missing_docs, reason = "test-only generated clients")]

use std::collections::BTreeSet;
use std::time::Duration;

use restate_e2e_harness::gate::{PROTOCOL_V7, SCOPED_VIRTUAL_OBJECTS, VQUEUES};
use restate_e2e_harness::{Admin, Call, ReusePolicy, ServerSpec, Target, Watch, launcher_or_skip};
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
    #[handler]
    #[allow(
        clippy::unused_async,
        clippy::unused_async_trait_impl,
        reason = "a handler is async"
    )]
    async fn key(&self, ctx: SharedObjectContext<'_>) -> HandlerResult<String> {
        Ok(ctx.key().to_owned())
    }

    // Keep every retry delay below the observed 2 s scheduler-yield threshold
    // on Restate 1.7.8 with vqueues; this improves visibility, not guarantees it.
    #[handler(
        journal_retention = "1d",
        invocation_retry_policy(initial_interval = "1s", max_interval = "1s")
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

/// The invocations remain in flight throughout sampling, but their failing
/// commands need not be visible in every successful sample. Accumulate evidence
/// across watches within one deadline, rejecting any label outside the selection.
async fn sample(admin: &Admin, target: &Target<'_>, expected: &[&str]) {
    let mut observed = BTreeSet::new();
    let mut samples = 0;
    let mut query_errors = 0;
    let result = tokio::time::timeout(Duration::from_secs(30), async {
        let mut window = Duration::from_millis(200);
        loop {
            let watch = Watch::start(admin.clone(), target);
            tokio::time::sleep(window).await;
            let retries = watch.finish().await;
            samples += retries.samples;
            query_errors += retries.query_errors;
            assert_eq!(query_errors, 0, "{target:?}: {retries:?}");
            assert!(
                !retries.observed_completion,
                "{target:?} is still in flight: {retries:?}"
            );
            for label in &retries.failing_commands {
                assert!(
                    expected.contains(&label.as_str()),
                    "{target:?}: unexpected label {label:?}, expected {expected:?}: {retries:?}"
                );
            }
            observed.extend(retries.failing_commands);
            if expected.iter().all(|label| observed.contains(*label)) {
                return;
            }
            // Give slow queries time to answer without an unbounded window
            // delaying the next check for unexpected labels.
            window = (window * 2).min(Duration::from_secs(2));
        }
    })
    .await;
    assert!(
        result.is_ok(),
        "{target:?}: timed out waiting for {expected:?}; observed {observed:?}, \
         samples: {samples}, query_errors: {query_errors}"
    );
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

    // The same logical key reaches both the ingress and SQL selector. Reserved
    // characters must survive URL construction and the server's decoding once.
    let key = "O'Brien/é ?#%2F";
    let object = Target::object("ScopedObject", key);
    let call = Call::object("ScopedObject", key, "retry").send();
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
        (Call::object("OtherObject", key, "hang").send(), None),
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
        sample(restate.admin(), target, &[*label]).await;
    }
    sample(restate.admin(), &all, &["alpha", "beta", "unscoped"]).await;

    for id in ids.iter().chain(&decoys) {
        restate.admin().kill(id).await;
    }
    restate.drain().await;

    for key in [
        "invoice/2026",
        "why?",
        "hash#tag",
        "%2F",
        "100%",
        "é space",
        "O'Brien",
        "a/../b",
    ] {
        let reply = restate
            .invoke(&Call::object("ScopedObject", key, "key"), None, None)
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        assert_eq!(reply.body, json!(key));
    }
}
