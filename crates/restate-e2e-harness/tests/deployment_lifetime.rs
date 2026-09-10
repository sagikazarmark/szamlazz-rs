//! Local endpoint ownership and URL normalization, with a mock admin boundary.

#![cfg(unix)]

use std::time::Duration;

use restate_e2e_harness::{Call, Launcher, ServerSpec};
use restate_sdk::prelude::Endpoint;
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
