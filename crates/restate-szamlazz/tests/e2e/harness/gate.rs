//! Where the suite's Restate server comes from (the *server gate*
//! ([`server_gate`]), decided once from the environment before anything
//! starts), and the server itself ([`Restate`]): a running one reused, a
//! `restate-server` binary spawned on the loopback, or a container of
//! [`IMAGE`]. The two suites of the binary run concurrently, each on a server
//! of its own shape ([`MAIN_SERVER`], [`WITHOUT_PROTOCOL_V7`]).

use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

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

/// The shape of a server the harness starts: its flags and the host ports of
/// its ingress and admin APIs (and, for a spawned binary, of its node port).
/// Two suites in one test binary run concurrently, so each has its own.
pub(crate) struct ServerSpec {
    flags: &'static [&'static str],
    ingress_port: u16,
    admin_port: u16,
    node_port: u16,
}

/// The main suite's server: the three flags.
pub(crate) const MAIN_SERVER: ServerSpec = ServerSpec {
    flags: &SERVER_FLAGS,
    ingress_port: 18080,
    admin_port: 19070,
    node_port: 15122,
};

/// The protocol-v7 canary's server: vqueues and scoped Virtual Objects on,
/// protocol v7 off (a deployment that forgot the one flag the scope needs to
/// reach the SDK). Its own ports: the two suites run concurrently.
pub(crate) const WITHOUT_PROTOCOL_V7: ServerSpec = ServerSpec {
    flags: &[FEATURES[0].1, FEATURES[2].1],
    ingress_port: 18081,
    admin_port: 19071,
    node_port: 15222,
};

/// A Restate server: an existing one (from the environment), a `restate-server`
/// process, or a container (the last two stopped on drop).
pub(crate) struct Restate {
    pub(crate) admin: String,
    pub(crate) ingress: String,
    /// The flags the server runs with: what `/version` must report.
    pub(crate) flags: &'static [&'static str],
    /// The host name under which the server reaches this process's endpoint.
    pub(crate) endpoint_host: String,
    container: Option<String>,
    process: Option<Child>,
    /// The spawned server's base directory, removed on drop unless the test
    /// is failing; then it stays, with `restate-server.log` in it.
    base_dir: Option<PathBuf>,
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
            Self::Reuse { admin, ingress } => {
                let mut restate =
                    Restate::on_host_ports(spec, endpoint_host("host.docker.internal"));
                restate.admin = admin;
                restate.ingress = ingress;
                restate
            }
            Self::Binary(binary) => Restate::spawn(&binary, spec, endpoint_host("127.0.0.1")),
            Self::Docker => Restate::container(spec, endpoint_host("host.docker.internal")),
        }
    }
}

impl Restate {
    /// A server reachable on `spec`'s host ports with `spec`'s flags, running
    /// nothing of its own yet: what every launcher fills in.
    fn on_host_ports(spec: &ServerSpec, endpoint_host: String) -> Self {
        Self {
            admin: format!("http://127.0.0.1:{}", spec.admin_port),
            ingress: format!("http://127.0.0.1:{}", spec.ingress_port),
            flags: spec.flags,
            endpoint_host,
            container: None,
            process: None,
            base_dir: None,
        }
    }

    /// A container of [`IMAGE`] with `spec`'s flags, its ingress and admin
    /// ports published on `spec`'s host ports.
    fn container(spec: &ServerSpec, endpoint_host: String) -> Self {
        let mut args = vec![
            "run".to_owned(),
            "--rm".to_owned(),
            "-d".to_owned(),
            // Docker Desktop resolves `host.docker.internal` on its own;
            // a Linux daemon needs the alias to reach the endpoint.
            "--add-host=host.docker.internal:host-gateway".to_owned(),
            "-p".to_owned(),
            format!("{}:8080", spec.ingress_port),
            "-p".to_owned(),
            format!("{}:9070", spec.admin_port),
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
        let container = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let mut restate = Self::on_host_ports(spec, endpoint_host);
        restate.container = Some(container);
        restate
    }

    /// A `restate-server` process from `binary` with `spec`'s flags, bound to
    /// the loopback on `spec`'s ports, its data and log under a directory of
    /// its own in the temp dir. Configured through Restate's environment
    /// (`RESTATE_<SECTION>__<KEY>`), so no config file is written.
    fn spawn(binary: &PathBuf, spec: &ServerSpec, endpoint_host: String) -> Self {
        let base_dir = std::env::temp_dir().join(format!(
            "restate-szamlazz-e2e-{}-{}",
            std::process::id(),
            spec.admin_port
        ));
        fs::create_dir_all(&base_dir).expect("the server's base dir");
        let log = fs::File::create(base_dir.join("restate-server.log")).expect("the server log");
        let mut command = Command::new(binary);
        command
            .arg("--no-logo")
            .env("RESTATE_BASE_DIR", &base_dir)
            .env("RESTATE_NODE_NAME", format!("e2e-{}", spec.admin_port))
            .env("RESTATE_LISTEN_MODE", "tcp")
            .env(
                "RESTATE_BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.node_port),
            )
            .env(
                "RESTATE_ADVERTISED_ADDRESS",
                format!("http://127.0.0.1:{}", spec.node_port),
            )
            .env(
                "RESTATE_INGRESS__BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.ingress_port),
            )
            .env(
                "RESTATE_ADMIN__BIND_ADDRESS",
                format!("127.0.0.1:{}", spec.admin_port),
            )
            .stdin(Stdio::null())
            .stdout(Stdio::from(log.try_clone().expect("the server log")))
            .stderr(Stdio::from(log));
        for flag in spec.flags {
            let (name, value) = flag.split_once('=').expect("NAME=value");
            command.env(name, value);
        }
        let process = command
            .spawn()
            .unwrap_or_else(|error| panic!("spawn {}: {error}", binary.display()));
        eprintln!(
            "restate-server (pid {}) on admin {} / ingress {}, base dir {}",
            process.id(),
            spec.admin_port,
            spec.ingress_port,
            base_dir.display()
        );
        let mut restate = Self::on_host_ports(spec, endpoint_host);
        restate.process = Some(process);
        restate.base_dir = Some(base_dir);
        restate
    }
}

impl Drop for Restate {
    fn drop(&mut self) {
        if let Some(container) = &self.container {
            let _ = Command::new("docker")
                .args(["rm", "-f", container])
                .output();
        }
        if let Some(process) = &mut self.process {
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
