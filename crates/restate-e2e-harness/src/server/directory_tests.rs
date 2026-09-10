//! Exercise launch and teardown in an isolated process with its own TMPDIR.
//! The executable boundary supplies a small admin server; the allocation hook
//! places failure evidence at the first candidate before directory creation.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nix::{
    sys::signal::{Signal, killpg},
    unistd::Pid,
};
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::path};

use crate::{Launcher, ServerSpec};

const ROOT: &str = "RESTATE_DIRECTORY_TEST_DIR";
const SPEC: ServerSpec = ServerSpec {
    name: "directories",
    features: &[],
    env: &[],
};

/// Independent of Restate teardown: a regression must not leave test servers
/// alive or hold Cargo indefinitely. Failure evidence stays under `root`.
struct Suite {
    child: Child,
    root: PathBuf,
}

impl Drop for Suite {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        for entry in fs::read_dir(&self.root).expect("suite directory") {
            let path = entry.expect("entry").path();
            if path
                .extension()
                .is_some_and(|extension| extension == "group")
            {
                let group = fs::read_to_string(path).expect("server group");
                let _ = killpg(
                    Pid::from_raw(group.parse().expect("group pid")),
                    Signal::SIGKILL,
                );
            }
        }
    }
}

pub(super) fn collide(candidate: &Path) {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let marker = Path::new(&root).join("collision");
    if fs::File::create_new(&marker).is_err() {
        return;
    }
    fs::create_dir(candidate).expect("pre-existing launch candidate");
    fs::write(candidate.join("restate-server.log"), "earlier failure log").expect("earlier log");
    fs::write(candidate.join("data"), "earlier persisted state").expect("earlier data");
    fs::write(marker, candidate.as_os_str().as_encoded_bytes()).expect("collision path");
}

