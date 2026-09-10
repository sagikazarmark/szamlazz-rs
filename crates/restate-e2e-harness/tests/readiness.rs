//! Readiness observes the owned child even when an independent admin answers.

#![cfg(unix)]

use std::time::Duration;

use restate_e2e_harness::{Launcher, ServerSpec};
use serde_json::json;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

const SERVER: ServerSpec = ServerSpec {
    name: "readiness",
    features: &[],
    env: &[],
};

// These targets deliberately have no non-reaping child-exit inspection.
#[cfg(not(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
)))]
mod exits {
    use super::*;
    use std::io::{Read, Write};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;
    use wiremock::{Request, Respond};

    #[test]
    fn readiness_child() {
        let Ok(control) = std::env::var("READINESS_CONTROL") else {
            return;
        };
        let mut stream = std::net::TcpStream::connect(control).expect("parent control listener");
        writeln!(
            stream,
            "{}",
            std::env::var("RESTATE_ADMIN__BIND_ADDRESS").expect("admin address")
        )
        .expect("report selected port");
        stream.read_exact(&mut [0]).expect("parent requests exit");
        eprintln!("controlled exit during readiness");
        std::process::exit(23);
    }

    struct ExitOnProbe(Mutex<std::net::TcpStream>);

    impl Respond for ExitOnProbe {
        fn respond(&self, _: &Request) -> ResponseTemplate {
            let _ = self.0.lock().expect("control stream").write_all(&[1]);
            ResponseTemplate::new(200)
                .set_body_json(json!({"rows": []}))
                .set_delay(Duration::from_millis(200))
        }
    }

    async fn exit_during(route: &'static str) {
        use tokio::io::{AsyncBufReadExt, BufReader};

        let root =
            std::env::temp_dir().join(format!("harness-readiness-{}-{route}", std::process::id()));
        std::fs::create_dir(&root).expect("exclusive test directory");
        let wrapper = root.join("server");
        std::fs::write(
            &wrapper,
            "#!/bin/sh\nexec \"$READINESS_EXE\" --exact exits::readiness_child --nocapture\n",
        )
        .expect("fake server executable");
        std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700))
            .expect("executable");
        let control = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("control listener");
        let exe = std::env::current_exe().expect("test executable");
        let env = [
            format!(
                "READINESS_CONTROL={}",
                control.local_addr().expect("control address")
            ),
            format!("READINESS_EXE={}", exe.display()),
        ];
        let spec = ServerSpec {
            env: Box::leak(
                env.into_iter()
                    .map(|pair| &*Box::leak(pair.into_boxed_str()))
                    .collect(),
            ),
            ..SERVER
        };
        let serving = tokio::spawn(async move {
            let (stream, _) = control.accept().await.expect("child reports port");
            let mut stream = BufReader::new(stream);
            let mut address = String::new();
            stream.read_line(&mut address).await.expect("port line");
            let listener =
                std::net::TcpListener::bind(address.trim()).expect("independent admin listener");
            let server = MockServer::builder().listener(listener).start().await;
            for (name, body) in [
                ("health", json!({})),
                ("query", json!({"rows": []})),
                ("version", json!({})),
            ] {
                if name != route {
                    Mock::given(path(format!("/{name}")))
                        .respond_with(ResponseTemplate::new(200).set_body_json(body))
                        .mount(&server)
                        .await;
                }
            }
            let stream = stream.into_inner().into_std().expect("control stream");
            Mock::given(path(format!("/{route}")))
                .respond_with(ExitOnProbe(Mutex::new(stream)))
                .mount(&server)
                .await;
            server
        });
        let launch = tokio::spawn(async move {
            Launcher::Binary {
                binary: wrapper,
                endpoint_host: "127.0.0.1".into(),
            }
            .launch(&spec)
            .await
        });
        let server = serving.await.expect("independent admin");
        let result = tokio::time::timeout(Duration::from_secs(5), launch)
            .await
            .expect("prompt child-exit diagnosis");
        let panic = result
            .expect_err("a dead child cannot be ready")
            .into_panic();
        let message = panic.downcast::<String>().expect("exit diagnostic");
        assert!(
            message.contains("controlled exit during readiness"),
            "{message}"
        );
        assert!(message.contains("exited"), "{message}");
        assert!(
            tokio::net::TcpStream::connect(server.address())
                .await
                .is_ok(),
            "independent server survives"
        );
        std::fs::remove_dir_all(root).expect("remove test wrapper");
    }

    #[tokio::test]
    async fn a_successful_health_probe_cannot_hide_a_dead_child() {
        exit_during("health").await;
    }

    #[tokio::test]
    async fn a_child_exiting_after_health_is_reported_during_sql_readiness() {
        exit_during("query").await;
    }

    #[tokio::test]
    async fn a_child_exiting_during_the_final_version_probe_is_reported() {
        exit_during("version").await;
    }
}

async fn await_request(server: &MockServer, route: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if server
                .received_requests()
                .await
                .expect("requests")
                .iter()
                .any(|request| request.url.path() == route)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("readiness request reached admin");
}

#[tokio::test]
async fn version_readiness_shares_the_launch_deadline() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let server = MockServer::start().await;
    let healthy = Arc::new(AtomicBool::new(false));
    let responding = healthy.clone();
    Mock::given(path("/health"))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(if responding.load(Ordering::SeqCst) {
                200
            } else {
                503
            })
        })
        .mount(&server)
        .await;
    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rows": []})))
        .mount(&server)
        .await;
    Mock::given(path("/version"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({}))
                .set_delay(Duration::from_secs(300)),
        )
        .mount(&server)
        .await;
    let launcher = Launcher::Reuse {
        admin: server.uri(),
        ingress: server.uri(),
        endpoint_host: "127.0.0.1".into(),
    };
    let launch = tokio::spawn(launcher.launch(&SERVER));
    await_request(&server, "/health").await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(40)).await;
    healthy.store(true, Ordering::SeqCst);
    tokio::time::resume();
    await_request(&server, "/version").await;
    tokio::time::pause();
    // Cross the original deadline, but not a fresh 90-second version deadline.
    tokio::time::advance(Duration::from_secs(51)).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert!(
        launch.is_finished(),
        "version must use the launch deadline, not a fresh HTTP timeout"
    );
    let panic = launch.await.expect_err("readiness expired").into_panic();
    let message = panic.downcast::<String>().expect("deadline diagnostic");
    assert!(message.contains("version"), "{message}");
}
