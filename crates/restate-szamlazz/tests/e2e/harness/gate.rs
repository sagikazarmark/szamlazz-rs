//! Where the suite's Restate server comes from (the *server gate*
//! ([`server_gate`]), decided once from the environment before anything
//! starts), and the server itself ([`Restate`]): a running one reused, a
//! `restate-server` binary spawned on the loopback, or a container of
//! [`IMAGE`]. The two suites of the binary run concurrently, each on a server
//! of its own shape ([`MAIN_SERVER`], [`WITHOUT_PROTOCOL_V7`]), on ports
//! chosen free at launch ([`Ports`]): nothing is fixed, so two runs on one
//! host collide with nothing.
//!
//! A server the harness starts is stopped when the harness drops, and by a
//! SIGINT or SIGTERM to the test process ([`stop_on_signal`]), which unwinds
//! nothing: the container is named per run and labelled, so a stale one (its
//! test process gone) is found and removed by the next run, while a
//! concurrent run's is left alone; the process leads a process group of its
//! own and the group is killed.

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::net::TcpListener;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, Once, PoisonError};

const IMAGE: &str = "docker.restate.dev/restatedev/restate:1.7.8";

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// Where the suite's Restate server comes from, decided from the environment
/// before anything starts ([`server_gate`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Launcher {
    /// `RESTATE_ADMIN_URL` / `RESTATE_INGRESS_URL`: a running server, with the
    /// three flags on ([`SERVER_FLAGS`]); nothing is started or stopped.
    Reuse { admin: String, ingress: String },
    /// `RESTATE_SERVER_BIN`: a `restate-server` binary the harness spawns on
    /// this host, on the ports the server spec names (what the Dagger check
    /// uses, where there is no docker).
    Binary(PathBuf),
    /// The docker daemon: a container of [`IMAGE`].
    Docker,
}

/// The server gate, the decision behind [`launcher_or_skip`]: `reuse` first,
/// then `binary`, then docker (probed only when neither is given); with none
/// of them `Ok(None)` (a skip) unless `ci` is set (non-empty), in which
/// case the suite must not pass by skipping and the answer is the failure
/// message, naming every way to provide a server.
fn server_gate(
    reuse: Option<(String, String)>,
    binary: Option<PathBuf>,
    docker_available: impl FnOnce() -> bool,
    ci: Option<&OsStr>,
) -> Result<Option<Launcher>, String> {
    if let Some((admin, ingress)) = reuse {
        return Ok(Some(Launcher::Reuse { admin, ingress }));
    }
    if let Some(binary) = binary {
        return Ok(Some(Launcher::Binary(binary)));
    }
    if docker_available() {
        return Ok(Some(Launcher::Docker));
    }
    if ci.is_some_and(|value| !value.is_empty()) {
        return Err(
            "no Restate server to run the end-to-end suite against, and CI is set: a skipped run \
             proves nothing. Provide one by setting RESTATE_SERVER_BIN to a restate-server binary \
             (spawned on this host), by making a docker daemon reachable (a container of the \
             Restate image), or by setting RESTATE_ADMIN_URL and RESTATE_INGRESS_URL to a running \
             server with the three experimental flags."
                .to_owned(),
        );
    }
    Ok(None)
}

/// Whether the suite may reuse a server from the environment: the main suite
/// may, a suite that needs a server of its own shape (without a flag) may not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reuse {
    Allowed,
    Never,
}

/// The launcher the environment provides, or `None` after printing why the
/// suite skips; panics with the gate's message under `CI`.
pub(crate) fn launcher_or_skip(reuse: Reuse) -> Option<Launcher> {
    let reusable = match reuse {
        Reuse::Allowed => std::env::var("RESTATE_ADMIN_URL")
            .ok()
            .zip(std::env::var("RESTATE_INGRESS_URL").ok()),
        Reuse::Never => None,
    };
    let binary = std::env::var_os("RESTATE_SERVER_BIN")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    match server_gate(
        reusable,
        binary,
        docker_available,
        std::env::var_os("CI").as_deref(),
    ) {
        Ok(Some(launcher)) => Some(launcher),
        Ok(None) => {
            eprintln!(
                "skipping: no Restate server (no docker daemon, no RESTATE_SERVER_BIN, no \
                 RESTATE_ADMIN_URL / RESTATE_INGRESS_URL)"
            );
            None
        }
        Err(message) => panic!("{message}"),
    }
}

