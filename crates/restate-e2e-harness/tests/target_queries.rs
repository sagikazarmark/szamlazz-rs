//! SQL escaping at the public admin HTTP boundary. A real Restate scope
//! cannot contain a quote; selectors still treat every value as literal data.

use std::time::Duration;

use restate_e2e_harness::{Admin, ScopeSelection, Target, Watch};
use serde_json::json;
use wiremock::matchers::{body_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn object_queries_escape_service_key_and_scope_in_every_selection() {
    let server = MockServer::start().await;
    let http = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .build()
        .expect("loopback client");
    let admin = Admin::new(server.uri(), http);
    let object = Target::object("Svc' OR '1'='1", "O'Brien");
    assert_eq!(object.scope, ScopeSelection::default());

    for (target, predicate) in [
        (
            object,
            "target_service_name = 'Svc'' OR ''1''=''1' AND target_service_key = 'O''Brien' AND scope IS NULL",
        ),
        (
            object.scoped("scope' OR '1'='1"),
            "target_service_name = 'Svc'' OR ''1''=''1' AND target_service_key = 'O''Brien' AND scope = 'scope'' OR ''1''=''1'",
        ),
        (
            object.all_scopes(),
            "target_service_name = 'Svc'' OR ''1''=''1' AND target_service_key = 'O''Brien'",
        ),
    ] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/query"))
            .and(body_json(json!({"query": format!(
                "SELECT id FROM sys_invocation WHERE {predicate} AND status <> 'completed' ORDER BY id"
            )})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [{"id": "one"}]})))
            .expect(3)
            .mount(&server).await;
        assert_eq!(admin.in_flight_ids_on(&target).await, ["one"]);
        assert_eq!(admin.in_flight_on(&target).await, "one");
        assert_eq!(admin.await_in_flight_on(&target, 1).await, ["one"]);

        Mock::given(method("POST"))
            .and(path("/query"))
            .and(body_json(json!({"query": format!(
                "SELECT status, retry_count, last_failure, last_failure_related_command_name FROM sys_invocation WHERE {predicate}"
            )})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [{
                "status": "running", "retry_count": 2, "last_failure_related_command_name": "retry"
            }]})))
            .expect(2..)
            .mount(&server).await;
        let watch = Watch::start(admin.clone(), &target);
        // Once the second watch query arrives, the first answer has been
        // processed. Bound this wait so a broken query cannot stall the test.
        tokio::time::timeout(Duration::from_secs(5), async {
            while server.received_requests().await.expect("requests").len() < 5 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("two watch queries");
        let retries = watch.finish().await;
        assert_eq!(retries.query_errors, 0, "{retries:?}");
        assert_eq!(retries.max_retry_count, 2);
        assert_eq!(retries.failing_commands, ["retry"]);
        server.verify().await;
    }
}
