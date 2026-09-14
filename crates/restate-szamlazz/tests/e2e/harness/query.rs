//! Exceptional completed-query replay on the same immutable deployment.

use restate_e2e_harness::Admin;
use serde_json::Value;

/// Restart through the completed query Run, excluding Output. Verify that
/// replay decodes the recorded facts and emits the same Output. The calling
/// scenario asserts that the provider receives no additional request.
pub(crate) async fn replay_completed(admin: &Admin, id: &str, expected: &Value) {
    let journal = admin.journal(id).await;
    let query = journal
        .iter()
        .find(|entry| entry.is_run() && entry.name.as_deref() == Some("query"))
        .expect("query command");
    let response = crate::common::http_client()
        .patch(format!(
            "{}/invocations/{id}/restart-as-new?from={}&deployment=keep",
            admin.base(),
            query.index
        ))
        .send()
        .await
        .expect("restart query prefix");
    let status = response.status();
    let restarted: Value = response.json().await.expect("restart response");
    assert!(status.is_success(), "{restarted}");
    let new_id = restarted["new_invocation_id"]
        .as_str()
        .expect("new invocation id");
    admin.await_status(new_id, &["completed"]).await;
    assert!(admin.invocation(new_id).await.completion_failure.is_none());
    let replayed = admin.journal(new_id).await;
    let output = replayed
        .iter()
        .find(|entry| entry.entry_type == "Command: Output")
        .expect("new output");
    let start = output
        .raw
        .iter()
        .position(|byte| *byte == b'{')
        .expect("JSON output");
    let actual: Value = serde_json::Deserializer::from_slice(&output.raw[start..])
        .into_iter()
        .next()
        .expect("JSON value")
        .expect("output");
    assert_eq!(&actual, expected);
    let original = restate_e2e_harness::run_result(&journal, "query").expect("original read");
    let copied = restate_e2e_harness::run_result(&replayed, "query").expect("copied read");
    assert_eq!(
        copied.raw, original.raw,
        "replay retains the same document facts"
    );
}