/// The three experimental server features multi-account mode depends on
/// (vqueues, protocol v7 (below it the SDK sees no scope) and scoped Virtual
/// Objects), as `/version` reports each, with the environment flag that
/// enables it.
pub(crate) const FEATURES: [(&str, &str); 3] = [
    ("vqueues", "RESTATE_EXPERIMENTAL_ENABLE_VQUEUES=true"),
    (
        "protocol_v7",
        "RESTATE_EXPERIMENTAL_ENABLE_PROTOCOL_V7=true",
    ),
    (
        "scoped_virtual_objects",
        "RESTATE_EXPERIMENTAL_ENABLE_SCOPED_VIRTUAL_OBJECTS=true",
    ),
];

/// The flags of the main suite's server: all three. Set on the server the
/// harness starts and expected of one reused through the environment.
const SERVER_FLAGS: [&str; 3] = [FEATURES[0].1, FEATURES[1].1, FEATURES[2].1];

/// The shape of a server the harness starts: its name (in the node name, the
/// base dir and the container name) and its flags. Its ports are chosen free
/// at launch ([`Ports`]), so two suites in one test binary, and two runs on
/// one host, each have their own.
pub(crate) struct ServerSpec {
    name: &'static str,
    flags: &'static [&'static str],
}

/// The main suite's server: the three flags.
pub(crate) const MAIN_SERVER: ServerSpec = ServerSpec {
    name: "main",
    flags: &SERVER_FLAGS,
};

/// The protocol-v7 canary's server: vqueues and scoped Virtual Objects on,
/// protocol v7 off (a deployment that forgot the one flag the scope needs to
/// reach the SDK).
pub(crate) const WITHOUT_PROTOCOL_V7: ServerSpec = ServerSpec {
    name: "canary",
    flags: &[FEATURES[0].1, FEATURES[2].1],
};

/// The host ports of a server the harness started, chosen free at launch: a
/// listener on port 0 for each of the spawned binary's (bound, read, released
/// and passed through `RESTATE_*`), a docker-assigned host port for the
/// container's (`-p 0:8080`, read back with `docker port`). Nothing here is
/// fixed, so a second run on the host, another Restate or anything else on a
/// port collides with nothing.
#[derive(Debug, Clone, Copy)]
struct Ports {
    ingress: u16,
    admin: u16,
    /// The spawned binary's node port; a container's is not published.
    node: Option<u16>,
}

impl fmt::Display for Ports {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "admin {} / ingress {}", self.admin, self.ingress)?;
        if let Some(node) = self.node {
            write!(f, " / node {node}")?;
        }
        Ok(())
    }
}

/// `N` distinct free ports on the loopback: each bound on port 0 and read
/// back, all released together. Free at this moment, not reserved: what the
/// server binds a moment later, so a port taken in between is reported by
/// the server failing to start ([`Restate::exited`]), naming the ports.
fn free_ports<const N: usize>() -> [u16; N] {
    let listeners: Vec<TcpListener> = (0..N)
        .map(|_| TcpListener::bind("127.0.0.1:0").expect("a free loopback port"))
        .collect();
    let mut ports = [0; N];
    for (port, listener) in ports.iter_mut().zip(&listeners) {
        *port = listener.local_addr().expect("the bound address").port();
    }
    ports
}

/// The label every container the harness starts carries, with the pid of the
/// test process that started it: how a stale one (its process gone) is found
/// and removed by the next run, whichever of the shapes it belonged to.
const CONTAINER_LABEL: &str = "szamlazz-e2e";
const CONTAINER_PID_LABEL: &str = "szamlazz-e2e.pid";

/// A Restate server: an existing one (from the environment), a `restate-server`
/// process, or a container (the last two stopped on drop, and on a stop
/// signal to the test process).
pub(crate) struct Restate {
    pub(crate) admin: String,
    pub(crate) ingress: String,
    /// The flags the server runs with: what `/version` must report.
    pub(crate) flags: &'static [&'static str],
    /// The host name under which the server reaches this process's endpoint.
    pub(crate) endpoint_host: String,
    /// The ports of a server the harness started; a reused one's are in its
    /// URLs.
    ports: Option<Ports>,
    /// What stops the server this handle started, if it started one.
    stopper: Option<Stopper>,
    /// The spawned server, kept to be reaped after the group is killed.
    process: Option<Child>,
    /// The spawned server's base directory, removed on drop unless the test
    /// is failing; then it stays, with `restate-server.log` in it.
    base_dir: Option<PathBuf>,
}

