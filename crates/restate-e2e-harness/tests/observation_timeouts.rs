//! Explicit observation deadlines bound stalled bodies and can outlast defaults.

use std::time::Duration;

use restate_e2e_harness::{Admin, Target};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn every_explicit_timeout_bounds_its_observation() {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    let server = MockServer::start().await;
    for route in ["/query", "/deployments"] {
        Mock::given(path(route))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
            .mount(&server)
            .await;
    }
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200))
        .expect(2)
        .mount(&server)
        .await;
    let admin = Admin::new(
        server.uri(),
        reqwest::Client::builder()
            .tls_certs_only(std::iter::empty())
            .build()
            .expect("loopback client"),
    );
    let mut waits = Vec::new();
    for operation in ["status", "in-flight", "drain", "pause", "purge", "register"] {
        let admin = admin.clone();
        let wait = tokio::spawn(async move {
            let timeout = Duration::from_millis(50);
            match operation {
                "status" => {
                    admin
                        .await_status_with_timeout("inv", &["completed"], timeout)
                        .await;
                }
                "in-flight" => {
                    admin
                        .await_in_flight_on_with_timeout(&Target::object("Svc", "key"), 1, timeout)
                        .await;
                }
                "drain" => admin.drain_with_timeout(timeout).await,
                "pause" => admin.pause_with_timeout("inv", timeout).await,
                "purge" => admin.purge_with_timeout("inv", timeout).await,
                "register" => {
                    admin
                        .register_with_timeout("http://127.0.0.1:1234", timeout)
                        .await;
                }
                _ => unreachable!(),
            }
        });
        waits.push((operation, wait));
    }
    tokio::time::timeout(Duration::from_secs(5), async {
        for (operation, wait) in waits {
            let error = wait.await.expect_err("observation times out");
            let message = error.to_string();
            assert!(message.contains("deadline passed after 50ms"), "{message}");
            let context: &[&str] = match operation {
                "status" => &["await invocation inv", "completed"],
                "in-flight" => &["at least 1 invocation(s)", "Svc", "key", "Unscoped"],
                "drain" => &["drain invocations", admin.base()],
                "pause" => &["await invocation inv", "paused"],
                "purge" => &["purge invocation inv"],
                "register" => &["register endpoint http://127.0.0.1:1234"],
                _ => unreachable!(),
            };
            for part in context {
                assert!(message.contains(part), "{operation}: {message}");
            }
        }
    })
    .await
    .expect("custom deadlines replace the 30/60-second defaults");
    server.verify().await;
}

#[tokio::test]
async fn status_observation_distinguishes_absent_and_malformed_rows() {
    use serde_json::json;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

    let server = MockServer::start().await;
    let admin = Admin::new(
        server.uri(),
        reqwest::Client::builder()
            .tls_certs_only(std::iter::empty())
            .build()
            .expect("loopback client"),
    );
    for row in [json!({}), json!({"status": null}), json!({"status": 42})] {
        Mock::given(path("/query"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": [row]})))
            .expect(1)
            .mount(&server)
            .await;
        let admin = admin.clone();
        let error =
            tokio::spawn(async move { admin.await_status("inv-malformed", &["completed"]).await })
                .await
                .expect_err("malformed status fails at decoding, not at the deadline");
        let message = error.to_string();
        assert!(message.contains("inv-malformed"), "{message}");
        assert!(message.contains("missing or malformed status"), "{message}");
        server.verify().await;
        server.reset().await;
    }

    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": []})))
        .expect(1)
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rows": [{"status": "future-status"}]
        })))
        .expect(1)
        .with_priority(2)
        .mount(&server)
        .await;
    assert_eq!(
        admin.await_status("inv-later", &["future-status"]).await,
        "future-status",
        "an absent row is retried, and unknown string statuses stay observable"
    );
    server.verify().await;
}

#[tokio::test]
async fn empty_status_selections_are_refused_before_querying() {
    let server = wiremock::MockServer::start().await;
    let admin = Admin::new(
        server.uri(),
        reqwest::Client::builder()
            .tls_certs_only(std::iter::empty())
            .build()
            .expect("loopback client"),
    );
    for statuses in [vec![], vec![""], vec!["completed", ""]] {
        let admin = admin.clone();
        let error = tokio::spawn(async move { admin.await_status("inv-empty", &statuses).await })
            .await
            .expect_err("empty requested statuses cannot match absent evidence");
        let message = error.to_string();
        assert!(message.contains("inv-empty"), "{message}");
        assert!(
            message.contains("expected statuses must be non-empty"),
            "{message}"
        );
    }
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}

#[tokio::test]
async fn a_status_observation_can_outlast_thirty_seconds_with_an_explicit_timeout() {
    for custom in [false, true] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("admin listener");
        let base = format!("http://{}", listener.local_addr().expect("address"));
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("admin query");
            let mut buffer = [0; 4096];
            assert!(socket.read(&mut buffer).await.expect("request") > 0);
            let body = r#"{"rows":[{"status":"completed"}]}"#;
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                )
                .await
                .expect("headers");
            entered.send(()).expect("test waiting");
            released.await.expect("release body");
            let _ = socket.write_all(body.as_bytes()).await;
        });
        let admin = Admin::new(
            base,
            reqwest::Client::builder()
                .tls_certs_only(std::iter::empty())
                .build()
                .expect("loopback client"),
        );
        let observing = tokio::spawn(async move {
            if custom {
                admin
                    .await_status_with_timeout("inv", &["completed"], Duration::from_secs(60))
                    .await
            } else {
                admin.await_status("inv", &["completed"]).await
            }
        });
        waiting.await.expect("query is in flight");
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        // Resume before network I/O so Tokio cannot auto-advance to the next
        // observation deadline while the OS is delivering the response body.
        tokio::time::resume();
        if custom {
            assert!(
                !observing.is_finished(),
                "the explicit deadline outlasts the default"
            );
            release.send(()).expect("release custom query");
            assert_eq!(
                observing.await.expect("longer observation succeeds"),
                "completed"
            );
        } else {
            let error = observing
                .await
                .expect_err("default deadline cuts a stalled body");
            assert!(error.to_string().contains("deadline passed"), "{error}");
            release.send(()).expect("release default query");
        }
        server.await.expect("controlled admin server");
    }
}
