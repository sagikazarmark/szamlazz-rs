//! Real signals in isolated suite processes. File barriers control race windows;
//! the assertions observe process creation, exit status, and surviving processes.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;

use crate::{Launcher, ServerSpec};

const DEADLINE: Duration = Duration::from_secs(15);
const ROOT: &str = "RESTATE_LIFECYCLE_TEST_DIR";

fn wait_for(mut predicate: impl FnMut() -> bool, description: &str) {
    let deadline = Instant::now() + DEADLINE;
    while !predicate() {
        assert!(Instant::now() < deadline, "timed out: {description}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Compiled only into the unit-test executable. A barrier is disabled unless
/// this is the isolated suite process and its parent selected this exact point.
pub(super) fn pause(point: &str) {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let thread = std::thread::current();
    let name = format!(
        "{point}-{}",
        thread.name().expect("a named lifecycle thread")
    );
    let root = Path::new(&root);
    fs::write(root.join(format!("{name}.ready")), "").expect("announce lifecycle point");
    let paused = std::env::var("RESTATE_LIFECYCLE_PAUSES").unwrap_or_default();
    if paused.split(',').any(|paused| paused == name) {
        wait_for(
            || root.join(format!("{name}.release")).exists(),
            &format!("release {name}"),
        );
    }
    assert!(
        point != "register" || std::env::var_os("RESTATE_LIFECYCLE_FAIL_REGISTRATION").is_none(),
        "injected signal registration failure"
    );
}

pub(super) fn spawned(group: u32) {
    if let Some(root) = std::env::var_os(ROOT) {
        fs::write(
            Path::new(&root).join(format!("{group}.group")),
            group.to_string(),
        )
        .expect("record child for independent failure cleanup");
    }
    pause("spawned");
}

// The fake server is the external executable boundary. Its descendant stays in
// the inherited group, so killing just the leader does not satisfy the tests.
const SERVER: &str = r#"#!/bin/sh
sleep 300 &
descendant=$!
printf '%s %s\n' "$$" "$descendant" > "$RESTATE_LIFECYCLE_TEST_DIR/$RESTATE_NODE_NAME.pids"
wait
"#;

struct Suite {
    child: Child,
    root: PathBuf,
}

impl Suite {
    fn start(scenario: &str, signal: Signal, pauses: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "restate-lifecycle-{}-{scenario}-{signal}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("a unique subprocess directory");
        let binary = root.join("server");
        fs::write(&binary, SERVER).expect("fake server executable");
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).expect("executable");
        let log = fs::File::create(root.join("suite.log")).expect("suite log");
        let mut command = Command::new(std::env::current_exe().expect("unit-test executable"));
        if scenario == "registration-failure" {
            command.env("RESTATE_LIFECYCLE_FAIL_REGISTRATION", "1");
        }
        let child = command
            .args([
                "--exact",
                "server::lifecycle_tests::suite_process",
                "--nocapture",
            ])
            .env(ROOT, &root)
            .env("RESTATE_LIFECYCLE_SCENARIO", scenario)
            .env("TMPDIR", &root)
            .env("RESTATE_LIFECYCLE_PAUSES", pauses)
            .stdout(Stdio::from(log.try_clone().expect("suite log")))
            .stderr(if matches!(scenario, "stderr-failure" | "signal-stderr-failure") {
                Stdio::piped()
            } else {
                Stdio::from(log)
            })
            .spawn()
            .expect("isolated suite process");
        Self { child, root }
    }

    fn reached(&self, point: &str) -> bool {
        self.root.join(format!("{point}.ready")).exists()
    }

    fn await_point(&self, point: &str) {
        wait_for(|| self.reached(point), point);
    }

    fn release(&self, point: &str) {
        fs::write(self.root.join(format!("{point}.release")), "").expect("release barrier");
    }

    fn signal(&self, signal: Signal) {
        kill(pid(self.child.id()), signal).expect("signal the suite");
    }

    fn launch_second(&self) {
        fs::write(self.root.join("launch-second"), "").expect("request concurrent launch");
    }

    fn await_server(&self, name: &str) -> Vec<Pid> {
        let path = self.root.join(format!("e2e-{name}.pids"));
        wait_for(
            || fs::read_to_string(&path).is_ok_and(|text| text.split_whitespace().count() == 2),
            "server and descendant started",
        );
        fs::read_to_string(path)
            .expect("server pids")
            .split_whitespace()
            .map(|text| Pid::from_raw(text.parse().expect("a pid")))
            .collect()
    }

    fn exit(&mut self) -> ExitStatus {
        let mut status = None;
        wait_for(
            || {
                status = self.child.try_wait().expect("suite exit status");
                status.is_some()
            },
            "suite exits after signal cleanup",
        );
        status.expect("the suite exited")
    }

    fn assert_stopped(&mut self, status: i32, processes: &[Pid]) {
        assert_eq!(
            self.exit().code(),
            Some(status),
            "conventional signal status"
        );
        wait_for(
            || processes.iter().all(|&pid| !alive(pid)),
            "no server or descendant left alive",
        );
    }
}

fn pid(raw: u32) -> Pid {
    Pid::from_raw(i32::try_from(raw).expect("a pid fits i32"))
}

fn alive(pid: Pid) -> bool {
    if kill(pid, None).is_err() {
        return false;
    }
    // An orphan may await reaping by the host's init (notably in containers).
    // Zombies have exited; they cannot execute or hold the server's ports.
    let output = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("inspect process status");
    let status = String::from_utf8_lossy(&output.stdout);
    !status.trim().is_empty() && !status.trim().starts_with('Z')
}

impl Drop for Suite {
    fn drop(&mut self) {
        // Failure cleanup is independent of the code under test, so a red test
        // does not itself leave the leaked process group behind.
        let _ = self.child.kill();
        let _ = self.child.wait();
        for entry in fs::read_dir(&self.root).expect("subprocess directory") {
            let path = entry.expect("directory entry").path();
            if path.extension().is_some_and(|ext| ext == "group")
                && let Ok(text) = fs::read_to_string(path)
                && let Some(group) = text.split_whitespace().next()
            {
                let _ = killpg(
                    Pid::from_raw(group.parse().expect("group pid")),
                    Signal::SIGKILL,
                );
            }
        }
        if std::thread::panicking() {
            eprintln!(
                "suite log: {}",
                fs::read_to_string(self.root.join("suite.log")).unwrap_or_default()
            );
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Entry point re-executed by the parent tests, never a signal to Cargo's suite.
#[test]
fn suite_process() {
    let Some(root) = std::env::var_os(ROOT) else {
        return;
    };
    let root = PathBuf::from(root);
    if std::env::var("RESTATE_LIFECYCLE_SCENARIO")
        .is_ok_and(|scenario| scenario.starts_with("exit-observation"))
    {
        observe_exit(&root);
        return;
    }
    let launch = |name: &'static str| {
        let root = root.clone();
        std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("launch runtime");
                    runtime.block_on(
                        Launcher::Binary {
                            binary: root.join("server"),
                            endpoint_host: "127.0.0.1".to_owned(),
                        }
                        .launch(&ServerSpec {
                            name,
                            features: &[],
                            env: &[],
                        }),
                    )
                });
                if result.is_err() {
                    if std::env::var("RESTATE_LIFECYCLE_SCENARIO")
                        .is_ok_and(|scenario| scenario == "stderr-failure")
                    {
                        assert!(
                            super::started().groups.is_empty(),
                            "failed launch unregistered"
                        );
                    }
                    fs::write(root.join(format!("refused-{name}.ready")), "")
                        .expect("launch refused");
                }
            })
            .expect("launch thread");
    };
    launch("first");
    wait_for(
        || root.join("launch-second").exists(),
        "second launch requested",
    );
    launch("second");
    std::thread::park();
}