/// What stops a server the harness started.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Stopper {
    /// `docker rm -f` of the container by its name.
    Container(String),
    /// `killpg(2)` of the process group the spawned server leads (its pid is
    /// the group's id: it was spawned with `process_group(0)`).
    ProcessGroup(u32),
}

/// Whether [`Stopper::stop`] waits for the docker daemon to finish removing
/// the container. The drop path waits; the signal path, which exits right
/// after, does not: a `docker rm -f` takes a few hundred milliseconds, during
/// which the server is already dead and a test thread's ingress call fails,
/// unwinds and races the handler to a failure exit, and the `docker` process
/// finishes the removal on its own either way. A process group is killed at
/// once in both cases; the drop path reaps the leader afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wait {
    ForRemoval,
    No,
}

impl Stopper {
    /// Stops the server; a failure to do so is reported on stderr (the next
    /// run removes a container left behind by its label; a process group is
    /// the operator's to find), never swallowed.
    fn stop(&self, wait: Wait) {
        match self {
            Self::Container(name) => {
                // stderr stays inherited: docker's own diagnostic, if any,
                // reaches the terminal.
                let mut rm = Command::new("docker");
                rm.args(["rm", "-f", name])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null());
                let outcome = match wait {
                    Wait::ForRemoval => rm.status().map(|status| status.success()),
                    Wait::No => rm.spawn().map(|_| true),
                };
                match outcome {
                    Ok(true) => {}
                    Ok(false) => eprintln!(
                        "WARNING: `docker rm -f {name}` failed; the container may be left behind \
                         (the next run removes it by its label)"
                    ),
                    Err(error) => eprintln!(
                        "WARNING: could not run `docker rm -f {name}` ({error}); the container is \
                         left behind (the next run removes it by its label)"
                    ),
                }
            }
            Self::ProcessGroup(pid) => kill_group(*pid),
        }
    }
}

/// `SIGKILL` to the process group `pid` leads; a group already gone
/// (`ESRCH`) is the wanted state, any other failure is reported.
fn kill_group(pid: u32) {
    use nix::errno::Errno;
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;
    let Ok(pid) = i32::try_from(pid) else {
        eprintln!(
            "WARNING: the pid {pid} does not fit `killpg(2)`; the restate-server is left behind"
        );
        return;
    };
    match killpg(Pid::from_raw(pid), Signal::SIGKILL) {
        Ok(()) | Err(Errno::ESRCH) => {}
        Err(error) => eprintln!(
            "WARNING: killpg({pid}, SIGKILL) failed ({error}); the restate-server may be left \
             behind (`pkill restate-server`)"
        ),
    }
}

/// Whether the process `pid` is alive: what tells a stale container (its
/// test process gone) from a concurrent run's live one.
fn process_alive(pid: i32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // No signal is sent, the check is made. EPERM is a live process of
    // another user.
    !matches!(kill(Pid::from_raw(pid), None), Err(Errno::ESRCH))
}

/// Removes every container of [`CONTAINER_LABEL`] whose starting process is
/// gone: the servers of interrupted runs (a `kill -9`, a runner cut off), which
/// `--rm` does not remove because the server itself is still running. A
/// container whose process is alive is a concurrent run's and is left alone.
fn remove_stale_containers() {
    let Ok(output) = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("label={CONTAINER_LABEL}"),
            "--format",
            &format!("{{{{.ID}}}} {{{{.Label \"{CONTAINER_PID_LABEL}\"}}}}"),
        ])
        .output()
    else {
        return;
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((id, pid)) = line.split_once(' ') else {
            continue;
        };
        let stale = pid.parse::<i32>().is_ok_and(|pid| !process_alive(pid));
        if stale {
            let _ = Command::new("docker").args(["rm", "-f", id]).output();
            eprintln!("removed the container {id} a run (pid {pid}) left behind");
        }
    }
}

/// Every server this test process started and has not stopped yet, by what
/// stops it: what [`stop_on_signal`] runs when the process is told to stop.
static STARTED: Mutex<Vec<Stopper>> = Mutex::new(Vec::new());

fn started() -> std::sync::MutexGuard<'static, Vec<Stopper>> {
    STARTED.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Stops every started server on SIGINT or SIGTERM to the test process, then