#[test]
fn launches_preserve_existing_failure_evidence() {
    let root = std::env::temp_dir().join(format!("restate-directory-test-{}", std::process::id()));
    fs::create_dir(&root).expect("exclusive subprocess directory");
    let binary = root.join("server");
    fs::write(
        &binary,
        "#!/bin/sh\nexec \"$RESTATE_DIRECTORY_TEST_EXE\" --exact server::directory_tests::server_process --nocapture\n",
    )
    .expect("controlled server executable");
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
    let executable = std::env::current_exe().expect("test executable");
    let log = fs::File::create_new(root.join("suite.log")).expect("suite log");
    let child = Command::new(&executable)
        .args([
            "--exact",
            "server::directory_tests::suite_process",
            "--nocapture",
        ])
        .env(ROOT, &root)
        .env("TMPDIR", &root)
        .env("RESTATE_DIRECTORY_TEST_EXE", &executable)
        .stdout(Stdio::from(log.try_clone().expect("suite log")))
        .stderr(Stdio::from(log))
        .spawn()
        .expect("isolated directory suite");
    let mut suite = Suite {
        child,
        root: root.clone(),
    };
    // On targets without non-reaping waitid, a failed launch is reported at
    // the readiness deadline rather than by early exit inspection.
    let deadline = Instant::now() + super::READY_DEADLINE + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = suite.child.try_wait().expect("suite exit status") {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "directory suite timed out; evidence at {}",
            root.display()
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    drop(suite);
    let output = fs::read_to_string(root.join("suite.log")).expect("suite output");
    if status.success() {
        fs::remove_dir_all(&root).expect("remove test fixtures");
    }
    assert!(
        status.success(),
        "directory suite failed; evidence at {}:\n{output}",
        root.display(),
    );
}

fn launch(root: &Path) -> impl Future<Output = crate::Restate> + use<> {
    Launcher::Binary {
        binary: root.join("server"),
        endpoint_host: "127.0.0.1".to_owned(),
    }
    .launch(&SPEC)
}

fn base_dir(root: &Path, restate: &crate::Restate) -> PathBuf {
    let address = restate
        .admin_url()
        .strip_prefix("http://")
        .expect("loopback URL");
    PathBuf::from(
        fs::read_to_string(root.join(format!("base-{address}"))).expect("server base dir"),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn suite_process() {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    let restate = launch(&root).await;
    let first = base_dir(&root, &restate);
    let earlier = fs::read_to_string(root.join("collision")).expect("collision path");
    let earlier = Path::new(&earlier);
    assert_eq!(
        fs::read_to_string(earlier.join("restate-server.log")).expect("earlier log survives"),
        "earlier failure log"
    );
    assert_eq!(
        fs::read_to_string(earlier.join("data")).expect("earlier data survives"),
        "earlier persisted state"
    );
    drop(restate);
    assert!(!first.exists(), "normal teardown removes its own directory");
    assert!(
        earlier.exists(),
        "teardown must not delete an earlier launch"
    );

    // Two actual launching threads, using the same spec, retain separate data.
    let (left, right) = tokio::join!(tokio::spawn(launch(&root)), tokio::spawn(launch(&root)),);
    let left = left.expect("left launch");
    let right = right.expect("right launch");
    let left_dir = base_dir(&root, &left);
    let right_dir = base_dir(&root, &right);
    assert_ne!(left_dir, right_dir);
    assert_ne!(first, left_dir);
    assert_ne!(first, right_dir);
    let right_data = fs::read(right_dir.join("data")).expect("right data");
    drop(left);
    assert!(!left_dir.exists());
    assert_eq!(
        fs::read(right_dir.join("data")).expect("right survives"),
        right_data
    );
    assert!(
        tokio::spawn(async move {
            let _restate = right;
            panic!("test failure after launch");
        })
        .await
        .expect_err("test panics")
        .is_panic()
    );
    assert_eq!(
        fs::read(right_dir.join("data")).expect("panic retains data"),
        right_data
    );

    cancelled_startup(&root).await;
    finish_evidence(&root).await;

    // An executable that exits before readiness also retains its own evidence.
    fs::write(root.join("fail-launch"), "").expect("fail next server startup");
    assert!(
        tokio::spawn(launch(&root))
            .await
            .expect_err("launch fails")
            .is_panic()
    );
    fs::remove_file(root.join("fail-launch")).expect("allow later launches");
    let failed = PathBuf::from(fs::read_to_string(root.join("failed-base")).expect("failed base"));
    let failed_log = fs::read(failed.join("restate-server.log")).expect("failed log retained");
    assert!(String::from_utf8_lossy(&failed_log).contains("controlled startup failure"));
    let failed_data = fs::read(failed.join("data")).expect("failed data retained");
    let later = launch(&root).await;
    let later_dir = base_dir(&root, &later);
    assert_ne!(later_dir, failed);
    assert_ne!(later_dir, right_dir);
    drop(later);
    assert!(!later_dir.exists());
    assert_eq!(
        fs::read(failed.join("restate-server.log")).expect("failed log survives"),
        failed_log
    );
    assert_eq!(
        fs::read(failed.join("data")).expect("failed data survives"),
        failed_data
    );
    assert_eq!(
        fs::read(right_dir.join("data")).expect("panic data survives"),
        right_data
    );
    assert_eq!(
        fs::read_to_string(earlier.join("restate-server.log")).expect("earlier log survives"),
        "earlier failure log"
    );
    assert_eq!(
        fs::read_to_string(earlier.join("data")).expect("earlier data survives"),
        "earlier persisted state"
    );
}

// An externally bounded startup is dropped without thread unwinding.
// Both timeout and task abortion must stop the child and preserve evidence.
async fn cancelled_startup(root: &Path) {
    for abort in [false, true] {
        fs::write(root.join("hold-launch"), "").expect("hold next startup");
        let mut launching = Box::pin(launch(root));
        let held = tokio::select! {
            _ = &mut launching => panic!("startup must remain pending"),
            held = tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if let Ok(base) = fs::read_to_string(root.join("held-base")) {
                        break PathBuf::from(base);
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }) => held.expect("controlled server is waiting before readiness"),
        };
        let group = fs::read_to_string(held.join("group")).expect("held process group");
        let group = Pid::from_raw(group.parse().expect("group pid"));
        if abort {
            let launching = tokio::spawn(launching);
            launching.abort();
            assert!(launching.await.expect_err("aborted startup").is_cancelled());
        } else {
            assert!(
                tokio::time::timeout(Duration::from_millis(10), launching)
                    .await
                    .is_err()
            );
        }
        assert!(
            held.join("restate-server.log").exists(),
            "cancelled launch retains log"
        );
        assert!(held.join("data").exists(), "cancelled launch retains data");
        assert_eq!(
            nix::sys::signal::kill(group, None),
            Err(nix::errno::Errno::ESRCH),
            "child killed and reaped"
        );
        fs::remove_file(root.join("hold-launch")).expect("allow startup");
        fs::remove_file(root.join("held-base")).expect("remove startup marker");
    }
}

async fn finish_evidence(root: &Path) {
    for collect in [false, true] {
        let restate = launch(root).await;
        let base = base_dir(root, &restate);
        let group = restate.process.as_ref().expect("owned process").group();
        // Cancelling before the first poll must retain evidence too.
        if collect {
            drop(restate.finish_with_failures());
        } else {
            drop(restate.finish());
        }
        assert!(
            base.join("restate-server.log").exists(),
            "cancelled finish retains log"
        );
        assert_eq!(
            nix::sys::signal::kill(Pid::from_raw(i32::try_from(group).expect("pid")), None),
            Err(nix::errno::Errno::ESRCH)
        );
    }

    let restate = launch(root).await;
    let base = base_dir(root, &restate);
    let failures = super::endpoint::Failures::default();
    restate
        .endpoints
        .lock()
        .expect("endpoints")
        .push(super::LocalEndpoint {
            uri: "http://test-endpoint".into(),
            stop: None,
            task: tokio::spawn(async { panic!("deliberate serving failure") }),
            failures,
        });
    let failures = restate.finish_with_failures().await;
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message.contains("deliberate serving failure"));
    assert!(
        base.join("restate-server.log").exists(),
        "collected endpoint failures retain logs"
    );
}

/// The executable the launcher starts, implementing just its readiness protocol.
#[tokio::test]
async fn server_process() {
    let Ok(address) = std::env::var("RESTATE_ADMIN__BIND_ADDRESS") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os(ROOT).expect("isolated test directory"));
    let group = std::process::id().to_string();
    fs::write(root.join(format!("{group}.group")), &group)
        .expect("record independent cleanup group");
    let base = PathBuf::from(std::env::var_os("RESTATE_BASE_DIR").expect("server base"));
    assert!(
        !base.join("data").exists(),
        "every server starts with fresh storage"
    );
    fs::write(base.join("data"), &address).expect("server persisted state");
    fs::write(base.join("group"), &group).expect("server pid");
    fs::write(
        root.join(format!("base-{address}")),
        base.as_os_str().as_encoded_bytes(),
    )
    .expect("report base");
    if root.join("hold-launch").exists() {
        eprintln!("controlled startup held before readiness");
        fs::write(root.join("held-base"), base.as_os_str().as_encoded_bytes()).expect("held base");
        std::future::pending::<()>().await;
    }
    if root.join("fail-launch").exists() {
        fs::write(
            root.join("failed-base"),
            base.as_os_str().as_encoded_bytes(),
        )
        .expect("failed base");
        panic!("controlled startup failure");
    }
    let listener = std::net::TcpListener::bind(address).expect("admin listener");
    let server = MockServer::builder().listener(listener).start().await;
    Mock::given(path("/health"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(path("/version"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"features": {}})))
        .mount(&server)
        .await;
    Mock::given(path("/query"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"rows": []})))
        .mount(&server)
        .await;
    std::future::pending::<()>().await;
}