#[test]
fn failing_stderr_after_spawn_stops_and_reaps_the_process_group() {
    let mut suite = Suite::start("stderr-failure", Signal::SIGTERM, "spawned-first");
    suite.await_point("spawned-first");
    let processes = suite.await_server("first");
    // The first diagnostic succeeded. Break the sink before the post-spawn
    // diagnostic, while both the server and its descendant are alive.
    drop(suite.child.stderr.take().expect("piped stderr"));
    suite.release("spawned-first");
    suite.await_point("refused-first");
    wait_for(
        || processes.iter().all(|&pid| !alive(pid)),
        "failed launch leaves no server or descendant alive",
    );
    assert_eq!(
        kill(processes[0], None),
        Err(nix::errno::Errno::ESRCH),
        "the server leader is reaped, not merely killed"
    );
    assert!(
        fs::read_dir(&suite.root)
            .expect("suite directory")
            .any(|entry| {
                entry
                    .expect("entry")
                    .path()
                    .join("restate-server.log")
                    .exists()
            }),
        "failed startup retains its diagnostic directory"
    );
}

#[test]
fn signals_with_broken_stderr_still_stop_the_process_group() {
    for (signal, status) in [(Signal::SIGINT, 130), (Signal::SIGTERM, 143)] {
        let mut suite = Suite::start("signal-stderr-failure", signal, "registered-first");
        // Hold startup after its diagnostics and outside the lifecycle lock:
        // only the signal thread writes to the broken sink from here.
        suite.await_point("registered-first");
        let processes = suite.await_server("first");
        drop(suite.child.stderr.take().expect("piped stderr"));
        suite.signal(signal);
        suite.assert_stopped(status, &processes);
    }
}

