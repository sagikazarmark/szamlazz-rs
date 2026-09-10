//! Local endpoint ownership and URL normalization, with a mock admin boundary.

#![cfg(unix)]
#![allow(missing_docs)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use restate_e2e_harness::{Call, Launcher, ServerSpec};
use restate_sdk::prelude::*;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const SERVER: ServerSpec = ServerSpec {
    name: "local-endpoints",
    features: &[],
    env: &[],
};

async fn reused(server: &MockServer, prefix: &str, suffix: &str) -> restate_e2e_harness::Restate {
    for (route, body) in [
        ("health", json!({})),
        ("version", json!({"features": {}})),
        ("query", json!({"rows": []})),
    ] {
        Mock::given(path(format!("{prefix}/{route}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
    let base = format!("{}{prefix}{suffix}", server.uri());
    Launcher::Reuse {
        admin: base.clone(),
        ingress: base,
        endpoint_host: "127.0.0.1".into(),
    }
    .launch(&SERVER)
    .await
}

async fn await_closed(ports: &[u16]) {
    tokio::time::timeout(Duration::from_secs(5), async {
        for port in ports {
            while tokio::net::TcpStream::connect(("127.0.0.1", *port))
                .await
                .is_ok()
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    })
    .await
    .expect("owned endpoints stop while the runtime remains alive");
}

#[tokio::test]
async fn base_urls_accept_trailing_slashes_and_preserve_path_prefixes() {
    let server = MockServer::start().await;
    for prefix in ["", "/proxy/restate"] {
        for suffix in ["", "/", "///"] {
            let restate = reused(&server, prefix, suffix).await;
            let expected = format!("{}{prefix}", server.uri());
            assert_eq!(restate.admin_url(), expected);
            assert_eq!(restate.ingress_url(), expected);
            Mock::given(method("POST"))
                .and(path(format!("{prefix}/restate/call/Svc/h")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!("ok")))
                .mount(&server)
                .await;
            let reply = restate.invoke(&Call::service("Svc", "h"), None, None).await;
            assert_eq!(reply.status, 200);
            assert_eq!(reply.body, json!("ok"));
            server.reset().await;
        }
    }
}

#[tokio::test]
async fn dropping_restate_stops_all_its_endpoints_but_not_another_handles() {
    let server = MockServer::start().await;
    let restate = reused(&server, "", "").await;
    let other = reused(&server, "", "").await;
    Mock::given(path("/deployments"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    let mut ports = Vec::new();
    for _ in 0..2 {
        let deployment = restate.deploy(Endpoint::builder().build()).await;
        ports.push(deployment.port);
        drop(deployment);
    }
    let other_deployment = other.deploy(Endpoint::builder().build()).await;
    for port in &ports {
        assert!(
            tokio::net::TcpStream::connect(("127.0.0.1", *port))
                .await
                .is_ok(),
            "discarding a descriptor keeps the endpoint alive"
        );
    }
    drop(restate);
    await_closed(&ports).await;
    assert!(
        tokio::net::TcpStream::connect(("127.0.0.1", other_deployment.port))
            .await
            .is_ok()
    );
    drop(other);
    await_closed(&[other_deployment.port]).await;
}

#[tokio::test]
async fn cancelling_registration_keeps_the_endpoint_owned_for_teardown() {
    let server = MockServer::start().await;
    let restate = reused(&server, "", "").await;
    Mock::given(path("/deployments"))
        .respond_with(ResponseTemplate::new(201).set_delay(Duration::from_secs(30)))
        .mount(&server)
        .await;
    let port = {
        let deploying = restate.deploy(Endpoint::builder().build());
        tokio::pin!(deploying);
        tokio::select! {
            _ = &mut deploying => panic!("registration must remain pending"),
            port = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    for request in server.received_requests().await.expect("requests") {
                        if request.url.path() == "/deployments" {
                            let body: serde_json::Value = serde_json::from_slice(&request.body).expect("registration JSON");
                            return body["uri"].as_str().expect("uri").rsplit_once(':').expect("port").1.parse::<u16>().expect("port number");
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }) => port.expect("registration reached admin"),
        }
    };
    drop(restate);
    await_closed(&[port]).await;
}

struct Active(Arc<AtomicUsize>);

impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct HeldService {
    active: Arc<AtomicUsize>,
    release: Arc<tokio::sync::Notify>,
}

#[restate_sdk::service(name = "HeldService")]
impl HeldService {
    #[handler]
    async fn wait(&self, _ctx: Context<'_>) -> HandlerResult<String> {
        self.active.fetch_add(1, Ordering::SeqCst);
        let _active = Active(self.active.clone());
        self.release.notified().await;
        Ok("drained".into())
    }
}

#[tokio::test]
async fn shutdown_releases_active_handlers_after_the_drain_deadline() {
    draining_handler(false).await;
}

#[tokio::test]
async fn shutdown_allows_active_handlers_to_finish_during_the_drain_window() {
    draining_handler(true).await;
}

/// Protocol v6 Start: 16-byte id, debug id, one known entry, random seed.
/// Followed by Input with an empty value.
fn invocation_frames() -> Vec<u8> {
    let mut start = vec![0x0a, 16];
    start.extend_from_slice(&[1; 16]);
    start.extend_from_slice(&[0x12, 4, b't', b'e', b's', b't', 0x18, 1, 0x48, 1]);
    let mut frames = Vec::new();
    for (kind, payload) in [(0u16, start.as_slice()), (0x0400, &[0x72, 0][..])] {
        frames.extend_from_slice(&kind.to_be_bytes());
        frames.extend_from_slice(&0u16.to_be_bytes());
        frames.extend_from_slice(
            &u32::try_from(payload.len())
                .expect("frame length")
                .to_be_bytes(),
        );
        frames.extend_from_slice(payload);
    }
    frames
}

async fn draining_handler(complete: bool) {
    let server = MockServer::start().await;
    let restate = reused(&server, "", "").await;
    Mock::given(path("/deployments"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;
    let active = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Notify::new());
    let deployment = restate
        .deploy(
            Endpoint::builder()
                .bind(HeldService {
                    active: active.clone(),
                    release: release.clone(),
                })
                .build(),
        )
        .await;
    let socket = tokio::net::TcpStream::connect(("127.0.0.1", deployment.port))
        .await
        .expect("endpoint socket");
    let (mut client, connection) = h2::client::handshake(socket)
        .await
        .expect("HTTP/2 handshake");
    let mut driver = tokio::spawn(connection);
    let request = http::Request::builder()
        .method("POST")
        .uri(format!("{}/invoke/HeldService/wait", deployment.uri))
        .header("content-type", "application/vnd.restate.invocation.v6")
        .body(())
        .expect("invocation request");
    let (response, mut send) = client
        .send_request(request, false)
        .expect("open invocation stream");
    // Keep the invocation stream open like Restate.
    send.send_data(invocation_frames().into(), false)
        .expect("start invocation");
    let mut response = tokio::time::timeout(Duration::from_secs(5), response)
        .await
        .expect("SDK answers")
        .expect("response");
    assert_eq!(response.status(), 200);
    tokio::time::timeout(Duration::from_secs(5), async {
        while active.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual handler entered");
    drop(restate);
    await_closed(&[deployment.port]).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        active.load(Ordering::SeqCst),
        1,
        "shutdown first allows active work to drain"
    );
    if complete {
        send.send_data(Vec::new().into(), true)
            .expect("end request stream");
        release.notify_one();
        let output = tokio::time::timeout(Duration::from_secs(5), async {
            let mut output = Vec::new();
            while let Some(data) = response.body_mut().data().await {
                output.extend_from_slice(&data.expect("successful drained response"));
            }
            output
        })
        .await
        .expect("handler completes within the drain window");
        assert!(
            output
                .windows(b"drained".len())
                .any(|part| part == b"drained"),
            "successful handler result is delivered"
        );
    }
    // Keep polling the actual connection; paused-time auto-advance can otherwise
    // skip past shutdown before the accept loop has observed its signal.
    tokio::time::timeout(Duration::from_secs(12), async {
        while active.load(Ordering::SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("shutdown releases active work even if the client holds the stream open");
    tokio::time::timeout(Duration::from_secs(5), &mut driver)
        .await
        .expect("the existing connection terminates on shutdown")
        .expect("client connection task")
        .expect("HTTP/2 connection closes");
    drop((response, send, client));
}