/// exits with the signal's conventional status. A signal ends the process
/// without unwinding, so nothing's `Drop` runs; without this, a Ctrl-C leaves
/// the container running under the daemon and the process (in a group of its
/// own, so the terminal's SIGINT does not reach it) holding its ports until
/// the next run finds it. Installed once, on the first server started; its
/// own thread and runtime, so it outlives the test that started the first
/// server.
fn stop_on_signal() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let handler = std::thread::Builder::new()
            .name("e2e-stop-on-signal".to_owned())
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let Ok(runtime) = runtime else {
                    eprintln!(
                        "WARNING: no runtime for the stop-signal handler; a Ctrl-C will leave the \
                         Restate servers this run starts behind (the next run removes them)"
                    );
                    return;
                };
                runtime.block_on(async {
                    let (signal, status) = match stop_signal().await {
                        Ok(stopped) => stopped,
                        Err(error) => {
                            eprintln!(
                                "WARNING: the stop-signal handler could not register ({error}); a \
                                 Ctrl-C will leave the Restate servers this run starts behind (the \
                                 next run removes them)"
                            );
                            return;
                        }
                    };
                    let stoppers = started().clone();
                    eprintln!(
                        "{signal}: stopping {} Restate server(s) the suite started, then exiting",
                        stoppers.len()
                    );
                    for stopper in &stoppers {
                        stopper.stop(Wait::No);
                    }
                    std::process::exit(status);
                });
            });
        if let Err(error) = handler {
            eprintln!(
                "WARNING: no thread for the stop-signal handler ({error}); a Ctrl-C will leave the \
                 Restate servers this run starts behind (the next run removes them)"
            );
        }
    });
}

/// The first of SIGINT and SIGTERM, with the exit status convention for it.
async fn stop_signal() -> std::io::Result<(&'static str, i32)> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(tokio::select! {
        _ = interrupt.recv() => ("SIGINT", 130),
        _ = terminate.recv() => ("SIGTERM", 143),
    })
}

impl Launcher {
    /// The server of `spec`'s shape: started from the binary or the image, or
    /// the running one taken as it is (checked against `spec`'s flags like
    /// the others; the caller reuses only where the shape is the main
    /// suite's).
    pub(crate) fn launch(self, spec: &ServerSpec) -> Restate {
        let endpoint_host = |default: &str| {
            std::env::var("RESTATE_ENDPOINT_HOST").unwrap_or_else(|_| default.to_owned())
        };
        match self {
            Self::Reuse { admin, ingress } => Restate {
                admin,
                ingress,
                flags: spec.flags,
                endpoint_host: endpoint_host("host.docker.internal"),
                ports: None,
                stopper: None,
                process: None,
                base_dir: None,
            },
            Self::Binary(binary) => Restate::spawn(&binary, spec, endpoint_host("127.0.0.1")),
            Self::Docker => Restate::container(spec, endpoint_host("host.docker.internal")),
        }
    }
}

impl Restate {
    /// A server reachable on the loopback at `ports` with `spec`'s flags,
    /// running nothing of its own yet: what the two launchers fill in.
    fn on_ports(ports: Ports, spec: &ServerSpec, endpoint_host: String) -> Self {
        Self {
            admin: format!("http://127.0.0.1:{}", ports.admin),
            ingress: format!("http://127.0.0.1:{}", ports.ingress),
            flags: spec.flags,
            endpoint_host,
            ports: Some(ports),
            stopper: None,
            process: None,
            base_dir: None,
        }
    }

    /// Records what stops the server this handle started, for drop and for a
    /// stop signal.
    fn started(&mut self, stopper: Stopper) {
        started().push(stopper.clone());
        stop_on_signal();
        self.stopper = Some(stopper);
    }