#[test]
fn signals_during_first_launch_leave_no_processes() {
    for (signal, status) in [(Signal::SIGINT, 130), (Signal::SIGTERM, 143)] {
        let mut suite = Suite::start(
            "startup",
            signal,
            "register-e2e-stop-on-signal,spawned-first,signal-e2e-stop-on-signal",
        );
        wait_for(
            || suite.reached("register-e2e-stop-on-signal") || suite.reached("spawned-first"),
            "first launch reaches signal registration or spawn",
        );
        assert!(
            !suite.reached("spawned-first"),
            "no spawn before both signals are registered"
        );
        suite.release("register-e2e-stop-on-signal");
        suite.await_point("spawned-first");
        let processes = suite.await_server("first");
        suite.signal(signal);
        suite.await_point("signal-e2e-stop-on-signal");
        suite.release("signal-e2e-stop-on-signal");
        suite.release("spawned-first");
        suite.assert_stopped(status, &processes);
    }
}

#[test]
fn shutdown_prevents_a_concurrent_launch_from_starting_a_server() {
    for (signal, status) in [(Signal::SIGINT, 130), (Signal::SIGTERM, 143)] {
        let mut suite = Suite::start(
            "shutdown",
            signal,
            "registered-first,shutdown-e2e-stop-on-signal,registered-second",
        );
        suite.await_point("registered-first");
        let processes = suite.await_server("first");
        suite.signal(signal);
        suite.await_point("shutdown-e2e-stop-on-signal");
        suite.launch_second();
        wait_for(
            || suite.reached("registered-second") || suite.reached("refused-second"),
            "concurrent launch finishes or is refused",
        );
        assert!(
            suite.reached("refused-second"),
            "shutdown must refuse the concurrent launch"
        );
        assert!(
            !suite.reached("spawned-second"),
            "no child spawned after shutdown began"
        );
        suite.release("shutdown-e2e-stop-on-signal");
        suite.assert_stopped(status, &processes);
    }
}

#[test]
fn signals_cover_a_concurrent_child_not_yet_registered() {
    for (signal, status) in [(Signal::SIGINT, 130), (Signal::SIGTERM, 143)] {
        let mut suite = Suite::start(
            "concurrent",
            signal,
            "registered-first,spawned-second,signal-e2e-stop-on-signal",
        );
        suite.await_point("registered-first");
        let mut processes = suite.await_server("first");
        suite.launch_second();
        suite.await_point("spawned-second");
        processes.extend(suite.await_server("second"));
        suite.signal(signal);
        suite.await_point("signal-e2e-stop-on-signal");
        suite.release("signal-e2e-stop-on-signal");
        suite.release("spawned-second");
        suite.assert_stopped(status, &processes);
    }
}

