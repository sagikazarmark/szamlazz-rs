//! Failure-closed inventory and command-boundary regressions.
#![allow(clippy::unwrap_used)]
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_partial_json, method, path},
};

const INVOCATIONS: &str = "SELECT id, scope, target_service_name, target_service_key, status FROM sys_invocation WHERE target_service_name IN ('Szamlazz.Order', 'Szamlazz.Agent') AND status <> 'completed'";
const STATE: &str =
    "SELECT scope, service_name, service_key, key FROM state WHERE service_name = 'Szamlazz.Order'";

async fn rows(server: &MockServer, sql: &str, rows: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/query"))
        .and(body_partial_json(json!({"query": sql})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": rows})))
        .mount(server)
        .await;
}

#[tokio::test]
async fn absence_is_only_a_clean_inventory() {
    let server = MockServer::start().await;
    rows(&server, INVOCATIONS, json!([])).await;
    rows(&server, STATE, json!([])).await;
    assert!(
        xtask::migration::inventory(&server.uri(), None)
            .await
            .unwrap()
            .is_clear()
    );
}

#[tokio::test]
async fn every_state_row_blocks_without_decoding_its_value() {
    let server = MockServer::start().await;
    for scope in [json!(null), json!("acme")] {
        for key in ["unresolved-write", "unknown-state-key"] {
            server.reset().await;
            rows(&server, INVOCATIONS, json!([])).await;
            rows(&server, STATE, json!([{"scope": scope, "key": key}])).await;
            assert!(
                !xtask::migration::inventory(&server.uri(), None)
                    .await
                    .unwrap()
                    .is_clear()
            );
        }
    }
}

#[tokio::test]
async fn unfinished_invocations_block() {
    let server = MockServer::start().await;
    rows(&server, INVOCATIONS, json!([{"status":"paused"}])).await;
    rows(&server, STATE, json!([])).await;
    assert!(
        !xtask::migration::inventory(&server.uri(), None)
            .await
            .unwrap()
            .is_clear()
    );
}

#[tokio::test]
async fn query_failures_never_pass() {
    let server = MockServer::start().await;
    for body in [json!({}), json!({"rows": null}), json!({"rows": {}})] {
        server.reset().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        assert!(
            xtask::migration::inventory(&server.uri(), None)
                .await
                .is_err()
        );
    }
    server.reset().await;
    rows(&server, INVOCATIONS, json!([])).await;
    Mock::given(body_partial_json(json!({"query":STATE})))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert!(
        xtask::migration::inventory(&server.uri(), None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn redirects_never_forward_credentials_or_pass() {
    let server = MockServer::start().await;
    let destination = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(302).insert_header("Location", destination.uri()))
        .mount(&server)
        .await;
    assert!(
        xtask::migration::inventory(&server.uri(), Some("synthetic-token"))
            .await
            .is_err()
    );
    assert!(destination.received_requests().await.unwrap().is_empty());
    assert_eq!(
        server.received_requests().await.unwrap()[0].headers["Authorization"],
        "Bearer synthetic-token"
    );
}

#[tokio::test]
async fn command_exit_codes_and_redacted_failures() {
    let server = MockServer::start().await;
    for (state, expected) in [(json!([]), 0), (json!([{"key":"unknown"}]), 1)] {
        server.reset().await;
        rows(&server, INVOCATIONS, json!([])).await;
        rows(&server, STATE, state).await;
        let output = command(&server.uri()).await;
        assert_eq!(output.status.code(), Some(expected));
        let inventory: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(inventory["order_state"].is_array());
    }
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(500).set_body_string("sensitive remote error"))
        .mount(&server)
        .await;
    let output = command(&format!("{}/sensitive-url", server.uri())).await;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "INVENTORY FAILED; migration remains blocked\n"
    );
}

async fn command(admin: &str) -> std::process::Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["check-order-migration", "--admin-url", admin])
        .env("RESTATE_ADMIN_TOKEN", "synthetic-token")
        .output()
        .await
        .unwrap()
}