    /// A container of [`IMAGE`] with `spec`'s flags, its ingress and admin
    /// ports published on docker-assigned host ports, under a name of this
    /// run (the pid and the shape's name) and the label a stale one is found
    /// by; the stale containers of earlier runs are removed first.
    fn container(spec: &ServerSpec, endpoint_host: String) -> Self {
        remove_stale_containers();
        let name = format!("restate-szamlazz-e2e-{}-{}", std::process::id(), spec.name);
        let mut args = vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            "--name".to_owned(),
            name.clone(),
            "--label".to_owned(),
            format!("{CONTAINER_LABEL}=1"),
            "--label".to_owned(),
            format!("{CONTAINER_PID_LABEL}={}", std::process::id()),
            // Docker Desktop resolves `host.docker.internal` on its own;
            // a Linux daemon needs the alias to reach the endpoint.
            "--add-host=host.docker.internal:host-gateway".to_owned(),
            "-p".to_owned(),
            "0:8080".to_owned(),
            "-p".to_owned(),
            "0:9070".to_owned(),
        ];
        for flag in spec.flags {
            args.push("-e".to_owned());
            args.push((*flag).to_owned());
        }
        args.push(IMAGE.to_owned());
        let output = Command::new("docker")
            .args(&args)
            .output()
            .expect("docker run");
        assert!(
            output.status.success(),
            "docker run failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // Stop the container on every failure from here on, the port
        // read-back included.
        let mut restate = Self {
            admin: String::new(),
            ingress: String::new(),
            flags: spec.flags,
            endpoint_host,
            ports: None,
            stopper: None,
            process: None,
            base_dir: None,
        };
        restate.started(Stopper::Container(name.clone()));
        let ports = Ports {
            ingress: published_port(&name, 8080),
            admin: published_port(&name, 9070),
            node: None,
        };
        restate.admin = format!("http://127.0.0.1:{}", ports.admin);
        restate.ingress = format!("http://127.0.0.1:{}", ports.ingress);
        restate.ports = Some(ports);
        eprintln!("restate-server container {name} on {ports}");
        restate
    }

    /// A `restate-server` process from `binary` with `spec`'s flags, bound to
    /// the loopback on three ports chosen free ([`free_ports`]), its data and
    /// log under a directory of its own in the temp dir, leading a process
    /// group of its own so that the group is what gets killed. Configured
    /// through Restate's environment (`RESTATE_<SECTION>__<KEY>`), so no
    /// config file is written.
    fn spawn(binary: &PathBuf, spec: &ServerSpec, endpoint_host: String) -> Self {
        let [ingress, admin, node] = free_ports::<3>();
        let ports = Ports {
            ingress,
            admin,
            node: Some(node),
        };
        let base_dir = std::env::temp_dir().join(format!(
            "restate-szamlazz-e2e-{}-{}",
            std::process::id(),
            spec.name
        ));
        fs::create_dir_all(&base_dir).expect("the server's base dir");
        let log = fs::File::create(base_dir.join("restate-server.log")).expect("the server log");
        let mut command = Command::new(binary);
        command
            .arg("--no-logo")
            .env("RESTATE_BASE_DIR", &base_dir)
            .env("RESTATE_NODE_NAME", format!("e2e-{}", spec.name))
            .env("RESTATE_LISTEN_MODE", "tcp")
            .env("RESTATE_BIND_ADDRESS", format!("127.0.0.1:{node}"))
            .env(
                "RESTATE_ADVERTISED_ADDRESS",
                format!("http://127.0.0.1:{node}"),
            )
            .env(
                "RESTATE_INGRESS__BIND_ADDRESS",
                format!("127.0.0.1:{ingress}"),
            )
            .env("RESTATE_ADMIN__BIND_ADDRESS", format!("127.0.0.1:{admin}"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().expect("the server log")))
            .stderr(Stdio::from(log))
            .process_group(0);
        for flag in spec.flags {
            let (name, value) = flag.split_once('=').expect("NAME=value");
            command.env(name, value);
        }
        let process = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));
        eprintln!(
            "restate-server (pid {}) on {ports}, base dir {}",
            process.id(),
            base_dir.display()
        );
        let mut restate = Self::on_ports(ports, spec, endpoint_host);
        restate.started(Stopper::ProcessGroup(process.id()));
        restate.process = Some(process);
        restate.base_dir = Some(base_dir);
        restate
    }

    /// Why the server the harness started is gone, if it is: the spawned
    /// process exited (its status and the tail of its log), or the container
    /// is not running. Polled while waiting for the admin API, so a server
    /// that cannot start (a port chosen free and taken since, most likely) is
    /// reported at once, naming its ports, rather than waited on until the
    /// deadline. `None` for a reused server, and for one still running.
    pub(crate) fn exited(&mut self) -> Option<String> {
        let ports = self.ports?;
        if let Some(process) = &mut self.process {
            let status = process.try_wait().ok().flatten()?;
            let log = self
                .base_dir
                .as_ref()
                .and_then(|base_dir| fs::read_to_string(base_dir.join("restate-server.log")).ok())
                .unwrap_or_default();
            let tail: Vec<&str> = log.lines().rev().take(30).collect();
            let tail: Vec<&str> = tail.into_iter().rev().collect();
            return Some(format!(
                "restate-server exited with {status} before its admin API came up, on ports \
                 {ports} (chosen free at launch; one may have been taken since). The last lines \
                 of its log:\n{}",
                tail.join("\n")
            ));
        }
        if let Some(Stopper::Container(name)) = &self.stopper {
            let running = Command::new("docker")
                .args(["inspect", "--format", "{{.State.Running}}", name])
                .output()
                .is_ok_and(|output| {
                    output.status.success()
                        && String::from_utf8_lossy(&output.stdout).trim() == "true"
                });
            if !running {
                return Some(format!(
                    "the container {name} is not running before its admin API came up, on ports \
                     {ports}"
                ));
            }
        }
        None
    }
}