#[test]
fn concurrent_first_launches_wait_for_signal_registration() {
    for (signal, status) in [(Signal::SIGINT, 130), (Signal::SIGTERM, 143)] {
        let mut suite = Suite::start(
            "initialization",
            signal,
            "register-e2e-stop-on-signal,registered-first,registered-second",
        );
        suite.await_point("register-e2e-stop-on-signal");
        suite.launch_second();
        suite.await_point("install-second");
        assert!(!suite.reached("spawned-first"));
        assert!(!suite.reached("spawned-second"));
        suite.release("register-e2e-stop-on-signal");
        suite.await_point("registered-first");
        suite.await_point("registered-second");
        let mut processes = suite.await_server("first");
        processes.extend(suite.await_server("second"));
        suite.signal(signal);
        suite.assert_stopped(status, &processes);
    }
}

#[test]
fn failed_signal_initialization_prevents_this_and_later_launches() {
    let suite = Suite::start("registration-failure", Signal::SIGTERM, "");
    suite.await_point("refused-first");
    suite.launch_second();
    suite.await_point("refused-second");
    assert!(!suite.reached("spawned-first"));
    assert!(!suite.reached("spawned-second"));
    assert!(!suite.root.join("e2e-first.pids").exists());
    assert!(!suite.root.join("e2e-second.pids").exists());
}

#[cfg(not(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
)))]
fn observe_exit(root: &Path) {
    use rustix::process::{WaitId, WaitIdOptions, waitid};
    let mut restate = super::Restate::spawn(
        &root.join("server"),
        &ServerSpec {
            name: "first",
            features: &[],
            env: &[],
        },
        "127.0.0.1".into(),
    );
    let group = restate.process.as_ref().expect("spawned").group();
    wait_for(|| restate.exited().is_some(), "leader exits");
    let child = rustix::process::Pid::from_raw(i32::try_from(group).expect("pid")).expect("pid");
    for _ in 0..2 {
        assert!(restate.exited().is_some(), "exit remains observable");
        assert!(
            waitid(
                WaitId::Pid(child),
                WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG
            )
            .expect("exit inspection must leave the leader waitable")
            .is_some()
        );
    }
    fs::write(root.join("exit-observed.ready"), "").expect("observed exit");
    wait_for(
        || root.join("drop.release").exists(),
        "drop requested or signal exits process",
    );
    drop(restate);
    assert!(!super::started().groups.contains(&group));
    assert_eq!(
        waitid(
            WaitId::Pid(child),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG
        )
        .expect_err("Drop reaped the child"),
        rustix::io::Errno::CHILD
    );
}

#[cfg(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
))]
fn observe_exit(_root: &Path) {
    panic!("exit observation is unavailable on this platform");
}

#[test]
#[cfg(not(any(
    target_os = "openbsd",
    target_os = "redox",
    target_os = "cygwin",
    target_os = "horizon"
)))]
fn exit_inspection_preserves_the_leader_until_drop_or_signal_cleanup() {
    for signal in [None, Some(Signal::SIGINT), Some(Signal::SIGTERM)] {
        let scenario = if signal.is_none() {
            "exit-observation-drop"
        } else {
            "exit-observation-signal"
        };
        let mut suite = Suite::start(scenario, signal.unwrap_or(Signal::SIGTERM), "");
        let processes = suite.await_server("first");
        kill(processes[0], Signal::SIGKILL).expect("exit the leader, keep its descendant");
        suite.await_point("exit-observed");
        assert!(
            alive(processes[1]),
            "inspection must not kill the descendant"
        );
        let status = if let Some(signal) = signal {
            suite.signal(signal);
            if signal == Signal::SIGINT { 130 } else { 143 }
        } else {
            fs::write(suite.root.join("drop.release"), "").expect("drop handle");
            0
        };
        suite.assert_stopped(status, &processes);
    }
}
