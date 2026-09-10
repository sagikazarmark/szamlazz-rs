//! Diagnostics at the admin HTTP boundary, including non-JSON proxy failures.

use restate_e2e_harness::Admin;
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

#[tokio::test]
async fn sql_preserves_unsuccessful_status_and_body_before_decoding_rows() {
    let server = MockServer::start().await;
    let http = reqwest::Client::builder()
        .tls_certs_only(std::iter::empty())
        .build()
        .expect("loopback client");
    let admin = Admin::new(server.uri(), http);
    for (status, body) in [
        (502, "upstream connection refused"),
        (503, "<html>unavailable</html>"),
        (400, r#"{"message":"bad query"}"#),
    ] {
        server.reset().await;
        Mock::given(path("/query"))
            .respond_with(ResponseTemplate::new(status).set_body_string(body))
            .mount(&server)
            .await;
        let error = admin.sql("SELECT 1").await.expect_err("unsuccessful query");
        assert!(error.contains(&status.to_string()), "{error}");
        assert!(error.contains(body), "{error}");
    }
    server.reset().await;
    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [{"value": 1}]})))
        .mount(&server)
        .await;
    assert_eq!(
        admin.sql("SELECT 1").await.expect("query rows"),
        [json!({"value": 1})]
    );

    server.reset().await;
    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not rows"))
        .mount(&server)
        .await;
    let error = admin
        .sql("SELECT 1")
        .await
        .expect_err("malformed successful reply");
    assert!(
        error.contains("200") && error.contains("not rows"),
        "{error}"
    );
}
