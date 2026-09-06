//! The binary stops cleanly on the signals a container runtime sends.
//!
//! Spawns `restate-szamlazz` on an ephemeral port (`--port 0`) with a
//! minimal environment-only configuration, waits for its start-up log line,
//! sends the signal — from this process, with `kill(2)`, so the test does not
//! depend on a `kill` executable the test image may not ship — and asserts a
//! prompt exit with status 0 — what `docker stop`, a Kubernetes rollout or
//! `kill -TERM` expect. Unix only: the signals are.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

const BINARY: &str = env!("CARGO_BIN_EXE_restate-szamlazz");

/// The default bind address, every interface.
const ANY: &str = "0.0.0.0";

/// The message of the start-up log line; it follows the bind and the signal
/// handlers, so once it is out a stop is honoured.
const STARTED: &str = "starting Restate szamlazz.hu endpoint";

/// How long the binary gets to log its start-up line.
const START_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the binary gets to exit after the signal. With nothing in
/// flight the SDK's drain completes at once, so this is generous.
const STOP_TIMEOUT: Duration = Duration::from_secs(5);

#[test]
fn sigterm_stops_the_endpoint_with_status_0() {
    let stopped = Endpoint::start(ANY).stop(Signal::SIGTERM);

    stopped.assert_status_0();
    assert!(
        stopped.log.contains(r#"signal="SIGTERM""#),
        "the stop should log which signal arrived:\n{}",
        stopped.log
    );
}

#[test]
fn sigint_stops_the_endpoint_with_status_0() {
    let stopped = Endpoint::start(ANY).stop(Signal::SIGINT);

    stopped.assert_status_0();
    assert!(
        stopped.log.contains(r#"signal="SIGINT""#),
        "the stop should log which signal arrived:\n{}",
        stopped.log
    );
}

/// The start-up log line names the bound address — the port the kernel
/// picked, since `--port 0` asked for one — and the signals that stop the
/// process.
#[test]
fn the_start_up_log_names_the_bound_address_and_the_stop_signals() {
    let endpoint = Endpoint::start(ANY);
    let line = endpoint.start_line();

    assert_ne!(endpoint.port(), 0, "an ephemeral port was bound: {line}");
    assert!(
        line.contains(r#"stop_on="SIGTERM, SIGINT""#),
        "the start-up line should name the stop signals: {line}"
    );

    endpoint.stop(Signal::SIGTERM).assert_status_0();
}

/// `--bind` replaces the default `0.0.0.0`: the start-up line names the
/// address that was asked for.
#[test]
fn bind_selects_the_address() {
    let endpoint = Endpoint::start("127.0.0.1");
    let line = endpoint.start_line();

    assert_ne!(endpoint.port(), 0, "bound on 127.0.0.1: {line}");

    endpoint.stop(Signal::SIGTERM).assert_status_0();
}

/// A running endpoint: the address it was asked to bind, the child, its
/// stdout lines as a reader thread hands them over, and the lines seen so far
/// (the last one is the start-up line).
struct Endpoint {
    bind: &'static str,
    child: Child,
    lines: Receiver<String>,
    log: Vec<String>,
}

/// An endpoint that has exited: its status and everything it logged.
struct Stopped {
    status: ExitStatus,
    log: String,
}

impl Endpoint {
    /// Spawns the binary on `bind` and an ephemeral port with an
    /// environment-only configuration and waits for its start-up line; fails
    /// with the log when the process exits first or [`START_TIMEOUT`] passes.
    fn start(bind: &'static str) -> Self {
        let mut child = Command::new(BINARY)
            .args(["--bind", bind, "--port", "0"])
            .env_clear()
            .env("RUST_LOG", "info")
            .env("NO_COLOR", "1")
            .env("RESTATE_SZAMLAZZ_NAMESPACE", "acct")
            .env("RESTATE_SZAMLAZZ_ACCOUNT__ID", "acme")
            .env("RESTATE_SZAMLAZZ_ACCOUNT__AGENT_KEY", "agent-key")
            .env("RESTATE_SZAMLAZZ_ACCOUNT__MODE", "test")
            .env("RESTATE_SZAMLAZZ_ACCOUNT__ENDPOINT", "http://127.0.0.1:1/")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("the endpoint binary should spawn");

        let stdout = child.stdout.take().expect("stdout is piped");
        let (sender, lines) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });

        let mut endpoint = Self {
            bind,
            child,
            lines,
            log: Vec::new(),
        };
        let deadline = Instant::now() + START_TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match endpoint.lines.recv_timeout(remaining) {
                Ok(line) => {
                    let started = line.contains(STARTED);
                    endpoint.log.push(line);
                    if started {
                        return endpoint;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    let _ = endpoint.child.kill();
                    panic!(
                        "the endpoint did not start within {START_TIMEOUT:?}\n{}",
                        endpoint.log.join("\n")
                    );
                }
                Err(RecvTimeoutError::Disconnected) => {
                    let status = endpoint.child.wait().expect("the exited child");
                    panic!(
                        "the endpoint exited with {status:?} before starting\n{}",
                        endpoint.log.join("\n")
                    );
                }
            }
        }
    }

    fn start_line(&self) -> &str {
        self.log.last().expect("the start-up line was seen")
    }

    /// The port of the `addr={bind}:{port}` field of the start-up line, at
    /// the address the endpoint was asked to bind.
    fn port(&self) -> u16 {
        let line = self.start_line();
        let (_, rest) = line
            .split_once(&format!("addr={}:", self.bind))
            .unwrap_or_else(|| panic!("the start-up line should name the bound address: {line}"));
        rest.split_whitespace()
            .next()
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("the bound address should end in a port: {line}"))
    }

    /// Sends `signal` and returns the exit status and the whole log once the
    /// process has exited; fails when [`STOP_TIMEOUT`] passes first.
    fn stop(mut self, signal: Signal) -> Stopped {
        let pid = Pid::from_raw(
            i32::try_from(self.child.id()).expect("a pid fits in the type `kill(2)` takes"),
        );
        kill(pid, signal)
            .unwrap_or_else(|error| panic!("sending {signal} should succeed: {error}"));

        let deadline = Instant::now() + STOP_TIMEOUT;
        let status = loop {
            if let Some(status) = self.child.try_wait().expect("try_wait should succeed") {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                self.log.extend(self.lines.iter());
                panic!(
                    "the endpoint did not exit within {STOP_TIMEOUT:?} of {signal}\n{}",
                    self.log.join("\n")
                );
            }
            thread::sleep(Duration::from_millis(20));
        };

        // The process is gone, so the pipe ends and the reader thread with it.
        self.log.extend(self.lines.iter());
        Stopped {
            status,
            log: self.log.join("\n"),
        }
    }
}

impl Stopped {
    fn assert_status_0(&self) {
        assert_eq!(
            self.status.code(),
            Some(0),
            "the endpoint should exit with status 0, not {:?}\n{}",
            self.status,
            self.log
        );
    }
}