/// The host port docker published `container_port` of `name` on: the first
/// line of `docker port` (`0.0.0.0:32768`, then the IPv6 twin).
fn published_port(name: &str, container_port: u16) -> u16 {
    let output = Command::new("docker")
        .args(["port", name, &container_port.to_string()])
        .output()
        .expect("docker port");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .find_map(|line| line.rsplit_once(':')?.1.parse().ok())
        .unwrap_or_else(|| {
            panic!(
                "docker port {name} {container_port}: no published port in {stdout:?} ({})",
                String::from_utf8_lossy(&output.stderr)
            )
        })
}

impl Drop for Restate {
    fn drop(&mut self) {
        if let Some(stopper) = self.stopper.take() {
            stopper.stop(Wait::ForRemoval);
            started().retain(|started| *started != stopper);
        }
        if let Some(process) = &mut self.process {
            // The group is killed above; this reaps the leader.
            let _ = process.kill();
            let _ = process.wait();
        }
        if let Some(base_dir) = &self.base_dir {
            if std::thread::panicking() {
                eprintln!(
                    "restate-server's base dir is kept for inspection: {}",
                    base_dir.display()
                );
            } else {
                let _ = fs::remove_dir_all(base_dir);
            }
        }
    }
}

// ----- the harness's server gate, without a server ------------------------------

/// The gate reads the environment once: a reusable server wins, then a
/// `restate-server` binary, then docker; with none of the three the suite
/// skips on a developer machine and **fails** under `CI`, naming every way to
/// provide a server; a suite that passes by skipping proves nothing.
#[test]
fn the_server_gate_prefers_a_reused_server_then_the_binary_then_docker() {
    let reuse = Some(("http://a:9070".to_owned(), "http://a:8080".to_owned()));
    let binary = Some(PathBuf::from("/opt/restate-server"));
    assert_eq!(
        server_gate(reuse.clone(), binary.clone(), || true, None),
        Ok(Some(Launcher::Reuse {
            admin: "http://a:9070".to_owned(),
            ingress: "http://a:8080".to_owned(),
        }))
    );
    assert_eq!(
        server_gate(None, binary.clone(), || true, None),
        Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server"))))
    );
    assert_eq!(
        server_gate(None, binary, || false, Some(OsStr::new("true"))),
        Ok(Some(Launcher::Binary(PathBuf::from("/opt/restate-server")))),
        "the binary needs no docker, under CI too"
    );
    assert_eq!(
        server_gate(None, None, || true, None),
        Ok(Some(Launcher::Docker))
    );
}

#[test]
fn the_server_gate_skips_without_a_server_and_fails_under_ci() {
    assert_eq!(
        server_gate(None, None, || false, None),
        Ok(None),
        "no CI: skip"
    );
    assert_eq!(
        server_gate(None, None, || false, Some(OsStr::new(""))),
        Ok(None),
        "an empty CI is unset"
    );
    for ci in ["true", "1", "yes"] {
        let message = server_gate(None, None, || false, Some(OsStr::new(ci)))
            .expect_err("CI is set and there is no server: a failure, never a skip");
        for named in ["CI", "docker", "RESTATE_SERVER_BIN", "RESTATE_ADMIN_URL"] {
            assert!(message.contains(named), "CI={ci}: {message}");
        }
    }
}
