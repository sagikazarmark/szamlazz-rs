//! `--check-config` loads and validates the configuration, builds the
//! endpoint and exits without listening — what a CI job or an init container
//! runs before the real process starts.
//!
//! Spawns `restate-szamlazz --check-config` on the fixtures and on a broken
//! file, with an otherwise empty environment, and asserts the exit status and
//! what was printed.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const BINARY: &str = env!("CARGO_BIN_EXE_restate-szamlazz");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures");

/// How long the binary gets to check a configuration and exit. It never
/// listens under `--check-config`, so an exit that does not come is the
/// failure this bound turns into a message.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The last line `--check-config` logs before exiting 0.
const VALID: &str = "configuration is valid; not listening (--check-config)";
/// The line the binary logs once it listens — which `--check-config` never
/// reaches.
const STARTED: &str = "starting Restate szamlazz.hu endpoint";

/// The single-account fixture — the README's example — is valid: exit 0, the
/// start-up summary (namespace, shape, account, bound services, identity
/// keys) on stdout, and no listening.
#[test]
fn a_valid_single_account_file_exits_0_and_prints_the_start_up_summary() {
    let output = check_config(&fixture("single.toml"), &[]);

    assert_eq!(output.status.code(), Some(0), "{output}");
    assert!(output.stdout.contains(VALID), "{output}");
    assert!(
        output
            .stdout
            .contains("loaded szamlazz.hu account configuration")
            && output.stdout.contains("namespace=acct")
            && output.stdout.contains("scoped=false"),
        "the summary names the namespace and the shape: {output}"
    );
    assert!(
        output
            .stdout
            .contains(r#"scope="<unscoped>" account=acme mode=Live"#),
        "the summary lists the account: {output}"
    );
    assert!(
        output.stdout.contains("service=Szamlazz.Order")
            && output.stdout.contains("service=Szamlazz.Agent"),
        "the summary lists the bound services: {output}"
    );
    assert!(
        output
            .stdout
            .contains("request identity verification enabled"),
        "the fixture's identity key was accepted: {output}"
    );
    assert!(
        !output.stdout.contains(STARTED),
        "--check-config must not listen: {output}"
    );
}

/// The multi-account fixture is valid too, and its summary lists each
/// account under its scope.
#[test]
fn a_valid_multi_account_file_exits_0_and_lists_every_scope() {
    let output = check_config(&fixture("multi.toml"), &[]);

    assert_eq!(output.status.code(), Some(0), "{output}");
    assert!(output.stdout.contains("scoped=true"), "{output}");
    assert!(
        output.stdout.contains(r#"scope="acme" account=acme"#)
            && output
                .stdout
                .contains(r#"scope="beta_events" account=beta"#),
        "{output}"
    );
    assert!(output.stdout.contains(VALID), "{output}");
}

/// An unknown key — in the file or in the environment — is a non-zero exit
/// with an error that names the key, its path and its source.
#[test]
fn an_unknown_key_exits_non_zero_with_the_error() {
    let file = temp_file(
        "unknown-key.toml",
        r#"
        namespace = "acct"

        [account]
        id = "acme"
        agent_key = "k"
        mod = "test"
        "#,
    );

    let output = check_config(&file, &[("RESTATE_SZAMLAZZ_ISUE__MAX_ATTEMPTS", "1")]);

    assert_ne!(output.status.code(), Some(0), "{output}");
    assert!(
        output.stderr.contains("unknown key `account.mod`"),
        "{output}"
    );
    assert!(output.stderr.contains("unknown-key.toml"), "{output}");
    assert!(
        output
            .stderr
            .contains("unknown key `isue` (RESTATE_SZAMLAZZ_ISUE__MAX_ATTEMPTS)"),
        "{output}"
    );
    assert!(!output.stdout.contains(VALID), "{output}");
}

/// An issue policy whose `initial_delay` is under the floor — the Számla Agent
/// client's timeout plus a margin, so that a create or storno step is never
/// re-executed while its send may still be in flight — is refused before the
/// endpoint starts: the file parses; it is the invariant that fails, and the
/// error names the table and the floor.
#[test]
fn an_issue_delay_below_the_floor_exits_non_zero_with_the_rule() {
    let file = temp_file(
        "issue-delay-floor.toml",
        r#"
        namespace = "acct"

        [issue]
        initial_delay = "5s"

        [account]
        id = "acme"
        agent_key = "k"
        "#,
    );

    let output = check_config(&file, &[]);

    assert_ne!(output.status.code(), Some(0), "{output}");
    assert!(
        output
            .stderr
            .contains("issue.initial_delay (5s) must be at least 90s"),
        "{output}"
    );
    assert!(!output.stdout.contains(VALID), "{output}");
}

/// The endpoint is built too, so an account or identity-key error the loader
/// cannot see surfaces here rather than at start-up.
#[test]
fn an_invalid_identity_key_exits_non_zero_with_the_error() {
    let file = temp_file(
        "identity-key.toml",
        r#"
        identity_keys = ["not-a-key"]
        namespace = "acct"

        [account]
        id = "acme"
        agent_key = "k"
        "#,
    );

    let output = check_config(&file, &[]);

    assert_ne!(output.status.code(), Some(0), "{output}");
    assert!(output.stderr.contains("not-a-key"), "{output}");
}

/// A `--config` path that does not exist is refused before anything is read.
#[test]
fn a_missing_file_exits_non_zero_with_the_error() {
    let output = check_config(Path::new("/nonexistent/restate-szamlazz.toml"), &[]);

    assert_ne!(output.status.code(), Some(0), "{output}");
    assert!(output.stderr.contains("config file not found"), "{output}");
}

fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURES).join(name)
}

/// Writes `contents` under cargo's per-crate temporary directory and returns
/// the path.
fn temp_file(name: &str, contents: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, contents).expect("the temporary file should be written");
    path
}

/// Runs `restate-szamlazz --check-config --config {file}` with an empty
/// environment plus `env`, and returns what it printed and how it exited.
fn check_config(file: &Path, env: &[(&str, &str)]) -> Checked {
    let mut command = Command::new(BINARY);
    command
        .args(["--check-config", "--config"])
        .arg(file)
        .env_clear()
        .env("RUST_LOG", "info")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("the endpoint binary should spawn");

    // The pipes are drained on threads while the parent polls for the exit,
    // so a binary that wrongly listens is killed and reported rather than
    // hanging the test — and a chatty one cannot block on a full pipe.
    let stdout = drain(child.stdout.take().expect("stdout is piped"));
    let stderr = drain(child.stderr.take().expect("stderr is piped"));
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(status) = child.try_wait().expect("try_wait should succeed") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "--check-config did not exit within {TIMEOUT:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
                stdout.join().expect("the reader thread should finish"),
                stderr.join().expect("the reader thread should finish"),
            );
        }
        thread::sleep(Duration::from_millis(20));
    };
    Checked {
        status,
        stdout: stdout.join().expect("the reader thread should finish"),
        stderr: stderr.join().expect("the reader thread should finish"),
    }
}

/// Reads `pipe` to its end on a thread and hands the text back on `join`.
fn drain<R: Read + Send + 'static>(mut pipe: R) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// A finished `--check-config` run.
struct Checked {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

impl std::fmt::Display for Checked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "exit {:?}\n--- stdout ---\n{}\n--- stderr ---\n{}",
            self.status, self.stdout, self.stderr
        )
    }
}
